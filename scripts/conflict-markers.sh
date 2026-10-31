#!/usr/bin/env bash
# Fails on a leftover merge-conflict marker in a file git knows (tracked, or
# untracked and not ignored): a line that is `<<<<<<<`, `|||||||` (diff3's
# base), `=======` or `>>>>>>>` alone or followed by a space and a label (#999).
#
# PR #997's rebase left one `|||||||` line in a stylesheet; only `vite build`
# refused it. This is the cheap catch for every file type, not just CSS.
#
# False positives, handled here:
#   - `=======` alone on a line is a Markdown setext heading underline, so in
#     `.md` files only the other three markers count.
#   - A fixture that holds a marker on purpose goes on
#     `scripts/conflict-markers-allow.txt`, one repo-relative path per line
#     (`#` starts a comment), and is skipped.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
allow="$root/scripts/conflict-markers-allow.txt"

hits=$(git -C "$root" grep -nIE --untracked \
    '^(<<<<<<<|\|\|\|\|\|\|\||=======|>>>>>>>)( |$)' -- . ':(exclude)*.md' || true)
md_hits=$(git -C "$root" grep -nIE --untracked \
    '^(<<<<<<<|\|\|\|\|\|\|\||>>>>>>>)( |$)' -- '*.md' || true)
hits=$(printf '%s\n%s\n' "$hits" "$md_hits" | sed '/^$/d')

if [ -n "$hits" ] && [ -f "$allow" ]; then
    allowed=$(sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$allow" | sed '/^$/d')
    kept=
    while IFS= read -r line; do
        file=${line%%:*}
        # A here-string, not a pipe: `grep -q` exiting early would SIGPIPE
        # printf on a list past a pipe buffer, and pipefail fails the `if` (#2305).
        if grep -qxF -- "$file" <<< "$allowed"; then continue; fi
        kept+="$line"$'\n'
    done <<< "$hits"
    hits=${kept%$'\n'}
fi

if [ -n "$hits" ]; then
    echo "conflict-markers: FAILED, a leftover conflict marker:" >&2
    printf '%s\n' "$hits" | cut -c1-200 >&2
    echo "conflict-markers: resolve it, or list a deliberate fixture in scripts/conflict-markers-allow.txt" >&2
    exit 1
fi
echo "conflict-markers: ok"
