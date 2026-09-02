#!/bin/sh
# Phase two of `./seed --teamcity`: the Tidewater Freight build content of
# fixtures/tidewater/work.json, in the real TeamCity that seed-teamcity.sh set
# up. Projects, build configurations, VCS roots pointing at the seeded Gitea,
# one command-line step per configuration, and enough real builds for the
# server to show the fixture's shapes.
#
#   ./seed --teamcity              # seed-teamcity.sh runs this at its end
#   ./seed --teamcity --running    # ... and leaves the fixture's running build running
#   ./seed-teamcity-builds.sh [--running]   # against an already set-up server
#
# The fixture is READ-ONLY here, as it is for seed-gitea.sh.
#
# WHAT IS DERIVED AND FROM WHERE. The ids are a cross-stream contract, and the
# rules are mockd's (crates/knobas-mockd/src/tc_state.rs, "The transcription
# rules"), reproduced rather than re-decided:
#
#   build type id        the fixture's `cfg`, byte for byte
#   project id and name  the `cfg` prefix before the first `_`
#   build type name      the rest of the `cfg`, `_` as space
#   build number         the fixture's `num`, by setting the configuration's
#                        build number counter before its first build
#   branch               the fixture's `branch`; `main` is the VCS root's
#                        default branch and is queued as `<default>` (no
#                        branchName), so the server marks the build
#                        `defaultBranch: true` -- see queue_build
#   description          none -- the dataset describes no configuration, and
#                        this script invents no prose either
#
# Two things the fixture does not say and this script has to decide: which
# repository a configuration builds (`repo_for_project` below: the Payout
# configurations build payout-service and Ledger builds ledger-api -- the
# fixture's tickets and branches point that way and nothing in it says
# otherwise), and what the build steps do. The steps reproduce the fixture's
# OUTCOME and nothing more: Ledger_Deploy_Staging succeeds,
# Payout_IntegrationTests prints the fixture's `log` and exits non-zero, and
# Payout_Build -- the fixture's running build, "step 3/5 `cargo test`" -- has
# five steps of which the third is named `cargo test` and sleeps. The other
# four are named `step 1` .. `step 5`, because the fixture names only the
# third.
#
# ENDPOINT AND FIELD PROVENANCE. Every /app/rest path and body shape below is
# in the vendored testenv/specs/teamcity.json (extracted from this very image:
# `addProject`, `addVcsRoot`, `createBuildType`, `addVcsRootToBuildType`,
# `addBuildStepToBuildType`, `addBuildToQueue`, `getBuild`, `getAllBuilds`,
# `getAllAgents`, `cancelBuild`), with three exceptions that the swagger the
# server serves does not list, each read off the running container on
# 2026-09-02 (TeamCity 2026.1.3, build 222742):
#
#   * the property NAMES of a Git VCS root -- the spec types `properties` as
#     an open name/value list; the names (`url`, `branch`,
#     `teamcity:branchSpec`, `authMethod`, `username`, `secure:password`)
#     are the <input name=...> fields of the server's own Git root form,
#     /opt/teamcity/webapps/ROOT/plugins/jetbrains.git/gitSettings.jsp, and
#     WEB-INF/tags/branchSpecProperty.tag for the spec;
#   * the property names of a command-line step (`use.custom.script`,
#     `script.content`): plugins/commandLineRunner/simpleRunnerParams.jsp;
#     the runner type `simpleRunner` is named in the spec's own locator
#     documentation;
#   * `/app/rest/buildTypes/<locator>/settings/buildNumberCounter`, a
#     text/plain GET/PUT that JetBrains documents under "Build Configuration
#     Settings" and this server answers, but which the extracted swagger
#     lists no path for (only `settingsFile`). Verified by PUT-then-GET here.
#
# The build log is `/downloadBuildLog.html?buildId=<id>` -- not /app/rest at
# all (the spec's `/app/rest/builds/<locator>/log` is POST-only, for ADDING a
# message). It is the URL behind the build log page's "Download" link
# (buildLog/buildLog.jsp in the same webapp), and it answers a bearer token
# like the REST paths do.
#
# The image all of that was read off is the one seed-teamcity.sh pins in
# VERIFIED_TEAMCITY_IMAGE and refuses to run against a substitute for. Through
# `./seed --teamcity` this script runs after that guard; a standalone run
# trusts that the container is still that image.
#
# WHAT THIS CANNOT REPRODUCE -- see README.md, "TeamCity, end to end": the
# server assigns build ids and timestamps, a build takes as long as it takes,
# and a build queued through this token is triggered by `knobas`, not by the
# fixture's `mara` or by a VCS trigger. The real ids go to seed-state.json.
#
# IDEMPOTENT: every create is preceded by a read. A finished build by number
# on its configuration is never re-triggered; a queued or running one from an
# interrupted run is waited for instead of duplicated. A re-run creates
# nothing and exits 0.
set -eu
cd "$(dirname "$0")"

