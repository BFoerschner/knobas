#!/bin/sh
# A seeded pull request the fixture says is open must still be open (issue
# #884). Sourced by seed-gitea.sh; gitea-pr-state-test.sh holds it to account
# against a stub Gitea. POSIX sh, like the seed.
#
# WHY A REFUSAL AND NOT A REPAIR. A merged or closed pull request cannot be
# reopened once its head branch is gone, and a replacement would need a new
# commit on a new branch while the merged commit already sits on `main` -- the
# seed would be inventing history on a branch every live suite reads. The
# only honest restore is a Gitea volume seeded afresh, so the seed says so
# instead of answering "already exists" over a fixture that is wrong.

# gitea_require_open_pr <repo> <index> <title> -- needs the seed's `api`,
# `die`, `ORG` and `ADMIN`. Returns 0 when the pull request is open, dies
# with the remedy otherwise.
gitea_require_open_pr() {
  api "$ADMIN" GET "/repos/$ORG/$1/pulls/$2"
  expect "read $1#$2" 200
  _gp_state=$(printf '%s' "$API_BODY" | jq -r '.state')
  [ "$_gp_state" = "open" ] && return 0
  _gp_merged=$(printf '%s' "$API_BODY" | jq -r 'if .merged then "merged" else "closed" end')
  die "pull request \"$3\" ($1#$2) is $_gp_merged, but the fixture says it is open. Something merged or closed a seeded pull request (a live test must only touch pull requests it opened). Gitea cannot take it back: put Gitea on a fresh volume and seed again --
    cd testenv && docker compose --env-file images.env rm -sfv gitea && docker volume rm \$(docker volume ls -q --filter name=gitea-data) && rm -f seed-state.json && ./seed-gitea.sh
  (testenv/reset does the same for every product, not only Gitea)"
}
