"""Jira at the worst a real team has: project XTRM (*Extreme shapes*).

- XTRM-1, a ticket with everything: 500 comments (one at Jira's 32,767-character limit), 200 sub-tasks
  (XTRM-2 .. XTRM-201), 100 links (to XTRM-202 .. XTRM-301), 200 attachments and 100 labels;
- XTRM-302 .. XTRM-5301, one ticket per person: 5,000 people (`xtrm-person-0001` ..) and 3,000 labels.

Jira's ten-user licence stops 5,000 people having application access, and Jira assigns no one without it, so the
people are made without it and set as the tickets' assignees in Jira's own database, then those tickets reindexed.
"""

from __future__ import annotations

import subprocess
import time

from common import Http, HttpError, chunks, die, paragraph, parallel, rng, say, sentence

P = "jira"
KEY = "XTRM"
NAME = "Extreme shapes"
PROPERTY = "knobas-extremes-seed"
TEMPLATE = "com.pyxis.greenhopper.jira:basic-software-development-template"
SUBTASKS = 200
LINKED = 100
PEOPLE = 5000
LABELS = 3000
COMMENTS = 500
ATTACHMENTS = 200
TEXT_LIMIT = 32767
LINK_TYPES = ["Blocks", "Relates", "Duplicate", "Cloners"]
PERSON_PASS = "xtrm-person-dev"

FIRST_SUB, FIRST_LINKED, FIRST_PERSON = 2, 2 + SUBTASKS, 2 + SUBTASKS + LINKED
LAST = FIRST_PERSON + PEOPLE - 1


def person(k: int) -> str:
    return f"xtrm-person-{k:04d}"


