"""Confluence at Björn's scale: three `SCL*` spaces beside ENG and ARCHIVE.

Each space is a tree under its home page: a dozen sections, some flat with
hundreds of children (meeting notes, incident reports, release notes), some
deep (an API reference, an architecture tree nine levels down), bodies from a
few hundred bytes to tens of kilobytes of storage format with headings,
tables, code, panels, status lozenges, expands, task lists, page links,
mentions and Jira macros. Pages are written by the fixture's people (made in
Confluence here if missing; six of the ten-user licence's accounts in all),
some carry labels, comments and attachments.

Phases, recorded in a space property (`knobas-scale-seed`) once done for a
count: setup (people, space), pages (level by level, parents first, found by
title so a rerun creates only what is missing), comments, attachments, and
dates -- a page's created and modified times rewritten in Confluence's
database and the index rebuilt, when this Confluence can rebuild it over
REST (`dates` says what it did).
"""

from __future__ import annotations

import datetime as dt
import io
import json
import struct
import time
import zlib
from xml.sax.saxutils import escape

from common import (END, START, Http, HttpError, basic, chunks, die, later, length_class, paragraph,
                    parallel, q, rng, say, sentence, summary, weighted, when, zipf_pick, COMPONENTS)

P = "confluence"
PROPERTY = "knobas-scale-seed"
SPACES = [
    ("SCLENG", "Scale test: Engineering Handbook", 0.55),
    ("SCLOPS", "Scale test: Operations Runbooks", 0.30),
    ("SCLPRD", "Scale test: Product Notes", 0.15),
]
PEOPLE = [("knobas", 20), ("mara.lindqvist", 22), ("jonas.becker", 18), ("priya.nair", 16),
          ("tomasz.wojcik", 16), ("lena.hoffmann", 8)]
FULL = {"mara.lindqvist": "Mara Lindqvist", "jonas.becker": "Jonas Becker", "priya.nair": "Priya Nair",
        "tomasz.wojcik": "Tomasz Wójcik", "lena.hoffmann": "Lena Hoffmann"}
PEOPLE_PASS = "tidewater-dev"
LABELS = ["runbook", "architecture", "how-to", "decision", "meeting-notes", "incident", "postmortem",
          "release-notes", "onboarding", "api", "deprecated", "draft", "security", "performance",
          "q3", "q4", "team-payments", "team-platform", "team-data", "kb"]
JIRA_PROJECTS = [("SCLPAY", 5600), ("SCLOPS", 4000), ("SCLWEB", 3600), ("SCLDATA", 2800)]

# (title, kind, weight): flat sections hold their pages directly, dated;
# deep ones are trees.
SECTIONS = [
    ("Meeting notes", "meetings", 16), ("Incident reports", "incidents", 9), ("Release notes", "releases", 7),
    ("Architecture", "deep", 12), ("API reference", "deep", 10), ("Runbooks", "deep", 10),
    ("How-to articles", "mixed", 8), ("Decision log", "decisions", 5), ("Team pages", "deep", 8),
    ("Onboarding", "mixed", 4), ("Retrospectives", "retros", 5), ("Archive", "deep", 6),
]


class Page:
    __slots__ = ("space", "title", "parent", "depth", "author", "created", "modified", "size", "labels",
                 "comments", "attachments", "kind", "seed")


