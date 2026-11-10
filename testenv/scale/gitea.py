"""Gitea (or Forgejo) at Björn's scale: three `scale-*` organizations beside
`tidewater`, ~60 repositories, their history, branches, pull requests,
issues, comments and reviews.

History is built here with `git fast-import` -- commits dated over the two
years, by the fixture's people -- and pushed over http. Pull requests are
opened through the API while `main` still points at the root commit; the
final `main`, holding the merge commits, is pushed afterwards, and Gitea's
manual-merge detection (`autodetect_manual_merge`) marks every pull request
whose head it now contains as merged, at the merge commit's own date. What
the API stamps with the seed's clock (an issue's or pull request's created,
updated and closed times, its comments', its reviews') is rewritten in the
forge's SQLite afterwards (`docker exec <forge> sqlite3`).

Each repository records its finished phases in an Actions variable,
`KNOBAS_SCALE_SEED`, which nothing knobas reads, so a rerun reads one
variable per repository and writes nothing.
"""

from __future__ import annotations

import datetime as dt
import json
import os
import shutil
import subprocess
import tempfile
import time

from common import (END, START, Http, HttpError, basic, chunks, die, later, paragraph, parallel,
                    q, rng, say, sentence, summary, weighted, when, zipf_pick, COMPONENTS, OBJECTS,
                    VERBS, PROBLEMS)

VAR = "KNOBAS_SCALE_SEED"
PEOPLE = [("mara.lindqvist", 24), ("jonas.becker", 22), ("priya.nair", 14), ("tomasz.wojcik", 18),
          ("lena.hoffmann", 6), ("knobas", 16)]
ORGS = [
    ("scale-platform", "Scale test: Platform", [
        "auth-gateway", "service-mesh", "deploy-tools", "terraform-modules", "k8s-manifests",
        "observability-stack", "secrets-operator", "ci-templates", "base-images", "dns-automation",
        "log-shipper", "rate-limiter", "feature-flags", "config-service", "backup-runner",
        "cert-manager-config", "incident-bot", "status-page", "cost-reports", "runbooks"]),
    ("scale-commerce", "Scale test: Commerce", [
        "checkout-web", "cart-service", "pricing-engine", "payments-gateway", "refund-service",
        "invoice-renderer", "catalog-api", "search-indexer", "promotions", "mobile-ios",
        "mobile-android", "storefront-cms", "partner-api", "notifications", "loyalty",
        "returns-portal", "tax-service", "shipping-rates", "gift-cards", "design-system"]),
    ("scale-data", "Scale test: Data", [
        "warehouse-loader", "event-bus", "schema-registry", "metrics-pipeline", "reporting",
        "dbt-models", "ml-features", "fraud-scoring", "airflow-dags", "kafka-connectors",
        "data-quality", "customer-360", "forecasting", "experimentation", "lakehouse-infra",
        "pii-scanner", "cdc-streams", "notebooks", "bi-dashboards", "data-contracts"]),
]
LANGS = {
    "Go": ("go", ["cmd/server/main.go", "internal/{c}/{c}.go", "internal/{c}/{c}_test.go", "go.mod"]),
    "Rust": ("rs", ["src/main.rs", "src/{c}.rs", "tests/{c}.rs", "Cargo.toml"]),
    "Kotlin": ("kt", ["src/main/kotlin/App.kt", "src/main/kotlin/{C}.kt", "src/test/kotlin/{C}Test.kt", "build.gradle.kts"]),
    "Python": ("py", ["app/__init__.py", "app/{c}.py", "tests/test_{c}.py", "pyproject.toml"]),
    "TypeScript": ("ts", ["src/index.ts", "src/{c}.ts", "src/{c}.test.ts", "package.json"]),
    "Java": ("java", ["src/main/java/App.java", "src/main/java/{C}.java", "src/test/java/{C}Test.java", "pom.xml"]),
    "HCL": ("tf", ["main.tf", "modules/{c}/main.tf", "modules/{c}/variables.tf", "versions.tf"]),
}
LABELS = [("kind/bug", "ee0701"), ("kind/feature", "0e8a16"), ("kind/chore", "c5def5"),
          ("kind/docs", "bfd4f2"), ("priority/high", "d93f0b"), ("priority/medium", "fbca04"),
          ("priority/low", "c2e0c6"), ("needs-triage", "ededed"), ("good first issue", "7057ff"),
          ("blocked", "000000"), ("security", "b60205"), ("performance", "5319e7")]


def users_map(h: Http):
    return {u: (h.get(f"/api/v1/users/{u}") or {}).get("email") or f"{u}@tidewater.test" for u, _ in PEOPLE}


