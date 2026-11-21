#!/usr/bin/env bash
# The soak's repository (testenv/perf/README.md, *The soak*): the scale estate's `scale-commerce/cart-service` built
# again for the stand-in forge (`standin-repos.py`), with one commit more on `main` that brings in a tree of thousands
# of files (this checkout's own, four times over: about 9,700 files in 800 folders), and a clone of it for knobas to
# read. The repository's pull requests and branches stay as the saved database mirrors them, so its other tabs draw.
#
#   testenv/perf/soak-repo.sh <dir>
#
# Leaves `<dir>/repos/scale-commerce-cart-service.git` (for `standin-forge.mjs --repos <dir>/repos`) and
# `<dir>/clone` (for `set_checkout_override`), and prints the commit the mirror's `main` must name:
#
#   update sync.item set payload = jsonb_set(payload, '{commit,id}', to_jsonb('<sha>'::text)), synced_at = now()
#    where entity_id = 'live-gitea:scale-commerce/cart-service@refs/heads/main'
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
dir=${1:?usage: soak-repo.sh <dir>}
mkdir -p "$dir/repos"
bare="$dir/repos/scale-commerce-cart-service.git"
[ -d "$bare" ] || python3 -I "$here/standin-repos.py" --out "$dir/repos" scale-commerce/cart-service
work="$dir/work"
rm -rf "$work" "$dir/clone"
git clone --quiet "$bare" "$work"
if [ "$(git -C "$work" ls-files | wc -l)" -lt 1000 ]; then
  for into in . packages/alpha packages/beta packages/gamma; do
    mkdir -p "$work/$into"
    git -C "$root" archive HEAD | tar -x -C "$work/$into"
  done
  git -C "$work" add -A . > /dev/null
  GIT_AUTHOR_DATE="2026-11-10T10:00:00Z" GIT_COMMITTER_DATE="2026-11-10T10:00:00Z" \
    git -C "$work" -c user.name=soak -c user.email=soak@tidewater.test commit --quiet -m "Bring in the platform tree"
  git -C "$work" push --quiet origin main
fi
rm -rf "$work"
git clone --quiet "$bare" "$dir/clone"
echo "soak-repo: $(git -C "$dir/clone" ls-files | wc -l | tr -d ' ') files in $(git -C "$dir/clone" ls-tree -r -d --name-only HEAD | wc -l | tr -d ' ') folders"
echo "soak-repo: main is $(git --git-dir "$bare" rev-parse main)"
echo "soak-repo: the clone is $dir/clone"
