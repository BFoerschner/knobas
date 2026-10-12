#!/usr/bin/env bash
# The Gitea seed token's own unit tests (issue #684), run by `just
# witness-unit`. No container is started: `curl` is a stub on PATH that plays
# Gitea's token API (`/user` and `/users/<admin>/tokens`) over a JSON file, so
# the rule `gitea-token.sh` keeps -- a worktree's seed never revokes another
# worktree's token -- is checked without the shared container it protects.
# `just gitea-live` from two worktrees is the live witness (testenv/README.md,
# "The Gitea token is per worktree").

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-gitea-token-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'rm -rf "$scratch"; trap - INT; kill -INT $$' INT
trap 'rm -rf "$scratch"; trap - TERM; kill -TERM $$' TERM

# The stub. Tokens live in $STUB_STORE as [{id, name, sha1}]; the admin's
# basic auth is taken on trust. STUB_DOWN=1 answers nothing, as curl does when
# the port is closed: `000` and exit 7. STUB_PROBE_STATUS answers `/user`
# alone with that status, while the token API keeps working.
mkdir -p "$scratch/bin"
cat >"$scratch/bin/curl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
method=GET body='' token='' fmt='' out='' url=''
while [ "$#" -gt 0 ]; do
  case $1 in
    -X) method=$2; shift ;;
    -d) body=$2; shift ;;
    -w) fmt=$2; shift ;;
    -o) out=$2; shift ;;
    -u) shift ;;
    -H) case $2 in "Authorization: token "*) token=${2#Authorization: token } ;; esac; shift ;;
    -*) ;;
    *) url=$1 ;;
  esac
  shift
done
echo "$method ${url#*/api/v1}" >>"$STUB_LOG"
if [ "${STUB_DOWN:-0}" = 1 ]; then
  [ -n "$fmt" ] && printf '%b' "${fmt//%\{http_code\}/000}"
  exit 7
fi
path=${url#*/api/v1}
query=''
case $path in *\?*) query=${path#*\?}; path=${path%%\?*} ;; esac
qarg() { printf '%s\n' "$query" | tr '&' '\n' | sed -n "s/^$1=//p"; }
status=200 resp=''
case "$method $path" in
  "GET /user")
    if [ -n "${STUB_PROBE_STATUS:-}" ]; then
      status=$STUB_PROBE_STATUS resp='{"message":"probe fault"}'
    elif jq -e --arg t "$token" 'any(.[]; .sha1 == $t)' "$STUB_STORE" >/dev/null; then
      resp='{"login":"knobas"}'
    else
      status=401 resp='{"message":"user does not exist"}'
    fi ;;
  "GET /users/knobas/tokens")
    # Paged as Gitea pages it: `limit` (default 30, at most 50) and `page`.
    limit=$(qarg limit); limit=${limit:-30}; [ "$limit" -le 50 ] || limit=50
    page=$(qarg page); page=${page:-1}
    resp=$(jq -c --argjson l "$limit" --argjson p "$page" \
      '.[($p - 1) * $l:$p * $l] | map({id, name, token_last_eight: .sha1[-8:]})' "$STUB_STORE") ;;
  "POST /users/knobas/tokens")
    name=$(printf '%s' "$body" | jq -r .name)
    if jq -e --arg n "$name" 'any(.[]; .name == $n)' "$STUB_STORE" >/dev/null; then
      status=400 resp='{"message":"access token name has been used already"}'
    else
      n=$(($(cat "$STUB_STORE.seq" 2>/dev/null || echo 0) + 1)); echo "$n" >"$STUB_STORE.seq"
      sha=$(printf 'sha-%s-%040d' "$name" "$n")
      jq --argjson id "$n" --arg n "$name" --arg s "$sha" '. + [{id:$id, name:$n, sha1:$s}]' \
        "$STUB_STORE" >"$STUB_STORE.tmp" && mv "$STUB_STORE.tmp" "$STUB_STORE"
      status=201 resp=$(jq -n --argjson id "$n" --arg n "$name" --arg s "$sha" '{id:$id, name:$n, sha1:$s}')
    fi ;;
  "DELETE /users/knobas/tokens/"*)
    id=${path##*/}
    jq --argjson id "$id" 'map(select(.id != $id))' "$STUB_STORE" >"$STUB_STORE.tmp" \
      && mv "$STUB_STORE.tmp" "$STUB_STORE"
    status=204 ;;
  *) status=404 resp='{"message":"not found"}' ;;
esac
[ -n "$out" ] || printf '%s' "$resp"
[ -n "$fmt" ] && printf '%b' "${fmt//%\{http_code\}/$status}"
exit 0
STUB
chmod +x "$scratch/bin/curl"
export PATH="$scratch/bin:$PATH" STUB_LOG="$scratch/curl.log" STUB_STORE="$scratch/tokens.json"

# shellcheck source=testenv/gitea-token.sh disable=SC1091
. "$here/gitea-token.sh" || { echo "gitea-token-test: cannot source gitea-token.sh" >&2; exit 1; }

API=http://gitea.test/api/v1
failures=0
checks=0

fail() {
  printf 'gitea-token-test: FAIL %s\n' "$1" >&2
  shift
  printf '  %s\n' "$@" >&2
  failures=$((failures + 1))
}

check_eq() {  # check_eq <what> <expected> <actual>
  checks=$((checks + 1))
  [ "$2" = "$3" ] || fail "$1" "expected: $2" "actual:   $3"
}

authenticates() {  # authenticates <token> -> 200 or 401, as Gitea answers
  curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: token $1" "$API/user"
}

