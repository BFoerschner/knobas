#!/usr/bin/env bash
# The integration-test layout of ADR-0017 (issue #573): each crate's
# integration tests are one binary, `tests/it/main.rs`, except the files on
# `test-layout-exceptions.txt`. Two checks:
#
#   test-layout.sh [-C ROOT] guard
#
#     Fails when a crate has a top-level `tests/*.rs` that the exception list
#     does not name, and when the list names a file that does not exist, so the
#     list cannot rot. `just check` runs this. Files are found on disk, not via
#     `git ls-files`: cargo compiles an untracked `tests/stray.rs` all the same.
#
#   test-layout.sh [-C ROOT] rename OLD NEW CRATE...
#
#     Fails unless inventory NEW is inventory OLD with every test of the named
#     crates' top-level test files moved into the merged binary,
#
#         <crate>\ttest/<file>\t<name>  ->  <crate>\ttest/it\t<file>::<name>
#
#     except the files the exception list still names, and with every other
#     line unchanged -- compared byte for byte after the inventory's own
#     `LC_ALL=C sort`. A merge PR runs it as `just inventory-rename <crate>...`,
#     which hands it the merge-base with `main`'s inventory as OLD. "No test was lost" is then a
#     comparison and not a judgment: a dropped, added or misplaced line is red.
#
# ROOT is the repository root (default: the current directory). The script is
# bash 3.2 compatible, because that is macOS's /bin/bash; the set logic is awk.

set -euo pipefail

prog=test-layout
root=.
scratch=

# The traps of the justfile's `inventory` recipe: bash runs no EXIT trap when a
# signal it has no handler for ends the shell, so INT and TERM clean up too and
# then re-raise.
cleanup() { [ -z "$scratch" ] || rm -f "$scratch"; }
trap 'cleanup' EXIT
trap 'cleanup; trap - INT; kill -INT $$' INT
trap 'cleanup; trap - TERM; kill -TERM $$' TERM

usage() {
    echo "usage: $prog.sh [-C ROOT] guard" >&2
    echo "       $prog.sh [-C ROOT] rename OLD NEW CRATE..." >&2
    exit 2
}

die() {
    echo "$prog: $*" >&2
    exit 1
}

