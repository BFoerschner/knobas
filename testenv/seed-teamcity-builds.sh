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
# the host-side 127.0.0.1:3000 means nothing to either container. knobas
# matches a change's commit through the Gitea source's `clone_urls`, which
# name this address (#683).
GITEA_INTERNAL_URL=http://gitea:3000
# Gitea's published port, which this script writes the change witness's
# branch through.
GITEA_URL=http://127.0.0.1:3000
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
    "$CHAIN_PROJECT") echo payout-service ;;
    *) die "no repository decided for project $1 -- add it to repo_for_project" ;;
  esac
}
# The change witness (#683): what a build lists under *Changes*. TeamCity
# lists none for a configuration's first build on a branch, and collects a
# change only for a commit it has never seen on any branch -- re-pointing a
# branch at a commit another branch already holds lists nothing (measured
# 2026-10-13). So this is the one place the TeamCity seed writes to Gitea:
# a branch of its own in ledger-api (no capped suite budgets that repository,
# and nothing writes to its default branch), cut at `main`'s head, built,
# given one commit -- one file, through the contents API -- and built again,
# so the second build lists that commit. The branch stays: the commit has to
# be on a branch for knobas' Gitea sync to mirror it. The configuration is the
# one whose build succeeds in seconds.
CHANGE_CONFIG=Ledger_Deploy_Staging
CHANGE_REPO=ledger-api
CHANGE_BRANCH=seed/change-witness

# The build chain (#1010): what the Builds view's chain line and the run's
# *Chain on <branch>* panel (#857) read, which the fixture has none of. It is
# NOT in fixtures/tidewater/work.json -- that file is a cross-stream contract
# (mockd's tc_state.rs, the seeds, the three exact-count live tests) -- so it
# lives in a project of its own under _Root, which no fixture configuration
# belongs to, and the live suite counts it apart from the fixture's three
# (seed-state.json's teamcity.chain). Three configurations, each waiting on the
# one before by a snapshot dependency, each building payout-service with one
# `echo` step. The project id is not a prefix of any fixture `cfg` and not the
# suite's own ChainProject (KnobasChain857), which it creates and deletes.
#
# One build of the last step on a feature branch queues the whole chain on it
# (a snapshot dependency queues what it depends on), so the three builds come
# from one request and none of the fixture's configurations is in the chain:
# no fixture build number moves.
CHAIN_PROJECT=SeededChain
# Step names that no fixture configuration is called: the app's `under:` search
# finds a configuration by its name, so a chain step named `Build` would be
# found beside Payout's `Build`.
CHAIN_STEPS="Compile Package Release"
CHAIN_BRANCH=feature/PAY-231-sepa-retry

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
    [ "$1" = "$CHANGE_REPO" ] && echo "$CHANGE_BRANCH"
    echo main
  } | awk '!seen[$0]++ { printf "+:refs/heads/(%s)\n", $0 }'
}

# --------------------------------------------------------------------------
# What the failed build reports beyond its log (#665), derived from the
# fixture's `log` rather than invented: the test is the one its
# `test <name> ... FAILED` line names, and its failure message is the line
# after it. The artifact is the log again, as a file, under a name this script
# decides.
SEEDED_ARTIFACT=sepa-report.txt
# The same file again inside a folder, so the run page's artifact tree has a
# folder to open and to download as a zip (#856).
SEEDED_FOLDER=reports
fixture_log() {  # fixture_log <build type id> -> the fixture's log, blank lines dropped
  jqf ".builds[] | select(.cfg==\"$1\") | .log" | sed '/^$/d'
}
failed_test_name() {  # failed_test_name <build type id>
  jqf ".builds[] | select(.cfg==\"$1\") | .log | capture(\"test (?<t>[^ ]+) [.][.][.] FAILED\").t"
}
failed_test_details() {  # failed_test_details <build type id>
  jqf ".builds[] | select(.cfg==\"$1\") | .log | capture(\"FAILED\\n(?<d>[^\\n]+)\").d"
}
# A value inside a TeamCity service message: `|`, `'`, `[`, `]` escaped by
# `|`, the grammar JetBrains documents for service messages.
tc_escape() {
  printf '%s' "$1" | sed -e 's/|/||/g' -e "s/'/|'/g" -e 's/\[/|[/g' -e 's/\]/|]/g'
}

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
      # The fixture's `log`, verbatim, then what a build view reads beyond
      # the log (#665): the failing test the log names, reported to TeamCity
      # as a failed test, and the log kept as an artifact -- then the failure.
      # The quoted heredocs inside the build script mean nothing in the log or
      # the service messages is interpreted.
      printf '%s\n' "# Seeded by testenv/seed-teamcity-builds.sh: the fixture's log, its failed test, an artifact, then the fixture's outcome." \
                    "cat <<'KNOBAS_FIXTURE_LOG'"
      fixture_log Payout_IntegrationTests
      printf '%s\n' "KNOBAS_FIXTURE_LOG" \
                    "cat > $SEEDED_ARTIFACT <<'KNOBAS_FIXTURE_LOG'"
      fixture_log Payout_IntegrationTests
      printf '%s\n' "KNOBAS_FIXTURE_LOG" "cat <<'KNOBAS_SERVICE_MESSAGES'"
      _test=$(tc_escape "$(failed_test_name Payout_IntegrationTests)")
      _why=$(tc_escape "$(failed_test_details Payout_IntegrationTests)")
      printf '%s\n' "##teamcity[publishArtifacts '$SEEDED_ARTIFACT']" \
                    "##teamcity[publishArtifacts '$SEEDED_ARTIFACT => $SEEDED_FOLDER']" \
                    "##teamcity[testStarted name='$_test']" \
                    "##teamcity[testFailed name='$_test' message='$_why']" \
                    "##teamcity[testFinished name='$_test']" \
                    "KNOBAS_SERVICE_MESSAGES" "exit 1"
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