class Item:
    __slots__ = ("kind", "n", "title", "body", "author", "created", "end", "state", "labels",
                 "assignee", "milestone", "comments", "branch", "commits", "reviews", "keep_branch", "fork_at")


class Repo:
    def __init__(self, org, name, k, share, total):
        r = rng("gitea", org, name)
        self.org, self.name, self.k = org, name, k
        self.lang = r.choice(list(LANGS))
        self.created = START + dt.timedelta(days=r.uniform(0, 420)) if k > 4 else START + dt.timedelta(days=r.uniform(0, 30))
        self.description = f"{name.replace('-', ' ').capitalize()}: {sentence(r, 5, 12).rstrip('.!?').lower()}."
        self.main_commits = int(min(max(25 + share * total * 0.9, 25), 1500))
        n_prs = int(min(max(round(share * total * 0.17), 2), 190))
        n_issues = max(1, round(share * total * 0.26))
        self.plain_branches = [f"{r.choice(['release', 'experiment', 'spike', 'hotfix'])}/{r.choice(COMPONENTS).replace(' ', '-')}-{j}"
                               for j in range(1 + round(share * total * 0.02))]
        self.comps = r.sample(COMPONENTS, 4)
        self.milestones = []
        t = self.created
        while t < END + dt.timedelta(days=60):
            q_ = (t.month - 1) // 3 + 1
            self.milestones.append((f"{t.year} Q{q_}", dt.date(t.year, q_ * 3, 28)))
            t = dt.datetime(t.year + (q_ == 4), (q_ % 4) * 3 + 1, 1, tzinfo=dt.timezone.utc)
        items = []
        for j in range(n_prs):
            items.append(self.pr(rng("gitea", org, name, "pr", j), j, n_prs))
        for j in range(n_issues):
            items.append(self.issue(rng("gitea", org, name, "issue", j), j, n_issues))
        items.sort(key=lambda it: it.created)
        for n, it in enumerate(items, 1):
            it.n = n
        self.items = items

    def span(self, j, n, r):
        frac = (j + r.random()) / max(n, 1)
        return self.created + (END - self.created) * frac ** 0.8   # busier lately

    def pr(self, r, j, n):
        it = Item()
        it.kind = "pr"
        comp = r.choice(self.comps)
        kind = weighted(r, [("feat", 45), ("fix", 35), ("chore", 12), ("docs", 8)])
        it.title = (f"{kind}({comp.split()[0]}): {r.choice(VERBS).lower()} {r.choice(OBJECTS)}"
                    if r.random() < 0.6 else summary(r))
        it.branch = f"{'feature' if kind == 'feat' else kind}/{comp.replace(' ', '-')}-{j + 1}"
        it.fork_at = self.span(j, n, r)
        it.commits = r.randint(1, 6) if r.random() < 0.9 else r.randint(7, 25)
        it.created = it.fork_at + dt.timedelta(hours=it.commits * 2 + r.uniform(0.2, 3))
        it.author = weighted(r, PEOPLE)
        age = (END - it.created).days
        x = r.random()
        if age < 10 and x < 0.7 or x < 0.2 and age < 120:
            it.state, it.end = "open", None
        elif x < 0.32:
            it.state, it.end = "closed", later(it.created, r, 6, 1.2)
        else:
            it.state, it.end = "merged", later(it.created, r, 1.5, 1.2)
        if it.end and it.end >= END - dt.timedelta(minutes=10):
            it.state, it.end = "open", None
        it.keep_branch = it.state != "merged" or r.random() < 0.3
        it.body = md_body(r, "pr")
        it.labels = sorted({zipf_pick(r, [lb for lb, _ in LABELS]) for _ in range(weighted(r, [(0, 40), (1, 40), (2, 20)]))})
        it.assignee = it.author if r.random() < 0.5 else None
        it.milestone = None
        reviewers = [u for u, _ in PEOPLE if u != it.author]
        it.reviews = []
        if it.state == "merged" and r.random() < 0.75 or it.state == "open" and r.random() < 0.3:
            if r.random() < 0.25:
                it.reviews.append((r.choice(reviewers), "REQUEST_CHANGES", sentence(r)))
            it.reviews.append((r.choice(reviewers), "APPROVED", r.choice(["LGTM", "Looks good.", "Ship it", sentence(r)])))
        nc = 0 if r.random() < 0.4 else min(int(r.lognormvariate(0.5, 0.8)), 40)
        it.comments = [(weighted(r, PEOPLE), comment_md(r)) for _ in range(nc)]
        return it

    def issue(self, r, j, n):
        it = Item()
        it.kind = "issue"
        x = r.random()
        if x < 0.45:
            it.title = f"{r.choice(self.comps).capitalize()} {r.choice(PROBLEMS)}"
            labels = ["kind/bug"]
        elif x < 0.8:
            it.title = summary(r)
            labels = ["kind/feature"]
        else:
            it.title = f"{r.choice(VERBS)} {r.choice(OBJECTS)}"
            labels = [r.choice(["kind/chore", "kind/docs"])]
        labels += [lb for lb in [zipf_pick(r, [lb for lb, _ in LABELS[4:]])] if r.random() < 0.6]
        it.labels = sorted(set(labels))
        it.created = self.span(j, n, r)
        it.author = weighted(r, PEOPLE)
        age = (END - it.created).days
        if r.random() < min(0.15 + age / 500, 0.85):
            it.state, it.end = "closed", later(it.created, r, 12, 1.3)
        else:
            it.state, it.end = "open", None
        it.assignee = weighted(r, PEOPLE) if r.random() < 0.55 else None
        it.milestone = next((m for m, d in self.milestones if d >= it.created.date()), None) if r.random() < 0.4 else None
        it.body = md_body(r, "issue")
        nc = 0 if r.random() < 0.35 else min(int(r.lognormvariate(0.3, 0.9)), 50)
        it.comments = [(weighted(r, PEOPLE), comment_md(r)) for _ in range(nc)]
        it.branch, it.commits, it.reviews, it.keep_branch, it.fork_at = None, 0, [], False, None
        return it


