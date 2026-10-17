#!/usr/bin/env bash
# The test-prefix guard's own test (#803), run by `just witness-unit`: a clean
# tree passes, a comment mention passes, a code use fails in scripts/ and in
# testenv/. The planted-hit cases are the control: without them a guard that
# matches nothing would pass.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
guard=$here/test-prefix-guard.sh

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-prefix-guard-check.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/scripts" "$scratch/testenv"

failures=0
expect() { # expect <pass|fail> <label>
    local want=$1 label=$2 got=pass
    "$guard" "$scratch" >/dev/null 2>&1 || got=fail
    if [ "$got" != "$want" ]; then echo "FAIL: $label (wanted $want, got $got)" >&2; failures=$((failures + 1)); fi
}

expect pass "empty tree"
printf '# not knobas-test-... (reaper prefix)\n' > "$scratch/scripts/a.sh"
expect pass "comment mention"
# shellcheck disable=SC2016  # the planted line is literal script text
printf 'd=$(mktemp -d "$TMPDIR/knobas-test-x.XXXX")\n' > "$scratch/scripts/b.sh"
expect fail "planted hit in scripts/"
rm "$scratch/scripts/b.sh"
expect pass "hit removed"
printf 'mkdir /tmp/knobas-test-seed\n' > "$scratch/testenv/c.sh"
expect fail "planted hit in testenv/"

[ "$failures" -eq 0 ] && echo "test-prefix-guard-test: ok"
exit "$failures"