def plan_space(key, n):
    r = rng(P, key, "tree")
    titles = set()

    def unique(t):
        base, k = t, 2
        while t.lower() in titles:
            t = f"{base} ({k})"
            k += 1
        titles.add(t.lower())
        return t

    pages = []

    def add(title, parent, depth, kind, i, total):
        p = Page()
        p.space, p.title, p.parent, p.depth, p.kind = key, unique(title), parent, depth, kind
        pr = rng(P, key, "page", len(pages))
        p.seed = len(pages)
        p.author = weighted(pr, PEOPLE)
        p.created = when(i, total, pr, 20)
        p.modified = later(p.created, pr, 30, 1.6) if pr.random() < 0.6 else p.created + dt.timedelta(minutes=pr.randint(1, 50))
        p.size = int(min(max(pr.lognormvariate(8.0, 1.0), 150), 90000))   # bytes of storage, median ~3 KB
        p.labels = sorted({zipf_pick(pr, LABELS) for _ in range(weighted(pr, [(0, 45), (1, 30), (2, 18), (3, 7)]))})
        nc = 0 if pr.random() < 0.7 else min(int(pr.lognormvariate(0.4, 0.9)) + 1, 30)
        p.comments = [(weighted(pr, PEOPLE), comment_storage(pr)) for _ in range(nc)]
        p.attachments = []
        if pr.random() < 0.035:
            for a in range(pr.randint(1, 4)):
                p.attachments.append(pr.choice(["png", "pdf", "csv", "txt", "json"]))
        pages.append(p)
        return p.title

    weights = [w for _, _, w in SECTIONS]
    budget = n - len(SECTIONS)
    alloc = [max(1, int(budget * w / sum(weights))) for w in weights]
    alloc[0] += budget - sum(alloc)
    for (title, kind, _), m in zip(SECTIONS, alloc):
        sec = add(title, None, 1, "section", 0, 1)
        if kind in ("meetings", "incidents", "releases", "retros", "decisions"):
            flat(add, sec, kind, m, r)
        elif kind == "mixed":
            flat(add, sec, "howto", m, r)
        else:
            tree(add, sec, m, r, title)
    return pages


def flat(add, parent, kind, m, r):
    # Meeting notes in yearly folders with hundreds each; the rest directly.
    if kind == "meetings":
        series = ["Platform sync", "Payments standup", "Architecture review", "Ops weekly", "Product council",
                  "Data guild", "Security office hours", "1:1 notes"]
        per_year = {}
        for i in range(m):
            t = START + (END - START) * (i + 0.5) / m
            y = str(t.year)
            if y not in per_year:
                per_year[y] = add(f"Meeting notes {y}", parent, 2, "folder", i, m)
            add(f"{t.date().isoformat()} {series[i % len(series)]}", per_year[y], 3, "meeting", i, m)
        return
    for i in range(m):
        t = START + (END - START) * (i + 0.5) / m
        if kind == "incidents":
            title = f"INC-{1000 + i}: {COMPONENTS[i * 7 % len(COMPONENTS)]} {['outage', 'degradation', 'data delay', 'elevated errors'][i % 4]}"
        elif kind == "releases":
            title = f"Release {t.year % 100}.{t.month}.{i % 9} notes"
        elif kind == "retros":
            title = f"Retrospective {t.date().isoformat()}"
        elif kind == "decisions":
            title = f"ADR-{i + 1:03d}: {summary(rng(P, 'adr', i))}"
        else:
            title = f"How to {summary(rng(P, 'howto', parent, i)).lower()}"
        add(title, parent, 2, kind, i, m)


def tree(add, root, m, r, section):
    """A tree of m pages under root, deep in places: each node takes a few
    children, now and then a long chain."""
    frontier = [(root, 2)]
    made = 0
    k = 0
    while made < m and frontier:
        # Mostly breadth first, now and then down the newest branch: deep in places.
        parent, depth = frontier.pop(0 if r.random() < 0.6 else -1)
        kids = r.randint(2, 7) if depth < 4 else r.randint(0, 4)
        if depth >= 9:
            kids = 0
        if r.random() < 0.05:
            kids = r.randint(30, 120)    # the odd page with very many children
        for _ in range(min(kids, m - made)):
            title = f"{summary(rng(P, section, k))}"
            if section == "API reference":
                title = f"{r.choice(['GET', 'POST', 'PUT', 'DELETE'])} /v{r.randint(1, 3)}/{r.choice(COMPONENTS).replace(' ', '-')}/{r.choice(['items', 'status', 'batches', 'events', 'config'])}"
            t = add(title, parent, depth, "doc", made, m)
            frontier.append((t, depth + 1))
            made += 1
            k += 1
        if not frontier and made < m:
            frontier.append((root, 2))


