#!/usr/bin/env bash
# The test-layout script's own tests (issue #573), run by `just witness-unit`.
#
# Each check builds a fixture tree -- a `crates/<crate>/tests/` directory and a
# `test-layout-exceptions.txt` -- or a pair of fixture inventories in a scratch
# directory, runs `test-layout.sh` against it with `-C`, and asserts on the exit
# status. Every check that expects green has a sibling that expects red, so the
# suite cannot pass on a script that always says yes, nor on one that always
# says no.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
script=$here/test-layout.sh

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-test-layout-test.XXXXXX")
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

# --- the rename check -------------------------------------------------------

# Fixture inventories in the committed file's shape: tab-separated package,
# target and name, sorted byte-wise. `knobas-a` is the crate being merged;
# `perf` and `coverage` are on the list and so keep their own binaries (two
# entries, because a list of one once hid an awk that choked on the second);
# `knobas-b` is not being merged and must come through untouched.
rename_root() {
    local root
    root=$(mktemp -d "$scratch/case.XXXXXX")
    mkdir -p "$root/crates/knobas-a/tests/it" "$root/crates/knobas-b/tests"
    : >"$root/crates/knobas-a/tests/it/main.rs"
    : >"$root/crates/knobas-a/tests/coverage.rs"
    : >"$root/crates/knobas-a/tests/perf.rs"
    : >"$root/crates/knobas-b/tests/home.rs"
    printf '%s\n' "knobas-a/coverage  a recipe runs it with --test coverage" \
        "knobas-a/perf  a recipe runs it with --test perf" >"$root/test-layout-exceptions.txt"
    printf '%s\t%s\t%s\n' \
        knobas-a lib/knobas_a 'tests::unit_one' \
        knobas-a test/home 'a_home_test' \
        knobas-a test/lists 'mod_x::a_list_test' \
        knobas-a test/lists 'z_list_test' \
        knobas-a test/coverage 'the_coverage_test' \
        knobas-a test/perf 'the_perf_test' \
        knobas-b test/home 'b_home_test' | LC_ALL=C sort >"$root/old.txt"
    printf '%s\t%s\t%s\n' \
        knobas-a lib/knobas_a 'tests::unit_one' \
        knobas-a test/it 'home::a_home_test' \
        knobas-a test/it 'lists::mod_x::a_list_test' \
        knobas-a test/it 'lists::z_list_test' \
        knobas-a test/coverage 'the_coverage_test' \
        knobas-a test/perf 'the_perf_test' \
        knobas-b test/home 'b_home_test' | LC_ALL=C sort >"$root/new.txt"
    echo "$root"
}

root=$(rename_root)
check "a pure rename passes" pass \
    "$script" -C "$root" rename "$root/old.txt" "$root/new.txt" knobas-a

root=$(rename_root)
check "the unchanged inventory is not a rename of a merged crate" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/old.txt" knobas-a

root=$(rename_root)
grep -v 'lists::z_list_test' "$root/new.txt" >"$root/dropped.txt"
check "a dropped line goes red" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/dropped.txt" knobas-a

root=$(rename_root)
{ cat "$root/new.txt"; printf 'knobas-a\ttest/it\tlists::a_new_test\n'; } | LC_ALL=C sort >"$root/added.txt"
check "an added line goes red" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/added.txt" knobas-a

root=$(rename_root)
sed 's/lists::z_list_test/home::z_list_test/' "$root/new.txt" | LC_ALL=C sort >"$root/wrong.txt"
check "a line renamed to the wrong module goes red" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/wrong.txt" knobas-a

root=$(rename_root)
sed 's|knobas-a\ttest/perf\tthe_perf_test|knobas-a\ttest/it\tperf::the_perf_test|' "$root/new.txt" \
    | LC_ALL=C sort >"$root/listed.txt"
check "a listed file renamed into the binary goes red" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/listed.txt" knobas-a

root=$(rename_root)
sed 's|knobas-b\ttest/home\tb_home_test|knobas-b\ttest/it\thome::b_home_test|' "$root/new.txt" \
    | LC_ALL=C sort >"$root/other.txt"
check "a crate that was not named is not renamed" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/other.txt" knobas-a

# A crate whose files are all still on the list renames nothing, so the old
# inventory "passes" as its own rename -- which is what running the check
# before taking the crate's files off the exception list would otherwise report.
root=$(rename_root)
printf '%s\n' "knobas-b/home  still waiting for its merge" >>"$root/test-layout-exceptions.txt"
check "a crate with nothing left to rename is refused" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/old.txt" knobas-b

root=$(rename_root)
check "a rename with no crate named is refused" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/new.txt"

root=$(rename_root)
check "a crate the old inventory never mentions is refused" fail \
    "$script" -C "$root" rename "$root/old.txt" "$root/new.txt" knobas-a knobas-typo

root=$(rename_root)
check "an unknown subcommand is refused" fail "$script" -C "$root" frobnicate

# --- verdict ----------------------------------------------------------------

if [ "$failures" -ne 0 ]; then
    printf 'test-layout-test: FAILED (%d of %d checks)\n' "$failures" "$checks" >&2
    exit 1
fi
printf 'test-layout-test: ok (%d checks)\n' "$checks"
