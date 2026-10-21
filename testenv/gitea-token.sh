#!/bin/sh
# The Gitea access token the seed hands the live suites, one per worktree
# (issue #684). Sourced by seed-gitea.sh; gitea-token-test.sh holds it to
# account against a stub Gitea. POSIX sh, like the seed.
#
# WHY ONE PER WORKTREE. seed-state.json is git-ignored, so it lives inside one
# worktree, while the Gitea container is shared by every worktree on the
# machine. Until #684 every seed minted under one name, `knobas-seed`, and
# deleted every token of that name first -- so a seed run from a tree with no
# usable recorded token revoked the token every other tree had recorded, and
# their next `just gitea-live` answered 401 on every test (four times in the
# #660 wave). Now a tree mints under a name derived from its own path and
# deletes only tokens of that name: its own earlier token, whose value it has
# lost. Another tree's token, and the pre-#684 `knobas-seed`, are never
# touched.

# gitea_seed_token_name <testenv dir> -- `knobas-seed-<worktree>-<checksum>`:
# the worktree's directory name to read, the checksum of the testenv
# directory's real path so two trees of one name stay apart.
gitea_seed_token_name() {
  _gt_dir=$(cd "$1" && pwd -P)
  _gt_sum=$(printf '%s' "$_gt_dir" | cksum | cut -d ' ' -f 1)
  printf 'knobas-seed-%s-%s\n' "$(basename "$(dirname "$_gt_dir")")" "$_gt_sum"
}

# gitea_token_status <api url> <token> -- the one probe: GET /user, prints the
# HTTP status (empty when Gitea gave none).
gitea_token_status() {
  curl -sS -o /dev/null -w '%{http_code}' \
    -H "Authorization: token $2" "$1/user" 2>/dev/null || true
}

# gitea_token_require <api url> <token> -- 0 when Gitea accepts the token (200);
# otherwise says so on stderr, naming the status and the remedy, and returns 1.
# Read-only: re-minting is ./seed's job, never a print's (#1377).
gitea_token_require() {
  _gr_status=$(gitea_token_status "$1" "$2")
  [ "$_gr_status" = 200 ] && return 0
  echo "seed: the recorded Gitea access token in seed-state.json is refused by $1/user (HTTP ${_gr_status:-000}, not 200); run ./seed (from testenv/) and retry" >&2
  return 1
}

# gitea_seed_token <api url> <admin user:pass> <recorded token or ""> <name>
# Prints the token to use. A recorded token is kept while Gitea accepts it --
# one call to /user -- whatever its name, so a tree still holding a working
# pre-#684 `knobas-seed` token keeps it. A token Gitea refuses (401) is replaced:
# tokens named <name> are deleted and one is minted under it. Any other answer
# to the probe, or none, is a refusal that changes nothing: replacing a token
# on a guess would cost a working one.
gitea_seed_token() {
  _gt_api=$1 _gt_admin=$2 _gt_token=$3 _gt_name=$4
  _gt_user=${_gt_admin%%:*}
  if [ -n "$_gt_token" ]; then
    _gt_status=$(gitea_token_status "$_gt_api" "$_gt_token")
    case $_gt_status in
      200) printf '%s\n' "$_gt_token"; return 0 ;;
      401) echo "seed-gitea: the recorded access token no longer authenticates; minting a new one" >&2 ;;
      *) echo "seed-gitea: checking the recorded access token got HTTP ${_gt_status:-000}, not 200 or 401; changing nothing" >&2
         return 1 ;;
    esac
  fi
  # The list is paged (Gitea's default is 30 a page) and grows by one token per
  # worktree that ever seeded, so this tree's own can sit past the first page.
  # Every page is read before anything is deleted: deleting shifts the pages.
  _gt_ids='' _gt_page=1
  while :; do
    _gt_req "$_gt_admin" GET "$_gt_api/users/$_gt_user/tokens?limit=50&page=$_gt_page" || return 1
    [ "$_GT_STATUS" = 200 ] || { _gt_die "list access tokens"; return 1; }
    _gt_found=$(printf '%s' "$_GT_BODY" | jq -r --arg n "$_gt_name" '.[] | select(.name==$n) | .id') \
      || { _gt_die "read the access token list"; return 1; }
    _gt_count=$(printf '%s' "$_GT_BODY" | jq length) \
      || { _gt_die "read the access token list"; return 1; }
    _gt_ids="$_gt_ids $_gt_found"
    [ "$_gt_count" -ge 50 ] || break
    _gt_page=$((_gt_page + 1))
  done
  for _gt_id in $_gt_ids; do
    _gt_req "$_gt_admin" DELETE "$_gt_api/users/$_gt_user/tokens/$_gt_id" || return 1
    [ "$_GT_STATUS" = 204 ] || { _gt_die "delete this worktree's stale token $_gt_id"; return 1; }
  done
  _gt_req "$_gt_admin" POST "$_gt_api/users/$_gt_user/tokens" \
    "$(jq -n --arg n "$_gt_name" '{name:$n, scopes:["write:repository","write:issue","write:user","write:organization"]}')" \
    || return 1
  [ "$_GT_STATUS" = 201 ] || { _gt_die "create access token $_gt_name"; return 1; }
  echo "seed-gitea: minted access token $_gt_name" >&2
  printf '%s' "$_GT_BODY" | jq -r '.sha1'
}

# _gt_req <user:pass> <METHOD> <url> [<json body>] -- sets _GT_STATUS, _GT_BODY.
# Fails, saying so, when curl got no answer at all; an HTTP error is an answer
# and the caller judges it.
_gt_req() {
  if [ "$#" -gt 3 ]; then
    _gt_raw=$(curl -sS -u "$1" -X "$2" -H 'Content-Type: application/json' \
                -H 'Accept: application/json' -w '\n%{http_code}' -d "$4" "$3") \
      || { echo "seed-gitea: $2 $3: no answer" >&2; return 1; }
  else
    _gt_raw=$(curl -sS -u "$1" -X "$2" -H 'Accept: application/json' \
                -w '\n%{http_code}' "$3") \
      || { echo "seed-gitea: $2 $3: no answer" >&2; return 1; }
  fi
  _GT_STATUS=$(printf '%s' "$_gt_raw" | tail -n 1)
  _GT_BODY=$(printf '%s' "$_gt_raw" | sed '$d')
}

_gt_die() {
  echo "seed-gitea: $1: HTTP $_GT_STATUS: $(printf '%s' "$_GT_BODY" | head -c 400)" >&2
}
