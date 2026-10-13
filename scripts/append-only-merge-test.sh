#!/usr/bin/env bash
# The append-only merge rule's own test (issue #736), run by `just witness-unit`.
#
# Every PR appends to `test-inventory.txt` and to
# `docs/decisions/2026-10-unattended-rulings.md`; `.gitattributes` gives both
# `merge=union`, so two branches appending to the same spot rebase onto each
# other without a conflict. This builds a scratch repo carrying the repo's real
# `.gitattributes`, makes two branches append to both files at the same place,
# and rebases one onto the other. The control sibling does the same with no
# `.gitattributes` and must conflict, so the suite cannot pass on a scenario
# that never conflicts. A third check pins that `docs/contract.md` is NOT union
# merged: its sections are edited in place, and a union merge would keep both
# sides of an edited line.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
attrs=$here/../.gitattributes

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-append-only-merge-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'rm -rf "$scratch"; trap - INT; kill -INT $$' INT
trap 'rm -rf "$scratch"; trap - TERM; kill -TERM $$' TERM

failures=0
checks=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

# git in the scratch repo, with no user config in the way.
g() { git -C "$repo" -c user.name=t -c user.email=t@example.invalid -c commit.gpgsign=false "$@"; }

# build <with-attributes: yes|no> -- a repo whose main has the two files and
# two branches, `a` and `b`, each appending a line at the end of both and
# inserting one inventory line between the same two neighbours.
build() {
    repo=$(mktemp -d "$scratch/repo.XXXXXX")
    git init -q -b main "$repo"
    mkdir -p "$repo/docs/decisions"
    printf 'k\tt\tm_a\nk\tt\tm_z\n' >"$repo/test-inventory.txt"
    printf '# Rulings\n\n---\n' >"$repo/docs/decisions/2026-10-unattended-rulings.md"
    printf 'one\ntwo\n' >"$repo/contract.txt"
    if [ "$1" = yes ]; then cp "$attrs" "$repo/.gitattributes"; fi
    g add -A && g commit -q -m base
    for b in a b; do
        g checkout -q -b "$b" main
        printf 'k\tt\tm_m_%s\n' "$b" >"$repo/ins"
        # between m_a and m_z, the same spot on both branches
        awk -v l="$(cat "$repo/ins")" '{print} /m_a$/{print l}' "$repo/test-inventory.txt" >"$repo/inv.new"
        mv "$repo/inv.new" "$repo/test-inventory.txt"; rm "$repo/ins"
        printf '\n## Ruling %s\n\nbody %s\n' "$b" "$b" >>"$repo/docs/decisions/2026-10-unattended-rulings.md"
        g commit -q -am "branch $b"
    done
}

checks=$((checks + 1))
build yes
if g rebase -q a b >/dev/null 2>&1; then
    inv=$(cat "$repo/test-inventory.txt"); rul=$(cat "$repo/docs/decisions/2026-10-unattended-rulings.md")
    for want in m_m_a m_m_b; do
        grep -q "$want" <<<"$inv" || fail "union rebase lost inventory line $want"
    done
    for want in "Ruling a" "Ruling b"; do
        grep -q "$want" <<<"$rul" || fail "union rebase lost $want from the rulings file"
    done
    # The gate regenerates the inventory sorted (`inventory-update`); a union
    # merge only has to keep both lines, so sorting must make it canonical.
    LC_ALL=C sort "$repo/test-inventory.txt" | LC_ALL=C sort -c || fail "sorted inventory not canonical"
else
    fail "rebase with .gitattributes conflicted"
fi

checks=$((checks + 1))
build no
if g rebase -q a b >/dev/null 2>&1; then
    fail "control: rebase with no .gitattributes did not conflict, so the scenario proves nothing"
fi
g rebase --abort >/dev/null 2>&1 || true

checks=$((checks + 1))
if grep -Eq '^[^#]*contract\.md.*merge=union' "$attrs"; then
    fail "docs/contract.md must not be merge=union (sections are edited in place)"
fi
for f in test-inventory.txt docs/decisions/2026-10-unattended-rulings.md; do
    grep -Eq "^${f}[[:space:]]+merge=union" "$attrs" || fail "$f is not merge=union in .gitattributes"
done

if [ "$failures" -ne 0 ]; then echo "append-only-merge-test: $failures failure(s)" >&2; exit 1; fi
echo "append-only-merge-test: $checks checks passed"
