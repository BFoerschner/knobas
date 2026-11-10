"""Jira at Björn's scale: six `SCL*` projects beside PAY, OPS and NARROW.

Phases, each idempotent and each recorded in a project property
(`knobas-scale-seed`) once done for a count, so a rerun at the same count
reads one property per project and writes nothing:

  setup       the project (the fixture's template), Story/Bug/Sub-task in its
              scheme, components, monthly versions, a Scrum board
  issues      bulk create, 50 a request, keys asserted (project from empty)
  status      transitions to each issue's planned status
  comments    authored by the fixture's people, over basic auth
  worklogs    with their `started` dates
  links       Blocks, Relates, Duplicate, Cloners, across the six projects
  sprints     two-week sprints over the two years: closed with their done
              issues, one active, two future
  dates       created, updated and resolved rewritten in Jira's database
              (no REST call sets them), then a background reindex
"""

from __future__ import annotations

import datetime as dt
import json
import math
import subprocess

from common import (END, START, Http, HttpError, chunks, die, iso, later, length_class,
                    paragraph, parallel, q, rng, say, sentence, summary, weighted, when,
                    zipf_pick, COMPONENTS)

P = "jira"
PROPERTY = "knobas-scale-seed"
TEMPLATE = "com.pyxis.greenhopper.jira:basic-software-development-template"

# key, name, share of the issues
PROJECTS = [
    ("SCLPAY", "Scale test: Payments Core", 0.28),
    ("SCLOPS", "Scale test: Operations", 0.20),
    ("SCLWEB", "Scale test: Web Storefront", 0.18),
    ("SCLDATA", "Scale test: Data Platform", 0.14),
    ("SCLMOB", "Scale test: Mobile Apps", 0.12),
    ("SCLSEC", "Scale test: Security", 0.08),
]

# The fixture's people (seed-atlassian-content.sh made them, password
# tidewater-dev) and the admin. A ten-user licence: the scale seed adds none.
PEOPLE = [
    ("mara.lindqvist", 26), ("jonas.becker", 22), ("priya.nair", 14),
    ("tomasz.wojcik", 18), ("lena.hoffmann", 8), ("knobas", 12),
]
PEOPLE_PASS = "tidewater-dev"

LABELS = [
    "backend", "frontend", "tech-debt", "customer-reported", "regression", "performance",
    "security", "flaky-test", "on-call", "api", "database", "infra", "ux", "docs",
    "needs-design", "quick-win", "migration", "hotfix", "observability", "compliance",
    "accessibility", "i18n", "mobile", "billing", "partner", "data-quality", "cost",
    "q1-goal", "q2-goal", "q3-goal", "q4-goal", "spike", "blocked-external", "gdpr",
]
PRIORITIES = [("Highest", 3), ("High", 17), ("Medium", 55), ("Low", 20), ("Lowest", 5)]
LINK_TYPES = [("Relates", 45), ("Blocks", 35), ("Duplicate", 10), ("Cloners", 10)]
SPRINT_DAYS = 14


def counts(total: int):
    """Issues per project: the shares of `total`, the remainder on the first."""
    n = [int(total * share) for _, _, share in PROJECTS]
    n[0] += total - sum(n)
    return {k: c for (k, _, _), c in zip(PROJECTS, n)}


# -- the plan: every issue, from its own generator -----------------------------


def versions_of(pkey):
    """Monthly releases over the two years: (name, release date)."""
    out = []
    y, m = START.year, START.month
    while True:
        d = dt.date(y, m, 28)
        if d > END.date() + dt.timedelta(days=90):
            break
        out.append((f"{pkey[3:].lower()}-{y}.{m:02d}", d))
        m += 1
        if m == 13:
            y, m = y + 1, 1
    return out


def component_names(pkey):
    r = rng(P, pkey, "components")
    return sorted(r.sample(COMPONENTS, 7))