def md_body(r, kind):
    if r.random() < 0.12:
        return ""
    parts = []
    if kind == "pr":
        parts.append("## What\n" + paragraph(r, 1, 3))
        parts.append("## Why\n" + paragraph(r, 1, 3))
        if r.random() < 0.5:
            parts.append("## Testing\n" + "\n".join(f"- [{'x' if r.random() < 0.7 else ' '}] {sentence(r, 3, 8)}"
                                                    for _ in range(r.randint(2, 5))))
    else:
        parts.append(paragraph(r, 1, 4))
    for _ in range(min(int(r.lognormvariate(0.3, 0.9)), 20)):
        x = r.random()
        if x < 0.2:
            lang = r.choice(["go", "rust", "sql", "bash", "yaml", "json"])
            parts.append(f"```{lang}\n" + "\n".join(sentence(r, 2, 7).lower() for _ in range(r.randint(2, 20))) + "\n```")
        elif x < 0.35:
            parts.append("\n".join(f"- {sentence(r, 3, 10)}" for _ in range(r.randint(2, 6))))
        elif x < 0.42:
            parts.append("| case | before | after |\n|---|---|---|\n" + "\n".join(
                f"| {r.choice(COMPONENTS)} | {r.randint(1, 999)} ms | {r.randint(1, 999)} ms |" for _ in range(r.randint(2, 6))))
        else:
            parts.append(paragraph(r))
    if r.random() < 0.2:
        parts.append(f"Related to #{r.randint(1, 40)}")
    return "\n\n".join(parts)


def comment_md(r):
    x = r.random()
    if x < 0.2:
        return r.choice(["LGTM", "+1", "Thanks!", "Fixed in the latest push.", "Can we get a test for this?",
                         "Merging once CI is green.", "Duplicate of an older issue, closing soon.", "Done."])
    if x < 0.3:
        return f"@{weighted(r, PEOPLE)} {sentence(r)}"
    if x < 0.38:
        return sentence(r) + "\n\n```\n" + "\n".join(sentence(r, 2, 6).lower() for _ in range(r.randint(2, 10))) + "\n```"
    return paragraph(r, 1, 4)


