#!/bin/sh
# Reproduce the Tidewater Freight content of fixtures/tidewater/work.json in
# the real Gitea container: org, people, repos, branches, commits, pull
# requests, comments, reviews.
#
# The fixture is READ-ONLY here -- it belongs to another stream and this script
# must never write to it.
#
# IDEMPOTENT BY CONSTRUCTION: every create is preceded by a read, and an
# existing object means skip. Re-running against a seeded environment is a
# no-op that exits 0.
#
# AUTHENTICATION is HTTP basic throughout, not tokens. Gitea accepts basic auth
# on /api/v1, and a token would have to be minted, stored and re-validated on
# every run for no gain. Exactly one token is minted, at the end, because
# stream B's live suite needs one: see KNOBAS_GITEA_TOKEN.
#
# WHAT THIS CANNOT REPRODUCE -- see README.md: git assigns commit shas and
# Gitea assigns pull request indices, so the fixture's `c90d11` and `#142` are
# reachable only approximately. The sha never; the PR number by burning the
# preceding issue indices (SEED_EXACT_PR_NUMBERS=1, the default). Both real
# values are recorded in seed-state.json.
set -eu
cd "$(dirname "$0")"

GITEA_URL=${KNOBAS_GITEA_URL:-http://127.0.0.1:3000}
API="$GITEA_URL/api/v1"
ADMIN_USER=knobas
ADMIN_PASS=knobas-dev
ADMIN="$ADMIN_USER:$ADMIN_PASS"
PEOPLE_PASS=tidewater-dev
ORG=tidewater
FIXTURE=${FIXTURE:-../fixtures/tidewater/work.json}
STATE=seed-state.json
TOKEN_NAME=knobas-seed
: "${SEED_EXACT_PR_NUMBERS:=1}"

say() { echo "seed-gitea: $*"; }
die() { echo "seed-gitea: $*" >&2; exit 1; }

# --------------------------------------------------------------------------
# One HTTP helper so a failure is legible instead of a silent empty string.
# Sets API_STATUS and API_BODY; never exits on an HTTP error -- the callers
# decide which statuses are acceptable, because "already exists" is a normal
# outcome for most of them.
# --------------------------------------------------------------------------
API_STATUS=0
API_BODY=''
api() {  # api <user:pass> <METHOD> <path> [<json body>]
  _auth=$1; _m=$2; _p=$3
  if [ "$#" -gt 3 ]; then
    _raw=$(curl -sS -u "$_auth" -X "$_m" \
                -H 'Content-Type: application/json' -H 'Accept: application/json' \
                -w '\n%{http_code}' -d "$4" "$API$_p")
  else
    _raw=$(curl -sS -u "$_auth" -X "$_m" -H 'Accept: application/json' \
                -w '\n%{http_code}' "$API$_p")
  fi
  API_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  API_BODY=$(printf '%s' "$_raw" | sed '$d')
}

# Accept the listed statuses, die on anything else with the server's message.
expect() {  # expect <what> <status>...
  _what=$1; shift
  for _s in "$@"; do [ "$API_STATUS" = "$_s" ] && return 0; done
  die "$_what: HTTP $API_STATUS: $(printf '%s' "$API_BODY" | head -c 400)"
}

jqf() { jq -r "$1" "$FIXTURE"; }

[ -r "$FIXTURE" ] || die "$FIXTURE not found"

# --------------------------------------------------------------------------
# 1. Admin. `gitea admin user create` is a container command, not an API call:
#    on a fresh volume there is no account to authenticate as yet.
# --------------------------------------------------------------------------
api "$ADMIN" GET /user
if [ "$API_STATUS" != "200" ]; then
  say "creating the admin account $ADMIN_USER"
  docker compose exec -T -u git gitea gitea admin user create \
    --admin --username "$ADMIN_USER" --password "$ADMIN_PASS" \
    --email "$ADMIN_USER@tidewater.test" --must-change-password=false >/dev/null 2>&1 || true
  api "$ADMIN" GET /user
  expect "admin login" 200
fi
say "admin ok"

# --------------------------------------------------------------------------
# 2. People. Content must be authored AS THE FIXTURE SAYS or SyncItem.author is
#    wrong for every Gitea item, which is precisely what stream B's suite
#    checks. The e-mail is <id>@tidewater.test, matching mockd's Jira users.
# --------------------------------------------------------------------------
jqf '.people[] | [.id, .username, .name] | @tsv' | while IFS="$(printf '\t')" read -r id username name; do
  api "$ADMIN" GET "/users/$username"
  if [ "$API_STATUS" = "200" ]; then continue; fi
  api "$ADMIN" POST /admin/users "$(jq -n --arg u "$username" --arg e "$id@tidewater.test" \
        --arg p "$PEOPLE_PASS" --arg n "$name" \
        '{username:$u, email:$e, password:$p, full_name:$n, must_change_password:false}')"
  expect "create user $username" 201
  echo "seed-gitea: created user $username"
done

# --------------------------------------------------------------------------
# 3. Org, and everyone in it. The Owners team is what Gitea creates with the
#    org; putting the fixture's five people in it gives them the write access
#    that authoring commits and reviews needs.
# --------------------------------------------------------------------------
api "$ADMIN" GET "/orgs/$ORG"
if [ "$API_STATUS" != "200" ]; then
  api "$ADMIN" POST /orgs "$(jq -n --arg u "$ORG" '{username:$u, full_name:"Tidewater Freight"}')"
  expect "create org $ORG" 201
  say "created org $ORG"
fi

api "$ADMIN" GET "/orgs/$ORG/teams"
expect "list teams" 200
OWNERS_TEAM=$(printf '%s' "$API_BODY" | jq -r '.[] | select(.name=="Owners") | .id')
[ -n "$OWNERS_TEAM" ] || die "org $ORG has no Owners team"

for username in $(jqf '.people[].username'); do
  api "$ADMIN" GET "/teams/$OWNERS_TEAM/members/$username"
  [ "$API_STATUS" = "200" ] && continue
  api "$ADMIN" PUT "/teams/$OWNERS_TEAM/members/$username"
  expect "add $username to Owners" 204
  echo "seed-gitea: added $username to the $ORG Owners team"
done

# --------------------------------------------------------------------------
# 4. Repos. auto_init gives each one a main branch with a commit, without
#    which no branch can be created and no pull request can be opened.
# --------------------------------------------------------------------------
jqf '.repos[] | [.name, .lang] | @tsv' | while IFS="$(printf '\t')" read -r repo lang; do
  api "$ADMIN" GET "/repos/$ORG/$repo"
  [ "$API_STATUS" = "200" ] && continue
  api "$ADMIN" POST "/orgs/$ORG/repos" "$(jq -n --arg n "$repo" --arg d "$lang" \
        '{name:$n, description:$d, auto_init:true, default_branch:"main"}')"
  expect "create repo $repo" 201
  echo "seed-gitea: created repo $ORG/$repo"
done

# --------------------------------------------------------------------------
# Helpers used by the content sections below.
# --------------------------------------------------------------------------
ensure_branch() {  # ensure_branch <repo> <branch>
  api "$ADMIN" GET "/repos/$ORG/$1/branches/$2"
  [ "$API_STATUS" = "200" ] && return 0
  api "$ADMIN" POST "/repos/$ORG/$1/branches" \
      "$(jq -n --arg n "$2" '{new_branch_name:$n, old_branch_name:"main"}')"
  expect "create branch $1:$2" 201
  say "created branch $1:$2"
}

# A commit is "already there" if the branch carries one with the same message.
# Messages are unique within the fixture, and it is the only property that
# survives the round trip -- the sha does not, which is the whole point.
commit_exists() {  # commit_exists <repo> <branch> <message>
  api "$ADMIN" GET "/repos/$ORG/$1/commits?sha=$2&limit=50&stat=false&verification=false"
  [ "$API_STATUS" = "200" ] || return 1
  printf '%s' "$API_BODY" | jq -e --arg m "$3" 'any(.[]; .commit.message | rtrimstr("\n") == $m)' >/dev/null
}

ensure_commit() {  # ensure_commit <repo> <branch> <path> <message> <when> <username> <fullname> <email>
  _repo=$1; _branch=$2; _path=$3; _msg=$4; _when=$5; _user=$6; _name=$7; _email=$8
  commit_exists "$_repo" "$_branch" "$_msg" && return 0
  _body=$(jq -n --arg c "$(printf '%s\n' "$_msg" | base64 | tr -d '\n')" \
                --arg m "$_msg" --arg b "$_branch" --arg n "$_name" --arg e "$_email" --arg d "$_when" \
      '{content:$c, message:$m, branch:$b,
        author:{name:$n, email:$e}, committer:{name:$n, email:$e},
        dates:{author:$d, committer:$d}}')
  api "$_user:$PEOPLE_PASS" POST "/repos/$ORG/$_repo/contents/$_path" "$_body"
  expect "commit $_repo:$_branch $_path" 201
  say "committed $_repo:$_branch $_path ($_msg)"
}

# The highest issue/pull index in a repo. Gitea allocates issues and pulls from
# ONE per-repo counter, so this is the number a new pull request would follow.
# Paginated rather than "sort by newest and take one": issue list ordering is
# not documented to track the index, and a wrong answer here silently produces
# the wrong PR number.
max_index() {  # max_index <repo>
  _repo=$1; _page=1; _max=0
  while :; do
    api "$ADMIN" GET "/repos/$ORG/$_repo/issues?state=all&limit=50&page=$_page"
    expect "list issues of $_repo" 200
    _n=$(printf '%s' "$API_BODY" | jq 'length')
    [ "$_n" -eq 0 ] && break
    _pagemax=$(printf '%s' "$API_BODY" | jq '[.[].number] | max')
    [ "$_pagemax" -gt "$_max" ] && _max=$_pagemax
    [ "$_n" -lt 50 ] && break
    _page=$((_page + 1))
  done
  echo "$_max"
}

# Burn issue indices so the next allocated index is <target>. ~140 calls per
# repo and a few seconds; worth it because every mockup and link test was
# written against the fixture's PR numbers. The M1 Gitea adapter lists /pulls,
# not /issues, so these placeholders are invisible to it.
burn_to() {  # burn_to <repo> <target index>
  _repo=$1; _target=$2
  _cur=$(max_index "$_repo")
  if [ "$_cur" -ge "$_target" ]; then
    say "WARNING: $_repo is already at index $_cur, cannot place a pull request at $_target"
    return 0
  fi
  [ $((_target - _cur - 1)) -gt 0 ] && \
    say "burning issue indices $((_cur + 1))..$((_target - 1)) in $_repo so the next pull request is #$_target"
  while [ "$_cur" -lt $((_target - 1)) ]; do
    api "$ADMIN" POST "/repos/$ORG/$_repo/issues" \
        '{"title":"(reserved)","body":"Index placeholder created by testenv/seed-gitea.sh so the next pull request lands on the fixture'"'"'s number. Not fixture content.","closed":true}'
    expect "burn index in $_repo" 201
    _cur=$(printf '%s' "$API_BODY" | jq '.number')
  done
}

find_pr_by_title() {  # find_pr_by_title <repo> <title> -> index or empty
  _page=1
  while :; do
    api "$ADMIN" GET "/repos/$ORG/$1/pulls?state=all&limit=50&page=$_page"
    expect "list pulls of $1" 200
    _n=$(printf '%s' "$API_BODY" | jq 'length')
    [ "$_n" -eq 0 ] && break
    _hit=$(printf '%s' "$API_BODY" | jq -r --arg t "$2" '.[] | select(.title==$t) | .number' | head -n 1)
    [ -n "$_hit" ] && { echo "$_hit"; return 0; }
    [ "$_n" -lt 50 ] && break
    _page=$((_page + 1))
  done
  echo ""
}

# person_field <id> <jq field> -- the fixture is the only source of names.
person_field() { jqf ".people[] | select(.id==\"$1\") | .$2"; }

# --------------------------------------------------------------------------
# 5. Branches named by the fixture. `default` is main, which auto_init made.
# --------------------------------------------------------------------------
jqf '.branches[] | select(.state != "default") | [.repo, .name] | @tsv' \
  | while IFS="$(printf '\t')" read -r repo branch; do ensure_branch "$repo" "$branch"; done

# --------------------------------------------------------------------------
# 6. Commits, oldest first, each authored by its fixture person on its fixture
#    date. The file each one touches comes from the file list of the pull
#    request the branch belongs to, in fixture order; a commit on a branch with
#    no pull request writes NOTES.md.
# --------------------------------------------------------------------------
# The argument below is a jq PROGRAM, not shell: $f, $i, $c and $pr are jq
# variables and must not be expanded by the shell.
# shellcheck disable=SC2016
jqf '
  # index each commit within its branch, oldest first, and pair it with the
  # matching file of that branch'"'"'s PR
  . as $f
  | [.commits[]] | sort_by(.when)
  | to_entries[]
  | .key as $i | .value as $c
  | ($f.prs[] | select(.from == $c.branch)) as $pr
  | [$c.repo, $c.branch, ($pr.files[$i].path // "NOTES.md"), $c.msg, $c.when, $c.by]
  | @tsv' | while IFS="$(printf '\t')" read -r repo branch path msg when by; do
  ensure_commit "$repo" "$branch" "$path" "$msg" "$when" \
    "$(person_field "$by" username)" "$(person_field "$by" name)" "$by@tidewater.test"
done

# --------------------------------------------------------------------------
# 7. Pull requests.
#
# Two of the three have `from: null` in the fixture: it records what the UI
# shows, and the UI does not show a head branch for a merged pull request or
# for one the user has not opened. Gitea cannot create a pull request without
# one, so those two head branches are SYNTHESIZED, in the style of the branches
# the fixture does name, and recorded in seed-state.json under
# `synthesized_branches` so nothing downstream mistakes them for fixture data.
# --------------------------------------------------------------------------
synth_branch() {  # synth_branch <pr number> -> branch name, or empty
  case "$1" in
    144) echo "feature/PAY-236-payout-csv-export" ;;
    139) echo "fix/PAY-228-partial-refund-drift" ;;
    *)   echo "" ;;
  esac
}

