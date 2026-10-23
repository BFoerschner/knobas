#!/usr/bin/env bash
# The enumeration half of `just _inventory-write` (#1696). Reads
# `_test-executables` lines on stdin and prints `<package>\t<target>\t<test>`
# for every test each `test` binary lists.
#
# A binary whose `--list` exits non-zero is named on stderr and the script
# exits 1 once every binary has been tried. Without the check its tests simply
# vanished from the actual set, `just inventory` printed them as removed, and
# its message steered the reader to `inventory-update`, which would commit a
# crash as a reviewed removal.
set -uo pipefail

failed=0
while IFS=$'\t' read -r kind pkg target exe _; do
    [ "$kind" = test ] || continue
    err=$(mktemp "${TMPDIR:-/tmp}/knobas-inventory-list.XXXXXX")
    rc=0
    out=$("$exe" --list 2>"$err") || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "error: the test binary $pkg $target ($exe) failed to --list (exit $rc)." >&2
        echo "  This is a crash, not removed tests: do not run 'just inventory-update'." >&2
        sed 's/^/  | /' "$err" | head -20 >&2
        failed=1
    else
        sed -n 's/: test$//p' <<< "$out" | sed "s|^|${pkg}"$'\t'"${target}"$'\t|'
    fi
    rm -f "$err"
done
exit "$failed"