def wiki(r, paras: int, kind: str) -> str:
    parts = []
    if kind == "Bug":
        parts.append("h3. Steps to reproduce\n" + "\n".join(
            f"# {sentence(r, 4, 10)}" for _ in range(r.randint(2, 6))))
        parts.append("h3. Expected\n" + sentence(r))
        parts.append("h3. Actual\n" + sentence(r))
    for _ in range(paras):
        x = r.random()
        if x < 0.10:
            parts.append(f"h3. {summary(r)}")
        elif x < 0.22:
            parts.append("\n".join(f"* {sentence(r, 3, 12)}" for _ in range(r.randint(2, 7))))
        elif x < 0.29:
            lang = r.choice(["java", "sql", "bash", "python", "json", "rust"])
            body = "\n".join(f"  {sentence(r, 2, 8).lower()}" for _ in range(r.randint(3, 25)))
            parts.append(f"{{code:{lang}}}\n{body}\n{{code}}")
        elif x < 0.33:
            rows = ["||Metric||Before||After||"] + [
                f"|{r.choice(COMPONENTS)}|{r.randint(1, 900)} ms|{r.randint(1, 900)} ms|"
                for _ in range(r.randint(2, 8))]
            parts.append("\n".join(rows))
        elif x < 0.36:
            frames = "\n".join(
                f"\tat com.tidewater.{r.choice(['pay', 'ledger', 'ops'])}.{r.choice(['Retry', 'Batch', 'Gateway'])}"
                f".{r.choice(['run', 'apply', 'flush'])}({r.choice(['Job', 'Worker', 'Client'])}.java:{r.randint(10, 999)})"
                for _ in range(r.randint(4, 30)))
            parts.append(f"{{noformat}}\njava.lang.IllegalStateException: {sentence(r)}\n{frames}\n{{noformat}}")
        elif x < 0.40:
            parts.append(f"{{panel:title=Note}}{paragraph(r, 1, 3)}{{panel}}")
        else:
            parts.append(paragraph(r))
    return "\n\n".join(parts)


class Issue:
    __slots__ = ("pkey", "i", "type", "summary", "created", "updated", "resolved", "status",
                 "priority", "assignee", "reporter", "labels", "component", "version",
                 "description", "due", "estimate", "epic", "parent", "comments", "worklogs",
                 "links", "sprint", "popularity")

    @property
    def key(self):
        return f"{self.pkey}-{self.i}"


def plan(pkey: str, n: int, all_counts):
    """The project's issues, 1-based by key number. Epics and parents always
    come before what refers to them, so a bulk batch never names one of its own."""
    vers = versions_of(pkey)
    comps = component_names(pkey)
    issues = [None]
    epics = []          # (i, popularity)
    parents = []        # indices of stories, tasks and bugs
    for i in range(1, n + 1):
        r = rng(P, pkey, i)
        it = Issue()
        it.pkey, it.i = pkey, i
        x = r.random()
        if i <= 2 or x < 0.022:
            it.type = "Epic"
        elif x < 0.085 and len(parents) > 20:
            it.type = "Sub-task"
        else:
            it.type = weighted(r, [("Story", 45), ("Task", 33), ("Bug", 22)])
        it.summary = summary(r, it.type)
        it.created = when(i - 1, n, r)
        age = (END - it.created).total_seconds() / (END - START).total_seconds()
        p_done = 0.22 + 0.7 * age
        if it.type == "Epic":
            p_done = 0.1 + 0.6 * age
        if r.random() < p_done:
            it.status = "Done"
            it.resolved = later(it.created, r, 9 if it.type != "Epic" else 60, 1.1)
            it.updated = later(it.resolved, r, 0.3, 1.5)
        else:
            it.status = weighted(r, [("To Do", 50), ("In Progress", 30), ("In Review", 20)])
            it.resolved = None
            it.updated = later(it.created, r, 4, 1.4)
        it.priority = weighted(r, PRIORITIES)
        it.assignee = None if (it.status == "To Do" and r.random() < 0.45) or r.random() < 0.08 \
            else weighted(r, PEOPLE)
        it.reporter = weighted(r, PEOPLE)
        it.labels = sorted({zipf_pick(r, LABELS, 0.9) for _ in range(weighted(r, [(0, 35), (1, 35), (2, 20), (3, 10)]))})
        it.component = r.choice(comps) if r.random() < 0.7 else None
        it.version = None
        if it.resolved is not None and r.random() < 0.8:
            it.version = next((v for v, d in vers if d >= it.resolved.date()), None)
        elif it.resolved is None and r.random() < 0.25:
            it.version = next((v for v, d in vers if d >= END.date()), None)
        paras = length_class(r)
        it.description = None if r.random() < 0.1 else wiki(r, paras, it.type)
        it.due = (later(it.created, r, 20, 0.6).date().isoformat() if r.random() < 0.12 else None)
        it.estimate = f"{r.choice([1, 2, 3, 4, 6, 8, 12, 16, 24, 40])}h" if r.random() < 0.35 else None
        it.epic = None
        it.parent = None
        if it.type == "Sub-task":
            recent = parents[-300:]
            it.parent = r.choice(recent)
        elif it.type != "Epic" and epics and r.random() < 0.65:
            cands = [e for e in epics if issues[e[0]].created > it.created - dt.timedelta(days=420)] or epics
            it.epic = r.choices([e[0] for e in cands], [e[1] for e in cands])[0]
        it.popularity = r.lognormvariate(0, 1.6) if it.type == "Epic" else 0
        # comments: a third none, a long tail
        nc = 0 if r.random() < 0.35 else min(int(r.lognormvariate(0.3, 0.9)), 60)
        it.comments = [comment_text(r) for _ in range(nc)]
        it.worklogs = []
        if it.status in ("Done", "In Progress", "In Review") and it.type != "Epic" and r.random() < 0.12:
            end = it.resolved or it.updated
            for _ in range(r.randint(1, 4)):
                t = it.created + (end - it.created) * r.random()
                it.worklogs.append((t, r.choice([15, 30, 45, 60, 90, 120, 180, 240, 360, 480]) * 60, sentence(r, 3, 9)))
        it.links = []
        if i > 10 and r.random() < 0.18:
            for _ in range(r.randint(1, 3)):
                if r.random() < 0.8:
                    other = (pkey, r.randint(max(1, i - 400), i - 1))
                else:
                    ok, on = r.choice(list(all_counts.items()))
                    other = (ok, r.randint(1, max(1, min(on, int(on * i / n)))))
                it.links.append((weighted(r, LINK_TYPES), other))
        it.sprint = None
        issues.append(it)
        if it.type == "Epic":
            epics.append((i, it.popularity))
        elif it.type != "Sub-task":
            parents.append(i)
    return issues


