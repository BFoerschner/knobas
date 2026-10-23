#!/usr/bin/env bash
# `just docs-check`'s own test (#1691), run by `just witness-unit`: in a scratch
# copy of the files the recipe reads, a clean tree is green, a stray
# `docs/contract/10.8/` file with no regenerated index is red, and a conflict
# marker in prose is red. #1697 adds: a section 10.8 file that is no bullet is
# red, and prose after the index's end marker stays put. The clean run is the
# control: a recipe that failed for any other reason would pass the red cases.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")

scratch=$(mktemp -d "${TMPDIR:-/tmp}/docs-check-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/scripts" "$scratch/docs"
cp "$repo/justfile" "$scratch/justfile"
cp "$here/contract-index.py" "$here/conflict-markers.sh" "$here/conflict-markers-allow.txt" "$scratch/scripts/"
cp "$repo/docs/contract.md" "$scratch/docs/contract.md"
cp -R "$repo/docs/contract" "$scratch/docs/contract"
g() { git -C "$scratch" -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
g add -A
g commit -q -m base

run() { just --justfile "$scratch/justfile" --working-directory "$scratch" docs-check >/dev/null 2>&1; }

failures=0
expect() { # expect <green|red> <label>
    local rc=0
    run || rc=$?
    if { [ "$1" = green ] && [ "$rc" -ne 0 ]; } || { [ "$1" = red ] && [ "$rc" -eq 0 ]; }; then
        echo "FAIL: $2 (wanted $1, exit $rc)" >&2
        failures=$((failures + 1))
    fi
}

expect green "a clean tree"

printf -- '- **A stray entry (issue #1, 2026-10-22): no index regenerated.**\n' \
    > "$scratch/docs/contract/10.8/stray-entry.md"
expect red "a stray section 10.8 file"
rm "$scratch/docs/contract/10.8/stray-entry.md"

printf 'prose\n<<<<<<< HEAD\nmore prose\n' > "$scratch/docs/prose.md"
expect red "a conflict marker in prose"
rm "$scratch/docs/prose.md"

expect green "the tree after both are removed"

# #1697: an entry file must start with a bullet naming an issue. The index is
# regenerated around the stray file, so only the entry-file check can refuse it.
idx() { python3 -I "$scratch/scripts/contract-index.py" >/dev/null; }
check_err() { python3 -I "$scratch/scripts/contract-index.py" --check 2>&1 | cat; }
printf '# Notes\n\nNot an entry, though it names issue #12.\n' > "$scratch/docs/contract/10.8/README.md"
idx
expect red "a README.md in section 10.8 with the index regenerated"
case "$(check_err)" in
    *README.md*) ;;
    *) echo "FAIL: the refusal does not name README.md" >&2; failures=$((failures + 1)) ;;
esac
rm "$scratch/docs/contract/10.8/README.md"
printf -- '- **A bullet that names no issue.**\n' > "$scratch/docs/contract/10.8/n99-no-issue.md"
idx
expect red "a bullet entry file that names no issue"
rm "$scratch/docs/contract/10.8/n99-no-issue.md"
idx
expect green "the index regenerated without the stray files"

# #1697: prose after the index's end marker stays below it, on check and on regeneration.
python3 -I - "$scratch/docs/contract.md" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
end = "<!-- 10.8-index:end -->\n"
assert s.count(end) == 1
open(p, "w").write(s.replace(end, end + "\nProse below the index.\n"))
PY
expect green "prose below the index end marker"
idx
below=$(awk '/10.8-index:end/{e=NR} /^Prose below the index\.$/{p=NR} END{print (e && p > e) ? "below" : "moved"}' "$scratch/docs/contract.md")
if [ "$below" != below ]; then
    echo "FAIL: regeneration moved the prose above the index end marker" >&2
    failures=$((failures + 1))
fi
expect green "prose below the index end marker after regeneration"

[ "$failures" -eq 0 ] && echo "docs-check-test: ok"
