#!/usr/bin/env bash
# The test-layout script's own tests (issue #573), run by `just witness-unit`.
#
# Each check builds a fixture tree -- a `crates/<crate>/tests/` directory and a
# `test-layout-exceptions.txt` -- in a scratch directory, runs `test-layout.sh` against it with `-C`, and asserts on the exit
# status. Every check that expects green has a sibling that expects red, so the
# suite cannot pass on a script that always says yes, nor on one that always
# says no.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
script=$here/test-layout.sh

# Not `knobas-test-...`: that prefix names the scratch roots the test connector's
# reaper (knobas-db test_util) sweeps, and a directory so named with no lock
# beside it is deleted from under this script by the first cargo test binary
# that starts, mid-run (issue #799).
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-layout-check.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'rm -rf "$scratch"; trap - INT; kill -INT $$' INT
trap 'rm -rf "$scratch"; trap - TERM; kill -TERM $$' TERM

failures=0
checks=0

# check <what> <expected: pass|fail> <command...> -- runs the command with its
# output kept in a log, which is printed when the outcome is the wrong one.
check() {
    local what=$1 expected=$2 outcome=pass log="$scratch/log"
    shift 2
    checks=$((checks + 1))
    "$@" >"$log" 2>&1 || outcome=fail
    if [ "$outcome" = "$expected" ]; then return 0; fi
    printf 'test-layout-test: FAIL %s\n  expected: %s\n  actual:   %s\n' "$what" "$expected" "$outcome" >&2
    sed 's/^/  | /' "$log" >&2
    failures=$((failures + 1))
}

# new_root -- a fresh fixture root, printed. It starts as a tree that passes:
# one crate already merged into `tests/it/main.rs`, one with a listed file.
new_root() {
    local root
    root=$(mktemp -d "$scratch/case.XXXXXX")
    mkdir -p "$root/crates/knobas-a/tests/it" "$root/crates/knobas-b/tests/support"
    : >"$root/crates/knobas-a/tests/it/main.rs"
    : >"$root/crates/knobas-a/tests/it/lists.rs"
    : >"$root/crates/knobas-b/tests/live_b.rs"
    : >"$root/crates/knobas-b/tests/support/mod.rs"
    cat >"$root/test-layout-exceptions.txt" <<'EOF'
# A comment line.

knobas-b/live_b  a recipe runs it with --test live_b
EOF
    echo "$root"
}

# --- the layout guard -------------------------------------------------------

root=$(new_root)
check "a merged crate and a listed file pass" pass "$script" -C "$root" guard

root=$(new_root)
: >"$root/crates/knobas-a/tests/stray.rs"
check "a stray top-level file goes red" fail "$script" -C "$root" guard

root=$(new_root)
: >"$root/crates/knobas-a/tests/stray.rs"
echo "knobas-a/stray  listed on purpose" >>"$root/test-layout-exceptions.txt"
check "the same file passes once it is listed" pass "$script" -C "$root" guard

# The same file name in another crate is another file: an entry names a crate
# and a file, and must not excuse a namesake.
root=$(new_root)
: >"$root/crates/knobas-a/tests/live_b.rs"
check "a listed name does not excuse another crate's namesake" fail "$script" -C "$root" guard

root=$(new_root)
echo "knobas-b/gone  this file was deleted" >>"$root/test-layout-exceptions.txt"
check "a stale list entry goes red" fail "$script" -C "$root" guard

# A second binary by the two routes other than tests/*.rs (issue #1692).
root=$(new_root)
mkdir -p "$root/crates/knobas-a/tests/perf2"
: >"$root/crates/knobas-a/tests/perf2/main.rs"
check "a tests/<dir>/main.rs other than it goes red" fail "$script" -C "$root" guard

root=$(new_root)
mkdir -p "$root/crates/knobas-a/tests/perf2"
: >"$root/crates/knobas-a/tests/perf2/main.rs"
echo "knobas-a/perf2  listed on purpose" >>"$root/test-layout-exceptions.txt"
check "a tests/<dir>/main.rs cannot be put on the list" fail "$script" -C "$root" guard

root=$(new_root)
check "a tests/<dir>/mod.rs helper stays green" pass "$script" -C "$root" guard

root=$(new_root)
cat >"$root/crates/knobas-a/Cargo.toml" <<'EOF'
[package]
name = "knobas-a"

[[test]]
name = "extra"
path = "tests/extra_main.rs"
EOF
check "a [[test]] with a path goes red" fail "$script" -C "$root" guard

root=$(new_root)
cat >"$root/crates/knobas-a/Cargo.toml" <<'EOF'
[package]
name = "knobas-a"

[[test]]
name = "it"
path = "tests/it/main.rs"

[[test]]
name = "other"
harness = false

[[bin]]
name = "tool"
path = "src/bin/tool.rs"
EOF
check "[[test]] on tests/it/main.rs, one without a path, and a [[bin]] path stay green" pass "$script" -C "$root" guard

root=$(new_root)
echo "knobas-b/live_b" >"$root/test-layout-exceptions.txt"
check "an entry without a reason goes red" fail "$script" -C "$root" guard

root=$(new_root)
echo "knobas-b/live_b  listed twice" >>"$root/test-layout-exceptions.txt"
check "an entry listed twice goes red" fail "$script" -C "$root" guard

root=$(new_root)
echo "crates/knobas-b/tests/live_b.rs  spelled as a path" >"$root/test-layout-exceptions.txt"
check "an entry not spelled crate/file goes red" fail "$script" -C "$root" guard

root=$(new_root)
rm "$root/test-layout-exceptions.txt"
check "a missing exception list goes red" fail "$script" -C "$root" guard

# A tree with no crates at all is a guard pointed at the wrong directory, and
# must not read as a clean layout.
root="$scratch/empty"
mkdir -p "$root"
: >"$root/test-layout-exceptions.txt"
check "a root with no crates goes red" fail "$script" -C "$root" guard

root=$(new_root)
check "an unknown subcommand is refused" fail "$script" -C "$root" frobnicate

# --- verdict ----------------------------------------------------------------

if [ "$failures" -ne 0 ]; then
    printf 'test-layout-test: FAILED (%d of %d checks)\n' "$failures" "$checks" >&2
    exit 1
fi
printf 'test-layout-test: ok (%d checks)\n' "$checks"