def plan(total):
    repos = []
    names = [(org, name) for org, _, names in ORGS for name in names]
    n = max(3, min(len(names), round(total / 172)))
    # Interleave the organizations, so a smaller estate still has all three.
    order = sorted(range(len(names)), key=lambda k: (k % 20, k // 20))[:n]
    weights = [rng("gitea", "weight", names[k][1]).lognormvariate(0, 0.9) for k in order]
    s = sum(weights)
    for idx, (k, w) in enumerate(zip(order, weights)):
        org, name = names[k]
        repos.append(Repo(org, name, idx, w / s, total))
    return repos


def describe(total):
    repos = plan(total)
    prs = sum(1 for r in repos for it in r.items if it.kind == "pr")
    issues = sum(1 for r in repos for it in r.items if it.kind == "issue")
    merged = sum(1 for r in repos for it in r.items if it.kind == "pr" and it.state == "merged")
    branches = sum(len(r.plain_branches) + sum(1 for it in r.items if it.kind == "pr" and it.keep_branch) + 1 for r in repos)
    commits = sum(r.main_commits + sum(it.commits for it in r.items if it.kind == "pr") for r in repos)
    mirrored = sum(min(100, r.main_commits + sum(it.commits for it in r.items if it.kind == "pr")) for r in repos)
    comments = sum(len(it.comments) for r in repos for it in r.items)
    print(f"repos: {len(repos)} in {len(ORGS)} orgs")
    print(f"pull requests: {prs} ({merged} merged), max per repo {max(sum(1 for it in r.items if it.kind == 'pr') for r in repos)}")
    print(f"issues: {issues}, max per repo {max(sum(1 for it in r.items if it.kind == 'issue') for r in repos)}")
    print(f"branches left: {branches}")
    print(f"commits: {commits} (mirrored at the default 100 a repository: {mirrored})")
    print(f"comments: {comments}, reviews: {sum(len(it.reviews) for r in repos for it in r.items)}")
    print(f"items knobas mirrors: ~{len(repos) + prs + issues + branches + mirrored}")


# -- git history ---------------------------------------------------------------------


class History:
    """The repository's commits as a fast-import stream; marks name commits."""

    def __init__(self, repo: Repo, emails):
        self.repo, self.emails = repo, emails
        self.r = rng("gitea", repo.org, repo.name, "history")
        self.mark = 0
        self.out = []
        ext, files = LANGS[repo.lang]
        self.files = {}
        self.templates = files
        self.ext = ext

    def path(self, comp=None):
        comp = (comp or self.r.choice(self.repo.comps)).split()[0].replace("-", "_")
        t = self.r.choice(self.templates)
        return t.replace("{c}", comp.lower()).replace("{C}", comp.capitalize())

    def edit(self, files, n_files):
        changed = {}
        for _ in range(n_files):
            p = self.path() if self.r.random() < 0.85 else f"docs/{self.r.choice(self.repo.comps).replace(' ', '-')}.md"
            lines = list(files.get(p, []))
            x = self.r.random()
            if not lines or x < 0.55:
                at = self.r.randint(0, len(lines))
                lines[at:at] = [self.code_line() for _ in range(self.r.randint(1, 12))]
            elif x < 0.85:
                at = self.r.randrange(len(lines))
                lines[at] = self.code_line()
            else:
                at = self.r.randrange(len(lines))
                del lines[at:at + self.r.randint(1, 5)]
            if len(lines) > 400:
                lines = lines[-400:]
            files[p] = lines
            changed[p] = lines
        return changed

    def code_line(self):
        r = self.r
        w = sentence(r, 2, 6).lower().rstrip(".!?").replace(" ", "_")[:40]
        c = {"go": f"\t{w} := {r.randint(0, 999)} // {sentence(r, 2, 6).lower()}",
             "rs": f"    let {w} = {r.randint(0, 999)}; // {sentence(r, 2, 6).lower()}",
             "kt": f"    val {w} = {r.randint(0, 999)} // {sentence(r, 2, 6).lower()}",
             "py": f"    {w} = {r.randint(0, 999)}  # {sentence(r, 2, 6).lower()}",
             "ts": f"  const {w} = {r.randint(0, 999)}; // {sentence(r, 2, 6).lower()}",
             "java": f"        int {w} = {r.randint(0, 999)}; // {sentence(r, 2, 6).lower()}",
             "tf": f'  {w} = "{r.choice(COMPONENTS).replace(" ", "-")}"'}
        return c[self.ext]

    def commit(self, ref, t, who, msg, parent=None, merge=None, changed=None):
        self.mark += 1
        name = who.replace(".", " ").title() if who != "knobas" else "knobas"
        ts = int(t.timestamp())
        data = msg.encode()
        o = [f"commit {ref}\nmark :{self.mark}\n",
             f"author {name} <{self.emails[who]}> {ts} +0000\n",
             f"committer {name} <{self.emails[who]}> {ts} +0000\n",
             f"data {len(data)}\n"]
        self.out.append("".join(o).encode() + data + b"\n")
        if parent:
            self.out.append(f"from :{parent}\n".encode())
        if merge:
            self.out.append(f"merge :{merge}\n".encode())
        for p, lines in (changed or {}).items():
            body = ("\n".join(lines) + "\n").encode()
            self.out.append(f"M 100644 inline {p}\ndata {len(body)}\n".encode() + body + b"\n")
        self.out.append(b"\n")
        return self.mark

    def message(self, kind="feat"):
        r = self.r
        comp = r.choice(self.repo.comps).split()[0]
        m = f"{kind}({comp}): {r.choice(VERBS).lower()} {r.choice(OBJECTS)}"
        if r.random() < 0.4:
            m += "\n\n" + paragraph(r, 1, 3)
        return m

    def build(self):
        """The stream, and per pull request the mark of its head."""
        repo, r = self.repo, self.r
        main = {}
        readme = {"README.md": [f"# {repo.name}", "", repo.description, "", paragraph(r, 2, 4)]}
        main.update(readme)
        root = self.commit("refs/heads/main", repo.created, weighted(r, PEOPLE), "Initial commit", changed=dict(main))
        events = []
        n = repo.main_commits
        for j in range(n - 1):
            t = repo.created + (END - repo.created) * ((j + 1 + r.random() * 0.8) / n) ** 0.85
            events.append((t, 0, "commit", None))
        prs = [it for it in repo.items if it.kind == "pr"]
        for it in prs:
            events.append((it.fork_at, 1, "fork", it))
            if it.state == "merged":
                events.append((it.end, 2, "merge", it))
        for k, b in enumerate(repo.plain_branches):
            events.append((repo.created + (END - repo.created) * rng("gitea", repo.name, b).random(), 1, "plain", b))
        events.sort(key=lambda e: (e[0], e[1]))
        head = root
        heads = {}       # pr n -> (mark, files it changed)
        refs = {}        # branch -> mark
        for t, _, kind, x in events:
            if kind == "commit":
                ch = self.edit(main, r.randint(1, 3))
                head = self.commit("refs/heads/main", t, weighted(r, PEOPLE), self.message(r.choice(["feat", "fix", "chore", "refactor", "test"])),
                                   parent=head, changed=ch)
            elif kind == "fork":
                files = {p: list(v) for p, v in main.items()}
                mark, touched = head, {}
                for c in range(x.commits):
                    ch = self.edit(files, r.randint(1, 3))
                    touched.update(ch)
                    tc = min(x.fork_at + dt.timedelta(hours=2 * c + r.random()), x.created - dt.timedelta(minutes=5))
                    mark = self.commit(f"refs/heads/{x.branch}", tc, x.author, self.message(x.branch.split("/")[0].replace("feature", "feat")),
                                       parent=mark, changed=ch)
                heads[x.n] = (mark, touched)
                refs[x.branch] = mark
            elif kind == "merge":
                mark, touched = heads[x.n]
                for p, v in touched.items():
                    main[p] = list(v)
                head = self.commit("refs/heads/main", t, weighted(rng("gitea", repo.name, "merger", x.n), PEOPLE),
                                   f"Merge pull request '{x.title}' (#{x.n}) from {x.branch} into main",
                                   parent=head, merge=mark, changed={p: main[p] for p in touched})
            else:
                ch = self.edit({p: list(v) for p, v in main.items()}, r.randint(1, 2))
                refs[x] = self.commit(f"refs/heads/{x}", t, weighted(r, PEOPLE), self.message("chore"), parent=head, changed=ch)
        return b"".join(self.out), root, refs


# -- the seed --------------------------------------------------------------------------


class Forge:
    def __init__(self, state, total, workers, product):
        g = state.get("gitea") or die(product, f"no gitea block in the state file: just live-up {product} first")
        self.product = product
        self.url = g["url"].rstrip("/")
        self.container = "knobas-forgejo" if product == "forgejo" else "knobas-gitea"
        self.h = Http(self.url, user="knobas", password="knobas-dev")
        self.total, self.workers = total, workers
        self.made = {"orgs": 0, "repos": 0, "prs": 0, "issues": 0, "comments": 0, "reviews": 0}
        self.skipped_reviews = 0

    def sudo(self, who):
        return {"Sudo": who} if who != "knobas" else {}

    def marker(self, repo):
        try:
            v = self.h.get(f"/api/v1/repos/{repo.org}/{repo.name}/actions/variables/{VAR}")
            return json.loads(v["data"])
        except HttpError as e:
            if e.status == 404:
                return {}
            raise

    def mark(self, repo, phase):
        m = self.marker(repo)
        exists = bool(m)
        m[phase] = True
        body = {"value": json.dumps(m, sort_keys=True)}
        path = f"/api/v1/repos/{repo.org}/{repo.name}/actions/variables/{VAR}"
        if exists:
            self.h.put(path, body)
        else:
            self.h.post(path, body)

    def run(self):
        self.emails = users_map(self.h)
        for u, _ in PEOPLE:
            if u != "knobas":
                self.h.get("/api/v1/user", headers=self.sudo(u))
        self.repos = plan(self.total)
        for org, full, _ in ORGS:
            if any(r.org == org for r in self.repos):
                self.org(org, full)
        self.tmp = tempfile.mkdtemp(prefix="knobas-scale-git-")
        try:
            parallel(self.product, "repositories", self.repos, self.repo, self.workers, 5)
        finally:
            shutil.rmtree(self.tmp, ignore_errors=True)
        self.dates()
        self.report()

    def org(self, org, full):
        try:
            self.h.get(f"/api/v1/orgs/{org}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/api/v1/orgs", {"username": org, "full_name": full, "visibility": "public",
                                         "description": "Scale test data made by testenv/seed-scale.sh. Not fixture content."})
            self.made["orgs"] += 1
            say(self.product, f"created org {org}")
        teams = self.h.get(f"/api/v1/orgs/{org}/teams?limit=50")
        team = next((t for t in teams if t["name"] == "developers"), None)
        if team is None:
            team = self.h.post(f"/api/v1/orgs/{org}/teams", {
                "name": "developers", "permission": "write", "includes_all_repositories": True,
                "units": ["repo.code", "repo.issues", "repo.pulls", "repo.releases", "repo.wiki"]})
        members = {m["login"] for m in self.h.get(f"/api/v1/teams/{team['id']}/members?limit=50")}
        for u, _ in PEOPLE:
            if u not in members and u != "knobas":
                self.h.put(f"/api/v1/teams/{team['id']}/members/{u}")

    def repo(self, repo: Repo):
        base = f"/api/v1/repos/{repo.org}/{repo.name}"
        try:
            self.h.get(base)
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post(f"/api/v1/orgs/{repo.org}/repos", {"name": repo.name, "description": repo.description,
                                                           "default_branch": "main", "auto_init": False})
            self.made["repos"] += 1
        m = self.marker(repo)
        if m.get("done"):
            return
        if not m.get("setup"):
            self.h.request("PATCH", base, {"has_pull_requests": True, "autodetect_manual_merge": True,
                                           "default_merge_style": "merge"})
            have = {lb["name"] for lb in self.h.get(f"{base}/labels?limit=50")}
            for name, color in LABELS:
                if name not in have:
                    self.h.post(f"{base}/labels", {"name": name, "color": f"#{color}"})
            have = {ms["title"] for ms in self.h.get(f"{base}/milestones?state=all&limit=50")}
            for title, due in repo.milestones:
                if title not in have:
                    ms = self.h.post(f"{base}/milestones", {"title": title, "due_on": f"{due}T17:00:00Z"})
                    if due < END.date():
                        self.h.request("PATCH", f"{base}/milestones/{ms['id']}", {"state": "closed"})
            self.mark(repo, "setup")
        hist = History(repo, self.emails)
        stream, root, refs = hist.build()
        gitdir = os.path.join(self.tmp, f"{repo.org}-{repo.name}.git")
        if not os.path.isdir(gitdir):
            git(["init", "--quiet", "--bare", gitdir])
            p = subprocess.run(["git", "--git-dir", gitdir, "fast-import", "--quiet"], input=stream, capture_output=True)
            if p.returncode:
                die(self.product, f"{repo.name}: fast-import: {p.stderr.decode()[-800:]}")
        remote = f"{self.url}/{repo.org}/{repo.name}.git"
        auth = ["-c", f"http.extraHeader=Authorization: {basic('knobas', 'knobas-dev')}"]
        if not m.get("pushed"):
            # main at its first commit, every branch at its head: the pull
            # requests open against that, and the merges come with main later.
            first = git(["--git-dir", gitdir, "rev-list", "--max-parents=0", "main"], out=True).split()[0]
            specs = [f"{first}:refs/heads/main"] + [f"refs/heads/{b}:refs/heads/{b}" for b in refs]
            for c in chunks(specs, 200):
                git(auth + ["--git-dir", gitdir, "push", "--quiet", "--force", remote] + c)
            self.mark(repo, "pushed")
        if not m.get("items"):
            self.wait_branches(repo, base, refs)
            self.items(repo, base)
            self.mark(repo, "items")
        if not m.get("reviews"):
            # Before the merge and the closes: a forge takes no review on a
            # merged or closed pull request.
            self.reviews(repo, base)
            self.mark(repo, "reviews")
        if not m.get("merged"):
            git(auth + ["--git-dir", gitdir, "push", "--quiet", "--force", remote, "refs/heads/main:refs/heads/main"])
            self.wait_merged(repo, base)
            self.mark(repo, "merged")
        if not m.get("states"):
            self.states(repo, base)
            self.mark(repo, "states")
        if not m.get("comments"):
            self.comments(repo, base)
            self.mark(repo, "comments")
        if not m.get("branches"):
            have = {b["name"] for b in self.paged(f"{base}/branches")}
            for it in repo.items:
                if it.kind == "pr" and not it.keep_branch and it.branch in have:
                    self.h.request("DELETE", f"{base}/branches/{q(it.branch)}", ok=(204, 404))
            self.mark(repo, "branches")
        self.mark(repo, "done")

    def paged(self, path):
        out, page = [], 1
        sep = "&" if "?" in path else "?"
        while True:
            got = self.h.get(f"{path}{sep}limit=50&page={page}")
            if not got:
                return out
            out += got
            page += 1

    def items(self, repo, base):
        # Issues and pull requests share one counter: made in date order,
        # one after another, so #n is the plan's n.
        top = 0
        got = self.h.get(f"{base}/issues?state=all&limit=1")
        if got:
            top = max(x["number"] for x in got)
        labels = {lb["name"]: lb["id"] for lb in self.h.get(f"{base}/labels?limit=50")}
        miles = {ms["title"]: ms["id"] for ms in self.h.get(f"{base}/milestones?state=all&limit=50")}
        for it in repo.items:
            if it.n <= top:
                continue
            body = {"title": it.title, "body": it.body, "labels": [labels[lb] for lb in it.labels]}
            if it.assignee:
                body["assignees"] = [it.assignee]
            if it.milestone:
                body["milestone"] = miles[it.milestone]
            if it.kind == "pr":
                body.update({"head": it.branch, "base": "main"})
                res = self.h.post(f"{base}/pulls", body, headers=self.sudo(it.author))
                self.made["prs"] += 1
            else:
                res = self.h.post(f"{base}/issues", body, headers=self.sudo(it.author))
                self.made["issues"] += 1
            if res["number"] != it.n:
                die(self.product, f"{repo.name}: made #{res['number']} for the plan's #{it.n}: something else opened an "
                                  f"issue here; delete the repository and seed again")

    def wait_branches(self, repo, base, refs):
        # A push lands in the forge's branch table through a queue, after the
        # push has returned; a pull request opened on a branch not in it yet
        # is a 404.
        want = set(refs)
        deadline = time.time() + 600
        while True:
            have = {b["name"] for b in self.paged(f"{base}/branches")}
            if want <= have:
                return
            if time.time() > deadline:
                die(self.product, f"{repo.name}: {len(want - have)} pushed branches not listed after 10 min, "
                                  f"e.g. {sorted(want - have)[:3]}")
            time.sleep(3)

    def wait_merged(self, repo, base):
        want = {it.n for it in repo.items if it.kind == "pr" and it.state == "merged"}
        deadline = time.time() + 900
        while True:
            merged = {p["number"] for p in self.paged(f"{base}/pulls?state=closed") if p.get("merged")}
            if want <= merged:
                return
            if time.time() > deadline:
                die(self.product, f"{repo.name}: {len(want - merged)} pull requests not detected as merged after 15 min "
                                  f"(autodetect_manual_merge), e.g. #{sorted(want - merged)[:5]}")
            time.sleep(5)

    def states(self, repo, base):
        for it in repo.items:
            if it.state == "closed":
                path = f"{base}/{'pulls' if it.kind == 'pr' else 'issues'}/{it.n}"
                cur = self.h.get(path)
                if cur["state"] != "closed":
                    self.h.request("PATCH", path, {"state": "closed"}, headers=self.sudo(it.assignee or it.author))

    def reviews(self, repo, base):
        for it in repo.items:
            if it.kind != "pr" or not it.reviews:
                continue
            have = self.h.get(f"{base}/pulls/{it.n}/reviews")
            for who, event, text in it.reviews[len([x for x in have if x["state"] in ("APPROVED", "REQUEST_CHANGES")]):]:
                try:
                    self.h.post(f"{base}/pulls/{it.n}/reviews", {"event": event, "body": text}, headers=self.sudo(who))
                except HttpError as e:
                    # A repository an earlier version of this seed merged
                    # before its reviews: the forge refuses them now.
                    if e.status == 422 and "closed or merged" in e.body:
                        self.skipped_reviews += 1
                        break
                    raise
                self.made["reviews"] += 1

    def comments(self, repo, base):
        have = {}
        for x in self.paged(f"{base}/issues?state=all&type=issues") + self.paged(f"{base}/issues?state=all&type=pulls"):
            have[x["number"]] = x["comments"]
        for it in repo.items:
            already = have.get(it.n, 0)
            for who, text in it.comments[already:]:
                self.h.post(f"{base}/issues/{it.n}/comments", {"body": text}, headers=self.sudo(who))
                self.made["comments"] += 1

    # -- dates ---------------------------------------------------------------------------
    def dates(self):
        say(self.product, f"rewriting created, updated and closed times in {self.container}'s database")
        rows = []
        for repo in self.repos:
            for it in repo.items:
                end = it.end or later(it.created, rng("gitea", repo.name, "upd", it.n), 3, 1.2)
                # The merger is the merge commit's author (History.build).
                m = (f"'{weighted(rng('gitea', repo.name, 'merger', it.n), PEOPLE)}'"
                     if it.kind == "pr" and it.state == "merged" else "NULL")
                rows.append(f"('{repo.org}','{repo.name}',{it.n},{int(it.created.timestamp())},{int(end.timestamp())},"
                            f"{int(it.end.timestamp()) if it.end else 'NULL'},{m})")
        repos = [f"('{r.org}','{r.name}',{int(r.created.timestamp())})" for r in self.repos]
        sql = ["BEGIN;",
               "CREATE TEMP TABLE sd (org TEXT, repo TEXT, n INTEGER, c INTEGER, u INTEGER, x INTEGER, m TEXT);",
               "CREATE TEMP TABLE sr (org TEXT, repo TEXT, c INTEGER);"]
        for c in chunks(rows, 500):
            sql.append("INSERT INTO sd VALUES " + ",".join(c) + ";")
        sql.append("INSERT INTO sr VALUES " + ",".join(repos) + ";")
        sql.append("""
CREATE TEMP TABLE si AS
  SELECT i.id AS id, sd.c AS c, sd.u AS u, sd.x AS x, sd.m AS m
    FROM issue i JOIN repository r ON r.id = i.repo_id
    JOIN sd ON sd.org = r.owner_name AND sd.repo = r.lower_name AND sd.n = i."index";
UPDATE issue SET created_unix = (SELECT c FROM si WHERE si.id = issue.id),
                 updated_unix = (SELECT u FROM si WHERE si.id = issue.id),
                 closed_unix = COALESCE((SELECT x FROM si WHERE si.id = issue.id), 0)
 WHERE id IN (SELECT id FROM si);
CREATE TEMP TABLE sc AS
  SELECT c.id AS id, c.type AS type, si.c AS c, si.u AS u, si.x AS x,
         row_number() OVER (PARTITION BY c.issue_id ORDER BY c.id) AS rn,
         count(*) OVER (PARTITION BY c.issue_id) AS cnt
    FROM comment c JOIN si ON si.id = c.issue_id;
UPDATE comment SET created_unix = (SELECT CASE WHEN sc.type IN (0, 21, 22) THEN sc.c + (sc.u - sc.c) * sc.rn / (sc.cnt + 1)
                                               ELSE COALESCE(sc.x, sc.u) END FROM sc WHERE sc.id = comment.id)
 WHERE id IN (SELECT id FROM sc);
UPDATE comment SET updated_unix = created_unix WHERE id IN (SELECT id FROM sc);
UPDATE review SET created_unix = (SELECT si.c + (si.u - si.c) * 2 / 3 FROM si WHERE si.id = review.issue_id),
                  updated_unix = (SELECT si.c + (si.u - si.c) * 2 / 3 FROM si WHERE si.id = review.issue_id)
 WHERE issue_id IN (SELECT id FROM si);
-- The manual-merge detection stamps the detection's own time and no merger.
UPDATE pull_request SET merged_unix = (SELECT x FROM si WHERE si.id = pull_request.issue_id),
                        merger_id = COALESCE((SELECT u.id FROM "user" u JOIN si ON u.lower_name = si.m
                                               WHERE si.id = pull_request.issue_id), merger_id)
 WHERE has_merged = 1 AND issue_id IN (SELECT id FROM si WHERE m IS NOT NULL);
UPDATE repository SET created_unix = (SELECT c FROM sr WHERE sr.org = repository.owner_name AND sr.repo = repository.lower_name)
 WHERE EXISTS (SELECT 1 FROM sr WHERE sr.org = repository.owner_name AND sr.repo = repository.lower_name);
UPDATE milestone SET created_unix = (SELECT c FROM sr JOIN repository r ON sr.org = r.owner_name AND sr.repo = r.lower_name WHERE r.id = milestone.repo_id)
 WHERE repo_id IN (SELECT r.id FROM sr JOIN repository r ON sr.org = r.owner_name AND sr.repo = r.lower_name);
COMMIT;
""")
        p = subprocess.run(["docker", "exec", "-i", "-u", "git", self.container, "sqlite3", "-bail", "/data/gitea/gitea.db"],
                           input="\n".join(sql).encode(), capture_output=True)
        if p.returncode:
            die(self.product, f"sqlite3 in {self.container}: {p.stderr.decode()[-1500:]}")

    def report(self):
        prs = issues = 0
        for org, _, _ in ORGS:
            for r in self.paged(f"/api/v1/orgs/{org}/repos"):
                prs += r.get("open_pr_counter", 0)
                issues += r.get("open_issues_count", 0)
        made = ", ".join(f"{v} {k}" for k, v in self.made.items())
        say(self.product, f"done: {len(self.repos)} repositories ({issues} open issues, {prs} open pull requests "
                          f"across them); this run made {made}")


def git(args, out=False):
    p = subprocess.run(["git"] + args, capture_output=True)
    if p.returncode:
        raise RuntimeError(f"git {' '.join(a for a in args if not a.startswith('http.extraHeader'))}: {p.stderr.decode()[-800:]}")
    return p.stdout.decode().strip() if out else None


def main(state, count, workers, product):
    Forge(state, count, workers, product).run()