# Fixed, not `${KNOBAS_TEAMCITY_URL:-...}`: see seed-teamcity.sh for why.
TC_URL=http://127.0.0.1:8111
STATE=seed-state.json
FIXTURE=${FIXTURE:-../fixtures/tidewater/work.json}
# Inside the compose network, where the server fetches and the agent clones;
# the host-side 127.0.0.1:3000 means nothing to either container.
GITEA_INTERNAL_URL=http://gitea:3000
GITEA_ORG=tidewater
# The default branch of every seeded repository and so of every VCS root.
# A build on it is queued with no branchName at all -- see queue_build.
DEFAULT_BRANCH=main
# How long one build may take from queue to finish before this script gives
# up on it. Both finished shapes took well under a minute here; the first
# build of a configuration also fetches the repository.
BUILD_TIMEOUT=600

RUNNING=0
for arg in "$@"; do
  case "$arg" in
    --running) RUNNING=1 ;;
    *) echo "seed-teamcity-builds: unknown argument '$arg'; usage: ./seed-teamcity-builds.sh [--running]" >&2; exit 2 ;;
  esac
done

say() { echo "seed-teamcity-builds: $*"; }
die() { echo "seed-teamcity-builds: $*" >&2; exit 1; }

for tool in curl jq; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
[ -r "$FIXTURE" ] || die "$FIXTURE not found"
[ -r "$STATE" ] || die "$STATE not found -- run ./seed and ./seed --teamcity first"
TOKEN=$(jq -r '.teamcity.token // empty' "$STATE")
[ -n "$TOKEN" ] || die "$STATE has no teamcity.token -- run ./seed --teamcity first"
GITEA_TOKEN=$(jq -r '.gitea.token // empty' "$STATE")
[ -n "$GITEA_TOKEN" ] || die "$STATE has no gitea.token -- run ./seed first; the VCS roots authenticate with it"

jqf() { jq -r "$1" "$FIXTURE"; }

# --------------------------------------------------------------------------
# HTTP. seed-gitea.sh's `api` shape (an optional JSON body as the last
# argument), always as the bearer of the token; never exits on an HTTP error,
# because 404 is the normal answer to "is it there yet". --max-time for the
# reason seed-teamcity.sh gives.
REST_STATUS=0
REST_BODY=''
rest() {  # rest <METHOD> <path> [<json body>]
  _m=$1; _p=$2
  if [ "$#" -gt 2 ]; then
    _raw=$(curl -sS --max-time 60 -X "$_m" -H "Authorization: Bearer $TOKEN" \
                -H 'Content-Type: application/json' -H 'Accept: application/json' \
                -w '\n%{http_code}' -d "$3" "$TC_URL$_p") || _raw='
000'
  else
    _raw=$(curl -sS --max-time 60 -X "$_m" -H "Authorization: Bearer $TOKEN" \
                -H 'Accept: application/json' -w '\n%{http_code}' "$TC_URL$_p") || _raw='
000'
  fi
  REST_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  REST_BODY=$(printf '%s' "$_raw" | sed '$d')
}

