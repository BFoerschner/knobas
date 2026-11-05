#!/usr/bin/env bash
# Commit a release's screenshots to the branch the README lives on, and name the
# release in the README's stamp. The last step of release.yml's `readme` job.
#
# Usage: scripts/readme-shots-publish.sh <tag> <shots dir> [branch]   (branch: main)
#
# The pictures are the tag's and the commit goes on top of the branch, which has
# usually moved since the tag. It works in a worktree of its own, so the checkout
# it runs from stays on the tag. A release that changed nothing a picture shows
# commits nothing (scripts/readme-shots.sh draws the same bytes on a re-run).
#
# Other merges land on the branch all day, so a rejected push fetches, rebases
# and tries again. Only this script writes these files, so the rebase has nothing
# to conflict over; if it ever does, the run fails rather than guessing.
#
# The fetches and the push run from the checkout, not the worktree:
# actions/checkout scopes the token to the checkout's own git directory.
set -euo pipefail
[ $# -ge 2 ] || { echo "usage: $0 <tag> <shots dir> [branch]" >&2; exit 2; }
tag=$1 shots=$2 branch=${3:-main}
fail() { echo "readme-shots-publish: $1" >&2; exit 1; }
repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-readme-publish.XXXXXX")
tree=$scratch/tree
trap 'git -C "$repo" worktree remove --force "$tree" 2>/dev/null || true; rm -rf "$scratch"' EXIT
# Both write a commit, and a release runner has no identity of its own.
as_bot() { git -C "$tree" -c user.name='github-actions[bot]' -c user.email='41898282+github-actions[bot]@users.noreply.github.com' "$@"; }

git -C "$repo" fetch -q origin "$branch"
git -C "$repo" worktree add -q --detach "$tree" FETCH_HEAD

# `cp` of a glob that matched nothing fails: an empty directory is a job that did
# no work, and never reads as "nothing changed".
cp "$shots"/*.webp "$tree/docs/screenshots/"
sed -E "s#(<!-- shots-version -->)[^<]*(<!-- /shots-version -->)#\1$tag\2#" "$tree/README.md" >"$scratch/README.md"
cp "$scratch/README.md" "$tree/README.md"

git -C "$tree" add docs/screenshots README.md
if git -C "$tree" diff --cached --quiet; then
  echo "readme-shots-publish: $tag draws what $branch already shows; nothing to commit"
  exit 0
fi
as_bot commit -q -m "Refresh the README's screenshots for $tag"

for attempt in 1 2 3 4 5; do
  if git -C "$repo" push -q origin "$(git -C "$tree" rev-parse HEAD):refs/heads/$branch"; then
    echo "readme-shots-publish: $tag's screenshots are on $branch"
    exit 0
  fi
  sleep $((attempt * 5))
  git -C "$repo" fetch -q origin "$branch"
  # By its id: FETCH_HEAD is per worktree, and the fetch above wrote the checkout's.
  as_bot rebase -q "$(git -C "$repo" rev-parse FETCH_HEAD)" || fail "$tag's commit does not rebase onto $branch; nothing was pushed"
done
fail "$branch rejected $tag's commit five times"