# The seed's own lines go to a file: the test speaks for itself.
seed_token() { gitea_seed_token "$@" 2>>"$scratch/seed.err"; }

fresh() { echo '[]' >"$STUB_STORE"; rm -f "$STUB_STORE.seq"; : >"$STUB_LOG"; }

# Two worktrees, as `git worktree add` lays them out.
mkdir -p "$scratch/knobas/testenv" "$scratch/knobas/.worktrees/issue-1/testenv"
tree_a=$scratch/knobas/testenv
tree_b=$scratch/knobas/.worktrees/issue-1/testenv

# --- the name -----------------------------------------------------------------

name_a=$(gitea_seed_token_name "$tree_a")
name_b=$(gitea_seed_token_name "$tree_b")
checks=$((checks + 1))
[ "$name_a" != "$name_b" ] || fail "two worktrees get two token names" "both: $name_a"
check_eq "one worktree's name is stable" "$name_a" "$(gitea_seed_token_name "$tree_a")"
case $name_b in knobas-seed-issue-1-*) checks=$((checks + 1)) ;;
  *) checks=$((checks + 1)); fail "the name says which worktree it is" "got: $name_b" ;; esac

# --- a second worktree's mint leaves the first one's token alone -------------

fresh
tok_a=$(seed_token "$API" knobas:pw "" "$name_a")
tok_b=$(seed_token "$API" knobas:pw "" "$name_b")
check_eq "A's token still authenticates after B minted" 200 "$(authenticates "$tok_a")"
check_eq "B's token authenticates" 200 "$(authenticates "$tok_b")"
checks=$((checks + 1))
[ "$tok_a" != "$tok_b" ] || fail "B did not reuse A's token" "both: $tok_a"

# --- a recorded token that authenticates is kept ------------------------------

: >"$STUB_LOG"
check_eq "A's recorded token is kept" "$tok_a" "$(seed_token "$API" knobas:pw "$tok_a" "$name_a")"
check_eq "keeping it takes one call, the probe" "GET /user" "$(cat "$STUB_LOG")"

# --- a recorded token Gitea refuses is replaced -------------------------------

fresh
old_a=$(seed_token "$API" knobas:pw "" "$name_a")
tok_b=$(seed_token "$API" knobas:pw "" "$name_b")
# A's state file now holds a token Gitea no longer has (a volume reset, a
# hand-deleted token): B's state is untouched by A's replacement.
new_a=$(seed_token "$API" knobas:pw "dead-token-not-in-gitea" "$name_a")
check_eq "the replacement authenticates" 200 "$(authenticates "$new_a")"
check_eq "B's token survives A's replacement" 200 "$(authenticates "$tok_b")"
check_eq "A's own unrecorded token is the one revoked" 401 "$(authenticates "$old_a")"
check_eq "one token per worktree" 2 "$(jq length "$STUB_STORE")"

# --- this tree's stale token past the list's first page is still found -------

fresh
# Sixty other trees' tokens listed ahead of A's, so A's sits on the second
# page however the seed asks for them (30 or 50 a page).
jq -n '[range(1; 61) | {id: ., name: "knobas-seed-other-\(.)", sha1: "other-\(.)"}]' >"$STUB_STORE"
echo 60 >"$STUB_STORE.seq"
old_a=$(seed_token "$API" knobas:pw "" "$name_a")
new_a=$(seed_token "$API" knobas:pw "dead-token-not-in-gitea" "$name_a") || new_a=''
check_eq "a stale token past the first page is replaced" 200 "$(authenticates "$new_a")"
check_eq "and the old one under that name is revoked" 401 "$(authenticates "$old_a")"
check_eq "and the 60 other trees keep theirs" 61 "$(jq length "$STUB_STORE")"

# --- the pre-#684 shared token is never this seed's to revoke -----------------

fresh
jq '. + [{id: 99, name: "knobas-seed", sha1: "legacy-shared-token"}]' "$STUB_STORE" >"$STUB_STORE.tmp"
mv "$STUB_STORE.tmp" "$STUB_STORE"
tok_b=$(seed_token "$API" knobas:pw "" "$name_b")
check_eq "B mints its own beside it" 200 "$(authenticates "$tok_b")"
check_eq "a tree still on knobas-seed keeps it" 200 "$(authenticates legacy-shared-token)"
check_eq "a recorded knobas-seed token that works is kept" legacy-shared-token \
  "$(seed_token "$API" knobas:pw legacy-shared-token "$name_a")"

# --- no answer is not a refusal ------------------------------------------------

fresh
tok_a=$(seed_token "$API" knobas:pw "" "$name_a")
checks=$((checks + 1))
if STUB_DOWN=1 gitea_seed_token "$API" knobas:pw "$tok_a" "$name_a" >"$scratch/out" 2>"$scratch/err"; then
  fail "a Gitea that does not answer is refused" "printed: $(cat "$scratch/out")"
fi
check_eq "and the token it could not check is still there" 200 "$(authenticates "$tok_a")"

# A probe answered with something other than 200 or 401, while the token API
# works: replacing on that guess would cost a token that may be fine.
: >"$STUB_LOG"
checks=$((checks + 1))
if STUB_PROBE_STATUS=500 gitea_seed_token "$API" knobas:pw "$tok_a" "$name_a" >"$scratch/out" 2>"$scratch/err"; then
  fail "a probe answered 500 is refused" "printed: $(cat "$scratch/out")"
fi
check_eq "and refusing it makes no call past the probe" "GET /user" "$(cat "$STUB_LOG")"

if [ "$failures" -gt 0 ]; then
  echo "gitea-token-test: $failures of $checks checks failed" >&2
  exit 1
fi
echo "gitea-token-test: all $checks checks passed"