# The text/plain resources (a setting's value). Sets REST_STATUS and
# REST_BODY exactly as `rest` does, and deliberately does not print the body
# instead: a caller wrapping it in `$(...)` would run it in a subshell, and
# the status it set there would never reach the caller's check.
rest_text() {  # rest_text <METHOD> <path> [<text body>]
  _m=$1; _p=$2
  if [ "$#" -gt 2 ]; then
    _raw=$(curl -sS --max-time 60 -X "$_m" -H "Authorization: Bearer $TOKEN" \
                -H 'Content-Type: text/plain' -H 'Accept: text/plain' \
                -w '\n%{http_code}' --data "$3" "$TC_URL$_p") || _raw='
000'
  else
    _raw=$(curl -sS --max-time 60 -X "$_m" -H "Authorization: Bearer $TOKEN" \
                -H 'Accept: text/plain' -w '\n%{http_code}' "$TC_URL$_p") || _raw='
000'
  fi
  REST_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  REST_BODY=$(printf '%s' "$_raw" | sed '$d')
}

expect() {  # expect <what> <status>...
  _what=$1; shift
  for _s in "$@"; do [ "$REST_STATUS" = "$_s" ] && return 0; done
  die "$_what: HTTP $REST_STATUS: $(printf '%s' "$REST_BODY" | head -c 400)"
}

# --------------------------------------------------------------------------
# The derivation table (tc_state.rs), in shell.
project_of() { printf '%s' "${1%%_*}"; }               # Payout_Build -> Payout
name_of()    { printf '%s' "${1#*_}" | tr '_' ' '; }   # Ledger_Deploy_Staging -> Deploy Staging

# What the fixture does not say: the repository behind a project. Kept in one
# place, like seed-gitea.sh's synth_branch, so it reads as a decision and not
# as data.
repo_for_project() {  # repo_for_project <project id> -> repo name
  case "$1" in
    Payout) echo payout-service ;;
    Ledger) echo ledger-api ;;
    *) die "no repository decided for project $1 -- add it to repo_for_project" ;;
  esac
}
# One VCS root per project, since both Payout configurations build the same
# repository: Payout_PayoutService, Ledger_LedgerApi.
vcs_root_id() {  # vcs_root_id <project id>
  printf '%s_%s' "$1" "$(repo_for_project "$1" \
    | awk -F- '{ for (i = 1; i <= NF; i++) $i = toupper(substr($i, 1, 1)) substr($i, 2) } 1' OFS=)"
}

# The branch spec: every branch the seed put in the repository, one
# `+:refs/heads/(name)` line each, so a build on any of them carries that
# name as its `branchName` -- the parentheses make the whole ref the logical
# name, which is how the server answers `feature/PAY-231-sepa-retry` rather
# than `PAY-231-sepa-retry`. The fixture's `branches` are all in
# payout-service; ledger-api has main and the head seed-gitea.sh synthesized
# for pull request 139, which seed-state.json records.
branch_spec() {  # branch_spec <repo>
  {
    jqf ".branches[] | select(.repo==\"$1\") | .name"
    jq -r --arg r "$1" '.synthesized_branches[]? | select(.repo==$r) | .branch' "$STATE"
    echo main
  } | awk '!seen[$0]++ { printf "+:refs/heads/(%s)\n", $0 }'
}

# --------------------------------------------------------------------------
# The step scripts. Each is the whole `script.content` of one command-line
# step, so a heredoc below is the build's shell script, not this script's.
step_script() {  # step_script <build type id> -> script on stdout
  case "$1" in
    Ledger_Deploy_Staging)
      cat <<'EOF'
