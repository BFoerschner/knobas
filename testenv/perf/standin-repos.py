#!/usr/bin/env python3
"""The scale estate's Gitea repositories built again on this machine, for `standin-forge.mjs --repos` (the inner-6x
pass, 2026-11-15; README, *The inner pages on the Mac*).

The scale seed (`testenv/scale/gitea.py`) builds each repository's history with `git fast-import` from a seeded random
source, so the same count and the same people's addresses give the same commits, sha for sha, as the live Gitea holds
and the saved database mirrors. Each person's address is `<first name>@tidewater.test`, as the live Gitea's accounts
gave them. With `--port` and `--pr`, each pull request's record is read from a windowless backend on that saved
database (`get_entity`'s payload, Gitea's own `pulls/{n}`) into `--pulls`, which the stand-in answers from.

  python3 -I testenv/perf/standin-repos.py --out <dir> [--count 10000] scale-commerce/cart-service ...
      [--port 5472 --pulls pulls.json --pr scale-commerce/cart-service#124 ...]
"""
import argparse
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="where `<owner>-<name>.git` goes, one bare repository each")
    ap.add_argument("--count", type=int, default=10000, help="the seed's count (`seed-scale.sh gitea` takes 10,000)")
    ap.add_argument("--port", type=int, help="a windowless backend on the saved database, for --pr")
    ap.add_argument("--pulls", help="the file the pull requests' records go to")
    ap.add_argument("--pr", action="append", default=[], help="owner/name#n, again for each")
    ap.add_argument("repos", nargs="*", help="owner/name, each built")
    a = ap.parse_args()
    scale = HERE.parent / "scale"
    sys.path.insert(0, str(scale))
    os.chdir(scale)
    import gitea  # noqa: E402  the seed's own module, read from its directory

    emails = {who: f"{who.split('.')[0]}@tidewater.test" for who, _ in gitea.PEOPLE}
    want = set(a.repos)
    os.makedirs(a.out, exist_ok=True)
    for repo in gitea.plan(a.count):
        full = f"{repo.org}/{repo.name}"
        if full not in want:
            continue
        want.discard(full)
        stream, _, refs = gitea.History(repo, emails).build()
        target = os.path.join(a.out, f"{repo.org}-{repo.name}.git")
        if os.path.isdir(target):
            print(f"{full}: {target} is there already")
            continue
        subprocess.run(["git", "init", "--quiet", "--bare", target], check=True)
        subprocess.run(["git", "--git-dir", target, "fast-import", "--quiet"], input=stream, check=True)
        print(f"{full}: {target}, {len(refs)} branches besides main")
    if want:
        print(f"not in the estate at {a.count}: {', '.join(sorted(want))}")
        return 1
    if a.pr:
        if a.port is None or a.pulls is None:
            print("--pr needs --port and --pulls")
            return 2
        pulls = {}
        for name in a.pr:
            body = json.dumps({"cmd": "get_entity", "args": {"entityId": f"live-gitea:{name}"}}).encode()
            with urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{a.port}/invoke", data=body)) as answer:
                got = json.load(answer)
            if "ok" not in got:
                print(f"{name}: {got}")
                return 1
            pulls[name] = got["ok"]["payload"]
        Path(a.pulls).write_text(json.dumps(pulls))
        print(f"{len(pulls)} pull requests into {a.pulls}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