def comment_storage(r):
    x = r.random()
    if x < 0.2:
        return "<p>" + escape(r.choice(["Thanks, this helped.", "Is this still current?", "+1", "Updated the numbers.",
                                         "We should link the runbook here.", "Done."])) + "</p>"
    if x < 0.3:
        return f'<p><ac:link><ri:user ri:username="{weighted(r, PEOPLE)}" /></ac:link> {escape(sentence(r))}</p>'
    return "".join(f"<p>{escape(paragraph(r, 1, 3))}</p>" for _ in range(r.randint(1, 3)))


def body(p: Page, earlier):
    """Storage format of about p.size bytes."""
    r = rng(P, p.space, "body", p.seed)
    out = []
    if p.kind == "meeting":
        out.append("<h2>Attendees</h2><ul>" + "".join(
            f'<li><ac:link><ri:user ri:username="{u}" /></ac:link></li>' for u in sorted({weighted(r, PEOPLE) for _ in range(4)})) + "</ul>")
        out.append("<h2>Action items</h2>" + tasks(r))
    elif p.kind == "incidents":
        out.append('<ac:structured-macro ac:name="status"><ac:parameter ac:name="colour">'
                   f'{r.choice(["Red", "Yellow", "Green"])}</ac:parameter><ac:parameter ac:name="title">'
                   f'{r.choice(["SEV1", "SEV2", "SEV3"])}</ac:parameter></ac:structured-macro>')
        out.append("<h2>Timeline</h2><table><tbody><tr><th>Time</th><th>Event</th></tr>" + "".join(
            f"<tr><td>{r.randint(0, 23):02d}:{r.randint(0, 59):02d}</td><td>{escape(sentence(r))}</td></tr>"
            for _ in range(r.randint(3, 12))) + "</tbody></table>")
    elif p.kind == "section" or p.kind == "folder":
        out.append('<ac:structured-macro ac:name="children"><ac:parameter ac:name="all">true</ac:parameter></ac:structured-macro>')
    if r.random() < 0.15:
        out.append('<ac:structured-macro ac:name="toc" />')
    while sum(len(x) for x in out) < p.size:
        x = r.random()
        if x < 0.12:
            out.append(f"<h2>{escape(summary(r))}</h2>")
        elif x < 0.2:
            out.append(f"<h3>{escape(summary(r))}</h3>")
        elif x < 0.3:
            out.append("<ul>" + "".join(f"<li>{escape(sentence(r))}</li>" for _ in range(r.randint(2, 8))) + "</ul>")
        elif x < 0.36:
            lang = r.choice(["java", "sql", "bash", "python", "yaml", "json"])
            code = "\n".join(sentence(r, 2, 8).lower() for _ in range(r.randint(3, 40)))
            out.append(f'<ac:structured-macro ac:name="code"><ac:parameter ac:name="language">{lang}</ac:parameter>'
                       f"<ac:plain-text-body><![CDATA[{code}]]></ac:plain-text-body></ac:structured-macro>")
        elif x < 0.42:
            cols = r.randint(2, 6)
            rows = "".join("<tr>" + "".join(f"<td>{escape(sentence(r, 1, 5))}</td>" for _ in range(cols)) + "</tr>"
                           for _ in range(r.randint(2, 15)))
            out.append("<table><tbody><tr>" + "".join(f"<th>{escape(r.choice(COMPONENTS))}</th>" for _ in range(cols))
                       + "</tr>" + rows + "</tbody></table>")
        elif x < 0.47:
            kind = r.choice(["info", "note", "warning", "tip"])
            out.append(f'<ac:structured-macro ac:name="{kind}"><ac:rich-text-body><p>{escape(paragraph(r, 1, 3))}</p>'
                       "</ac:rich-text-body></ac:structured-macro>")
        elif x < 0.50:
            out.append(f'<ac:structured-macro ac:name="expand"><ac:parameter ac:name="title">{escape(summary(r))}'
                       f"</ac:parameter><ac:rich-text-body><p>{escape(paragraph(r))}</p></ac:rich-text-body></ac:structured-macro>")
        elif x < 0.53:
            out.append(tasks(r))
        elif x < 0.56 and earlier:
            t = escape(r.choice(earlier), {'"': "&quot;"})
            out.append(f'<p>See <ac:link><ri:page ri:content-title="{t}" /></ac:link>.</p>')
        elif x < 0.58:
            pk, pn = r.choice(JIRA_PROJECTS)
            out.append(f'<p><ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">{pk}-{r.randint(1, pn)}'
                       "</ac:parameter></ac:structured-macro></p>")
        elif x < 0.6:
            out.append(f'<p><ac:link><ri:user ri:username="{weighted(r, PEOPLE)}" /></ac:link> {escape(sentence(r))}</p>')
        else:
            out.append(f"<p>{escape(paragraph(r))}</p>")
    return "".join(out)