# Seeded by testenv/seed-teamcity-builds.sh. The fixture records only the
# outcome of this configuration's build (success) and no log.
echo "Ledger_Deploy_Staging: the fixture records a successful build here, and this is it."
EOF
      ;;
    Payout_IntegrationTests)
      # The fixture's `log`, verbatim, then the failure. The quoted heredoc
      # inside the build script means nothing in the log is interpreted.
      printf '%s\n' "# Seeded by testenv/seed-teamcity-builds.sh: the fixture's log, then the fixture's outcome." \
                    "cat <<'KNOBAS_FIXTURE_LOG'"
      jqf '.builds[] | select(.cfg=="Payout_IntegrationTests") | .log' | sed '/^$/d'
      printf '%s\n' "KNOBAS_FIXTURE_LOG" "exit 1"
      ;;
    *) die "no step script for $1" ;;
  esac
}

# --------------------------------------------------------------------------
ensure_project() {  # ensure_project <id>
  rest GET "/app/rest/projects/id:$1"
  [ "$REST_STATUS" = "200" ] && return 0
  [ "$REST_STATUS" = "404" ] || expect "read project $1" 200
  # newProjectDescription: id, name, parentProject. No description (see the header).
  rest POST /app/rest/projects "$(jq -n --arg i "$1" '{id:$i, name:$i, parentProject:{id:"_Root"}}')"
  expect "create project $1" 200
  say "created project $1"
}

ensure_vcs_root() {  # ensure_vcs_root <project id>
  _proj=$1; _id=$(vcs_root_id "$_proj"); _repo=$(repo_for_project "$_proj")
  rest GET "/app/rest/vcs-roots/id:$_id"
  [ "$REST_STATUS" = "200" ] && return 0
  [ "$REST_STATUS" = "404" ] || expect "read VCS root $_id" 200
  # PASSWORD with the seed's Gitea token as the password: Gitea takes an access
  # token there for git-over-HTTP, and it is the credential seed-state.json
  # already holds. `branch` is the default branch, spelled as a ref.
  _body=$(jq -n --arg i "$_id" --arg p "$_proj" --arg r "$_repo" \
               --arg url "$GITEA_INTERNAL_URL/$GITEA_ORG/$_repo.git" \
               --arg spec "$(branch_spec "$_repo")" --arg tok "$GITEA_TOKEN" \
               --arg default "$DEFAULT_BRANCH" \
    '{id:$i, name:($p + " / " + $r), vcsName:"jetbrains.git", project:{id:$p},
      properties:{property:[
        {name:"url", value:$url},
        {name:"branch", value:("refs/heads/" + $default)},
        {name:"teamcity:branchSpec", value:$spec},
        {name:"authMethod", value:"PASSWORD"},
        {name:"username", value:"knobas"},
        {name:"secure:password", value:$tok}]}}')
  rest POST /app/rest/vcs-roots "$_body"
  expect "create VCS root $_id" 200
  say "created VCS root $_id -> $GITEA_INTERNAL_URL/$GITEA_ORG/$_repo.git"
}

add_step() {  # add_step <build type id> <step name> <script>
  rest POST "/app/rest/buildTypes/id:$1/steps" "$(jq -n --arg n "$2" --arg s "$3" \
      '{name:$n, type:"simpleRunner", properties:{property:[
         {name:"use.custom.script", value:"true"}, {name:"script.content", value:$s}]}}')"
  expect "add step \"$2\" to $1" 200
}

# Payout_Build's five steps. The third is the fixture's `cargo test` and holds
# the build where the fixture holds it; it sleeps for four hours rather than
# forever so an unattended environment gets its one agent back. README.md
# says how to end it sooner.
add_running_steps() {
  for _n in 1 2 3 4 5; do
    if [ "$_n" = "3" ]; then
      # shellcheck disable=SC2016  # the backticks are literal text in the build's own script
      add_step Payout_Build 'cargo test' '# Seeded by testenv/seed-teamcity-builds.sh: the fixture holds Payout_Build at "step 3/5 `cargo test`".
echo "cargo test: this is the fixture'"'"'s running build, and it stays here (four hours at most)."
sleep 14400'
    else
      add_step Payout_Build "step $_n" "echo \"step $_n of 5: the fixture names only the third\""
    fi
  done
  say "added the five steps to Payout_Build (the third is \`cargo test\`, and it waits)"
}

