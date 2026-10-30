#!/usr/bin/env bash
# `scripts/gate-scope.sh`'s own test (#1543), run by `just witness-unit`: in a
# scratch repo, a head that changes only an app file a testenv script pins (or
# one a variable expands to, or one under a directory the script names bare) answers `full`; an unpinned app file (also one
# under a directory a variable holds and the script walks, #1577) and a
# prose file answer `front`. The unpinned case is the control: without it a
# script that answers `full` for everything would pass. `app/package.json`
# (#2301): a new dependency line answers `front` though `crates/` reads the
# file; a changed `@tauri-apps/plugin-*` line answers `full`, and so does any
# other app file `crates/` names.
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
echo a > "$scratch/app/src/lib/read.ts"
echo a > "$scratch/docs/x.md"
cat > "$scratch/app/package.json" <<'EOS'
{
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-notification": "=2.3.1",
    "@tauri-apps/plugin-opener": "=2.5.4",
    "svelte": "^5.0.0"
  }
}
EOS
echo '{"lockfileVersion": 3}' > "$scratch/app/package-lock.json"
cat > "$scratch/crates/wiring.rs" <<'EOS'
const PACKAGE: &str = include_str!("../app/package.json");
const READ: &str = include_str!("../app/src/lib/read.ts");
EOS
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
check() { # check <front|full> <label>: commit the worktree and ask gate-scope
    local want=$1 label=$2 got
    g commit -q -am change
    got=$(cd "$scratch" && "$scope" "$base" 2>/dev/null)
    if [ "$got" != "$want" ]; then echo "FAIL: $label (wanted $want, got $got)" >&2; failures=$((failures + 1)); fi
}
expect() { # expect <front|full> <label> <path>: append a line to path
    g checkout -q -B work "$base"
    echo b >> "$scratch/$3"
    check "$1" "$2"
}
edit_package() { # edit_package <front|full> <label> <perl -pi expression> [lock]
    g checkout -q -B work "$base"
    perl -pi -e "$3" "$scratch/app/package.json"
    if [ "${4:-}" = lock ]; then echo b >> "$scratch/app/package-lock.json"; fi
    check "$1" "$2"
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
expect full "an app file crates/ reads" app/src/lib/read.ts
# The perl expressions are single-quoted on purpose: `$1` and `$_` are perl's.
# shellcheck disable=SC2016
edit_package front "a new npm dependency with its lockfile (#2301)" \
    's/^(    "svelte".*)$/    "shiki": "^3.0.0",\n$1/' lock
edit_package front "a bumped npm dependency that is not a Tauri plugin (#2301)" \
    's/"svelte": "\^5.0.0"/"svelte": "^5.1.0"/'
edit_package full "a bumped Tauri plugin pin (#2301)" \
    's/"=2.5.4"/"=2.5.5"/'
# shellcheck disable=SC2016
edit_package full "a new Tauri plugin, which wiring.rs forbids for global-shortcut (#2301)" \
    's/^(    "svelte".*)$/    "\@tauri-apps\/plugin-global-shortcut": "=2.0.0",\n$1/'
# shellcheck disable=SC2016
edit_package full "a removed Tauri plugin (#2301)" \
    '$_ = "" if /plugin-notification/'

[ "$failures" -eq 0 ] && echo "gate-scope-test: ok"
exit "$failures"
