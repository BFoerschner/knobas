#!/usr/bin/env python3
"""Viewer for every pr-assets/* branch: its tickets, its PRs, its screenshots.

  python3 build.py serve [port] [--no-open]   live: every load re-lists pr-assets/*, so new branches appear
  python3 build.py [out.html]                 one static snapshot

Needs `gh` logged in. Repo BFoerschner/knobas.
"""
import json, re, subprocess, sys, threading, time, webbrowser
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

REPO = "BFoerschner/knobas"
HERE = Path(__file__).resolve().parent
ISSUE_FIELDS = "number,title,state,url,body,labels,comments,closedByPullRequestsReferences"
PR_FIELDS = "number,title,state,url,body,headRefName,isDraft,mergedAt,closingIssuesReferences"


def gh(*args):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)}: {r.stderr.strip()}")
    return json.loads(r.stdout) if r.stdout.strip() else None


class Store:
    """Fetches each ticket and PR once per gather; threads share it."""

    def __init__(self):
        self.issues, self.prs, self.lock = {}, {}, threading.Lock()

    def _get(self, table, key, fetch):
        with self.lock:
            if key in table:
                return table[key]
        val = fetch()
        with self.lock:
            table[key] = val
        return val

    def issue(self, n):
        def fetch():
            try:
                return gh("issue", "view", str(n), "-R", REPO, "--json", ISSUE_FIELDS)
            except RuntimeError:
                return None  # a PR number, or gone
        return self._get(self.issues, n, fetch)

    def pr(self, n):
        return self._get(self.prs, n, lambda: gh("pr", "view", str(n), "-R", REPO, "--json", PR_FIELDS))


def branch(ref, recent_prs, store):
    name = ref["ref"].removeprefix("refs/heads/pr-assets/")
    sha = ref["object"]["sha"]
    commit = gh("api", f"repos/{REPO}/git/commits/{sha}")
    tree = gh("api", f"repos/{REPO}/git/trees/{sha}?recursive=1")
    files = sorted(t["path"] for t in tree["tree"] if t["type"] == "blob")
    msg = commit["message"].strip()

    tickets = []
    for n in re.findall(r"issue-(\d+)", name) + re.findall(r"#(\d+)", msg):
        if int(n) not in tickets:
            tickets.append(int(n))

    # PRs: the branch's own (same head name), any PR body embedding this commit, and whatever closes its tickets.
    own = {p["number"] for p in recent_prs
           if p["headRefName"] == name or p["headRefName"].endswith("/" + name) or sha in (p["body"] or "")}
    prs = sorted(own)
    for t in list(tickets):
        for ref_pr in (store.issue(t) or {}).get("closedByPullRequestsReferences", []):
            if ref_pr["number"] not in prs:
                prs.append(ref_pr["number"])
    for p in prs:
        info = store.pr(p)
        if p in own:  # the branch's own PR (same head, or embeds this commit): its tickets are this branch's
            for c in info.get("closingIssuesReferences", []):
                if c["number"] not in tickets:
                    tickets.append(c["number"])
                    store.issue(c["number"])
    tickets = [t for t in tickets if store.issue(t)]

    return {
        "name": name,
        "sha": sha,
        "date": commit["committer"]["date"],
        "message": msg,
        "files": [{"path": f, "url": f"https://raw.githubusercontent.com/{REPO}/{sha}/{f}"} for f in files],
        "tickets": tickets,
        "prs": prs,
    }


def gather():
    store = Store()
    refs = gh("api", f"repos/{REPO}/git/matching-refs/heads/pr-assets/") or []
    recent = gh("pr", "list", "-R", REPO, "--state", "all", "--limit", "300", "--json", "number,headRefName,body")
    with ThreadPoolExecutor(8) as ex:
        branches = list(ex.map(lambda r: branch(r, recent, store), refs))
    branches.sort(key=lambda b: b["date"], reverse=True)
    return {
        "repo": REPO,
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "branches": branches,
        "issues": {n: v for n, v in store.issues.items() if v},
        "prs": store.prs,
    }


_cache = {"at": 0.0, "data": None}
_cache_lock = threading.Lock()


def cached(force=False, ttl=60):
    with _cache_lock:
        if force or _cache["data"] is None or time.time() - _cache["at"] > ttl:
            _cache["data"] = gather()
            _cache["at"] = time.time()
        return _cache["data"]


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path, _, query = self.path.partition("?")
        if path == "/":
            self._send(200, (HERE / "template.html").read_bytes(), "text/html; charset=utf-8")
        elif path == "/data.json":
            try:
                self._send(200, json.dumps(cached(force="refresh" in query)).encode(), "application/json")
            except Exception as e:  # gh failed: the page shows the message
                self._send(502, str(e).encode(), "text/plain; charset=utf-8")
        else:
            self._send(404, b"not found", "text/plain")

    def log_message(self, *a):
        pass


def main():
    args = sys.argv[1:]
    if args[:1] == ["serve"]:
        rest = [a for a in args[1:] if a != "--no-open"]
        port = int(rest[0]) if rest else 8765
        url = f"http://127.0.0.1:{port}/"
        srv = ThreadingHTTPServer(("127.0.0.1", port), Handler)
        print(f"pr-assets viewer on {url}  (Ctrl-C to stop)", flush=True)
        if "--no-open" not in args:
            webbrowser.open(url)
        srv.serve_forever()
        return
    out = Path(args[0]) if args else HERE / "pr-assets.html"
    data = gather()
    payload = json.dumps(data).replace("</", "<\\/")
    out.write_text((HERE / "template.html").read_text().replace("/*__DATA__*/null", payload))
    print(f"{out}  ({len(data['branches'])} branches, {len(data['issues'])} tickets, {len(data['prs'])} PRs)")


if __name__ == "__main__":
    main()