def tasks(r):
    return "<ac:task-list>" + "".join(
        f"<ac:task><ac:task-id>{k + 1}</ac:task-id><ac:task-status>{'complete' if r.random() < 0.6 else 'incomplete'}</ac:task-status>"
        f"<ac:task-body>{escape(sentence(r, 3, 10))}</ac:task-body></ac:task>" for k in range(r.randint(1, 6))) + "</ac:task-list>"


def describe(total):
    import collections
    agg = collections.Counter()
    for key, _, share in SPACES:
        pages = plan_space(key, counts(total)[key])
        kids = collections.Counter(p.parent for p in pages)
        agg["pages"] += len(pages)
        agg["comments"] += sum(len(p.comments) for p in pages)
        agg["attachments"] += sum(len(p.attachments) for p in pages)
        agg["storage bytes"] += sum(p.size for p in pages)
        agg["max depth"] = max(agg["max depth"], max(p.depth for p in pages))
        agg["most children"] = max(agg["most children"], max(kids.values()))
    for k, v in agg.items():
        print(f"{k}: {v}")


def counts(total):
    n = [int(total * s) for _, _, s in SPACES]
    n[0] += total - sum(n)
    return {k: c for (k, _, _), c in zip(SPACES, n)}


# -- files ----------------------------------------------------------------------------


def png(r, w=64, h=40):
    color = (r.randint(0, 255), r.randint(0, 255), r.randint(0, 255))
    raw = b"".join(b"\x00" + bytes(color) * w for _ in range(h))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def pdf(text):
    stream = f"BT /F1 14 Tf 40 760 Td ({text[:80]}) Tj ET".encode()
    objs = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
            b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream",
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    out = io.BytesIO()
    out.write(b"%PDF-1.4\n")
    offs = []
    for k, o in enumerate(objs, 1):
        offs.append(out.tell())
        out.write(b"%d 0 obj\n" % k + o + b"\nendobj\n")
    x = out.tell()
    out.write(b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1))
    for o in offs:
        out.write(b"%010d 00000 n \n" % o)
    out.write(b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, x))
    return out.getvalue()


def attachment(p, k, ext):
    r = rng(P, p.space, "att", p.seed, k)
    name = f"{r.choice(['diagram', 'export', 'report', 'screenshot', 'config', 'metrics'])}-{p.seed}-{k}.{ext}"
    if ext == "png":
        return name, "image/png", png(r)
    if ext == "pdf":
        return name, "application/pdf", pdf(summary(r))
    if ext == "csv":
        return name, "text/csv", ("component,p50_ms,p99_ms\n" + "".join(
            f"{r.choice(COMPONENTS)},{r.randint(1, 300)},{r.randint(300, 3000)}\n" for _ in range(r.randint(5, 200)))).encode()
    if ext == "json":
        return name, "application/json", json.dumps({"service": r.choice(COMPONENTS), "replicas": r.randint(1, 12),
                                                     "limits": {"cpu": r.randint(1, 8), "memory_mb": r.randint(256, 8192)}}, indent=2).encode()
    return name, "text/plain", "\n".join(sentence(r) for _ in range(r.randint(5, 80))).encode()


