#!/usr/bin/env bash
# `scripts/gate-scope.sh`'s own test (#1543), run by `just witness-unit`: in a
# scratch repo, a head that changes only an app file a testenv script pins (or
# one a variable expands to, or one under a directory the script names bare) answers `full`; an unpinned app file (also one
# under a directory a variable holds and the script walks, #1577) and a
# prose file answer `front`. The unpinned case is the control: without it a
# script that answers `full` for everything would pass. `app/package.json`
# (#2301): a new dependency line answers `front` though `crates/` reads the
# file; a changed `@tauri-apps/plugin-*` line answers `full`, and so does any
# other app file `crates/` names. The `scripts` scope (#2334): a head that
# changes only `scripts/`, `testenv/` and prose answers `scripts`, unless one of
# its paths is named by `crates/` or `app/src` (a directory named pins its
# contents; a comment or a bare `testenv/` pins nothing), is one the full gate
# runs (`scripts/gate-slot.sh` and the rest of the list in gate-scope.sh), or
# the head also touches the justfile or an app file. A Markdown file there is
# prose unless a crate opens it (`join("…/README.md")`): naming it in a panic
# message reads nothing.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scope=$here/gate-scope.sh

scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-scope-check.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
# No excludes file: the machine's global one ignores `.claude/*`, which the repo tracks.
g() { git -C "$scratch" -c core.excludesFile=/dev/null -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
mkdir -p "$scratch/scripts" "$scratch/testenv/specs" "$scratch/testenv/drivers" "$scratch/app/src/lib/shell" "$scratch/app/src/lib/tf/note" "$scratch/app/src/lib/tf/free" "$scratch/app/src/lib/other" "$scratch/app/src/lib/slash" "$scratch/crates" "$scratch/testenv" "$scratch/docs" "$scratch/.claude/commands"
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
for f in shot-test.sh rust-reads.sh commented.sh bash-guard.py gate-slot.sh; do echo a > "$scratch/scripts/$f"; done
echo a > "$scratch/testenv/specs/README.md"
echo a > "$scratch/testenv/README.md"
echo a > "$scratch/testenv/drivers/capture.sh"
echo a > "$scratch/justfile"
cat > "$scratch/crates/scripts.rs" <<'EOS'
// scripts/commented.sh is only mentioned here, which reads nothing.
const SEED: &str = include_str!("../scripts/rust-reads.sh");
fn specs() -> PathBuf { dir.join("../testenv/specs/README.md") }
fn hint() { panic!("run ./seed from testenv/ (testenv/README.md)") }
EOS
echo 'const drivers = import.meta.glob("../../../../testenv/drivers/*.sh");' > "$scratch/app/src/lib/shell/witness.ts"
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
check() { # check <front|scripts|full> <label>: commit the worktree and ask gate-scope
    local want=$1 label=$2 got
    g commit -q -am change
    got=$(cd "$scratch" && "$scope" "$base" 2>/dev/null)
    if [ "$got" != "$want" ]; then echo "FAIL: $label (wanted $want, got $got)" >&2; failures=$((failures + 1)); fi
}
expect() { # expect <front|scripts|full> <label> <path>...: append a line to each path
    local want=$1 label=$2 p
    shift 2
    g checkout -q -B work "$base"
    for p in "$@"; do echo b >> "$scratch/$p"; done
    check "$want" "$label"
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
expect scripts "a script nothing reads (#2334)" scripts/shot-test.sh
expect scripts "a testenv script and a prose file (#2334)" testenv/w-test.sh docs/x.md
expect scripts "a script only a crates/ comment names (#2334)" scripts/commented.sh
expect full "a script beside the justfile (#2334)" scripts/shot-test.sh justfile
expect full "a script beside an app file (#2334)" scripts/shot-test.sh app/src/lib/other/Plain.svelte
expect full "a script crates/ reads (#2334)" scripts/rust-reads.sh
expect full "a testenv file under a directory app/src names (#2334)" testenv/drivers/capture.sh
expect full "the Bash guard, which .claude/settings.json wires in (#2334)" scripts/bash-guard.py
expect full "a script the full gate's cargo half runs (#2334)" scripts/gate-slot.sh
expect full "a testenv Markdown file a crate opens (#2334)" testenv/specs/README.md
expect front "a testenv Markdown file a crate only names in a message (#2334)" testenv/README.md
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
