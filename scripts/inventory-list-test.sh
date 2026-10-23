#!/usr/bin/env bash
# `scripts/inventory-list.sh`'s own test (#1696), run by `just witness-unit`:
# stub test binaries stand in for cargo's. A binary whose `--list` exits 1 must
# fail the run naming that binary, never read as deleted tests; the good
# binaries beside it are the control that the script lists at all.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
list="$here/inventory-list.sh"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/inventory-list-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

printf '#!/usr/bin/env bash\nprintf "a::one: test\\nb::two: test\\n2 tests, 0 benchmarks\\n"\n' > "$scratch/good"
printf '#!/usr/bin/env bash\necho "dyld: Library not loaded: libgone" >&2\nexit 1\n' > "$scratch/crash"
printf '#!/usr/bin/env bash\necho "not a test binary"\nexit 9\n' > "$scratch/server"
chmod +x "$scratch/good" "$scratch/crash" "$scratch/server"

t=$'\t'
failures=0
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }

# Control: only good binaries -> exit 0 and the tests, keyed by package/target.
rc=0
out=$(printf 'test\tpkg-a\tlib/a\t%s\t/\nserver\tpkg-s\tbin/s\t%s\t/\n' "$scratch/good" "$scratch/server" \
    | "$list" 2>"$scratch/err1") || rc=$?
[ "$rc" -eq 0 ] || fail "good binaries: exit $rc"
want="pkg-a${t}lib/a${t}a::one
pkg-a${t}lib/a${t}b::two"
[ "$out" = "$want" ] || fail "good binaries: listed '$out'"

# A crashing binary: red, named, its stderr shown, the good one still listed.
rc=0
out=$(printf 'test\tpkg-a\tlib/a\t%s\t/\ntest\tpkg-c\ttests/crashy\t%s\t/\n' "$scratch/good" "$scratch/crash" \
    | "$list" 2>"$scratch/err2") || rc=$?
[ "$rc" -ne 0 ] || fail "crashing binary: exit 0"
grep -q 'pkg-c tests/crashy' "$scratch/err2" || fail "crashing binary: not named in stderr"
grep -q 'libgone' "$scratch/err2" || fail "crashing binary: its stderr not shown"
grep -q 'inventory-update' "$scratch/err2" || fail "crashing binary: no warning against inventory-update"
[ "$(grep -c "pkg-a${t}lib/a${t}" <<< "$out")" -eq 2 ] || fail "crashing binary: the good binary was not listed"

[ "$failures" -eq 0 ] || exit 1
echo "inventory-list-test: ok"