PR_MAP=''   # "<fixture num>=<real index>" lines, for seed-state.json

for num in $(jqf '.prs[] | .num'); do
  repo=$(jqf ".prs[] | select(.num==$num) | .repo")
  title=$(jqf ".prs[] | select(.num==$num) | .title")
  ticket=$(jqf ".prs[] | select(.num==$num) | .ticket")
  by=$(jqf ".prs[] | select(.num==$num) | .by")
  state=$(jqf ".prs[] | select(.num==$num) | .state")
  head=$(jqf ".prs[] | select(.num==$num) | .from // empty")
  base=$(jqf ".prs[] | select(.num==$num) | .to // empty")
  when=$(jqf ".prs[] | select(.num==$num) | .opened // .merged // empty")
  [ -n "$base" ] || base=main
  [ -n "$when" ] || when=$(jqf '.today')
  # synthesized=1 means the fixture gave no head branch, so this script had to
  # invent one -- and therefore also has to put a commit on it, because Gitea
  # refuses a pull request with an empty diff. A branch the fixture DID name
  # already carries the fixture's own commits and must not get an extra one:
  # stream B asserts against that commit list, and a fourth commit on
  # feature/PAY-231-sepa-retry would be this seed inventing history.
  synthesized=0
  if [ -z "$head" ]; then
    head=$(synth_branch "$num")
    [ -n "$head" ] || die "pull request $num has no head branch and no synthesized one"
    synthesized=1
  fi
  author=$(person_field "$by" username)

  existing=$(find_pr_by_title "$repo" "$title")
  if [ -n "$existing" ]; then
    say "pull request \"$title\" already exists as $repo#$existing"
    PR_MAP="$PR_MAP$num=$existing
