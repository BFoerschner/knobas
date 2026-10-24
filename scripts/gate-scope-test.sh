#!/usr/bin/env bash
# `scripts/gate-scope.sh`'s own test (#1543), run by `just witness-unit`: in a
# scratch repo, a head that changes only an app file a testenv script pins (or
# one a variable expands to, or one under a directory the script names bare) answers `full`; an unpinned app file (also one
# under a directory a variable holds and the script walks, #1577) and a
# prose file answer `front`. The unpinned case is the control: without it a
# script that answers `full` for everything would pass.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scope=$here/gate-scope.sh

scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-scope-check.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
# No excludes file: the machine's global one ignores `.claude/*`, which the repo tracks.
g() { git -C "$scratch" -c core.excludesFile=/dev/null -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
mkdir -p "$scratch/app/src/lib/tf/note" "$scratch/app/src/lib/tf/free" "$scratch/app/src/lib/other" "$scratch/app/src/lib/slash" "$scratch/crates" "$scratch/testenv" "$scratch/docs" "$scratch/.claude/commands"
echo a > "$scratch/app/src/lib/launcher.svelte"
echo a > "$scratch/app/src/lib/tf/note/TfNote.svelte"
echo a > "$scratch/app/src/lib/tf/free/Free.svelte"
echo a > "$scratch/app/src/lib/tf/tf.css"
echo a > "$scratch/app/src/lib/tf/Top.svelte"
echo a > "$scratch/app/src/lib/other/Plain.svelte"
echo a > "$scratch/app/src/lib/slash/S.svelte"
echo a > "$scratch/docs/x.md"
echo '{}' > "$scratch/.claude/settings.json"
echo a > "$scratch/.claude/commands/loop.md"
cat > "$scratch/testenv/w-test.sh" <<'EOS'
query_box=../app/src/lib/launcher.svelte
tf=../app/src/lib/tf
dir=../app/src/lib/slash/
note=$tf/note/TfNote.svelte
spare=$tf/free/Free.svelte
grep -q x "$query_box"
grep -q x $tf/Top.svelte
grep -q x "${tf}/tf.css"
grep -q x "$note"
ls "$dir"
EOS
g add -A
g commit -q -m base
base=$(g rev-parse HEAD)

failures=0
expect() { # expect <front|full> <label> <path>
    local want=$1 label=$2 path=$3 got
    g checkout -q -B work "$base"
    echo b >> "$scratch/$path"
    g commit -q -am change
    got=$(cd "$scratch" && "$scope" "$base" 2>/dev/null)
    if [ "$got" != "$want" ]; then echo "FAIL: $label (wanted $want, got $got)" >&2; failures=$((failures + 1)); fi
}

expect full "a pinned app file" app/src/lib/launcher.svelte
expect full "a file reached through a nested variable" app/src/lib/tf/note/TfNote.svelte
expect full "a file named through a directory variable" app/src/lib/tf/Top.svelte
expect full "a file named through a braced directory variable" app/src/lib/tf/tf.css
expect front "a file under a directory the script only walks" app/src/lib/tf/free/Free.svelte
expect full "a file under a directory pinned with a trailing slash" app/src/lib/slash/S.svelte
expect front "an unpinned app file" app/src/lib/other/Plain.svelte
expect front "a prose file" docs/x.md
expect full "the hook wiring, which bash-guard-test.sh tests (#1982)" .claude/settings.json
expect front "a Markdown command under .claude/" .claude/commands/loop.md

[ "$failures" -eq 0 ] && echo "gate-scope-test: ok"
exit "$failures"