class Jira:
    def __init__(self, state, workers):
        j = state.get("jira") or die(P, "seed-state.json has no jira block: just live-up jira first")
        self.user, self.password = j["user"], j["password"]
        self.h = Http(j["url"], user=self.user, password=self.password)
        self.workers = workers

    def marker(self):
        try:
            return self.h.get(f"/rest/api/2/project/{KEY}/properties/{PROPERTY}")["value"]
        except HttpError as e:
            if e.status == 404:
                return {}
            raise

    def mark(self, phase):
        m = self.marker()
        m[phase] = True
        self.h.put(f"/rest/api/2/project/{KEY}/properties/{PROPERTY}", m)

    def run(self):
        self.project()
        m = self.marker()
        if not m.get("issues"):
            self.issues()
            self.mark("issues")
        for phase in ("comments", "links", "attachments", "people", "assignees"):
            if not m.get(phase):
                getattr(self, phase)()
                self.mark(phase)
        say(P, f"{KEY}: done, {LAST} tickets")

    def project(self):
        try:
            self.h.get(f"/rest/api/2/project/{KEY}")
            return
        except HttpError as e:
            if e.status != 404:
                raise
        self.h.post("/rest/api/2/project", {
            "key": KEY, "name": NAME, "projectTypeKey": "software", "projectTemplateKey": TEMPLATE,
            "lead": self.user, "assigneeType": "UNASSIGNED",
            "description": "Made by testenv/seed-extremes.sh. Not fixture content."})
        say(P, f"created project {KEY}")

    def max_key(self):
        page = self.h.post("/rest/api/2/search", {"jql": f"project = {KEY} ORDER BY key DESC", "maxResults": 1,
                                                 "fields": ["summary"]})
        return int(page["issues"][0]["key"].split("-")[1]) if page["issues"] else 0

    def fields(self, i):
        f = {"project": {"key": KEY}, "issuetype": {"name": "Task"}}
        r = rng(P, "issue", i)
        if i == 1:
            desc = []
            while sum(len(s) + 2 for s in desc) < TEXT_LIMIT - 2000:
                desc.append(paragraph(r, 3, 6))
            f.update(summary="A ticket with everything: 500 comments, 200 sub-tasks, 100 links, 200 attachments",
                     description="\n\n".join(desc), labels=[f"everything-{k:03d}" for k in range(100)])
        elif i < FIRST_LINKED:
            f.update(issuetype={"name": "Sub-task"}, parent={"key": f"{KEY}-1"},
                     summary=f"Sub-task {i - 1:03d}: {sentence(r, 3, 9).rstrip('.!?')}")
        elif i < FIRST_PERSON:
            f.update(summary=f"Linked {i - FIRST_LINKED + 1:03d}: {sentence(r, 3, 9).rstrip('.!?')}")
        else:
            k = i - FIRST_PERSON
            f.update(summary=f"{person(k + 1)}'s ticket: {sentence(r, 3, 8).rstrip('.!?')}",
                     labels=[f"xl-{k % LABELS:04d}"])
        return f

    def issues(self):
        top = self.max_key()
        todo = list(range(top + 1, LAST + 1))
        say(P, f"{KEY}: creating {KEY}-{top + 1}..{KEY}-{LAST}")
        for b in chunks(todo, 50):
            # A sub-task's parent must exist before its batch.
            res = self.h.post("/rest/api/2/issue/bulk", {"issueUpdates": [{"fields": self.fields(i)} for i in b]})
            if res.get("errors"):
                die(P, f"bulk create refused: {str(res['errors'])[:800]}")
            got = [int(x["key"].split("-")[1]) for x in res["issues"]]
            if got != b:
                die(P, f"bulk create keyed {got[:3]}.. not {b[:3]}..: the counter is not where the plan is; --remove, then seed again")
            if b[-1] % 500 < 50 or b[-1] == LAST:
                say(P, f"tickets {b[-1]}/{LAST}")

    def comments(self):
        have = self.h.get(f"/rest/api/2/issue/{KEY}-1/comment?maxResults=0")["total"]

        def body(k):
            r = rng(P, "comment", k)
            if k == COMMENTS // 2:
                text = []
                while sum(len(s) + 2 for s in text) < TEXT_LIMIT - 1000:
                    text.append(paragraph(r, 4, 8))
                return "\n\n".join(text)
            return paragraph(r, 1, 4)

        # One after another, so the thread is in the order it was written.
        for k in range(have, COMMENTS):
            self.h.post(f"/rest/api/2/issue/{KEY}-1/comment", {"body": f"[{k + 1}] " + body(k)})
            if (k + 1) % 100 == 0:
                say(P, f"comments {k + 1}/{COMMENTS}")

    def links(self):
        got = self.h.get(f"/rest/api/2/issue/{KEY}-1?fields=issuelinks")["fields"]["issuelinks"]
        have = {(x.get("outwardIssue") or x.get("inwardIssue") or {}).get("key") for x in got}
        jobs = [i for i in range(FIRST_LINKED, FIRST_PERSON) if f"{KEY}-{i}" not in have]

        def link(i):
            self.h.post("/rest/api/2/issueLink", {"type": {"name": LINK_TYPES[i % len(LINK_TYPES)]},
                                                  "outwardIssue": {"key": f"{KEY}-1"}, "inwardIssue": {"key": f"{KEY}-{i}"}})

        parallel(P, "links", jobs, link, self.workers, 50)

    def attachments(self):
        from confluence import pdf, png  # the scale seed's file makers
        got = self.h.get(f"/rest/api/2/issue/{KEY}-1?fields=attachment")["fields"]["attachment"]
        have = {a["filename"] for a in got}
        jobs = []
        for k in range(ATTACHMENTS):
            r = rng(P, "attachment", k)
            ext = ["png", "pdf", "txt", "log", "csv"][k % 5]
            name = f"attachment-{k + 1:03d}.{ext}"
            if name in have:
                continue
            if ext == "png":
                data, ctype = png(r), "image/png"
            elif ext == "pdf":
                data, ctype = pdf(sentence(r)), "application/pdf"
            else:
                data, ctype = ("\n".join(sentence(r) for _ in range(r.randint(5, 200))) + "\n").encode(), "text/plain"
            jobs.append((name, ctype, data))

        def post(job):
            name, ctype, data = job
            boundary = "knobasextremes" + name.replace(".", "")
            payload = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
                       f"Content-Type: {ctype}\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
            self.h.request("POST", f"/rest/api/2/issue/{KEY}-1/attachments", raw=payload,
                           headers={"X-Atlassian-Token": "no-check",
                                    "Content-Type": f"multipart/form-data; boundary={boundary}"})

        parallel(P, "attachments", jobs, post, self.workers, 50)

    def people(self):
        def make(k):
            try:
                self.h.post("/rest/api/2/user", {"name": person(k), "password": PERSON_PASS,
                                                 "emailAddress": f"{person(k)}@knobas.test",
                                                 "displayName": f"Person {k:04d} {rng(P, 'name', k).choice(NAMES)}",
                                                 "applicationKeys": []})
            except HttpError as e:
                if e.status != 400 or "already exists" not in e.body:
                    raise

        parallel(P, "people (no application access)", list(range(1, PEOPLE + 1)), make, self.workers, 500)

    def assignees(self):
        sql = f"""
BEGIN;
UPDATE jiraissue j SET assignee = a.user_key
  FROM project p, app_user a
 WHERE j.project = p.id AND p.pkey = '{KEY}' AND j.issuenum >= {FIRST_PERSON}
   AND a.lower_user_name = 'xtrm-person-' || lpad((j.issuenum - {FIRST_PERSON} + 1)::text, 4, '0');
COMMIT;
SELECT j.id FROM jiraissue j JOIN project p ON p.id = j.project WHERE p.pkey = '{KEY}' AND j.issuenum >= {FIRST_PERSON};
"""
        p = subprocess.run(["docker", "exec", "-i", "knobas-jira-db", "psql", "-v", "ON_ERROR_STOP=1", "-qAt", "-U", "jira",
                            "-d", "jira"], input=sql.encode(), capture_output=True)
        if p.returncode:
            die(P, f"psql: {p.stderr.decode()[-1500:]}")
        ids = [x for x in p.stdout.decode().split() if x.isdigit()]
        say(P, f"assigned {len(ids)} tickets in Jira's database; reindexing them")
        for c in chunks(ids, 100):
            self.h.post("/rest/api/2/reindex/issue?" + "&".join(f"issueId={i}" for i in c), ok=(200, 201, 202, 204))
        probe = self.h.get(f"/rest/api/2/issue/{KEY}-{LAST}?fields=assignee")["fields"]["assignee"]
        if not probe or probe["name"] != person(PEOPLE):
            die(P, f"{KEY}-{LAST} reads assignee {probe}: Jira did not take it")
        time.sleep(2)
        n = self.h.post("/rest/api/2/search", {"jql": f"project = {KEY} AND assignee = {person(PEOPLE)}", "maxResults": 0})["total"]
        if n != 1:
            say(P, f"warning: JQL finds {n} tickets for {person(PEOPLE)}; the index may still be catching up")

    def remove(self):
        try:
            self.h.get(f"/rest/api/2/project/{KEY}")
            self.h.request("DELETE", f"/rest/api/2/project/{KEY}", ok=(204, 404), tries=3)
            say(P, f"deleted project {KEY}")
        except HttpError as e:
            if e.status != 404:
                raise
            say(P, f"no project {KEY}")

        def gone(k):
            self.h.request("DELETE", f"/rest/api/2/user?username={person(k)}", ok=(204, 404))

        parallel(P, "people removed", list(range(1, PEOPLE + 1)), gone, 8, 1000)


NAMES = ["Abernathy", "Brandt", "Castellanos", "Dimitrov", "Eriksen", "Fontaine", "Gallagher", "Haddad", "Ivanova",
         "Jablonski", "Kowalczyk", "Lindgren", "Moreau", "Nakamura", "Okafor", "Petrov", "Quintero", "Rasmussen",
         "Schmidt", "Takahashi", "Underwood", "Vasquez", "Whitfield", "Xiong", "Yilmaz", "Zielinski"]


def seed(state, workers):
    Jira(state, workers).run()


def remove(state):
    Jira(state, 1).remove()