"
    index=$existing
  else
    ensure_branch "$repo" "$head"
    if [ "$synthesized" = "1" ]; then
      ensure_commit "$repo" "$head" "docs/$ticket.md" "$ticket: $title" \
        "$when" "$author" "$(person_field "$by" name)" "$by@tidewater.test"
    fi

    [ "$SEED_EXACT_PR_NUMBERS" = "1" ] && burn_to "$repo" "$num"

    api "$author:$PEOPLE_PASS" POST "/repos/$ORG/$repo/pulls" \
        "$(jq -n --arg h "$head" --arg b "$base" --arg t "$title" --arg d "Refs $ticket" \
           '{head:$h, base:$b, title:$t, body:$d}')"
    expect "create pull request $repo \"$title\"" 201
    index=$(printf '%s' "$API_BODY" | jq '.number')
    say "opened $repo#$index \"$title\" (fixture #$num) as $author"
    PR_MAP="$PR_MAP$num=$index
"
  fi

  # -- comments, in fixture order, each as its own author -------------------
  api "$ADMIN" GET "/repos/$ORG/$repo/issues/$index/comments"
  expect "list comments of $repo#$index" 200
  have=$API_BODY
  jqf ".prs[] | select(.num==$num) | .comments[]? | [.who, .text] | @tsv" \
    | while IFS="$(printf '\t')" read -r who text; do
    if printf '%s' "$have" | jq -e --arg t "$text" 'any(.[]; .body == $t)' >/dev/null; then continue; fi
    api "$(person_field "$who" username):$PEOPLE_PASS" POST \
        "/repos/$ORG/$repo/issues/$index/comments" "$(jq -n --arg b "$text" '{body:$b}')"
    expect "comment on $repo#$index" 201
    echo "seed-gitea: commented on $repo#$index as $who"
  done

  # -- review requests ------------------------------------------------------
  api "$ADMIN" GET "/repos/$ORG/$repo/pulls/$index"
  expect "read $repo#$index" 200
  requested=$(printf '%s' "$API_BODY" | jq -r '[.requested_reviewers[]?.login // empty] | join(" ")')
  for who in $(jqf ".prs[] | select(.num==$num) | .review_requested_from[]?.who"); do
    reviewer=$(person_field "$who" username)
    case " $requested " in *" $reviewer "*) continue ;; esac
    api "$ADMIN" POST "/repos/$ORG/$repo/pulls/$index/requested_reviewers" \
        "$(jq -n --arg r "$reviewer" '{reviewers:[$r]}')"
    expect "request review from $reviewer on $repo#$index" 201
    say "requested review from $reviewer on $repo#$index"
  done

  # -- approvals ------------------------------------------------------------
  api "$ADMIN" GET "/repos/$ORG/$repo/pulls/$index/reviews"
  expect "list reviews of $repo#$index" 200
  reviews=$API_BODY
  for who in $(jqf ".prs[] | select(.num==$num) | .approved_by[]?"); do
    reviewer=$(person_field "$who" username)
    if printf '%s' "$reviews" | jq -e --arg u "$reviewer" \
         'any(.[]; .user.login == $u and .state == "APPROVED")' >/dev/null; then continue; fi
    api "$reviewer:$PEOPLE_PASS" POST "/repos/$ORG/$repo/pulls/$index/reviews" \
        "$(jq -n '{event:"APPROVED", body:"Looks good."}')"
    expect "approve $repo#$index as $reviewer" 200
    say "$reviewer approved $repo#$index"
  done

  # -- merge ----------------------------------------------------------------
  if [ "$state" = "merged" ]; then
    api "$ADMIN" GET "/repos/$ORG/$repo/pulls/$index"
    expect "read $repo#$index" 200
    if [ "$(printf '%s' "$API_BODY" | jq -r '.merged')" != "true" ]; then
      api "$ADMIN" POST "/repos/$ORG/$repo/pulls/$index/merge" '{"Do":"merge"}'
      expect "merge $repo#$index" 200 204
      say "merged $repo#$index"
    fi
  fi