def comment_text(r) -> str:
    x = r.random()
    if x < 0.15:
        return f"[~{weighted(r, PEOPLE)}] {sentence(r)}"
    if x < 0.22:
        lines = "\n".join(sentence(r, 2, 8).lower() for _ in range(r.randint(2, 12)))
        return f"{sentence(r)}\n{{code}}\n{lines}\n{{code}}"
    if x < 0.3:
        return r.choice(["LGTM", "Done.", "+1", "Merged, deploying tomorrow.", "Can't reproduce on staging.",
                         "Moving to next sprint.", "Duplicate, closing.", "Fixed in the last release."])
    return paragraph(r, 1, 6)


# -- the seed ----------------------------------------------------------------------


class Jira:
    def __init__(self, state, total, workers):
        j = state.get("jira") or die(P, "seed-state.json has no jira block: just live-up jira first")
        self.url = j["url"]
        self.user, self.password = j["user"], j["password"]
        self.h = Http(self.url, user=self.user, password=self.password)
        self.people = {u: Http(self.url, user=u, password=(self.password if u == self.user else PEOPLE_PASS))
                       for u, _ in PEOPLE}
        self.total = total
        self.workers = workers
        self.counts = counts(total)
        self.made = {"projects": 0, "issues": 0, "transitions": 0, "comments": 0, "worklogs": 0,
                     "links": 0, "sprints": 0}

    # project property: {"phases": {"issues": 3360, ...}}
    def marker(self, pkey):
        try:
            return self.h.get(f"/rest/api/2/project/{pkey}/properties/{PROPERTY}")["value"]
        except HttpError as e:
            if e.status == 404:
                return {"phases": {}}
            raise

    def mark(self, pkey, phase, n):
        m = self.marker(pkey)
        m.setdefault("phases", {})[phase] = n
        self.h.put(f"/rest/api/2/project/{pkey}/properties/{PROPERTY}", m)

    def done(self, pkey, phase):
        return self.marker(pkey).get("phases", {}).get(phase) == self.counts[pkey]

    def search(self, jql, fields):
        out, start = [], 0
        while True:
            page = self.h.post("/rest/api/2/search", {"jql": jql, "startAt": start, "maxResults": 1000,
                                                     "fields": fields, "validateQuery": True})
            out += page["issues"]
            start += len(page["issues"])
            if not page["issues"] or start >= page["total"]:
                return out

    def run(self):
        self.h.get("/rest/api/2/myself")
        for u, h in self.people.items():
            h.get("/rest/api/2/myself")
        fields = self.h.get("/rest/api/2/field")
        by_name = {f["name"]: f["id"] for f in fields}
        self.epic_name = by_name["Epic Name"]
        self.epic_link = by_name["Epic Link"]
        self.sprint_field = next(f["id"] for f in fields
                                 if (f.get("schema") or {}).get("custom") == "com.pyxis.greenhopper.jira:gh-sprint")
        self.plans = {}
        for pkey, name, _ in PROJECTS:
            self.plans[pkey] = plan(pkey, self.counts[pkey], self.counts)
        for pkey, name, _ in PROJECTS:
            self.setup(pkey, name)
        # Keys are handed out in request order within a project, so one
        # project's bulk requests go one after another -- and the six
        # projects side by side (one bulk request of 50 takes ~8 s here).
        parallel(P, "projects' issues", [p for p, _, _ in PROJECTS], self.issues, len(PROJECTS), 6)
        for phase in (self.status, self.comments, self.worklogs, self.links, self.sprints):
            for pkey, _, _ in PROJECTS:
                phase(pkey)
        self.dates()
        self.report()

    # -- setup -------------------------------------------------------------------
    def setup(self, pkey, name):
        if self.done(pkey, "setup"):
            return
        try:
            self.h.get(f"/rest/api/2/project/{pkey}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/rest/api/2/project", {
                "key": pkey, "name": name, "projectTypeKey": "software", "projectTemplateKey": TEMPLATE,
                "lead": self.user, "assigneeType": "UNASSIGNED",
                "description": "Scale test data made by testenv/seed-scale.sh. Not fixture content."})
            self.made["projects"] += 1
            say(P, f"created project {pkey} ({name})")
        proj = self.h.get(f"/rest/api/2/project/{pkey}")
        have = {t["name"] for t in proj["issueTypes"]}
        want = {"Epic", "Story", "Task", "Bug", "Sub-task"}
        if not want <= have:
            alltypes = self.h.get("/rest/api/2/issuetype")
            scheme = None
            for s in self.h.get("/rest/api/2/issuetypescheme")["schemes"]:
                assoc = self.h.get(f"/rest/api/2/issuetypescheme/{s['id']}/associations")
                if any(a.get("key") == pkey for a in assoc):
                    scheme = s["id"]
                    break
            if scheme is None:
                die(P, f"no issue type scheme is associated with {pkey}")
            sc = self.h.get(f"/rest/api/2/issuetypescheme/{scheme}?expand=defaultIssueType,issueTypes")
            ids = [t["id"] for t in sc["issueTypes"]] + [t["id"] for t in alltypes
                                                          if t["name"] in want - have and
                                                          (t["name"] != "Sub-task" or t.get("subtask"))]
            body = {"name": sc["name"], "description": sc.get("description", ""), "issueTypeIds": ids}
            if sc.get("defaultIssueType"):
                body["defaultIssueTypeId"] = sc["defaultIssueType"]["id"]
            self.h.put(f"/rest/api/2/issuetypescheme/{scheme}", body)
            say(P, f"{pkey}: added {', '.join(sorted(want - have))} to its issue type scheme")
        comps = {c["name"] for c in self.h.get(f"/rest/api/2/project/{pkey}/components")}
        for c in component_names(pkey):
            if c not in comps:
                self.h.post("/rest/api/2/component", {"name": c, "project": pkey})
        vers = {v["name"] for v in self.h.get(f"/rest/api/2/project/{pkey}/versions")}
        for v, d in versions_of(pkey):
            if v not in vers:
                self.h.post("/rest/api/2/version", {"name": v, "project": pkey, "releaseDate": d.isoformat(),
                                                    "released": d <= END.date()})
        self.board(pkey)
        self.mark(pkey, "setup", self.counts[pkey])

    def board(self, pkey):
        name = f"{pkey} scale board"
        found = self.h.get(f"/rest/agile/1.0/board?name={q(name)}&type=scrum")["values"]
        found = [b for b in found if b["name"] == name]
        if found:
            return found[0]["id"]
        f = self.h.post("/rest/api/2/filter", {"name": f"{name} filter", "jql": f"project = {pkey} ORDER BY Rank ASC",
                                               "favourite": False})
        b = self.h.post("/rest/agile/1.0/board", {"name": name, "type": "scrum", "filterId": int(f["id"])})
        say(P, f"created board {name}")
        return b["id"]

    # -- issues ------------------------------------------------------------------
    def fields_of(self, it):
        f = {"project": {"key": it.pkey}, "issuetype": {"name": it.type}, "summary": it.summary,
             "priority": {"name": it.priority}, "reporter": {"name": it.reporter}, "labels": it.labels}
        if it.description:
            f["description"] = it.description
        if it.assignee:
            f["assignee"] = {"name": it.assignee}
        if it.component:
            f["components"] = [{"name": it.component}]
        if it.version:
            f["fixVersions"] = [{"name": it.version}]
        if it.due:
            f["duedate"] = it.due
        if it.estimate and it.type != "Epic":
            f["timetracking"] = {"originalEstimate": it.estimate}
        if it.type == "Epic":
            f[self.epic_name] = it.summary[:250]
        if it.epic:
            f[self.epic_link] = f"{it.pkey}-{it.epic}"
        if it.parent:
            f["parent"] = {"key": f"{it.pkey}-{it.parent}"}
        return f

    def max_key(self, pkey):
        page = self.h.post("/rest/api/2/search", {"jql": f"project = {pkey} ORDER BY key DESC", "maxResults": 1,
                                                 "fields": ["summary"]})
        if not page["issues"]:
            return 0, 0
        return int(page["issues"][0]["key"].split("-")[1]), page["total"]

    def issues(self, pkey):
        if self.done(pkey, "issues"):
            return
        n = self.counts[pkey]
        top, total = self.max_key(pkey)
        if top != total:
            die(P, f"{pkey} holds {total} issues but its highest key is {pkey}-{top}: an issue was deleted, so the "
                   f"keys cannot be the plan's. just live-down jira, then seed again from empty.")
        if top > n:
            die(P, f"{pkey} already holds {top} issues, more than the {n} this count plans; seed with the count it was made with")
        plan_ = self.plans[pkey]
        # Batches of at most 50 in which no issue names another of the same batch.
        batches, cur = [], []
        for it in plan_[top + 1:]:
            ref = max(it.epic or 0, it.parent or 0)
            if len(cur) == 50 or (cur and ref >= cur[0].i):
                batches.append(cur)
                cur = []
            cur.append(it)
        if cur:
            batches.append(cur)
        say(P, f"{pkey}: creating issues {pkey}-{top + 1}..{pkey}-{n} in {len(batches)} bulk requests")
        # Keys are handed out in request order, so the batches go one after
        # another; each is one request of up to 50 issues.
        done_ = 0
        for b in batches:
            res = self.h.post("/rest/api/2/issue/bulk", {"issueUpdates": [{"fields": self.fields_of(it)} for it in b]})
            if res.get("errors"):
                die(P, f"{pkey}: bulk create refused: {json.dumps(res['errors'])[:800]}")
            keys = [x["key"] for x in res["issues"]]
            want = [it.key for it in b]
            if keys != want:
                die(P, f"{pkey}: bulk create keyed {keys[:3]}.. not {want[:3]}..: the counter is not where the plan is")
            done_ += len(b)
            self.made["issues"] += len(b)
            if done_ % 1000 < len(b) or done_ == n - top:
                say(P, f"{pkey}: issues {top + done_}/{n}")
        self.mark(pkey, "issues", n)

    # -- status --------------------------------------------------------------------
    def status(self, pkey):
        if self.done(pkey, "status"):
            return
        plan_ = self.plans[pkey]
        have = {x["key"]: x["fields"]["status"]["name"] for x in self.search(f"project = {pkey}", ["status"])}
        todo = [it for it in plan_[1:] if have.get(it.key) != it.status]
        if todo:
            ts = self.h.get(f"/rest/api/2/issue/{todo[0].key}/transitions")["transitions"]
            tid = {t["to"]["name"]: t["id"] for t in ts}
            missing = {it.status for it in todo} - set(tid)
            if missing:
                die(P, f"{pkey}: the workflow offers no transition to {missing}")

            def move(it):
                self.h.post(f"/rest/api/2/issue/{it.key}/transitions", {"transition": {"id": tid[it.status]}})

            parallel(P, f"{pkey} transitions", todo, move, self.workers, 1000)
            self.made["transitions"] += len(todo)
        self.mark(pkey, "status", self.counts[pkey])

    # -- comments --------------------------------------------------------------------
    def comments(self, pkey):
        if self.done(pkey, "comments"):
            return
        plan_ = self.plans[pkey]
        have = {x["key"]: x["fields"]["comment"]["total"] for x in self.search(f"project = {pkey}", ["comment"])}
        jobs = []
        for it in plan_[1:]:
            r = rng(P, pkey, it.i, "comment-authors")
            authors = [it.assignee or weighted(r, PEOPLE) if r.random() < 0.5 else weighted(r, PEOPLE)
                       for _ in it.comments]
            for k in range(have.get(it.key, 0), len(it.comments)):
                jobs.append((it.key, authors[k], it.comments[k]))

        def post(job):
            key, who, body = job
            self.people[who].post(f"/rest/api/2/issue/{key}/comment", {"body": body})

        # One issue's comments in order: a job per issue would keep that, but
        # Jira orders comments by id, and ids follow the posts, so the jobs go
        # by issue and comment index and the dates are set by rank later.
        parallel(P, f"{pkey} comments", jobs, post, self.workers, 2000)
        self.made["comments"] += len(jobs)
        self.mark(pkey, "comments", self.counts[pkey])

    def worklogs(self, pkey):
        if self.done(pkey, "worklogs"):
            return
        plan_ = self.plans[pkey]
        want = [it for it in plan_[1:] if it.worklogs]
        have = {x["key"]: x["fields"]["worklog"]["total"]
                for x in self.search(f"project = {pkey} AND timespent > 0", ["worklog"])}
        jobs = [(it, k) for it in want for k in range(have.get(it.key, 0), len(it.worklogs))]

        def post(job):
            it, k = job
            t, secs, text = it.worklogs[k]
            who = it.assignee or self.user
            self.people[who].post(f"/rest/api/2/issue/{it.key}/worklog?adjustEstimate=leave",
                                  {"started": iso(t), "timeSpentSeconds": secs, "comment": text})

        parallel(P, f"{pkey} worklogs", jobs, post, self.workers, 1000)
        self.made["worklogs"] += len(jobs)
        self.mark(pkey, "worklogs", self.counts[pkey])

    def links(self, pkey):
        if self.done(pkey, "links"):
            return
        plan_ = self.plans[pkey]
        have = set()
        for x in self.search(f"project = {pkey}", ["issuelinks"]):
            for lk in x["fields"]["issuelinks"]:
                if "outwardIssue" in lk:
                    have.add((lk["type"]["name"], x["key"], lk["outwardIssue"]["key"]))
                else:
                    have.add((lk["type"]["name"], lk["inwardIssue"]["key"], x["key"]))
        jobs = []
        for it in plan_[1:]:
            for typ, (ok, on) in it.links:
                if on < 1 or on > self.counts[ok] or (ok, on) == (pkey, it.i):
                    continue
                # The newer issue is the outward one: "PAY-900 blocks PAY-850",
                # "clones", "duplicates".
                rec = (typ, it.key, f"{ok}-{on}")
                if rec not in have:
                    have.add(rec)
                    jobs.append(rec)

        def post(rec):
            typ, outward, inward = rec
            self.h.post("/rest/api/2/issueLink", {"type": {"name": typ}, "outwardIssue": {"key": outward},
                                                  "inwardIssue": {"key": inward}})

        parallel(P, f"{pkey} links", jobs, post, self.workers, 1000)
        self.made["links"] += len(jobs)
        self.mark(pkey, "links", self.counts[pkey])

    # -- sprints ---------------------------------------------------------------------
    def sprint_windows(self):
        out, k = [], 0
        while True:
            s = START + dt.timedelta(days=SPRINT_DAYS * k)
            if s > END + dt.timedelta(days=2 * SPRINT_DAYS):
                return out
            out.append((k + 1, s.replace(hour=9, minute=0, second=0), (s + dt.timedelta(days=SPRINT_DAYS)).replace(hour=17, minute=0, second=0)))
            k += 1

    def sprints(self, pkey):
        if self.done(pkey, "sprints"):
            return
        plan_ = self.plans[pkey]
        board = self.board(pkey)
        windows = self.sprint_windows()
        active = next(k for k, s, e in windows if s <= END < e)
        members = {k: [] for k, _, _ in windows}
        for it in plan_[1:]:
            if it.type in ("Epic", "Sub-task"):
                continue
            r = rng(P, pkey, it.i, "sprint")
            if it.status == "Done":
                if r.random() < 0.9:
                    k = next((k for k, s, e in windows if s <= it.resolved < e), active)
                    members[min(k, active)].append(it.key)
            elif it.status in ("In Progress", "In Review"):
                members[active].append(it.key)
            elif r.random() < 0.3:
                members[active + r.randint(0, 2)].append(it.key)
        have = {}
        start = 0
        while True:
            page = self.h.get(f"/rest/agile/1.0/board/{board}/sprint?state=future,active,closed&startAt={start}&maxResults=50")
            for s in page["values"]:
                have[s["name"]] = s
            start += len(page["values"])
            if page.get("isLast", True) or not page["values"]:
                break
        in_sprint = {}
        for x in self.search(f"project = {pkey} AND sprint is not EMPTY", [self.sprint_field]):
            v = x["fields"].get(self.sprint_field) or []
            in_sprint[x["key"]] = {self.sprint_name(s) for s in v}
        for k, s, e in windows:
            name = f"{pkey} Sprint {k}"
            sp = have.get(name)
            if sp and sp["state"] == "closed":
                continue
            if sp is None:
                sp = self.h.post("/rest/agile/1.0/sprint", {"name": name, "originBoardId": board,
                                                            "startDate": iso(s), "endDate": iso(e),
                                                            "goal": sentence(rng(P, pkey, "goal", k), 4, 10)})
                self.made["sprints"] += 1
            add = [key for key in members.get(k, []) if name not in in_sprint.get(key, set())]
            for c in chunks(add, 50):
                self.h.post(f"/rest/agile/1.0/sprint/{sp['id']}/issue", {"issues": c})
            if k <= active and sp["state"] == "future":
                self.h.post(f"/rest/agile/1.0/sprint/{sp['id']}", {"state": "active", "startDate": iso(s),
                                                                   "endDate": iso(e)})
                sp["state"] = "active"
            if k < active and sp["state"] == "active":
                self.h.post(f"/rest/agile/1.0/sprint/{sp['id']}", {"state": "closed"})
        say(P, f"{pkey}: {len(windows)} sprints on board {board}, sprint {active} active")
        self.mark(pkey, "sprints", self.counts[pkey])

    @staticmethod
    def sprint_name(s):
        if isinstance(s, dict):
            return s.get("name")
        # Jira 10 answers the field as strings: "com.atlassian...Sprint@1a[id=1,...,name=PAY Sprint 1,...]"
        for part in str(s).split(","):
            if part.startswith("name="):
                return part[5:]
        return None

    # -- dates -------------------------------------------------------------------------
    def dates(self):
        if all(self.done(p, "dates") for p, _, _ in PROJECTS):
            return
        say(P, "rewriting created, updated and resolved in Jira's database")
        sql = ["BEGIN;", "CREATE TEMP TABLE scale_dates (pkey text, num bigint, c timestamptz, u timestamptz, r timestamptz);"]
        for pkey, _, _ in PROJECTS:
            rows = []
            for it in self.plans[pkey][1:]:
                r = f"'{it.resolved.isoformat()}'" if it.resolved else "NULL"
                rows.append(f"('{pkey}',{it.i},'{it.created.isoformat()}','{it.updated.isoformat()}',{r})")
            for c in chunks(rows, 2000):
                sql.append("INSERT INTO scale_dates VALUES " + ",".join(c) + ";")
        windows = self.sprint_windows()
        active = next(k for k, s, e in windows if s <= END < e)
        sql.append("""
UPDATE jiraissue j SET created = d.c, updated = d.u, resolutiondate = d.r
  FROM scale_dates d, project p
 WHERE p.pkey = d.pkey AND j.project = p.id AND j.issuenum = d.num;
WITH a AS (
  SELECT a.id, j.created c, coalesce(j.resolutiondate, j.updated) e,
         row_number() OVER (PARTITION BY a.issueid ORDER BY a.id) rn,
         count(*) OVER (PARTITION BY a.issueid) cnt
    FROM jiraaction a JOIN jiraissue j ON j.id = a.issueid JOIN project p ON p.id = j.project
   WHERE p.pkey IN (SELECT DISTINCT pkey FROM scale_dates) AND a.actiontype = 'comment')
UPDATE jiraaction x SET created = a.c + (a.e - a.c) * (a.rn::float8 / (a.cnt + 1)),
                        updated = a.c + (a.e - a.c) * (a.rn::float8 / (a.cnt + 1))
  FROM a WHERE x.id = a.id;
WITH g AS (
  SELECT g.id, j.created c, coalesce(j.resolutiondate, j.updated) e,
         row_number() OVER (PARTITION BY g.issueid ORDER BY g.id) rn,
         count(*) OVER (PARTITION BY g.issueid) cnt
    FROM changegroup g JOIN jiraissue j ON j.id = g.issueid JOIN project p ON p.id = j.project
   WHERE p.pkey IN (SELECT DISTINCT pkey FROM scale_dates))
UPDATE changegroup x SET created = g.c + (g.e - g.c) * (g.rn::float8 / (g.cnt + 1))
  FROM g WHERE x.id = g.id;
UPDATE worklog w SET created = w.startdate, updated = w.startdate
  FROM jiraissue j, project p
 WHERE w.issueid = j.id AND j.project = p.id AND p.pkey IN (SELECT DISTINCT pkey FROM scale_dates);
""")
        # Closed sprints completed on their end date, not on the seed's day.
        sql.append("""UPDATE "AO_60DB71_SPRINT" SET "COMPLETE_DATE" = "END_DATE"
 WHERE "CLOSED" AND "NAME" ~ '^SCL[A-Z]+ Sprint [0-9]+$';""")
        sql.append("COMMIT;")
        psql("knobas-jira-db", "jira", "jira", "\n".join(sql))
        say(P, "reindexing (background), so search and JQL see the dates")
        self.h.post("/rest/api/2/reindex?type=BACKGROUND_PREFERRED&indexComments=true&indexChangeHistory=true&indexWorklogs=true",
                    ok=(200, 201, 202, 409))  # 409: one already runs, started after the rewrite: wait for it
        import time
        while True:
            time.sleep(10)
            st = self.h.get("/rest/api/2/reindex", ok=(200, 303))  # 303 while it runs
            if st.get("finishTime"):
                if not st.get("success", True):
                    die(P, f"reindex failed: {st}")
                break
            say(P, f"reindex {st.get('currentProgress', st.get('progress', '?'))}%")
        probe = self.plans[PROJECTS[0][0]][1]
        got = self.h.get(f"/rest/api/2/issue/{probe.key}?fields=created")["fields"]["created"]
        if not got.startswith(probe.created.strftime("%Y-%m-%d")):
            die(P, f"{probe.key} reads created {got}, not {probe.created.date()}: Jira did not take the rewritten dates")
        for pkey, _, _ in PROJECTS:
            self.mark(pkey, "dates", self.counts[pkey])

    def report(self):
        tot = 0
        for pkey, _, _ in PROJECTS:
            n = self.h.post("/rest/api/2/search", {"jql": f"project = {pkey}", "maxResults": 0})["total"]
            tot += n
            say(P, f"{pkey}: {n} issues")
        made = ", ".join(f"{v} {k}" for k, v in self.made.items())
        say(P, f"done: {tot} issues in {len(PROJECTS)} projects; this run made {made}")


