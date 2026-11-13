"""The extremes seed's entry point; `testenv/seed-extremes.sh` runs it. Usage there.

Each product's extremes live under a scope of their own (Jira project XTRM, Confluence space XTRM, Gitea org
knobas-extremes, TeamCity project Extremes), never in the Tidewater seed's or the scale estate's, so the live
suites and a profile that syncs those see no change. `--remove` deletes the scope again.
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
sys.path.insert(0, os.path.join(os.path.dirname(here), "scale"))

PRODUCTS = ("jira", "confluence", "gitea", "teamcity")


def main():
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    ap = argparse.ArgumentParser(prog="seed-extremes.sh")
    ap.add_argument("product", choices=PRODUCTS)
    ap.add_argument("--state", required=True)
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--remove", action="store_true", help="delete the product's extremes scope")
    a = ap.parse_args()
    with open(a.state) as f:
        state = json.load(f)
    if a.product == "jira":
        import xjira as mod
    elif a.product == "confluence":
        import xconfluence as mod
    elif a.product == "gitea":
        import xgitea as mod
    else:
        import xteamcity as mod
    t0 = time.time()
    if a.remove:
        mod.remove(state)
    else:
        mod.seed(state, a.workers)
    print(f"seed-extremes {a.product}: {'removed' if a.remove else 'finished'} in {time.time() - t0:.0f} s", flush=True)


if __name__ == "__main__":
    main()