# exceptions -- the list's entries, one `<crate>/<file>` per line, sorted; dies
# on a list that is missing or malformed. An entry is its first field and the
# rest of the line is its reason, which must be there.
exceptions() {
    local list="$root/test-layout-exceptions.txt"
    [ -f "$list" ] || die "no exception list at $list"
    awk -v list="$list" '
        /^[[:space:]]*(#|$)/ { next }
        {
            if ($1 !~ /^[a-z0-9-]+\/[A-Za-z0-9_]+$/) {
                printf "%s:%d: \"%s\" is not <crate>/<file>, e.g. knobas-http/transport\n", list, NR, $1 > "/dev/stderr"; bad = 1; next
            }
            if (NF < 2) {
                printf "%s:%d: %s has no reason; every exception says why it is one\n", list, NR, $1 > "/dev/stderr"; bad = 1; next
            }
            if ($1 in seen) {
                printf "%s:%d: %s is listed twice (first on line %d)\n", list, NR, $1, seen[$1] > "/dev/stderr"; bad = 1; next
            }
            seen[$1] = NR
            print $1
        }
        END { exit bad }
    ' "$list" | LC_ALL=C sort
    # This runs inside `$(...)`, where bash 3.2 does not apply `set -e`, so the
    # pipeline's failure has to be checked by hand.
    [ "${PIPESTATUS[0]}" -eq 0 ] || die "the exception list is malformed"
}

# nonblank TEXT -- TEXT's lines without the empty ones, so that an empty list
# is no lines rather than one blank line.
nonblank() { printf '%s\n' "$1" | sed '/^$/d'; }

guard() {
    [ $# -eq 0 ] || usage
    local listed found='' crate f n=0
    listed=$(exceptions)

    for crate in "$root"/crates/*/; do
        [ -d "$crate" ] || continue
        n=$((n + 1))
        for f in "$crate"tests/*.rs; do
            [ -f "$f" ] || continue
            found="$found$(basename "$crate")/$(basename "$f" .rs)
"
        done
    done
    found=$(printf '%s' "$found" | LC_ALL=C sort)
    # A root with no crates is a guard pointed at the wrong place, not a tree
    # with a clean layout.
    [ "$n" -gt 0 ] || die "no crates under $root/crates -- is -C pointing at the repository root?"

    local unlisted stale status=0
    unlisted=$(LC_ALL=C comm -23 <(nonblank "$found") <(nonblank "$listed"))
    stale=$(LC_ALL=C comm -13 <(nonblank "$found") <(nonblank "$listed"))
    if [ -n "$unlisted" ]; then
        echo "$prog: top-level test files that are neither tests/it/main.rs nor on test-layout-exceptions.txt:" >&2
        printf '%s\n' "$unlisted" | sed -E 's|^([^/]*)/(.*)$|  crates/\1/tests/\2.rs|' >&2
        echo "  Put the tests in the crate's tests/it/ as a module (ADR-0017), or add" >&2
        echo "  the file to test-layout-exceptions.txt with a one-line reason." >&2
        status=1
    fi
    if [ -n "$stale" ]; then
        echo "$prog: test-layout-exceptions.txt names files that do not exist:" >&2
        printf '%s\n' "$stale" | sed 's/^/  /' >&2
        echo "  Delete those lines; an exception for nothing is a list going stale." >&2
        status=1
    fi
    [ "$status" -eq 0 ] || exit 1
    echo "$prog: ok ($n crates, $(nonblank "$listed" | wc -l | tr -d ' ') files on the exception list)"
}

rename() {
    [ $# -ge 3 ] || usage
    local old=$1 new=$2 crate
    shift 2
    [ -f "$old" ] || die "no old inventory at $old"
    [ -f "$new" ] || die "no new inventory at $new"
    for crate in "$@"; do
        # A typo'd crate would rename nothing and pass as a rename of nothing.
        awk -F'\t' -v c="$crate" '$1 == c { found = 1; exit } END { exit !found }' "$old" \
            || die "$crate has no tests in $old -- is the crate name right?"
    done

    local listed
    listed=$(exceptions)
    scratch=$(mktemp "${TMPDIR:-/tmp}/knobas-test-layout.XXXXXX")
    # Entries go to awk space-separated: macOS's awk refuses a newline in a
    # `-v` string, and an entry cannot contain a space (exceptions() checks).
    awk -F'\t' -v OFS='\t' -v prog="$prog" -v crates="$*" -v listed="$(printf '%s\n' "$listed" | tr '\n' ' ')" '
        BEGIN {
            n = split(crates, c, " "); for (i = 1; i <= n; i++) merging[c[i]] = 1
            n = split(listed, l, " "); for (i = 1; i <= n; i++) keep[l[i]] = 1
        }
        ($1 in merging) && $2 ~ /^test\// && $2 != "test/it" {
            file = substr($2, 6)
            if (!(($1 "/" file) in keep)) { $3 = file "::" $3; $2 = "test/it"; renamed[$1]++ }
        }
        { print }
        # A crate with nothing to rename would pass as its own rename: that is
        # the check run before the crate'"'"'s files left the exception list.
        END {
            for (cr in merging) if (!(cr in renamed)) {
                printf "%s: %s has no test outside test/it and off the exception list to rename;\n", prog, cr > "/dev/stderr"
                printf "  take the files being merged off test-layout-exceptions.txt first\n" > "/dev/stderr"
                bad = 1
            }
            exit bad
        }
    ' "$old" | LC_ALL=C sort >"$scratch"

    if ! diff -u --label "expected (old inventory, renamed)" --label "$new" "$scratch" "$new"; then
        echo >&2
        echo "$prog: $new is not $old with $* merged into test/it." >&2
        echo "  '-' lines are tests the rename expected and did not find; '+' lines" >&2
        echo "  are tests it did not expect. A merge moves tests and changes nothing" >&2
        echo "  else, so either is a test lost, added or put in the wrong module." >&2
        exit 1
    fi
    echo "$prog: ok ($new is the old inventory with $* merged into test/it; $(wc -l <"$new" | tr -d ' ') tests)"
}

if [ "${1:-}" = "-C" ]; then
    [ $# -ge 2 ] || usage
    root=$2
    shift 2
fi
[ $# -ge 1 ] || usage
cmd=$1
shift
case $cmd in
guard) guard "$@" ;;
rename) rename "$@" ;;
*) usage ;;
esac