ensure_build_type() {  # ensure_build_type <id>
  _id=$1; _proj=$(project_of "$_id"); _name=$(name_of "$_id")
  rest GET "/app/rest/buildTypes/id:$_id"
  if [ "$REST_STATUS" = "404" ]; then
    rest POST /app/rest/buildTypes "$(jq -n --arg i "$_id" --arg n "$_name" --arg p "$_proj" \
        '{id:$i, name:$n, project:{id:$p}}')"
    expect "create build configuration $_id" 200
    say "created build configuration $_id (\"$_name\" in $_proj)"
  else
    expect "read build configuration $_id" 200
  fi

  # -- its VCS root ---------------------------------------------------------
  _root=$(vcs_root_id "$_proj")
  rest GET "/app/rest/buildTypes/id:$_id/vcs-root-entries"
  expect "list VCS roots of $_id" 200
  if [ "$(printf '%s' "$REST_BODY" | jq --arg r "$_root" '[."vcs-root-entry"[]? | select(.id==$r)] | length')" = "0" ]; then
    rest POST "/app/rest/buildTypes/id:$_id/vcs-root-entries" \
        "$(jq -n --arg r "$_root" '{"vcs-root":{id:$r}}')"
    expect "attach VCS root $_root to $_id" 200
    say "attached VCS root $_root to $_id"
  fi

  # -- its steps ------------------------------------------------------------
  rest GET "/app/rest/buildTypes/id:$_id/steps"
  expect "list steps of $_id" 200
  if [ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" = "0" ]; then
    if [ "$_id" = "Payout_Build" ]; then
      add_running_steps
    else
      add_step "$_id" "$_name" "$(step_script "$_id")"
      say "added the step to $_id"
    fi
  fi
}

# The build by its fixture number, if the server has one in any state -- and
# failing that, a QUEUED build of the configuration. A build takes its number
# when it starts, so a queued one has none yet and no `number:` locator can
# find it (a queued build answers no `number` field at all, 2026-09-02); this
# script queues at most one build per configuration, so a queued one is what
# an interrupted run left behind, and it is waited for rather than doubled.
# Sets FOUND_ID, FOUND_STATE, FOUND_STATUS (empty when none).
find_build() {  # find_build <build type id> <number>
  rest GET "/app/rest/builds?locator=buildType:(id:$1),number:$2,defaultFilter:false,state:any&fields=count,build(id,state,status)"
  expect "look for build $2 of $1" 200
  if [ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" = "0" ]; then
    rest GET "/app/rest/builds?locator=buildType:(id:$1),defaultFilter:false,state:queued&fields=count,build(id,state,status)"
    expect "look for a queued build of $1" 200
  fi
  FOUND_ID=$(printf '%s' "$REST_BODY" | jq -r '.build[0].id // empty')
  FOUND_STATE=$(printf '%s' "$REST_BODY" | jq -r '.build[0].state // empty')
  FOUND_STATUS=$(printf '%s' "$REST_BODY" | jq -r '.build[0].status // empty')
}

# The counter is what makes the fixture's number come out; set only while
# the build with that number does not exist yet, and only when it differs.
ensure_counter() {  # ensure_counter <build type id> <number>
  rest_text GET "/app/rest/buildTypes/id:$1/settings/buildNumberCounter"
  [ "$REST_STATUS" = "200" ] || die "reading the build number counter of $1 answered $REST_STATUS"
  _cur=$REST_BODY
  [ "$_cur" = "$2" ] && return 0
  rest_text PUT "/app/rest/buildTypes/id:$1/settings/buildNumberCounter" "$2"
  [ "$REST_STATUS" = "200" ] && [ "$REST_BODY" = "$2" ] \
    || die "setting the build number counter of $1 to $2 answered $REST_STATUS '$REST_BODY'"
  say "build number counter of $1: $_cur -> $2"
}

# Queue a build on the fixture's branch. Prints the new build's id.
#
# The default branch is queued WITHOUT a branchName. The VCS root's branch
# specification names `main` as well as the feature branches (every branch
# the Gitea seed created), and TeamCity resolves an explicit
# `branchName: main` against that specification to a logical branch called
# `main` that is a different branch from `<default>` -- the build then comes
# out `defaultBranch: false`, and every REST listing under the server's
# default filter hides it. Measured on 2026-09-02 (issue #266): queued with no
# branch, a build is `<default>` while it waits and `main` with
# `defaultBranch: true` once it runs, which is the branch the fixture means.
queue_build() {  # queue_build <build type id> <branch> <number>
  if [ "$2" = "$DEFAULT_BRANCH" ]; then
    rest POST /app/rest/buildQueue "$(jq -n --arg b "$1" --arg n "$3" \
        '{buildType:{id:$b},
          comment:{text:("testenv/seed-teamcity-builds.sh: fixture build " + $n)}}')"
  else
    rest POST /app/rest/buildQueue "$(jq -n --arg b "$1" --arg br "$2" --arg n "$3" \
        '{buildType:{id:$b}, branchName:$br,
          comment:{text:("testenv/seed-teamcity-builds.sh: fixture build " + $n)}}')"
  fi
  expect "queue a build of $1 on $2" 200
  printf '%s' "$REST_BODY" | jq -r '.id'
}