done

# --------------------------------------------------------------------------
# 8. One access token, for stream B's live suite. Gitea shows a token's value
#    exactly once, at creation, so a token recorded in seed-state.json is
#    reused while it still authenticates and replaced when it does not.
# --------------------------------------------------------------------------
TOKEN=''
if [ -r "$STATE" ]; then
  TOKEN=$(jq -r '.gitea.token // empty' "$STATE" 2>/dev/null || echo '')
fi
if [ -n "$TOKEN" ]; then
  if [ "$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: token $TOKEN" "$API/user")" != "200" ]; then
    say "the recorded access token no longer authenticates; minting a new one"
    TOKEN=''
  fi
fi
if [ -z "$TOKEN" ]; then
  api "$ADMIN" GET "/users/$ADMIN_USER/tokens"
  expect "list access tokens" 200
  old=$(printf '%s' "$API_BODY" | jq -r --arg n "$TOKEN_NAME" '.[] | select(.name==$n) | .id')
  for id in $old; do
    api "$ADMIN" DELETE "/users/$ADMIN_USER/tokens/$id"
    expect "delete stale token $id" 204
  done
  api "$ADMIN" POST "/users/$ADMIN_USER/tokens" \
      "$(jq -n --arg n "$TOKEN_NAME" '{name:$n, scopes:["write:repository","write:issue","write:user","write:organization"]}')"
  expect "create access token" 201
  TOKEN=$(printf '%s' "$API_BODY" | jq -r '.sha1')
  say "minted access token $TOKEN_NAME"
