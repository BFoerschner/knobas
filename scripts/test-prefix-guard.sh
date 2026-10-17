#!/usr/bin/env bash
# Guards the `knobas-test-` temp prefix the knobas-db test reaper owns (#803).
#
# `reap_abandoned` (crates/knobas-db/src/test_util.rs) deletes any
# `$TMPDIR/knobas-test-*` directory that has no `.lock`. A script that picked
# that prefix for an unrelated temp dir would have it deleted under it. So the
# prefix may not appear in `scripts/` or `testenv/` as code. A comment that says
# "not `knobas-test-...`" is harmless and passes: only non-comment lines count.
#
# Usage: test-prefix-guard.sh [root]   (root defaults to the repo root)
set -euo pipefail

root=${1:-$(cd "$(dirname "$0")/.." && pwd)}
self=$(basename "$0")

hits=$(grep -rIn --exclude="$self" --exclude="test-prefix-guard-test.sh" 'knobas-test-' \
    "$root/scripts" "$root/testenv" 2>/dev/null \
    | grep -Ev '^[^:]+:[0-9]+:[[:space:]]*#' || true)

if [ -n "$hits" ]; then
    echo "FAIL: 'knobas-test-' is the prefix the test reaper deletes (crates/knobas-db/src/test_util.rs); pick another name for a temp dir:" >&2
    echo "$hits" >&2
    exit 1
fi
echo "test-prefix-guard: ok"