# Poll until the build's state is one of the given ones, or BUILD_TIMEOUT
# passes. Leaves the last answer in REST_BODY.
wait_for_state() {  # wait_for_state <build id> <states...>
  _bid=$1; shift
  _deadline=$(( $(date +%s) + BUILD_TIMEOUT ))
  printf 'seed-teamcity-builds: waiting for build id %s to be %s ' "$_bid" "$*"
  while :; do
    rest GET "/app/rest/builds/id:$_bid?fields=id,number,state,status,branchName,buildTypeId,failedToStart,statusText"
    expect "read build id $_bid" 200
    _st=$(printf '%s' "$REST_BODY" | jq -r '.state')
    for _want in "$@"; do
      [ "$_st" = "$_want" ] && { echo " $_st"; return 0; }
    done
    [ "$(date +%s)" -lt "$_deadline" ] || { echo; die "build id $_bid is still $_st after ${BUILD_TIMEOUT}s: $(printf '%s' "$REST_BODY" | jq -c .)"; }
    printf '[%s] ' "$_st"; sleep 5
  done
}

# A finished build's shape must be the fixture's, or the seed has not done
# what it says. `$REST_BODY` is the build as wait_for_state last read it.
assert_finished() {  # assert_finished <build type id> <number> <branch> <SUCCESS|FAILURE>
  _got=$(printf '%s' "$REST_BODY" | jq -r '[.state, .status, .branchName, .number, .buildTypeId, (.failedToStart // false | tostring)] | join(" ")')
  [ "$_got" = "finished $4 $3 $2 $1 false" ] \
    || die "build $2 of $1 came out as '$_got', wanted 'finished $4 $3 $2 $1 false' (statusText: $(printf '%s' "$REST_BODY" | jq -r .statusText))"
}

# The failed build's log carries every line of the fixture's `log`.
assert_log() {  # assert_log <build id> <build type id>
  _log=$(curl -sS --max-time 60 -H "Authorization: Bearer $TOKEN" "$TC_URL/downloadBuildLog.html?buildId=$1") \
    || die "downloading the log of build id $1 failed"
  # The loop is a subshell (a pipeline), so it reports the missing line
  # itself and the `||` after it is what stops the script.
  jqf ".builds[] | select(.cfg==\"$2\") | .log // empty" | sed '/^$/d' | while IFS= read -r _line; do
    printf '%s' "$_log" | grep -qF -- "$_line" \
      || { echo "seed-teamcity-builds: the log of build id $1 lacks the fixture line: $_line" >&2; exit 1; }
  done || die "build id $1 does not carry the fixture's log"
}