fi

# --------------------------------------------------------------------------
# 9. seed-state.json -- the bridge between the fixture's ids and reality.
# --------------------------------------------------------------------------
SHA_MAP=$(jqf '[.commits[] | {short: .sha, repo: .repo, branch: .branch, msg: .msg}]')
RESOLVED='[]'
for row in $(printf '%s' "$SHA_MAP" | jq -r '.[] | @base64'); do
  d=$(printf '%s' "$row" | base64 -d)
  repo=$(printf '%s' "$d" | jq -r .repo)
  branch=$(printf '%s' "$d" | jq -r .branch)
  msg=$(printf '%s' "$d" | jq -r .msg)
  short=$(printf '%s' "$d" | jq -r .short)
  api "$ADMIN" GET "/repos/$ORG/$repo/commits?sha=$branch&limit=50&stat=false&verification=false"
  real=$(printf '%s' "$API_BODY" | jq -r --arg m "$msg" \
          'map(select(.commit.message | rtrimstr("\n") == $m)) | .[0].sha // empty')
  RESOLVED=$(printf '%s' "$RESOLVED" | jq --arg s "$short" --arg r "$real" \
              --arg repo "$repo" --arg b "$branch" \
              '. + [{fixture_sha:$s, real_sha:$r, repo:$repo, branch:$b}]')