def psql(container, user, db, sql):
    """Run SQL in a product's PostgreSQL container, on stdin."""
    p = subprocess.run(["docker", "exec", "-i", container, "psql", "-v", "ON_ERROR_STOP=1", "-q", "-U", user, "-d", db],
                       input=sql.encode(), capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(f"psql in {container}: {p.stderr.decode()[-1500:]}")
    return p.stdout.decode()


def main(state, count, workers):
    Jira(state, count, workers).run()


def describe(total):
    """The plan's numbers, without a server: --plan-only."""
    import collections
    c = counts(total)
    agg = collections.Counter()
    biggest_epic = 0
    for pkey, _, _ in PROJECTS:
        pl = plan(pkey, c[pkey], c)
        kids = collections.Counter(it.epic for it in pl[1:] if it.epic)
        biggest_epic = max(biggest_epic, max(kids.values(), default=0))
        for it in pl[1:]:
            agg["issues"] += 1
            agg[f"type {it.type}"] += 1
            agg[f"status {it.status}"] += 1
            agg["comments"] += len(it.comments)
            agg["worklogs"] += len(it.worklogs)
            agg["links"] += len(it.links)
            agg["description chars"] += len(it.description or "")
    for k, v in sorted(agg.items()):
        print(f"{k}: {v}")
    print(f"largest epic: {biggest_epic} children")