# --------------------------------------------------------------------------
rest GET /app/rest/server
expect "GET /app/rest/server as the recorded token" 200
say "server $(printf '%s' "$REST_BODY" | jq -r .version), token ok"

rest GET '/app/rest/agents?locator=connected:true,authorized:true,enabled:true'
expect "list agents" 200
[ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" -ge 1 ] \
  || die "no connected, authorised, enabled agent -- nothing would run the builds (./seed --teamcity authorises one)"

# 1. Projects, VCS roots, build configurations, ascending by configuration id
#    as tc_state.rs lists them (first appearance in the fixture would make
#    the order a matter of presentation).
for cfg in $(jqf '[.builds[].cfg] | unique | .[]'); do
  proj=$(project_of "$cfg")
  ensure_project "$proj"
  ensure_vcs_root "$proj"
  ensure_build_type "$cfg"
done

# 2. The finished shapes, always: 412 on Ledger_Deploy_Staging (success) and
#    1187 on Payout_IntegrationTests (failure). Queued together -- the one
#    agent takes them in turn -- then each waited for and checked.
# Lines split on whitespace and fields on `|`, which is safe because a git
# refname cannot contain a space and a TeamCity id is [A-Za-z0-9_]: nothing
# the fixture can put in a field carries a separator.
BUILD_MAP=''   # "<fixture num> <build type> <real id>" lines, for seed-state.json
QUEUED=''      # "<fixture num>|<build type>|<branch>|<status>|<real id>" lines to wait for
for num in $(jqf '.builds[] | select(.status != "running") | .num'); do
  cfg=$(jqf ".builds[] | select(.num==$num) | .cfg")
  branch=$(jqf ".builds[] | select(.num==$num) | .branch")
  status=$(jqf ".builds[] | select(.num==$num) | if .status == \"failed\" then \"FAILURE\" else \"SUCCESS\" end")
  find_build "$cfg" "$num"
  if [ -n "$FOUND_ID" ] && [ "$FOUND_STATE" = "finished" ]; then
    say "build $num of $cfg already exists as id $FOUND_ID ($FOUND_STATUS)"
    BUILD_MAP="$BUILD_MAP$num $cfg $FOUND_ID
"
    continue
  fi
  if [ -n "$FOUND_ID" ]; then
    say "build $num of $cfg is already $FOUND_STATE as id $FOUND_ID (an earlier run); waiting for it"
    # A queued build takes its number when it starts, so the counter still
    # decides what it comes out as; a running one has its number already.
    [ "$FOUND_STATE" = "queued" ] && ensure_counter "$cfg" "$num"
    bid=$FOUND_ID
  else
    ensure_counter "$cfg" "$num"
    bid=$(queue_build "$cfg" "$branch" "$num")
    say "queued build $num of $cfg on $branch as id $bid"
  fi
  QUEUED="$QUEUED$num|$cfg|$branch|$status|$bid
"
done

for line in $QUEUED; do
  num=${line%%|*}; line=${line#*|}
  cfg=${line%%|*}; line=${line#*|}
  branch=${line%%|*}; line=${line#*|}
  status=${line%%|*}; bid=${line#*|}
  wait_for_state "$bid" finished
  assert_finished "$cfg" "$num" "$branch" "$status"
  if [ "$status" = "FAILURE" ]; then assert_log "$bid" "$cfg"; fi
  say "build $num of $cfg finished $status on $branch (id $bid)"
  BUILD_MAP="$BUILD_MAP$num $cfg $bid
"
done

# 3. The running shape, only when asked: 1188 on Payout_Build, held at step 3.
RUNNING_LINE=''
for num in $(jqf '.builds[] | select(.status == "running") | .num'); do
  cfg=$(jqf ".builds[] | select(.num==$num) | .cfg")
  branch=$(jqf ".builds[] | select(.num==$num) | .branch")
  find_build "$cfg" "$num"
  if [ -n "$FOUND_ID" ]; then
    case "$FOUND_STATE" in
      finished) say "build $num of $cfg already exists as id $FOUND_ID (finished, $FOUND_STATUS); delete it for --running to start it again: curl -X DELETE -H 'Authorization: Bearer \$KNOBAS_TEAMCITY_TOKEN' $TC_URL/app/rest/builds/id:$FOUND_ID" ;;
      queued)
        # An interrupted run left it in the queue. The agent is free now that
        # the finished shapes are done, so it starts within seconds.
        say "build $num of $cfg is queued as id $FOUND_ID (an earlier run); waiting for it to start"
        ensure_counter "$cfg" "$num"
        wait_for_state "$FOUND_ID" running finished
        FOUND_STATE=$(printf '%s' "$REST_BODY" | jq -r .state)
        say "build $num of $cfg is $FOUND_STATE as id $FOUND_ID" ;;
      *) say "build $num of $cfg is $FOUND_STATE as id $FOUND_ID" ;;
    esac
    BUILD_MAP="$BUILD_MAP$num $cfg $FOUND_ID
"
    [ "$FOUND_STATE" = "running" ] && RUNNING_LINE="$num $cfg $FOUND_ID"
    continue
  fi
  if [ "$RUNNING" = "1" ]; then
    ensure_counter "$cfg" "$num"
    bid=$(queue_build "$cfg" "$branch" "$num")
    say "queued build $num of $cfg on $branch as id $bid"
    wait_for_state "$bid" running finished
    [ "$(printf '%s' "$REST_BODY" | jq -r .state)" = "running" ] \
      || die "build $num of $cfg finished instead of running: $(printf '%s' "$REST_BODY" | jq -c .)"
    _br=$(printf '%s' "$REST_BODY" | jq -r .branchName)
    [ "$_br" = "$branch" ] || die "build $num of $cfg runs on '$_br', wanted '$branch'"
    BUILD_MAP="$BUILD_MAP$num $cfg $bid
"
    RUNNING_LINE="$num $cfg $bid"
  else
    say "build $num of $cfg (the fixture's running build) is not started: pass --running to hold it at step 3/5"
  fi
done

# 4. seed-state.json: fixture number -> real id, under the teamcity block
#    seed-teamcity.sh wrote, without disturbing the rest of it.
BUILDS_JSON=$(printf '%s' "$BUILD_MAP" | jq -R -s '
  split("\n") | map(select(length > 0) | split(" ")
  | {fixture_number: (.[0] | tonumber), build_type: .[1], real_id: (.[2] | tonumber)})
  | sort_by(.fixture_number)')
jq --argjson b "$BUILDS_JSON" '.teamcity += {builds: $b}' "$STATE" > "$STATE.tmp"
mv "$STATE.tmp" "$STATE"
say "wrote $STATE (teamcity.builds: $(printf '%s' "$BUILDS_JSON" | jq -r 'map("\(.fixture_number)->\(.real_id)") | join(", ")'))"

if [ -n "$RUNNING_LINE" ]; then
  # shellcheck disable=SC2086  # three words, split on purpose
  set -- $RUNNING_LINE
  echo
  say "build $1 of $2 is RUNNING as id $3 and holds the agent for up to four hours. To end it now:"
  echo "  curl -X POST -H 'Authorization: Bearer \$KNOBAS_TEAMCITY_TOKEN' -H 'Content-Type: application/json' \\"
  echo "       -d '{\"comment\":\"released by hand\",\"readdIntoQueue\":false}' $TC_URL/app/rest/builds/id:$3"
  echo "  (it finishes with status UNKNOWN, TeamCity's shape for a cancelled build; delete it for --running to start it again)"
fi
say "done"
