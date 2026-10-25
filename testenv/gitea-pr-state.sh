#!/bin/sh
# A seeded pull request the fixture says is open must still be open (issue
# #884). Sourced by seed-gitea.sh; gitea-pr-state-test.sh holds it to account
# against a stub Gitea. POSIX sh, like the seed.
#
# A MERGED ONE IS REFUSED, NOT REPAIRED. A merged pull request cannot be
# reopened, and a replacement would need a new commit on a new branch while
# the merged commit already sits on `main` -- the seed would be inventing
# history on a branch every live suite reads. The only honest restore is a
# Gitea volume seeded afresh, so the seed says so instead of answering
# "already exists" over a fixture that is wrong.
#
# A CLOSED ONE IS REOPENED (#2110). Closing merges nothing: the head branch and
# `main` are as the seed left them, so reopening restores the fixture exactly.
# Somebody clicking *Close* on payout-service#142 in the app took every Gitea
# live suite down on 2026-10-25, and a new pull request was opened from its
# branch. Gitea refuses the reopen while another open pull request holds the
# same head and base, so the seed closes that one first: a seeded branch
# carries only its seeded pull request. A reopen Gitea still refuses (the head
# branch is gone) ends in the refusal, with Gitea's answer in it.

# gitea_require_open_pr <repo> <index> <title> -- needs the seed's `api`,
# `die`, `say`, `ORG` and `ADMIN`. Returns 0 when the pull request is open or
# reopens, dies with the remedy otherwise.
gitea_require_open_pr() {
  api "$ADMIN" GET "/repos/$ORG/$1/pulls/$2"
  expect "read $1#$2" 200
  _gp_state=$(printf '%s' "$API_BODY" | jq -r '.state')
  [ "$_gp_state" = "open" ] && return 0
  _gp_merged=$(printf '%s' "$API_BODY" | jq -r 'if .merged then "merged" else "closed" end')
  _gp_why=''
  if [ "$_gp_merged" = closed ]; then
    _gp_head=$(printf '%s' "$API_BODY" | jq -r '.head.ref')
    _gp_base=$(printf '%s' "$API_BODY" | jq -r '.base.ref')
    api "$ADMIN" GET "/repos/$ORG/$1/pulls?state=open&limit=50"
    expect "list the open pull requests of $1" 200
    for _gp_other in $(printf '%s' "$API_BODY" | jq -r --arg h "$_gp_head" --arg b "$_gp_base" \
        '.[] | select(.head.ref == $h and .base.ref == $b) | .number'); do
      api "$ADMIN" PATCH "/repos/$ORG/$1/pulls/$_gp_other" '{"state":"closed"}'
      expect "close $1#$_gp_other, opened from seeded $_gp_head after $1#$2 was closed" 201
      say "closed $1#$_gp_other: it was opened from seeded $_gp_head after $1#$2 was closed"
    done
    api "$ADMIN" PATCH "/repos/$ORG/$1/pulls/$2" '{"state":"open"}'
    if [ "$API_STATUS" = 201 ]; then
      say "reopened $1#$2 \"$3\": something had closed it (#2110)"
      return 0
    fi
    _gp_why=" Reopening it failed: HTTP $API_STATUS: $(printf '%s' "$API_BODY" | head -c 300)."
  fi
  die "pull request \"$3\" ($1#$2) is $_gp_merged, but the fixture says it is open. Something merged or closed a seeded pull request (a live test must only touch pull requests it opened).$_gp_why The seed cannot repair it: put Gitea on a fresh volume and seed again --
    cd testenv && docker compose --env-file images.env rm -sfv gitea && docker volume rm \$(docker volume ls -q --filter name=gitea-data) && rm -f seed-state.json && ./seed-gitea.sh
  (testenv/reset does the same for every product, not only Gitea)"
}