# -- the seed ----------------------------------------------------------------------------


class Confluence:
    def __init__(self, state, total, workers):
        c = state.get("confluence") or die(P, "seed-state.json has no confluence block: just live-up confluence first")
        self.url, self.user, self.password = c["url"], c["user"], c["password"]
        self.h = Http(self.url, user=self.user, password=self.password)
        self.total, self.workers = total, workers
        self.counts = counts(total)
        self.made = {"users": 0, "spaces": 0, "pages": 0, "comments": 0, "attachments": 0}

    def as_(self, who):
        if who == self.user:
            return self.h
        if not hasattr(self, "_people"):
            self._people = {}
        if who not in self._people:
            self._people[who] = Http(self.url, user=who, password=PEOPLE_PASS)
        return self._people[who]

    def marker(self, key):
        try:
            return json.loads(json.dumps(self.h.get(f"/rest/api/space/{key}/property/{PROPERTY}")))
        except HttpError as e:
            if e.status == 404:
                return None
            raise

    def phase_done(self, key, phase):
        m = self.marker(key)
        return bool(m) and m["value"].get(phase) == self.counts[key]

    def mark(self, key, phase):
        m = self.marker(key)
        if m is None:
            self.h.post(f"/rest/api/space/{key}/property", {"key": PROPERTY, "value": {phase: self.counts[key]}})
        else:
            v = m["value"]
            v[phase] = self.counts[key]
            self.h.put(f"/rest/api/space/{key}/property/{PROPERTY}",
                       {"key": PROPERTY, "value": v, "version": {"number": m["version"]["number"] + 1}})

    def run(self):
        self.h.get("/rest/api/user/current")
        self.people()
        for key, name, _ in SPACES:
            self.space(key, name)
        for key, _, _ in SPACES:
            self.pages(key)
        for key, _, _ in SPACES:
            self.comments(key)
            self.attachments(key)
        self.dates()
        self.report()

    def people(self):
        for u, _ in PEOPLE:
            if u == self.user:
                continue
            try:
                self.h.get(f"/rest/api/user?username={q(u)}")
            except HttpError as e:
                if e.status != 404:
                    raise
                self.h.post("/rest/api/admin/user", {"userName": u, "fullName": FULL[u], "email": f"{u.split('.')[0]}@example.com",
                                                     "password": PEOPLE_PASS, "notifyViaEmail": False})
                self.made["users"] += 1
                say(P, f"created user {u}")

    def space(self, key, name):
        try:
            sp = self.h.get(f"/rest/api/space/{key}?expand=homepage")
        except HttpError as e:
            if e.status != 404:
                raise
            sp = self.h.post("/rest/api/space", {"key": key, "name": name, "description": {"plain": {
                "value": "Scale test data made by testenv/seed-scale.sh. Not fixture content.", "representation": "plain"}}})
            self.made["spaces"] += 1
            say(P, f"created space {key} ({name})")
            sp = self.h.get(f"/rest/api/space/{key}?expand=homepage")
        return sp["homepage"]["id"]

    def existing(self, key):
        out, start = {}, 0
        while True:
            page = self.h.get(f"/rest/api/content?spaceKey={key}&type=page&status=current&limit=200&start={start}")
            for c in page["results"]:
                out[c["title"].lower()] = c["id"]
            start += len(page["results"])
            if not page["results"] or "next" not in page.get("_links", {}):
                return out

    def pages(self, key):
        if self.phase_done(key, "pages"):
            return
        home = self.space(key, dict((k, n) for k, n, _ in SPACES)[key])
        plan_ = plan_space(key, self.counts[key])
        self.plan = getattr(self, "plan", {})
        self.plan[key] = plan_
        have = self.existing(key)
        ids = dict(have)
        titles = [p.title for p in plan_]
        todo = [p for p in plan_ if p.title.lower() not in have]
        say(P, f"{key}: {len(plan_) - len(todo)} of {len(plan_)} pages there, creating {len(todo)}")
        for depth in sorted({p.depth for p in todo}):
            level = [p for p in todo if p.depth == depth]

            def create(p):
                parent = ids[p.parent.lower()] if p.parent else home
                earlier = titles[max(0, p.seed - 200):p.seed]
                b = {"type": "page", "title": p.title, "space": {"key": key}, "ancestors": [{"id": parent}],
                     "body": {"storage": {"value": body(p, earlier), "representation": "storage"}}}
                try:
                    res = self.as_(p.author).post("/rest/api/content", b)
                except HttpError as e:
                    if e.status == 400 and "already exists" in e.body:
                        res = self.h.get(f"/rest/api/content?spaceKey={key}&type=page&title={q(p.title)}")["results"][0]
                    else:
                        raise
                ids[p.title.lower()] = res["id"]
                # Labels in the create's `metadata` are dropped without a word
                # (Data Center 9.2); they are a call of their own.
                if p.labels:
                    self.as_(p.author).post(f"/rest/api/content/{res['id']}/label",
                                            [{"prefix": "global", "name": lb} for lb in p.labels])

            parallel(P, f"{key} pages at depth {depth}", level, create, self.workers, 500)
            self.made["pages"] += len(level)
        self.mark(key, "pages")

    def plan_of(self, key):
        if not hasattr(self, "plan") or key not in self.plan:
            self.plan = getattr(self, "plan", {})
            self.plan[key] = plan_space(key, self.counts[key])
        return self.plan[key]

    def search_all(self, cql, expand=""):
        out, start = [], 0
        while True:
            page = self.h.get(f"/rest/api/content/search?cql={q(cql)}&limit=100&start={start}"
                              + (f"&expand={expand}" if expand else ""))
            out += page["results"]
            start += len(page["results"])
            if not page["results"] or "next" not in page.get("_links", {}):
                return out

    def comments(self, key):
        if self.phase_done(key, "comments"):
            return
        plan_ = self.plan_of(key)
        ids = self.existing(key)
        have = {}
        for c in self.search_all(f'space = "{key}" and type = comment', "container"):
            have[c["container"]["id"]] = have.get(c["container"]["id"], 0) + 1
        jobs = []
        for p in plan_:
            pid = ids[p.title.lower()]
            for who, text in p.comments[have.get(pid, 0):]:
                jobs.append((pid, who, text))

        def post(job):
            pid, who, text = job
            self.as_(who).post("/rest/api/content", {"type": "comment", "container": {"id": pid, "type": "page"},
                                                     "body": {"storage": {"value": text, "representation": "storage"}}})

        parallel(P, f"{key} comments", jobs, post, self.workers, 500)
        self.made["comments"] += len(jobs)
        self.mark(key, "comments")

    def attachments(self, key):
        if self.phase_done(key, "attachments"):
            return
        plan_ = self.plan_of(key)
        ids = self.existing(key)
        have = {(a["container"]["id"], a["title"]) for a in self.search_all(f'space = "{key}" and type = attachment', "container")}
        jobs = []
        for p in plan_:
            pid = ids[p.title.lower()]
            for k, ext in enumerate(p.attachments):
                name, ctype, data = attachment(p, k, ext)
                if (pid, name) not in have:
                    jobs.append((pid, p.author, name, ctype, data))

        def post(job):
            pid, who, name, ctype, data = job
            boundary = "knobasscale" + str(abs(hash(name)))
            payload = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
                       f"Content-Type: {ctype}\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
            self.as_(who).request("POST", f"/rest/api/content/{pid}/child/attachment", raw=payload,
                                  headers={"X-Atlassian-Token": "nocheck",
                                           "Content-Type": f"multipart/form-data; boundary={boundary}"})

        parallel(P, f"{key} attachments", jobs, post, self.workers, 100)
        self.made["attachments"] += len(jobs)
        self.mark(key, "attachments")

    def dates(self):
        if all(self.phase_done(k, "dates") for k, _, _ in SPACES):
            return
        # The search index answers CQL's lastmodified, and knobas' sync walks
        # by it; the database answers version.when. Rewriting one without the
        # other would make every incremental sync re-read the whole space.
        import jira as _j  # psql
        rows = []
        for key, _, _ in SPACES:
            for p in self.plan_of(key):
                rows.append(f"('{key}',{sql_str(p.title)},'{p.created.isoformat()}','{p.modified.isoformat()}')")
        sql = ["BEGIN;", "CREATE TEMP TABLE sd (k text, t text, c timestamptz, m timestamptz);"]
        for c in chunks(rows, 1000):
            sql.append("INSERT INTO sd VALUES " + ",".join(c) + ";")
        sql.append("""
CREATE TEMP TABLE sp AS
  SELECT c.contentid id, sd.c, sd.m FROM content c JOIN spaces s ON s.spaceid = c.spaceid
    JOIN sd ON sd.k = s.spacekey AND lower(sd.t) = lower(c.title)
   WHERE c.contenttype = 'PAGE' AND c.prevver IS NULL AND c.content_status = 'current';
UPDATE content c SET creationdate = sp.c, lastmoddate = sp.m FROM sp WHERE c.contentid = sp.id;
UPDATE content c SET creationdate = sp.c, lastmoddate = sp.c FROM sp WHERE c.prevver = sp.id;
WITH x AS (
  SELECT c.contentid id, sp.c, sp.m, row_number() OVER (PARTITION BY c.pageid ORDER BY c.contentid) rn,
         count(*) OVER (PARTITION BY c.pageid) cnt
    FROM content c JOIN sp ON sp.id = c.pageid WHERE c.contenttype IN ('COMMENT', 'ATTACHMENT'))
UPDATE content c SET creationdate = x.c + (x.m - x.c + interval '1 hour') * (x.rn::float8 / (x.cnt + 1)),
                     lastmoddate = x.c + (x.m - x.c + interval '1 hour') * (x.rn::float8 / (x.cnt + 1))
  FROM x WHERE c.contentid = x.id;
COMMIT;
""")
        say(P, "rewriting created and modified times in Confluence's database")
        _j.psql("knobas-confluence-db", "confluence", "confluence", "\n".join(sql))
        self.reindex_after_restart()
        for key, _, _ in SPACES:
            self.mark(key, "dates")

    def reindex_after_restart(self):
        import subprocess
        say(P, "restarting Confluence, so its caches read the rewritten dates (it also renews the licence)")
        subprocess.run(["docker", "restart", "knobas-confluence"], check=True, capture_output=True)
        deadline = time.time() + 600
        while time.time() < deadline:
            try:
                self.h.get("/rest/api/user/current", tries=1)
                break
            except Exception:
                time.sleep(10)
        else:
            die(P, "Confluence did not answer within 10 min of its restart")
        say(P, "rebuilding the search index")
        self.h.request("POST", "/rest/prototype/1/index/reindex", headers={"X-Atlassian-Token": "no-check"},
                       ok=(200, 202, 204))
        while True:
            time.sleep(10)
            st = self.h.get("/rest/prototype/1/index/reindex")
            if not isinstance(st, dict):   # "Reindex status not available." before a job exists
                continue
            say(P, f"reindex {st.get('percentageComplete', '?')}%")
            if st.get("finished") or st.get("percentageComplete") == 100:
                return

    def report(self):
        tot = 0
        for key, _, _ in SPACES:
            n = self.h.get(f"/rest/api/content/search?cql={q(f'space = {key} and type = page')}&limit=1")
            n = n.get("totalSize", n.get("size"))
            tot += n or 0
            say(P, f"{key}: {n} pages")
        say(P, f"done: {', '.join(f'{v} {k}' for k, v in self.made.items())} made by this run")


def sql_str(s):
    return "'" + s.replace("'", "''") + "'"


def main(state, count, workers):
    Confluence(state, count, workers).run()
