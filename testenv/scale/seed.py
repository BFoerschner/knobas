"""The scale seed's entry point; `testenv/seed-scale.sh` runs it. Usage there."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

DEFAULTS = {"jira": 20000, "confluence": 5000, "teamcity": 3800, "gitea": 10000, "forgejo": 10000}


def main():
    # A kill runs the `finally`s (TeamCity's extra agents are removed there).
    import signal
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    ap = argparse.ArgumentParser(prog="seed-scale.sh")
    ap.add_argument("product", choices=sorted(DEFAULTS))
    ap.add_argument("--count", type=int)
    ap.add_argument("--workers", type=int, default=12)
    ap.add_argument("--agents", type=int, default=3, help="TeamCity: build agents for the run (the licence allows 3)")
    ap.add_argument("--state", required=True)
    ap.add_argument("--plan-only", action="store_true", help="print what the plan holds; touch no server")
    a = ap.parse_args()
    count = a.count or DEFAULTS[a.product]
    with open(a.state) as f:
        state = json.load(f)
    t0 = time.time()
    if a.product == "jira":
        import jira as mod
    elif a.product == "confluence":
        import confluence as mod
    elif a.product == "teamcity":
        import teamcity as mod
    else:
        import gitea as mod
    if a.plan_only:
        mod.describe(count)
        return
    if a.product in ("gitea", "forgejo"):
        mod.main(state, count, a.workers, a.product)
    elif a.product == "teamcity":
        mod.main(state, count, a.workers, a.agents)
    else:
        mod.main(state, count, a.workers)
    print(f"seed-scale {a.product}: finished in {time.time() - t0:.0f} s", flush=True)


if __name__ == "__main__":
    main()
