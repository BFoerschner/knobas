#!/usr/bin/env bash
# The diagrams in docs/how-it-works.md: every docs/diagrams/*.d2, laid out by TALA, as
# <name>-light.svg and <name>-dark.svg beside it (`just readme-diagrams`).
#
# TALA ships inside d2 since v0.9.0, with no plugin and no key. A d2 without it is
# refused rather than falling back to another engine, which would redraw every
# diagram differently, and an older TALA plugin's watermarked output is refused
# rather than written. Nothing is replaced until every render has worked.
#
# The same source and the same d2 draw the same bytes. TALA picks from a fixed set
# of seeds, but a small edit can still move the whole layout, so look at the result.
#
# Env: READMES_D2 replaces the d2 binary (scripts/readme-diagrams-test.sh stubs it).
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
dir=$root/docs/diagrams
d2=${READMES_D2:-d2}
light_theme=1
dark_theme=200
pad=20
fail() { echo "readme-diagrams: $1" >&2; exit 1; }

command -v "$d2" >/dev/null || fail "no d2 found; install v0.9.0 or newer (brew install d2)"
"$d2" layout 2>/dev/null | grep -q '^tala (bundled)' || fail "$("$d2" --version) has no bundled TALA; it needs d2 v0.9.0 or newer (brew upgrade d2)"

sources=("$dir"/*.d2)
[ -e "${sources[0]}" ] || fail "no .d2 files in docs/diagrams"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-readme-diagrams.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for source in "${sources[@]}"; do
  name=$(basename "$source" .d2)
  "$d2" --layout tala --pad "$pad" --theme "$light_theme" "$source" "$scratch/$name-light.svg" >/dev/null 2>"$scratch/err" \
    || fail "$name (light) failed; nothing was replaced: $(cat "$scratch/err")"
  "$d2" --layout tala --pad "$pad" --theme "$dark_theme" "$source" "$scratch/$name-dark.svg" >/dev/null 2>"$scratch/err" \
    || fail "$name (dark) failed; nothing was replaced: $(cat "$scratch/err")"
  for theme in light dark; do
    [ -s "$scratch/$name-$theme.svg" ] || fail "$name ($theme) wrote nothing; nothing was replaced"
    if grep -q 'UNLICENSED' "$scratch/$name-$theme.svg"; then
      fail "$name ($theme) came out watermarked, so this is an old TALA plugin; nothing was replaced"
    fi
  done
done
cp "$scratch"/*.svg "$dir/"
echo "readme-diagrams: ${#sources[@]} diagrams in 2 themes, with $("$d2" --version)"
