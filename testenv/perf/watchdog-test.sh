#!/usr/bin/env bash
# The probes' watchdog tests (the probe-watchdog pass, 2026-11-18), run by `just witness-unit`: against a stand-in
# Chrome (`test/fake-chrome.mjs`) that leaves a chosen DevTools call unanswered, dies, or never opens its socket, every
# wait in `cdp.mjs` ends, a stalled case is tried once more on a fresh page and then reported failed-to-run, and
# `keys.mjs`, `open.mjs`, `moves.mjs` and `run.mjs` go on and report instead of hanging. About 10 s; no Chrome.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec node --test --test-timeout=90000 "$here/test/"