done

PR_JSON=$(printf '%s' "$PR_MAP" | jq -R -s 'split("\n") | map(select(length>0) | split("=")
          | {fixture_number: (.[0]|tonumber), real_index: (.[1]|tonumber)})')

# MERGED INTO the state file, not written over it: `seed-atlassian.sh` keeps
# its own `jira` and `confluence` blocks in the same file, and a plain `>` here
# deleted them -- so seeding Gitea after Atlassian silently dropped the real
# instances back out of `./seed --env`. Only the keys below are this script's
# to replace.
OLD='{}'
[ -s "$STATE" ] && OLD=$(cat "$STATE")
jq -n --arg url "$GITEA_URL" --arg token "$TOKEN" --arg org "$ORG" \
      --argjson commits "$RESOLVED" --argjson prs "$PR_JSON" \
      --arg exact "$SEED_EXACT_PR_NUMBERS" --argjson old "$OLD" \
  '$old + {
     _comment: "Written by testenv/seed-gitea.sh. The bridge between fixtures/tidewater/work.json and what Gitea and git actually assigned. Git chooses commit shas and Gitea chooses pull request indices; neither can be dictated, so live assertions go by form and title and look literal ids up here.",
     gitea: { url: $url, org: $org, admin_user: "knobas", token: $token },
     exact_pr_numbers: ($exact == "1"),
     synthesized_branches: [
       { pull_request: 144, repo: "payout-service", branch: "feature/PAY-236-payout-csv-export",
         why: "the fixture records from:null -- not fixture content" },
       { pull_request: 139, repo: "ledger-api", branch: "fix/PAY-228-partial-refund-drift",
         why: "the fixture records from:null -- not fixture content" }
     ],
     commits: $commits,
     pull_requests: $prs
   }' > "$STATE.tmp"
mv "$STATE.tmp" "$STATE"

say "wrote $STATE"
say "done"
