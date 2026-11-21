"""Confluence at the worst a real team has: space XTRM (*Extreme shapes*).

- *A page with everything*: a 2,000-row table, 100 expand macros and 300 comments (one 300 KB);
- *A 5 MB page*: a storage body of 5 MB (headings, paragraphs, tables, code);
- 3,000 pages at one level, under the space's home page.
"""

from __future__ import annotations

from html import escape

from common import Http, HttpError, die, paragraph, parallel, q, rng, say, sentence

P = "confluence"
KEY = "XTRM"
NAME = "Extreme shapes"
EVERYTHING = "A page with everything"
HUGE = "A 5 MB page"
ROWS = 2000
EXPANDS = 100
COMMENTS = 300
BIG_COMMENT_AT = 150
BIG_COMMENT_BYTES = 300_000
HUGE_BYTES = 5_000_000
SIBLINGS = 3000


def everything_body() -> str:
    r = rng(P, "everything")
    out = ["<h1>Two thousand rows</h1>", "<table><tbody>",
           "<tr><th>#</th><th>Service</th><th>Owner</th><th>Status</th><th>Notes</th></tr>"]
    for k in range(ROWS):
        out.append(f"<tr><td>{k + 1}</td><td>{escape(sentence(r, 1, 3).rstrip('.!?'))}</td><td>team-{k % 37}</td>"
                   f"<td>{['green', 'amber', 'red'][k % 3]}</td><td>{escape(sentence(r, 4, 14))}</td></tr>")
    out.append("</tbody></table>")
    out.append("<h1>A hundred expands</h1>")
    for k in range(EXPANDS):
        out.append(f'<ac:structured-macro ac:name="expand"><ac:parameter ac:name="title">Expand {k + 1}: '
                   f"{escape(sentence(r, 2, 6).rstrip('.!?'))}</ac:parameter><ac:rich-text-body>"
                   f"<p>{escape(paragraph(r, 2, 5))}</p><table><tbody>"
                   + "".join(f"<tr><td>{j}</td><td>{escape(sentence(r, 3, 8))}</td></tr>" for j in range(8))
                   + "</tbody></table></ac:rich-text-body></ac:structured-macro>")
    return "".join(out)


def huge_body() -> str:
    r = rng(P, "huge")
    out, size, k = [], 0, 0
    while size < HUGE_BYTES:
        k += 1
        part = (f"<h2>Section {k}</h2><p>{escape(paragraph(r, 4, 9))}</p>"
                f"<ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA["
                + "\n".join(f"line {j}: {sentence(r, 4, 12)}" for j in range(20))
                + "]]></ac:plain-text-body></ac:structured-macro>"
                + "<table><tbody>" + "".join(f"<tr><td>{j}</td><td>{escape(sentence(r, 3, 9))}</td></tr>" for j in range(10))
                + "</tbody></table>")
        out.append(part)
        size += len(part)
    return "".join(out)


class Confluence:
    def __init__(self, state, workers):
        c = state.get("confluence") or die(P, "seed-state.json has no confluence block: just live-up confluence first")
        self.h = Http(c["url"], user=c["user"], password=c["password"], timeout=600)
        self.workers = workers

    def space(self):
        try:
            sp = self.h.get(f"/rest/api/space/{KEY}?expand=homepage")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/rest/api/space", {"key": KEY, "name": NAME, "description": {"plain": {
                "value": "Made by testenv/seed-extremes.sh. Not fixture content.", "representation": "plain"}}})
            say(P, f"created space {KEY}")
            sp = self.h.get(f"/rest/api/space/{KEY}?expand=homepage")
        return sp["homepage"]["id"]

    def titles(self):
        out, start = {}, 0
        while True:
            page = self.h.get(f"/rest/api/content?spaceKey={KEY}&type=page&status=current&limit=200&start={start}")
            for p in page["results"]:
                out[p["title"]] = p["id"]
            if page["size"] < 200:
                return out
            start += page["size"]

    def page(self, have, home, title, body):
        if title in have:
            return have[title]
        res = self.h.post("/rest/api/content", {"type": "page", "title": title, "space": {"key": KEY},
                                                "ancestors": [{"id": home}],
                                                "body": {"storage": {"value": body, "representation": "storage"}}})
        say(P, f"created {title!r} ({len(body):,} bytes)")
        return res["id"]

    def run(self):
        home = self.space()
        have = self.titles()
        pid = self.page(have, home, EVERYTHING, everything_body())
        self.page(have, home, HUGE, huge_body())
        self.comments(pid)
        jobs = [k for k in range(1, SIBLINGS + 1) if self.sibling(k) not in have]

        def make(k):
            r = rng(P, "sibling", k)
            try:
                self.h.post("/rest/api/content", {"type": "page", "title": self.sibling(k), "space": {"key": KEY},
                                                  "ancestors": [{"id": home}],
                                                  "body": {"storage": {"value": f"<p>{escape(paragraph(r, 1, 3))}</p>",
                                                                       "representation": "storage"}}})
            except HttpError as e:
                if e.status != 400 or "already exists" not in e.body:
                    raise

        parallel(P, "pages at one level", jobs, make, self.workers, 250)
        say(P, f"{KEY}: done, {len(self.titles())} pages")

    @staticmethod
    def sibling(k):
        return f"Sibling {k:04d}: {sentence(rng(P, 'sibling-title', k), 2, 6).rstrip('.!?')}"

    def comments(self, pid):
        have = self.h.get(f"/rest/api/content/{pid}/child/comment?limit=1")
        n = have.get("totalSize", have.get("size", 0))
        if n == 0 or "totalSize" not in have:
            # Older servers give no total: count them.
            n, start = 0, 0
            while True:
                page = self.h.get(f"/rest/api/content/{pid}/child/comment?limit=100&start={start}")
                n += page["size"]
                if page["size"] < 100:
                    break
                start += page["size"]

        def body(k):
            r = rng(P, "comment", k)
            if k == BIG_COMMENT_AT:
                text, size = [], 0
                while size < BIG_COMMENT_BYTES:
                    s = paragraph(r, 4, 8)
                    text.append(f"<p>{escape(s)}</p>")
                    size += len(s) + 7
                return "".join(text)
            return f"<p>{escape(paragraph(r, 1, 4))}</p>"

        for k in range(n, COMMENTS):
            self.h.post("/rest/api/content", {"type": "comment", "container": {"id": pid, "type": "page"},
                                              "body": {"storage": {"value": f"<p>[{k + 1}]</p>" + body(k),
                                                                   "representation": "storage"}}})
            if (k + 1) % 50 == 0:
                say(P, f"comments {k + 1}/{COMMENTS}")

    def remove(self):
        try:
            self.h.get(f"/rest/api/space/{KEY}")
        except HttpError as e:
            if e.status == 404:
                say(P, f"no space {KEY}")
                return
            raise
        task = self.h.request("DELETE", f"/rest/api/space/{KEY}", ok=(200, 202, 204))[1]
        import time
        for _ in range(360):
            try:
                self.h.get(f"/rest/api/space/{KEY}")
            except HttpError as e:
                if e.status == 404:
                    say(P, f"deleted space {KEY}")
                    return
                raise
            time.sleep(5)
        die(P, f"space {KEY} still there 30 minutes after its delete ({task})")


def seed(state, workers, parts=None):
    Confluence(state, workers).run()


def remove(state):
    Confluence(state, 1).remove()
