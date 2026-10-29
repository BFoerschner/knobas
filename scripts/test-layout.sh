#!/usr/bin/env bash
# The integration-test layout of ADR-0017 (issue #573): each crate's
# integration tests are one binary, `tests/it/main.rs`, except the files on
# `test-layout-exceptions.txt`. One check (the `rename` mode that compared two
# test inventories went with the inventory on 2026-10-29):
#
#   test-layout.sh [-C ROOT] guard
#
#     Fails when a crate has a top-level `tests/*.rs` that the exception list
#     does not name, and when the list names a file that does not exist, so the
#     list cannot rot. It also fails on the two other routes to a second
#     integration binary, which the list cannot excuse: a `tests/<dir>/main.rs`
#     other than `tests/it/main.rs` (cargo auto-discovers it), and a `[[test]]`
#     table with a `path` other than `tests/it/main.rs` in a crate's
#     Cargo.toml. `just check` runs this. Files are found on disk, not via
#     `git ls-files`: cargo compiles an untracked `tests/stray.rs` all the same.
#
# ROOT is the repository root (default: the current directory). The script is
# bash 3.2 compatible, because that is macOS's /bin/bash; the set logic is awk.

set -euo pipefail

prog=test-layout
root=.
scratch=

# The traps of the justfile's `test` recipe: bash runs no EXIT trap when a
# signal it has no handler for ends the shell, so INT and TERM clean up too and
# then re-raise.
cleanup() { [ -z "$scratch" ] || rm -f "$scratch"; }
trap 'cleanup' EXIT
trap 'cleanup; trap - INT; kill -INT $$' INT
trap 'cleanup; trap - TERM; kill -TERM $$' TERM

usage() {
    echo "usage: $prog.sh [-C ROOT] guard" >&2
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
    local listed found='' crate f n=0 extra='' d
    listed=$(exceptions)

    for crate in "$root"/crates/*/; do
        [ -d "$crate" ] || continue
        n=$((n + 1))
        for f in "$crate"tests/*.rs; do
            [ -f "$f" ] || continue
            found="$found$(basename "$crate")/$(basename "$f" .rs)
"
        done
        for f in "$crate"tests/*/main.rs; do
            [ -f "$f" ] || continue
            d=$(basename "$(dirname "$f")")
            [ "$d" = it ] || extra="$extra  crates/$(basename "$crate")/tests/$d/main.rs (auto-discovered binary)
"
        done
        if [ -f "$crate"Cargo.toml ]; then
            # `[[test]]` tables with a `path`: any but tests/it/main.rs is a
            # second binary. A table without a path only configures a
            # discovered target.
            extra="$extra$(awk -v c="$(basename "$crate")" '
                function flush() { if (intest && path != "" && path != "tests/it/main.rs") printf "  crates/%s/Cargo.toml: [[test]] path = \"%s\"\n", c, path; path = "" }
                /^[[:space:]]*\[/ { flush(); intest = ($0 ~ /^[[:space:]]*\[\[test\]\][[:space:]]*(#.*)?$/); next }
                intest && /^[[:space:]]*path[[:space:]]*=/ { v = $0; sub(/^[^=]*=[[:space:]]*/, "", v); gsub(/["\047]/, "", v); sub(/[[:space:]]*(#.*)?$/, "", v); path = v }
                END { flush() }
            ' "$crate"Cargo.toml)
"
        fi
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
    extra=$(nonblank "$extra")
    if [ -n "$extra" ]; then
        echo "$prog: second integration binaries (a crate has one, tests/it/main.rs, ADR-0017):" >&2
        printf '%s\n' "$extra" >&2
        echo "  Make it a module of tests/it/. These cannot go on test-layout-exceptions.txt." >&2
        status=1
    fi
    [ "$status" -eq 0 ] || exit 1
    echo "$prog: ok ($n crates, $(nonblank "$listed" | wc -l | tr -d ' ') files on the exception list)"
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
*) usage ;;
esac