# A root an older seed made heals here: its branch spec lacked the change
# witness's branch (#683). Put back only when it differs, so a current root
# costs one read.
heal_vcs_root() {  # heal_vcs_root <root id> <property> <value>
  rest_text GET "/app/rest/vcs-roots/id:$1/properties/$2"
  expect "read $2 of VCS root $1" 200 404
  [ "$REST_STATUS" = "200" ] && [ "$REST_BODY" = "$3" ] && return 0
  rest_text PUT "/app/rest/vcs-roots/id:$1/properties/$2" "$3"
  expect "set $2 of VCS root $1" 200
  say "VCS root $1: $2 set (an older seed made it)"
}

ensure_vcs_root() {  # ensure_vcs_root <project id>
  _proj=$1; _id=$(vcs_root_id "$_proj"); _repo=$(repo_for_project "$_proj")
  rest GET "/app/rest/vcs-roots/id:$_id"
  if [ "$REST_STATUS" = "200" ]; then
    heal_vcs_root "$_id" teamcity:branchSpec "$(branch_spec "$_repo")"
    return 0
  fi
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
  elif [ "$_id" != "Payout_Build" ]; then
    # A server seeded by an older version of this script keeps its older step
    # (#665 added the failed test and the artifact to Payout_IntegrationTests).
    # The one step's script is brought up to date in place; the build it
    # already ran is re-run by ensure_current_build.
    _step=$(printf '%s' "$REST_BODY" | jq -r '.step[0].id')
    _have=$(printf '%s' "$REST_BODY" | jq -r '.step[0].properties.property[]? | select(.name=="script.content") | .value')
    _want=$(step_script "$_id")
    if [ "$_have" != "$_want" ]; then
      _raw=$(curl -sS --max-time 60 -X PUT -H "Authorization: Bearer $TOKEN" \
                  -H 'Content-Type: text/plain' -H 'Accept: text/plain' -w '\n%{http_code}' \
                  --data-binary "$_want" \
                  "$TC_URL/app/rest/buildTypes/id:$_id/steps/$_step/parameters/script.content") || _raw='
000'
      REST_STATUS=$(printf '%s' "$_raw" | tail -n 1)
      REST_BODY=$(printf '%s' "$_raw" | sed '$d')
      expect "update the step script of $_id" 200
      say "updated the step script of $_id to this seed's"
    fi
  fi
}

