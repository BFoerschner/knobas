#!/usr/bin/env bash
# `scripts/ipc-mirror-check.sh`'s own test (#1694), run by `just witness-unit`.
# In a scratch git repo a stub generator stands in for the `ipc_mirror` test.
# The reference is the tree before the run: a mirror regenerated and left
# uncommitted (what `pre-gate` does) is green; a committed stale mirror, a new
# file and a failing generator are red. The first case is the control.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
check="$here/ipc-mirror-check.sh"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/ipc-mirror-check-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
g() { git -C "$scratch" -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
mkdir -p "$scratch/gen"
echo current > "$scratch/gen/a.ts"
g add -A
g commit -q -m base

# The stub generator writes what the Rust types "now produce": a.ts = current.
cat > "$scratch/generate" <<'GEN'
#!/usr/bin/env bash
echo current > gen/a.ts
[ -z "${EXTRA:-}" ] || echo x > "gen/$EXTRA"
[ -z "${BOOM:-}" ] || exit 3
GEN
chmod +x "$scratch/generate"

run() { env -C "$scratch" "$check" gen ./generate >/dev/null 2>&1; }
failures=0
expect() { # expect <green|red> <label>
    local rc=0
    run || rc=$?
    if { [ "$1" = green ] && [ "$rc" -ne 0 ]; } || { [ "$1" = red ] && [ "$rc" -eq 0 ]; }; then
        echo "FAIL: $2 (wanted $1, exit $rc)" >&2
        failures=$((failures + 1))
    fi
}

expect green "a current committed mirror"

echo old > "$scratch/gen/a.ts"; g commit -qam stale
expect red "a committed stale mirror with no pre-gate"
# The run above rewrote it: this is the tree pre-gate leaves, uncommitted.
expect green "a mirror regenerated and not yet committed"
g commit -qam fixed

EXTRA=new.ts expect red "a file the generator adds"
rm -f "$scratch/gen/new.ts"
BOOM=1 expect red "a failing generator"

[ "$failures" -eq 0 ] || exit 1
echo "ipc-mirror-check-test: ok"
