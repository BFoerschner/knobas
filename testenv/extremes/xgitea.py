"""Gitea at the worst a real team has: org `knobas-extremes`, repository `everything`.

- 5,000 branches (`many/b-0001` ..), each one commit off the root;
- 2,000 open pull requests, from `many/b-0001` .. `many/b-2000` into `main`;
- one pull request (`wide` into `main`) with 3,000 changed files over 500 commits (one file a single 1 MB line)
  and 300 comments, one of them 300 KB.

History is built with `git fast-import` at fixed dates, so every run pushes the same commits.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile

from common import Http, HttpError, basic, chunks, die, paragraph, parallel, q, rng, say, sentence

P = "gitea"
ORG = "knobas-extremes"
REPO = "everything"
BRANCHES = 5000
OPEN_PRS = 2000
WIDE_COMMITS = 500
FILES_PER_COMMIT = 6
COMMENTS = 300
BIG_COMMENT_AT = 150
BIG_COMMENT_BYTES = 300_000
EPOCH = 1761984000  # 2025-11-01T08:00:00Z: every commit is dated from it, so a rerun makes the same SHAs


def blob(data: bytes) -> bytes:
    return b"data %d\n" % len(data) + data + b"\n"


def commit(ref: str, mark: int, t: int, msg: str, parent: int | None, files: list[tuple[str, bytes]]) -> bytes:
    who = b"Extreme Seed <extremes@knobas.test> %d +0000" % t
    out = [b"commit " + ref.encode() + b"\n", b"mark :%d\n" % mark, b"author " + who + b"\n",
           b"committer " + who + b"\n", blob(msg.encode())]
    if parent is not None:
        out.append(b"from :%d\n" % parent)
    for path, data in files:
        out.append(b"M 100644 inline " + path.encode() + b"\n" + blob(data))
    return b"".join(out)


def history() -> bytes:
    """main at its root; `wide` 500 commits on top of it; 5,000 branches of one commit each."""
    r = rng(P, "history")
    parts = [commit("refs/heads/main", 1, EPOCH, "Start the repository\n", None,
                    [("README.md", b"# everything\n\nThe extremes seed's repository (testenv/seed-extremes.sh).\n")])]
    mark = 2
    prev = 1
    n = 0
    for k in range(WIDE_COMMITS):
        files = []
        for j in range(FILES_PER_COMMIT):
            if k == 0 and j == 0:
                # One file of a single 1 MB line, as a minified bundle or a lockfile would be.
                files.append(("data/one-line.json", b'{"rows":[' + b",".join(b'"%07d"' % i for i in range(104_000)) + b"]}\n"))
            else:
                files.append((f"area-{k % 40:02d}/module-{k:03d}/file-{j}.txt",
                              ("\n".join(sentence(r) for _ in range(r.randint(3, 30))) + "\n").encode()))
            n += 1
        parts.append(commit("refs/heads/wide", mark, EPOCH + 60 * (k + 1), f"Wide change {k + 1} of {WIDE_COMMITS}\n\n{sentence(r)}\n",
                            prev, files))
        prev = mark
        mark += 1
    assert n == WIDE_COMMITS * FILES_PER_COMMIT
    for b in range(1, BRANCHES + 1):
        parts.append(commit(f"refs/heads/many/b-{b:04d}", mark, EPOCH + 86400 + b, f"Branch {b:04d}\n", 1,
                            [(f"branches/b-{b:04d}.txt", f"branch {b}\n".encode())]))
        mark += 1
    return b"".join(parts)


def git(args):
    p = subprocess.run(["git"] + args, capture_output=True)
    if p.returncode:
        raise RuntimeError(f"git {' '.join(a for a in args if not a.startswith('http.extraHeader'))}: {p.stderr.decode()[-800:]}")
    return p.stdout.decode()


class Forge:
    def __init__(self, state, workers, parts=None):
        self.parts = set(parts or ["branches", "prs", "comments"])
        g = state.get("gitea") or die(P, "no gitea block in seed-state.json: just live-up gitea first")
        self.url = g["url"].rstrip("/")
        self.h = Http(self.url, user="knobas", password="knobas-dev")
        self.workers = workers
        self.base = f"/api/v1/repos/{ORG}/{REPO}"

    def paged(self, path, limit=50):
        out, page = [], 1
        sep = "&" if "?" in path else "?"
        while True:
            got = self.h.get(f"{path}{sep}limit={limit}&page={page}")
            out += got
            if len(got) < limit:
                return out
            page += 1

    def run(self):
        try:
            self.h.get(f"/api/v1/orgs/{ORG}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/api/v1/orgs", {"username": ORG, "full_name": "Extreme shapes", "visibility": "public",
                                         "description": "Made by testenv/seed-extremes.sh. Not fixture content."})
            say(P, f"created org {ORG}")
        try:
            self.h.get(self.base)
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post(f"/api/v1/orgs/{ORG}/repos", {"name": REPO, "default_branch": "main", "auto_init": False,
                                                       "description": "The worst shapes a real team has."})
            say(P, f"created {ORG}/{REPO}")
        self.push()
        self.wait_branches()
        if "prs" in self.parts:
            self.pulls()
        if "comments" in self.parts:
            self.comments()
            self.report()

    def push(self):
        have = {b["name"] for b in self.paged(f"{self.base}/branches")} if self.has_branches() else set()
        if len(have) >= BRANCHES + 2:
            return
        tmp = tempfile.mkdtemp(prefix="knobas-extremes-git-")
        try:
            gitdir = os.path.join(tmp, "everything.git")
            git(["init", "--quiet", "--bare", gitdir])
            p = subprocess.run(["git", "--git-dir", gitdir, "fast-import", "--quiet"], input=history(), capture_output=True)
            if p.returncode:
                die(P, f"fast-import: {p.stderr.decode()[-800:]}")
            remote = f"{self.url}/{ORG}/{REPO}.git"
            auth = ["-c", f"http.extraHeader=Authorization: {basic('knobas', 'knobas-dev')}"]
            refs = ["refs/heads/main:refs/heads/main", "refs/heads/wide:refs/heads/wide"] + [
                f"refs/heads/many/b-{b:04d}:refs/heads/many/b-{b:04d}" for b in range(1, BRANCHES + 1)
                if f"many/b-{b:04d}" not in have]
            for k, c in enumerate(chunks(refs, 250)):
                git(auth + ["--git-dir", gitdir, "push", "--quiet", "--force", remote] + c)
                say(P, f"pushed {min((k + 1) * 250, len(refs))}/{len(refs)} refs")
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def has_branches(self):
        return bool(self.h.get(f"{self.base}/branches?limit=1"))

    def wait_branches(self):
        import time
        for _ in range(120):
            # The branch list is written by the forge's post-receive, after the push answers.
            n = len(self.paged(f"{self.base}/branches"))
            if n >= BRANCHES + 2:
                return
            say(P, f"waiting for the forge to list the branches: {n}/{BRANCHES + 2}")
            time.sleep(10)
        die(P, "the forge never listed every branch")

    def pulls(self):
        open_ = self.paged(f"{self.base}/pulls?state=open")
        heads = {pr["head"]["ref"] for pr in open_}
        jobs = []
        if "wide" not in heads:
            jobs.append(("wide", f"Everything at once: {WIDE_COMMITS * FILES_PER_COMMIT:,} files over {WIDE_COMMITS} commits"))
        for b in range(1, OPEN_PRS + 1):
            name = f"many/b-{b:04d}"
            if name not in heads:
                jobs.append((name, f"Extreme pull request {b:04d}: {sentence(rng(P, 'pr', b), 4, 10).rstrip('.!?')}"))

        def open_pr(job):
            head, title = job
            body = paragraph(rng(P, "body", head), 1, 3)
            self.h.post(f"{self.base}/pulls", {"head": head, "base": "main", "title": title, "body": body})

        # The wide one first, alone: it is the one the page measures, and the others queue behind its diff check.
        if jobs and jobs[0][0] == "wide":
            open_pr(jobs.pop(0))
        parallel(P, "pull requests", jobs, open_pr, self.workers, 200)

    def wide(self):
        for pr in self.paged(f"{self.base}/pulls?state=open"):
            if pr["head"]["ref"] == "wide":
                return pr["number"]
        die(P, "the wide pull request is not open")

    def comments(self):
        n = self.wide()
        have = len(self.paged(f"{self.base}/issues/{n}/comments"))
        if have >= COMMENTS:
            return

        def body(k):
            r = rng(P, "comment", k)
            if k == BIG_COMMENT_AT:
                text = []
                size = 0
                while size < BIG_COMMENT_BYTES:
                    s = paragraph(r, 4, 8)
                    text.append(s)
                    size += len(s) + 2
                return "A long one, as a pasted log or a design discussion is.\n\n" + "\n\n".join(text)
            return paragraph(r, 1, 4)

        # In order, so the thread reads as written; one at a time keeps the order the forge dates them by.
        for k in range(have, COMMENTS):
            self.h.post(f"{self.base}/issues/{n}/comments", {"body": f"[{k + 1}] " + body(k)})
            if (k + 1) % 50 == 0:
                say(P, f"comments {k + 1}/{COMMENTS}")

    def report(self):
        n = self.wide()
        say(P, f"{ORG}/{REPO}: pull request #{n} is the wide one")

    def remove(self):
        try:
            self.h.get(f"/api/v1/orgs/{ORG}")
        except HttpError as e:
            if e.status == 404:
                say(P, f"no org {ORG}: nothing to remove")
                return
            raise
        self.h.request("DELETE", self.base, ok=(204, 404))
        self.h.request("DELETE", f"/api/v1/orgs/{ORG}", ok=(204, 404))
        say(P, f"deleted {ORG}/{REPO} and the org")


def seed(state, workers, parts=None):
    Forge(state, workers, parts).run()


def remove(state):
    Forge(state, 1).remove()