# A finished build that an older seed ran is not this seed's build: the
# failed one must carry the failed test its step now reports (#665), and
# every one must have run from the root's own URL -- a build keeps the VCS
# root instance it ran from, and a branch of #683 once pointed the roots at
# another address, so a build from then names its revision by that one. Such
# a build is deleted, so the loop below runs it again under the same number --
# its id changes, and seed-state.json records the new one.
ensure_current_build() {  # ensure_current_build <build id> <FAILURE|SUCCESS>
  rest GET "/app/rest/builds/id:$1?fields=testOccurrences(failed),revisions(revision(vcs-root-instance(properties(property(name,value)))))"
  expect "read build id $1" 200
  _why=''
  case "$(printf '%s' "$REST_BODY" | jq -r '[.revisions.revision[]?."vcs-root-instance".properties.property[]? | select(.name=="url") | .value][0] // ""')" in
    "$GITEA_INTERNAL_URL"/*) ;;
    *) _why="it ran from another VCS root URL" ;;
  esac
  if [ "$2" = "FAILURE" ] && [ "$(printf '%s' "$REST_BODY" | jq '.testOccurrences.failed // 0')" -lt 1 ]; then
    _why="it reports no failed test"
  fi
  if [ -z "$_why" ] && [ "$2" = "FAILURE" ]; then
    # #856: the failed build publishes the artifact folder too.
    rest GET "/app/rest/builds/id:$1/artifacts?fields=file(name)"
    expect "list the artifacts of build id $1" 200
    if ! printf '%s' "$REST_BODY" | jq -e --arg f "$SEEDED_FOLDER" '[.file[]?.name] | index($f)' >/dev/null; then
      _why="it publishes no $SEEDED_FOLDER/ folder"
    fi
  fi
  [ -z "$_why" ] && return 0
  rest DELETE "/app/rest/builds/id:$1"
  expect "delete build id $1, which an older seed ran" 200 204
  say "deleted build id $1: an older seed ran it and $_why; running it again"
  FOUND_ID=''
}

# --------------------------------------------------------------------------
# What a build configuration's source view reads beyond its steps and VCS root
# (#664): parameters -- one the project defines, which the configuration
# inherits, one of its own, and one password -- and a trigger. The fixture says
# nothing about either, so these are decisions, kept here in one place, and
# all on Payout_IntegrationTests, the configuration with the failed build.
#
#   project Payout              tidewater.region = eu-central-1   (inherited)
#   Payout_IntegrationTests     env.SEPA_RETRIES = 5
#                               env.SEPA_SANDBOX_KEY = <password, SETTINGS_SECRET>
#                               env.LOG_LEVEL = info  (a select of info, debug; #852)
#                               a VCS trigger on +:feature/*, DISABLED
#
# The trigger is disabled on purpose: the Gitea live suite pushes branches to
# payout-service, and an enabled VCS trigger would queue a build on each push,
# which spends the agent and moves the build counts the seeded TeamCity suite
# asserts on (one environment, one fixture). A disabled trigger is still a
# trigger the REST API answers with its type and properties.
#
# The password's value is a made-up seed string, recorded in seed-state.json
# so the live suite can prove it never appears in a view. TeamCity masks it on
# every read; the suite asserts that too.
#
# Endpoints: `setParameter` on a project and on a build type (PUT
# .../parameters/<name>, idempotent: the same body twice is the same
# parameter) and `addTriggerToBuildType` (POST .../triggers), all in the
# vendored testenv/specs/teamcity.json. A password parameter is a parameter
# whose `type.rawValue` is `password display='hidden'`, the spec TeamCity's
# own UI writes for one.
SETTINGS_CONFIG=Payout_IntegrationTests
SETTINGS_SECRET=knobas-seed-sepa-sandbox-key
ensure_parameter() {  # ensure_parameter <projects|buildTypes> <id> <name> <value> [<spec>]
  if [ "$#" -gt 4 ]; then
    _body=$(jq -n --arg n "$3" --arg v "$4" --arg t "$5" '{name:$n, value:$v, type:{rawValue:$t}}')
  else
    _body=$(jq -n --arg n "$3" --arg v "$4" '{name:$n, value:$v}')
  fi
  rest PUT "/app/rest/$1/id:$2/parameters/$3" "$_body"
  expect "set parameter $3 on $2" 200
}

ensure_configuration_settings() {
  ensure_parameter projects "$(project_of "$SETTINGS_CONFIG")" tidewater.region eu-central-1
  ensure_parameter buildTypes "$SETTINGS_CONFIG" env.SEPA_RETRIES 5
  ensure_parameter buildTypes "$SETTINGS_CONFIG" env.SEPA_SANDBOX_KEY "$SETTINGS_SECRET" "password display='hidden'"
  ensure_parameter buildTypes "$SETTINGS_CONFIG" env.LOG_LEVEL info "select data_1='info' data_2='debug' label_2='Debug' display='normal'"
  rest GET "/app/rest/buildTypes/id:$SETTINGS_CONFIG/triggers"
  expect "list triggers of $SETTINGS_CONFIG" 200
  if [ "$(printf '%s' "$REST_BODY" | jq '[.trigger[]? | select(.type=="vcsTrigger")] | length')" = "0" ]; then
    rest POST "/app/rest/buildTypes/id:$SETTINGS_CONFIG/triggers" \
      '{"type":"vcsTrigger","disabled":true,"properties":{"property":[
         {"name":"branchFilter","value":"+:feature/*"},
         {"name":"quietPeriodMode","value":"DO_NOT_USE"}]}}'
    expect "add a VCS trigger to $SETTINGS_CONFIG" 200
    say "added a disabled VCS trigger to $SETTINGS_CONFIG"
  fi
  say "parameters of $SETTINGS_CONFIG: tidewater.region (inherited), env.SEPA_RETRIES, env.SEPA_SANDBOX_KEY (password), env.LOG_LEVEL (select)"
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

# ... and reports the fixture's failed test, and the artifact (#665).
assert_reports() {  # assert_reports <build id> <build type id>
  _want=$(failed_test_name "$2")
  rest GET "/app/rest/testOccurrences?locator=build:(id:$1),status:FAILURE&fields=testOccurrence(name)"
  expect "read the failed tests of build id $1" 200
  [ "$(printf '%s' "$REST_BODY" | jq -r '[.testOccurrence[]?.name] | join(",")')" = "$_want" ] \
    || die "build id $1 reports failed tests $(printf '%s' "$REST_BODY" | jq -c '[.testOccurrence[]?.name]'), wanted [\"$_want\"]"
  rest GET "/app/rest/builds/id:$1/artifacts?fields=file(name)"
  expect "list the artifacts of build id $1" 200
  _want=$(printf '%s\n' "$SEEDED_ARTIFACT" "$SEEDED_FOLDER" | sort | paste -sd, -)
  [ "$(printf '%s' "$REST_BODY" | jq -r '[.file[]?.name] | sort | join(",")')" = "$_want" ] \
    || die "build id $1 publishes $(printf '%s' "$REST_BODY" | jq -c '[.file[]?.name]'), wanted the file and the folder: $_want"
}

# --------------------------------------------------------------------------
# Gitea, for the change witness's branch. Sets GITEA_STATUS and GITEA_BODY.
gitea() {  # gitea <METHOD> <path under /api/v1> [<json body>]
  if [ "$#" -gt 2 ]; then
    _raw=$(curl -sS --max-time 60 -X "$1" -H "Authorization: token $GITEA_TOKEN" \
                -H 'Content-Type: application/json' -H 'Accept: application/json' \
                -w '\n%{http_code}' -d "$3" "$GITEA_URL/api/v1$2") || _raw='
000'
  else
    _raw=$(curl -sS --max-time 60 -X "$1" -H "Authorization: token $GITEA_TOKEN" \
                -H 'Accept: application/json' -w '\n%{http_code}' "$GITEA_URL/api/v1$2") || _raw='
000'
  fi
  GITEA_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  GITEA_BODY=$(printf '%s' "$_raw" | sed '$d')
}

# Cut the witness branch afresh at <sha>, deleting it first if it is there.
cut_witness_branch() {  # cut_witness_branch <sha>
  gitea DELETE "/repos/$GITEA_ORG/$CHANGE_REPO/branches/$(printf '%s' "$CHANGE_BRANCH" | jq -sRr @uri)"
  case "$GITEA_STATUS" in
    204|404) ;;
    *) die "delete $CHANGE_BRANCH in $CHANGE_REPO: HTTP $GITEA_STATUS: $GITEA_BODY" ;;
  esac
  gitea POST "/repos/$GITEA_ORG/$CHANGE_REPO/branches" \
    "$(jq -n --arg b "$CHANGE_BRANCH" --arg r "$1" '{new_branch_name:$b, old_ref_name:$r}')"
  [ "$GITEA_STATUS" = "201" ] || die "cut $CHANGE_BRANCH at $1 in $CHANGE_REPO: HTTP $GITEA_STATUS: $GITEA_BODY"
}

# The witness's one commit, on its branch. Prints the commit's sha.
commit_witness() {
  gitea POST "/repos/$GITEA_ORG/$CHANGE_REPO/contents/seed-change-witness.txt" \
    "$(jq -n --arg b "$CHANGE_BRANCH" \
         --arg c "$(printf 'Written by testenv/seed-teamcity-builds.sh (#683): the commit a build lists as its change.\n' | base64)" \
         '{branch:$b, content:$c, message:"Seed: the change a TeamCity build lists (#683)"}')"
  [ "$GITEA_STATUS" = "201" ] || die "commit to $CHANGE_BRANCH in $CHANGE_REPO: HTTP $GITEA_STATUS: $GITEA_BODY"
  printf '%s' "$GITEA_BODY" | jq -r .commit.sha
}

# Ask TeamCity to look at the root now, and wait until it has seen the
# witness branch at <sha> -- a build queued sooner would take the old head.
wait_for_head() {  # wait_for_head <sha>
  _root=$(vcs_root_id "$(project_of "$CHANGE_CONFIG")")
  _deadline=$(( $(date +%s) + BUILD_TIMEOUT ))
  printf 'seed-teamcity-builds: waiting for TeamCity to see %s at %s ' "$CHANGE_BRANCH" "$1"
  while :; do
    rest POST "/app/rest/vcs-root-instances/checkingForChangesQueue?locator=vcsRoot:(id:$_root)" '{}'
    # `{"entry":[{"name":"refs/heads/...","value":"<sha>"}]}`, measured.
    rest GET "/app/rest/vcs-root-instances/vcsRoot:(id:$_root)/repositoryState"
    if [ "$REST_STATUS" = "200" ] && [ "$(printf '%s' "$REST_BODY" \
         | jq -r --arg b "refs/heads/$CHANGE_BRANCH" '.entry[]? | select(.name==$b) | .value')" = "$1" ]; then
      echo ok; return 0
    fi
    [ "$(date +%s)" -lt "$_deadline" ] || { echo; die "TeamCity never saw $CHANGE_BRANCH at $1: $REST_BODY"; }
    printf '.'; sleep 3
  done
}

# Build the witness branch as it is now, and check the build ran on <sha>.
# Prints nothing; leaves the build id in WITNESS_ID.
build_witness() {  # build_witness <sha>
  WITNESS_ID=$(queue_build "$CHANGE_CONFIG" "$CHANGE_BRANCH" "change witness")
  wait_for_state "$WITNESS_ID" finished
  rest GET "/app/rest/builds/id:$WITNESS_ID?fields=status,branchName,revisions(revision(version))"
  expect "read build id $WITNESS_ID" 200
  _got=$(printf '%s' "$REST_BODY" | jq -r '[.status, .branchName, .revisions.revision[0].version] | join(" ")')
  [ "$_got" = "SUCCESS $CHANGE_BRANCH $1" ] \
    || die "the change witness's build id $WITNESS_ID came out as '$_got', wanted 'SUCCESS $CHANGE_BRANCH $1'"
}

# The two builds, or the recorded pair where both are still there, the
# second still lists its change and the branch still holds it. Sets
# CHANGES_JSON for seed-state.json.
ensure_change_witness() {
  _branch=$(printf '%s' "$CHANGE_BRANCH" | jq -sRr @uri)
  _rec=$(jq -c '.teamcity.changes // empty' "$STATE")
  if [ -n "$_rec" ]; then
    _sha=$(printf '%s' "$_rec" | jq -r .sha)
    _b=$(printf '%s' "$_rec" | jq -r .before_id); _a=$(printf '%s' "$_rec" | jq -r .after_id)
    gitea GET "/repos/$GITEA_ORG/$CHANGE_REPO/branches/$_branch"
    _head=$(printf '%s' "$GITEA_BODY" | jq -r '.commit.id // empty' 2>/dev/null || true)
    rest GET "/app/rest/builds/id:$_b?fields=state"
    _bs=$REST_STATUS
    # Both builds ran on the fixture's one agent (#1560): a pair that ran on
    # an agent another worktree's compose registered is made again.
    _ag=1
    for _x in "$_b" "$_a"; do
      rest GET "/app/rest/builds/id:$_x?fields=agent(name)"
      [ "$(printf '%s' "$REST_BODY" | jq -r '.agent.name // empty' 2>/dev/null)" = "knobas-agent" ] || _ag=0
    done
    rest GET "/app/rest/changes?locator=build:(id:$_a)&fields=change(version)"
    if [ "$_ag" = 1 ] && [ "$_head" = "$_sha" ] && [ "$_bs" = "200" ] && [ "$REST_STATUS" = "200" ] \
       && [ "$(printf '%s' "$REST_BODY" | jq -r '[.change[]?.version] | join(",")')" = "$_sha" ]; then
      say "the change witness is there: build id $_a on $CHANGE_BRANCH lists $_sha (after build id $_b)"
      CHANGES_JSON=$_rec
      return 0
    fi
  fi

  # Anything an earlier run left on the branch goes first, so the pair is
  # the branch's first and second build.
  rest GET "/app/rest/builds?locator=buildType:(id:$CHANGE_CONFIG),branch:(name:$CHANGE_BRANCH),defaultFilter:false,state:any&fields=build(id)"
  expect "list the builds of $CHANGE_CONFIG on $CHANGE_BRANCH" 200
  for _old in $(printf '%s' "$REST_BODY" | jq -r '.build[]?.id'); do
    rest DELETE "/app/rest/builds/id:$_old"
    expect "delete build id $_old on $CHANGE_BRANCH" 200 204
    say "deleted build id $_old on $CHANGE_BRANCH (an earlier run's)"
  done

  gitea GET "/repos/$GITEA_ORG/$CHANGE_REPO/branches/$DEFAULT_BRANCH"
  [ "$GITEA_STATUS" = "200" ] || die "read $DEFAULT_BRANCH of $CHANGE_REPO: HTTP $GITEA_STATUS"
  _base=$(printf '%s' "$GITEA_BODY" | jq -r .commit.id)
  cut_witness_branch "$_base"
  wait_for_head "$_base"
  build_witness "$_base"; _b=$WITNESS_ID
  _sha=$(commit_witness)
  wait_for_head "$_sha"
  build_witness "$_sha"; _a=$WITNESS_ID

  rest GET "/app/rest/changes?locator=build:(id:$_a)&fields=change(version)"
  expect "read the changes of build id $_a" 200
  [ "$(printf '%s' "$REST_BODY" | jq -r '[.change[]?.version] | join(",")')" = "$_sha" ] \
    || die "build id $_a lists changes $(printf '%s' "$REST_BODY" | jq -c '[.change[]?.version]'), wanted [\"$_sha\"]"
  say "the change witness: build id $_a on $CHANGE_BRANCH lists $_sha (after build id $_b on $_base)"
  CHANGES_JSON=$(jq -n --arg c "$CHANGE_CONFIG" --arg r "$CHANGE_REPO" --arg br "$CHANGE_BRANCH" \
    --arg s "$_sha" --argjson b "$_b" --argjson a "$_a" \
    '{build_type:$c, repo:$r, branch:$br, sha:$s, before_id:$b, after_id:$a}')
}

# --------------------------------------------------------------------------
# The build chain (#1010). Sets CHAIN_JSON for seed-state.json.
CHAIN_JSON=''
chain_last() { printf '%s' "$CHAIN_STEPS" | awk '{ print $NF }'; }

# A chain whose builds name a commit this Gitea never held goes whole, for the
# reason drop_projects_built_from_another_gitea gives.
drop_chain_built_from_another_gitea() {
  rest GET "/app/rest/builds?locator=buildType:(id:${CHAIN_PROJECT}_$(chain_last)),defaultFilter:false,state:finished,count:1&fields=build(id,revisions(revision(version)))"
  [ "$REST_STATUS" = "200" ] || return 0
  _sha=$(printf '%s' "$REST_BODY" | jq -r '.build[0].revisions.revision[0].version // empty')
  [ -n "$_sha" ] || return 0
  gitea GET "/repos/$GITEA_ORG/$(repo_for_project "$CHAIN_PROJECT")/git/commits/$_sha"
  case "$GITEA_STATUS" in
    200) return 0 ;;
    404|422) ;;
    *) die "look for commit $_sha of the chain in Gitea: HTTP $GITEA_STATUS: $GITEA_BODY" ;;
  esac
  rest DELETE "/app/rest/projects/id:$CHAIN_PROJECT"
  expect "delete project $CHAIN_PROJECT, whose builds ran on commit $_sha" 200 204
  say "deleted project $CHAIN_PROJECT: its builds ran on $_sha, which is not a commit of this Gitea; it is made again"
}

ensure_chain() {
  drop_chain_built_from_another_gitea
  # A chain an older version of this script made, with other steps, goes whole.
  rest GET "/app/rest/projects/id:$CHAIN_PROJECT/buildTypes?fields=buildType(id)"
  if [ "$REST_STATUS" = "200" ]; then
    _have=$(printf '%s' "$REST_BODY" | jq -r '[.buildType[]?.id] | sort | join(" ")')
    _want=$(for _s in $CHAIN_STEPS; do echo "${CHAIN_PROJECT}_$_s"; done | sort | paste -sd' ' -)
    if [ -n "$_have" ] && [ "$_have" != "$_want" ]; then
      rest DELETE "/app/rest/projects/id:$CHAIN_PROJECT"
      expect "delete project $CHAIN_PROJECT, which an older seed made with other steps" 200 204
      say "deleted project $CHAIN_PROJECT: it held '$_have', wanted '$_want'; it is made again"
    fi
  fi
  ensure_project "$CHAIN_PROJECT"
  ensure_vcs_root "$CHAIN_PROJECT"
  _root=$(vcs_root_id "$CHAIN_PROJECT")
  _before=''
  for _step in $CHAIN_STEPS; do
    _id="${CHAIN_PROJECT}_$_step"
    rest GET "/app/rest/buildTypes/id:$_id"
    if [ "$REST_STATUS" = "404" ]; then
      rest POST /app/rest/buildTypes "$(jq -n --arg i "$_id" --arg n "$_step" --arg p "$CHAIN_PROJECT" \
          '{id:$i, name:$n, project:{id:$p}}')"
      expect "create build configuration $_id" 200
      say "created build configuration $_id (\"$_step\" in $CHAIN_PROJECT)"
    else
      expect "read build configuration $_id" 200
    fi
    rest GET "/app/rest/buildTypes/id:$_id/vcs-root-entries"
    expect "list VCS roots of $_id" 200
    if [ "$(printf '%s' "$REST_BODY" | jq '."vcs-root-entry" // [] | length')" = "0" ]; then
      rest POST "/app/rest/buildTypes/id:$_id/vcs-root-entries" "$(jq -n --arg r "$_root" '{"vcs-root":{id:$r}}')"
      expect "attach VCS root $_root to $_id" 200
    fi
    rest GET "/app/rest/buildTypes/id:$_id/steps"
    expect "list steps of $_id" 200
    if [ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" = "0" ]; then
      add_step "$_id" "$_step" "# Seeded by testenv/seed-teamcity-builds.sh (#1010): one step of the build chain.
echo \"$_step: a step of the seeded chain\""
    fi
    if [ -n "$_before" ]; then
      rest GET "/app/rest/buildTypes/id:$_id/snapshot-dependencies"
      expect "list snapshot dependencies of $_id" 200
      if [ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" = "0" ]; then
        rest POST "/app/rest/buildTypes/id:$_id/snapshot-dependencies" \
          "$(jq -n --arg d "$_before" '{id:$d, type:"snapshot_dependency", "source-buildType":{id:$d}}')"
        expect "make $_id wait on $_before" 200
        say "$_id waits on $_before"
      fi
    fi
    _before=$_id
  done

  # One build of the last step on the feature branch queues the others.
  _last="${CHAIN_PROJECT}_$(chain_last)"
  _loc="branch:(name:$CHAIN_BRANCH),defaultFilter:false,state:finished,status:SUCCESS"
  rest GET "/app/rest/builds?locator=buildType:(id:$_last),$_loc&fields=count"
  expect "look for a finished build of $_last on $CHAIN_BRANCH" 200
  if [ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" = "0" ]; then
    rest POST /app/rest/buildQueue "$(jq -n --arg b "$_last" --arg br "$CHAIN_BRANCH" \
      '{buildType:{id:$b}, branchName:$br, comment:{text:"testenv/seed-teamcity-builds.sh: the chain (#1010)"}}')"
    expect "queue a build of $_last on $CHAIN_BRANCH" 200
    _bid=$(printf '%s' "$REST_BODY" | jq -r '.id')
    say "queued the chain: build id $_bid of $_last on $CHAIN_BRANCH"
    wait_for_state "$_bid" finished
  fi

  _builds='[]'
  for _step in $CHAIN_STEPS; do
    _id="${CHAIN_PROJECT}_$_step"
    rest GET "/app/rest/builds?locator=buildType:(id:$_id),$_loc,count:1&fields=build(id,number,branchName)"
    expect "read the chain's build of $_id" 200
    _bi=$(printf '%s' "$REST_BODY" | jq -r '.build[0].id // empty')
    [ -n "$_bi" ] || die "the chain has no finished successful build of $_id on $CHAIN_BRANCH"
    _got=$(printf '%s' "$REST_BODY" | jq -r '.build[0].branchName')
    [ "$_got" = "$CHAIN_BRANCH" ] || die "the chain's build id $_bi of $_id ran on '$_got', wanted '$CHAIN_BRANCH'"
    _builds=$(printf '%s' "$_builds" | jq -c --arg t "$_id" --argjson i "$_bi" '. + [{build_type:$t, real_id:$i}]')
    say "chain: build id $_bi of $_id on $CHAIN_BRANCH"
  done
  CHAIN_JSON=$(jq -n -c --arg p "$CHAIN_PROJECT" --arg b "$CHAIN_BRANCH" --argjson s "$(printf '%s' "$CHAIN_STEPS" | jq -R -c --arg p "$CHAIN_PROJECT" 'split(" ") | map($p + "_" + .)')" --argjson x "$_builds" \
    '{project:$p, branch:$b, build_types:$s, builds:$x}')
}

# --------------------------------------------------------------------------
rest GET /app/rest/server
expect "GET /app/rest/server as the recorded token" 200
say "server $(printf '%s' "$REST_BODY" | jq -r .version), token ok"

rest GET '/app/rest/agents?locator=connected:true,authorized:true,enabled:true'
expect "list agents" 200
[ "$(printf '%s' "$REST_BODY" | jq '.count // 0')" -ge 1 ] \
  || die "no connected, authorised, enabled agent -- nothing would run the builds (./seed --teamcity authorises one)"

# 0. A project whose finished build names a commit this Gitea never held (#964).
#    A build keeps the commit it ran on, and TeamCity's data outlives Gitea's:
#    a Gitea recreated and reseeded since has other shas for the same fixture
#    (the repository's first commit carries the seed's own date), so the
#    build's Revision row names a commit no sync can mirror. Re-running the
#    build is not enough -- the VCS root keeps the old modifications, and the
#    new build then lists the whole branch as its changes instead of none, as
#    the first build of a configuration must. So the project goes whole, with
#    its roots, configurations and builds, and step 1 makes it again. Not
#    touched while its builds still name commits Gitea has.
drop_projects_built_from_another_gitea() {
  for _cfg in $(jqf '[.builds[].cfg] | unique | .[]'); do
    _proj=$(project_of "$_cfg")
    rest GET "/app/rest/builds?locator=buildType:(id:$_cfg),defaultFilter:false,state:finished,count:1&fields=build(id,revisions(revision(version)))"
    [ "$REST_STATUS" = "200" ] || continue
    _sha=$(printf '%s' "$REST_BODY" | jq -r '.build[0].revisions.revision[0].version // empty')
    [ -n "$_sha" ] || continue
    gitea GET "/repos/$GITEA_ORG/$(repo_for_project "$_proj")/git/commits/$_sha"
    case "$GITEA_STATUS" in
      200) continue ;;
      404|422) ;;
      *) die "look for commit $_sha of $_cfg in Gitea: HTTP $GITEA_STATUS: $GITEA_BODY" ;;
    esac
    rest GET "/app/rest/projects/id:$_proj"
    [ "$REST_STATUS" = "200" ] || continue
    rest DELETE "/app/rest/projects/id:$_proj"
    expect "delete project $_proj, whose build of $_cfg ran on commit $_sha" 200 204
    say "deleted project $_proj: its build of $_cfg ran on $_sha, which is not a commit of this Gitea; it is made again"
  done
}
drop_projects_built_from_another_gitea

# 1. Projects, VCS roots, build configurations, ascending by configuration id
#    as tc_state.rs lists them (first appearance in the fixture would make
#    the order a matter of presentation).
for cfg in $(jqf '[.builds[].cfg] | unique | .[]'); do
  proj=$(project_of "$cfg")
  ensure_project "$proj"
  ensure_vcs_root "$proj"
  ensure_build_type "$cfg"
done
ensure_configuration_settings

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
    ensure_current_build "$FOUND_ID" "$status"
  fi
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
  if [ "$status" = "FAILURE" ]; then assert_log "$bid" "$cfg"; assert_reports "$bid" "$cfg"; fi
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

# 4. The change witness (#683), after the fixture's builds: the agent is
#    free, and a held 1188 (--running) would keep it from running anything.
if [ -n "$RUNNING_LINE" ]; then
  CHANGES_JSON=$(jq -c '.teamcity.changes // empty' "$STATE")
  say "the change witness waits while build $RUNNING_LINE holds the agent; re-run without --running for it"
else
  ensure_change_witness
fi

# 4b. The build chain (#1010), on the same terms: it needs the agent.
if [ -n "$RUNNING_LINE" ]; then
  CHAIN_JSON=$(jq -c '.teamcity.chain // empty' "$STATE")
  say "the build chain waits while build $RUNNING_LINE holds the agent; re-run without --running for it"
else
  ensure_chain
fi

# 5. seed-state.json: fixture number -> real id, under the teamcity block
#    seed-teamcity.sh wrote, without disturbing the rest of it.
BUILDS_JSON=$(printf '%s' "$BUILD_MAP" | jq -R -s '
  split("\n") | map(select(length > 0) | split(" ")
  | {fixture_number: (.[0] | tonumber), build_type: .[1], real_id: (.[2] | tonumber)})
  | sort_by(.fixture_number)')
jq --argjson b "$BUILDS_JSON" --arg c "$SETTINGS_CONFIG" --arg s "$SETTINGS_SECRET" \
   --argjson w "${CHANGES_JSON:-null}" --argjson ch "${CHAIN_JSON:-null}" \
  '.teamcity += {builds: $b, settings: {build_type: $c, secret_parameter: "env.SEPA_SANDBOX_KEY", secret_value: $s}}
   | if $w == null then del(.teamcity.changes) else .teamcity.changes = $w end
   | if $ch == null then del(.teamcity.chain) else .teamcity.chain = $ch end' \
  "$STATE" > "$STATE.tmp"
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
