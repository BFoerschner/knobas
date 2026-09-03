#!/bin/sh
# Set up the real Jira and Confluence containers end to end, unattended.
#
#   ./seed-atlassian.sh              both products
#   ./seed-atlassian.sh jira         one of them
#   ./seed-atlassian.sh confluence
#
# WHAT THIS IS AND WHY IT IS NOT LIKE seed-gitea.sh
#
# `seed-gitea.sh` drives Gitea's documented, versioned `/api/v1`. There is no
# equivalent here. Atlassian's position is that unattended installation is not
# supported, and everything below the database step is the setup wizard's own
# HTML forms -- `/setup/*.action` and `/secure/Setup*.jspa` -- which are not an
# API, carry no compatibility promise, and change between versions. The field
# names in this file were read off the running containers named in
# VERIFIED_*_IMAGE below, on 2026-08-31, by walking the wizard and inspecting
# each form. `testenv/specs/fetch.sh` carries the same warning about TeamCity's
# first-start wizard, and that one cost two days.
#
# Hence the version guard: this script REFUSES to run against an image digest
# it was not derived on. A wizard half-completed by a script working from stale
# field names is worse than one not started -- the instance looks configured
# and is not. If the guard fires, the fix is to re-derive the sequence against
# the new image and update both the digest and whatever moved.
#
# WHAT IS SUPPORTED, AND THEREFORE NOT DONE HERE: the licence and the database.
# `docker-compose.yml` passes `ATL_JDBC_*` (plus `ATL_DB_DRIVER`, whose absence
# is silent -- see the comment there) so both products skip the wizard's
# database step, and `ATL_LICENSE_KEY` so Confluence skips its licence step.
# Jira has no licence variable, so its key goes in through the form.
#
# THE LICENCES ARE TIMEBOMBS: 10 user, valid 3 HOURS from when applied, from
#   https://developer.atlassian.com/platform/marketplace/timebomb-licenses-for-testing-server-apps/
# under "Data Center host product licenses". They are free and need no
# my.atlassian.com account, which since 2026-03-30 is the only free door left.
# Three hours is the shape of this environment: it is stand up, run what needs
# a real instance, then `docker compose --profile real-atlassian down -v jira
# jira-db confluence confluence-db` -- the four services NAMED, because a bare
# profile-scoped `down -v` also takes the default profile's containers and
# volumes, Gitea and its seeded corpus included (README.md, "Jira and
# Confluence, end to end"). It is not a long-lived seeded environment like
# Gitea's.
#
# The keys are public and `fetch-timebomb-keys.sh` pulls them off Atlassian's
# page. Run it BEFORE `docker compose up`, because Confluence reads its key at
# first start:
#   eval "$(./fetch-timebomb-keys.sh)"
# Never put them in `testenv/.env` -- that file is tracked. Jira's key goes in
# through the wizard, so an unset JIRA_LICENSE_KEY is fetched here on the spot;
# an unset CONFLUENCE_LICENSE_KEY is fetched too, but the container must have
# been started with it, which is checked below against the container's own
# environment rather than this shell's.
set -eu
cd "$(dirname "$0")"

if [ -z "${JIRA_LICENSE_KEY:-}" ] || [ -z "${CONFLUENCE_LICENSE_KEY:-}" ]; then
  eval "$(./fetch-timebomb-keys.sh)"
fi

JIRA_URL=${KNOBAS_JIRA_URL:-http://127.0.0.1:8080}
CONFLUENCE_URL=${KNOBAS_CONFLUENCE_URL:-http://127.0.0.1:8090}
ADMIN_USER=${ATLASSIAN_ADMIN_USER:-knobas}
ADMIN_PASS=${ATLASSIAN_ADMIN_PASS:-knobas-dev}
ADMIN_MAIL=${ATLASSIAN_ADMIN_MAIL:-knobas@example.invalid}
STATE=seed-state.json

# The digests this script's form fields were read off. Keep in step with
# `pin-images.sh`; a mismatch is a refusal, not a warning.
VERIFIED_JIRA_IMAGE=sha256:64e139808556925e87a63db57455764519ceb8d213a11c9af106e525836a3114
VERIFIED_CONFLUENCE_IMAGE=sha256:d15c23a1dfea0d390536115003cd732c9b404571f85bc081f9ae507e51feeafd

# THE WAIT CAPS, AND WHY THE SECOND ONE IS FIFTEEN MINUTES.
#
# FIRST_RUN_CAP_S bounds the wait for a container to answer /status with a
# state a wizard can be driven from. JIRA_RUNNING_CAP_S bounds Jira's
# post-wizard restart, which is the slow one: Jira writes its schema and
# re-initialises the whole plugin system before /status says RUNNING.
#
# MEASURED, on this machine -- 12 cores, an 8 GB Docker VM, images already
# pulled, volumes empty, and Jira starting alone:
#   <FILL:314 the idle post-wizard restart, from the live run>
# Under load it is far worse. On 2026-09-03, with seven agents working and
# Confluence's JVM starting beside it, that restart did NOT finish inside the
# old 300 s cap -- twice, at this same line, each time killing a whole
# `just atlassian-live` window before a single suite ran (#313 -> #314). 300 s
# was the cap and not a measurement, so the slowest start under that load is
# unknown and above it.
#
# Two things came out of that. `just atlassian-live` now starts and seeds Jira
# with the box to itself, before Confluence exists at all (see the recipe's
# header), and this cap is 900 s. The cap costs a working run nothing -- the
# loop breaks the moment /status says RUNNING -- so all it decides is how long
# a run that is going to fail takes to say so, and the three-hour licence
# window has ample room for that. A cap that fires here is a report about the
# machine, not a flake to widen again.
#
# All four are seconds of WALL CLOCK, not counts of anything.
FIRST_RUN_CAP_S=600
JIRA_RUNNING_CAP_S=900
POLL_S=5
PROGRESS_EVERY_S=30

say() { echo "seed-atlassian: $*"; }
die() { echo "seed-atlassian: $*" >&2; exit 1; }

for tool in docker curl jq; do
  command -v "$tool" >/dev/null || die "$tool is required"
done

# --------------------------------------------------------------------------
# The container is the one this sequence was derived on, or we stop.
#
# Compared by digest rather than by the version the product reports, because
# the digest is what `pin-images.sh` pins and what a rebuild would move. Read
# from the container rather than from `.env` so that editing `.env` without
# recreating the container cannot make the check pass.
guard_image() {  # guard_image <container> <expected digest> <product>
  _running=$(docker inspect -f '{{.Config.Image}}' "$1" 2>/dev/null) \
    || die "$1 is not running -- 'docker compose --profile real-atlassian up -d $1'"
  case "$_running" in
    *"$2") ;;
    *)
      echo "seed-atlassian: REFUSING to touch $3." >&2
      echo "  container runs: $_running" >&2
      echo "  derived on:     ...@$2" >&2
      echo "  The wizard endpoints this script POSTs to are not an API and" >&2
      echo "  change between versions. Re-derive them against the new image," >&2
      echo "  then update VERIFIED_${3}_IMAGE. Half-completing a setup with" >&2
      echo "  stale field names leaves an instance that looks configured." >&2
      exit 1 ;;
  esac
}

# --------------------------------------------------------------------------
# One wizard-walking helper for both products, because the machinery is
# identical and it is the part most likely to rot: a cookie jar for the
# session, the XSRF token scraped from whatever page we are on, and a retry
# (the retry's why sits on `wizard_post` below, where it fires).
JAR=

# The trap is for the `die` paths: every wizard failure exits mid-walk, and
# without it each such exit leaves a session cookie jar in $TMPDIR.
wizard_begin() {
  JAR=$(mktemp "${TMPDIR:-/tmp}/knobas-wizard.XXXXXX")
  trap '[ -z "$JAR" ] || rm -f "$JAR" "$JAR.body"' EXIT
}
wizard_end()   { [ -n "$JAR" ] && rm -f "$JAR" "$JAR.body"; JAR=; }

# Read a wizard page into $PAGE, and pull out the form action it is showing
# ($STEP) plus its XSRF token ($ATL_TOKEN).
wizard_read() {  # wizard_read <url>
  _raw=$(curl -sS -c "$JAR" -b "$JAR" -L -w '\n@@%{url_effective}' "$1")
  WIZARD_URL=$(printf '%s' "$_raw" | tail -n 1 | sed 's/^@@//')
  PAGE=$(printf '%s' "$_raw" | sed '$d' | tr '\n' ' ')
  STEP=$(printf '%s' "$PAGE" | grep -o '<form[^>]*action="[^"]*"' | head -1 \
         | sed 's/.*action="//; s/"$//')
  ATL_TOKEN=$(printf '%s' "$PAGE" | grep -o 'name="atl_token"[^>]*value="[^"]*"' \
              | head -1 | sed 's/.*value="//; s/"$//')
}

# POST the step being shown, then move to whichever step comes next.
#
# THE NEXT STEP COMES FROM THE RESPONSE, NOT FROM `GET /`. Confluence does not
# commit its `setupStep` until the whole wizard finishes, so `GET /` answers
# with the *same* step after a POST that genuinely advanced -- an earlier
# version of this script re-read `/` each iteration and looped on one step
# until its own cap stopped it, having done the work eleven times.
#
# The two products disagree about how a step answers, so neither shape is
# treated as the success condition: Confluence redirects to the next step,
# while Jira's administrator step returns 200 with the next form in the body.
# What both agree on is that the step CHANGES. So that is the check, and a
# step that is still itself afterwards is the failure -- which is exactly how
# these forms report a rejected field, and what made that loop silent.
#
# THE RETRY IS NOT PADDING. Both products answer `/status`, and render the
# wizard's first page, minutes before they can *process* a POST to it;
# Confluence fails that window with a 500 whose body says "Spring Application
# context has not been set". No readiness endpoint distinguishes the two
# states, so the honest check is the POST itself. Retrying is safe only
# because a 500 there means the step was refused, not half-applied -- and if
# that ever stops holding, the step-change check below and the final REST
# probe still refuse to report success; the cost is a worse message, not a
# silent half-setup.
wizard_post() {  # wizard_post <url> <curl --data args...>
  _url=$1; shift
  _was=$STEP
  _i=0
  while :; do
    _out=$(curl -sS -c "$JAR" -b "$JAR" -o "$JAR.body" \
                -w '%{http_code} %{redirect_url}' -X POST "$_url" "$@")
    _code=${_out%% *}; _loc=${_out#* }
    case "$_code" in
      2*|3*) break ;;
      5*)
        _i=$((_i + 1))
        [ "$_i" -lt 60 ] || { echo; die "$_url still answering $_code after 300s"; }
        [ "$_i" -eq 1 ] && printf 'seed-atlassian: waiting for the app to accept POSTs '
        printf '.'; sleep 5 ;;
      *) die "$_url answered $_code" ;;
    esac
  done
  [ "$_i" -gt 0 ] && echo ' ok'

  if [ -n "$_loc" ]; then
    # Followed by hand rather than with `curl -L`: Jira answers one step with
    # a redirect curl does not downgrade to GET, and re-POSTing the form to
    # the next step's URL earns a 405.
    rm -f "$JAR.body"
    wizard_read "$_loc"
  else
    PAGE=$(tr '\n' ' ' < "$JAR.body")
    WIZARD_URL=$_url
    STEP=$(printf '%s' "$PAGE" | grep -o '<form[^>]*action="[^"]*"' | head -1 \
           | sed 's/.*action="//; s/"$//')
    ATL_TOKEN=$(printf '%s' "$PAGE" | grep -o 'name="atl_token"[^>]*value="[^"]*"' \
                | head -1 | sed 's/.*value="//; s/"$//')
    rm -f "$JAR.body"
  fi

  if [ "$STEP" = "$_was" ]; then
    echo "seed-atlassian: $_url re-rendered its own form instead of advancing." >&2
    echo "  Reported reason:" >&2
    printf '%s' "$PAGE" \
      | grep -o 'class="[^"]*error[^"]*"[^>]*>[^<]\{3,200\}' \
      | sed 's/.*>//; s/^/    /' | head -5 >&2
    die "wizard step $_was did not advance"
  fi
}

# An unrecognised step is a hard stop carrying the evidence needed to fix it,
# never a guess: these forms are not an API, and this is how their drift
# surfaces.
unknown_step() {  # unknown_step <product>
  echo "seed-atlassian: $1 is showing a wizard step this script does not know:" >&2
  echo "  url:  $WIZARD_URL" >&2
  echo "  form: $STEP" >&2
  echo "  fields:" >&2
  printf '%s' "$PAGE" | grep -o '<input[^>]*name="[^"]*"[^>]*>' \
    | sed 's/^/    /' | cut -c1-160 >&2
  echo "  Add a branch for it, or re-derive the sequence if the version moved." >&2
  exit 1
}

# Both products expose /status, and RUNNING means setup is complete. This is
# the idempotency check: re-running against a set-up instance is a no-op.
# A container that is not listening yet answers nothing at all, and an empty
# body is not a state -- mapping it to UNREACHABLE rather than to the empty
# string is what stops the wait below from treating "no answer" as "ready".
state_of() {  # state_of <url>
  _s=$(curl -sS --max-time 10 "$1/status" 2>/dev/null | jq -r '.state // empty' 2>/dev/null) || _s=
  [ -n "$_s" ] || _s=UNREACHABLE
  printf '%s' "$_s"
}

# --------------------------------------------------------------------------
# THE WAITS. Two of them -- one for a state a wizard can be driven from, one
# for RUNNING after a wizard has been walked -- and they are one loop, because
# everything except which states they accept is the same and the interesting
# parts (the clock, the progress line, the diagnosis) are worth having once.
#
# Acceptance comes in as a PREDICATE rather than as a `case` pattern: `case`'s
# `|` is syntax, parsed before expansion, so "FIRST_RUN|RUNNING" out of a
# variable would be one pattern containing a literal bar and would match
# nothing.
state_is_wizard_ready() {  # <state>
  case "$1" in FIRST_RUN|RUNNING) return 0 ;; esac
  return 1
}
state_is_running() {  # <state>
  [ "$1" = RUNNING ]
}

# ELAPSED IS READ OFF THE CLOCK, NOT COUNTED IN SLEEPS. An earlier spelling
# added POLL_S per iteration, which undercounts: each pass also spends up to
# `curl --max-time 10` inside state_of, so against a container that is bound
# but not answering a "900s" cap counted in sleeps is up to 2700s of real
# time -- and the seconds in the failure message would then be the one number
# in it that had not been measured. Reading `date` makes the cap, the progress
# line and the message all the same seconds, the ones that actually passed.
#
# The progress line is the whole reason a caller can tell a slow start from a
# hung one: every 30 s it prints the state the product is reporting and how
# far into the cap we are. #313's failed run printed 59 anonymous dots, off
# which you could read neither how much of the cap was left nor whether
# anything was moving. A state that climbs -- UNREACHABLE, then FIRST_RUN or
# STARTING, then RUNNING -- is a slow start; one state repeated to the cap is
# a hang; and UNREACHABLE the whole way is a container that never bound its
# port, or died, which is a `docker logs` question and not a waiting one.
wait_for_state() {  # wait_for_state <predicate> <name> <url> <cap seconds> <accepted, in words>
  say "waiting for $2 to reach $5 (cap ${4}s)"
  _t0=$(date +%s)
  _next=$PROGRESS_EVERY_S
  while :; do
    _st=$(state_of "$3")
    "$1" "$_st" && break
    [ "$(( $(date +%s) - _t0 ))" -lt "$4" ] || die "$2 never reached $5 (${4}s; last answer: $_st).
  A wait that long is the machine and not the product. Something heavy was
  starting beside it -- two JVMs at once is what made 300s too tight for Jira
  (#314) -- or the container is not alive at all. Its name is knobas-<service>
  (docker-compose.yml), so:
    docker logs --tail 50 knobas-$2"
    sleep "$POLL_S"
    _waited=$(( $(date +%s) - _t0 ))
    if [ "$_waited" -ge "$_next" ]; then
      say "  $2 is $_st -- ${_waited}s of ${4}s"
      # From now, not from the last multiple: a poll that took 15 s must not
      # earn two progress lines, and PROGRESS_EVERY_S need not divide POLL_S.
      _next=$(( _waited + PROGRESS_EVERY_S ))
    fi
  done
  say "$2 is $_st after $(( $(date +%s) - _t0 ))s"
}

# FIRST_RUN DOES NOT MEAN THE WIZARD IS BEING SERVED, and the gap between the
# two is minutes wide. FIRST_RUN is Jira saying it has decided it needs setting
# up; for a good while after that `GET /` still answers with the database step,
# or with nothing that carries a form at all.
#
# The old confluence-first order hid this: Confluence's entire wizard walk ran
# inside the gap, so by the time Jira's `/` was read it was serving. Seeding
# Jira first (#314) walks straight into it, and the failure was silent -- `GET
# /` did not land on a step, the walk loop's out-of-the-wizard exit fired
# having walked nothing, and the script then sat in the post-wizard wait for a
# RUNNING that could never come, until the cap killed it 900 s later. The
# instance looked like the wizard had been walked and it had not, which is
# exactly what this file's header says must never happen quietly.
#
# So the wizard being served is waited for, positively, the way every other
# readiness question here is answered: by asking for the thing we are about to
# use, not for a proxy. The step it must be showing is named rather than
# pattern-matched, because the script is pinned to one image digest and this is
# the step that image opens on; a wizard sitting on the database step instead
# means ATL_JDBC_* never reached the container.
JIRA_FIRST_STEP=SetupApplicationProperties.jspa

wait_for_jira_wizard() {  # wait_for_jira_wizard <cap seconds>
  say "waiting for jira to serve $JIRA_FIRST_STEP (cap ${1}s)"
  _t0=$(date +%s)
  _next=$PROGRESS_EVERY_S
  while :; do
    wizard_read "$JIRA_URL/"
    [ "$STEP" = "$JIRA_FIRST_STEP" ] && break
    [ "$(( $(date +%s) - _t0 ))" -lt "$1" ] || die "jira never served its first setup step (${1}s).
  expected form: $JIRA_FIRST_STEP
  last url:      $WIZARD_URL
  last form:     ${STEP:-none}
  A wizard stuck on the database step means ATL_JDBC_* is not reaching the
  container (docker-compose.yml). Anything else means the wizard moved, and
  the sequence has to be re-derived against the new image."
    sleep "$POLL_S"
    _waited=$(( $(date +%s) - _t0 ))
    if [ "$_waited" -ge "$_next" ]; then
      say "  jira is serving ${STEP:-no form yet} -- ${_waited}s of ${1}s"
      _next=$(( _waited + PROGRESS_EVERY_S ))
    fi
  done
  say "jira is serving $STEP after $(( $(date +%s) - _t0 ))s"
}

# Merge one product's block into seed-state.json rather than rewriting it:
# seed-gitea.sh owns its own keys in the same file.
record() {  # record <key> <json>
  _old='{}'
  [ -r "$STATE" ] && _old=$(cat "$STATE" 2>/dev/null) && [ -n "$_old" ] || _old='{}'
  printf '%s' "$_old" | jq --arg k "$1" --argjson v "$2" '. + {($k): $v}' > "$STATE.tmp"
  mv "$STATE.tmp" "$STATE"
}

# --------------------------------------------------------------------------
# Confluence: four POSTs. The licence and database steps are already answered
# by ATL_LICENSE_KEY and ATL_JDBC_*, so the wizard opens on the cluster choice.
setup_confluence() {
  guard_image knobas-confluence "$VERIFIED_CONFLUENCE_IMAGE" CONFLUENCE
  wait_for_state state_is_wizard_ready confluence "$CONFLUENCE_URL" \
    "$FIRST_RUN_CAP_S" "FIRST_RUN or RUNNING"

  if [ "$(state_of "$CONFLUENCE_URL")" = "RUNNING" ]; then
    say "confluence is already set up"
    record confluence "$(jq -n --arg u "$CONFLUENCE_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" \
      '{url:$u, user:$n, password:$p}')"
    return 0
  fi

  # The key is read by the container at FIRST-TIME setup only, so what counts
  # is the environment the container was started with, not this shell's.
  _in_container=$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' knobas-confluence \
    | sed -n 's/^ATL_LICENSE_KEY=//p')
  [ -n "$_in_container" ] || die "confluence was started WITHOUT a licence key.
  It reads ATL_LICENSE_KEY at first-time setup only, so export the key and
  recreate the container:
    eval \"\$(./fetch-timebomb-keys.sh)\"
    docker compose --profile real-atlassian down -v confluence
    docker compose --profile real-atlassian up -d confluence"

  wizard_begin
  wizard_read "$CONFLUENCE_URL/"
  _n=0
  while [ "$_n" -lt 12 ]; do
    _n=$((_n + 1))
    case "$STEP" in
      ""|finishsetup.action) break ;;
      setupcluster.action)
        # Non-clustered. The radio's VALUES ARE INVERTED with respect to its
        # ids -- the checked input is id="clusteringDisabled" with value="true"
        # -- and what actually decides the branch is `newCluster`: the page's
        # own JavaScript sets it to "skipCluster" for the non-clustered path
        # and "Create cluster" for the other. Sending isClusteringEnabled=false
        # reaches the cluster branch with no cluster name, and answers 500.
        say "confluence: skipping the cluster step"
        wizard_post "$CONFLUENCE_URL/setup/setupcluster.action" \
          --data "atl_token=$ATL_TOKEN" \
          --data "isClusteringEnabled=true" \
          --data "newCluster=skipCluster" ;;
      setupdata.action)
        say "confluence: empty site"
        wizard_post "$CONFLUENCE_URL/setup/setupdata.action" \
          --data "atl_token=$ATL_TOKEN" \
          --data "contentChoice=blank" \
          --data-urlencode "dbchoiceSelect=Empty Site" ;;
      setupusermanagementchoice.action)
        # `--data-urlencode`, not `--data`, and the difference is not cosmetic:
        # these submit-button values carry spaces, curl's `--data` sends them
        # raw, and the action then re-renders its own form with a 200 instead
        # of advancing. That reads as a step that silently does nothing.
        say "confluence: internal user management"
        wizard_post "$CONFLUENCE_URL/setup/setupusermanagementchoice.action" \
          --data "atl_token=$ATL_TOKEN" \
          --data "userManagementChoice=internal" \
          --data-urlencode "internal=Manage users and groups within Confluence" ;;
      setupadministrator.action)
        say "confluence: administrator $ADMIN_USER"
        wizard_post "$CONFLUENCE_URL/setup/setupadministrator.action" \
          --data "atl_token=$ATL_TOKEN" \
          --data-urlencode "username=$ADMIN_USER" \
          --data-urlencode "fullName=knobas seed" \
          --data-urlencode "email=$ADMIN_MAIL" \
          --data-urlencode "password=$ADMIN_PASS" \
          --data-urlencode "confirm=$ADMIN_PASS" \
          --data "setup-next-button=Next" ;;
      *) unknown_step confluence ;;
    esac
  done
  wizard_end

  # Proof, not hope: the wizard reporting success and the API answering are
  # different claims, and only the second one is what a test suite needs.
  _code=$(curl -sS -o /dev/null -w '%{http_code}' -u "$ADMIN_USER:$ADMIN_PASS" \
               "$CONFLUENCE_URL/rest/api/user/current")
  [ "$_code" = "200" ] || die "confluence set up but /rest/api/user/current answered $_code"
  say "confluence: $(state_of "$CONFLUENCE_URL"), REST 200"
  record confluence "$(jq -n --arg u "$CONFLUENCE_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" \
    '{url:$u, user:$n, password:$p}')"
}

# --------------------------------------------------------------------------
# Jira: four POSTs, and the licence is one of them -- there is no environment
# variable for it.
setup_jira() {
  guard_image knobas-jira "$VERIFIED_JIRA_IMAGE" JIRA
  wait_for_state state_is_wizard_ready jira "$JIRA_URL" \
    "$FIRST_RUN_CAP_S" "FIRST_RUN or RUNNING"

  if [ "$(state_of "$JIRA_URL")" = "RUNNING" ]; then
    say "jira is already set up"
    _v=$(curl -sS -u "$ADMIN_USER:$ADMIN_PASS" "$JIRA_URL/rest/api/2/serverInfo" \
         | jq -r '.version // "unknown"')
    record jira "$(jq -n --arg u "$JIRA_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" --arg v "$_v" \
      '{url:$u, user:$n, password:$p, version:$v}')"
    return 0
  fi

  [ -n "${JIRA_LICENSE_KEY:-}" ] || die "JIRA_LICENSE_KEY is unset and
  fetch-timebomb-keys.sh did not set it -- see its error above."

  wizard_begin
  wait_for_jira_wizard "$FIRST_RUN_CAP_S"
  _n=0
  _walked=0
  while [ "$_n" -lt 12 ]; do
    _n=$((_n + 1))
    # Out of the wizard is the exit condition: the last step lands on
    # WelcomeToJIRA.jspa, which has forms of its own that are not setup steps.
    case "$WIZARD_URL" in *"/secure/Setup"*) ;; *) break ;; esac
    _walked=$((_walked + 1))
    case "$STEP" in
      SetupApplicationProperties.jspa)
        say "jira: application properties"
        wizard_post "$JIRA_URL/secure/SetupApplicationProperties.jspa" \
          --data "atl_token=$ATL_TOKEN" \
          --data-urlencode "title=knobas testenv" \
          --data "mode=private" \
          --data-urlencode "baseURL=$JIRA_URL" \
          --data "nextStep=" ;;
      SetupLicense.jspa)
        # The step Jira 11.x is reported to fail (#49). The key is accepted on
        # 10.3.24 -- verified, and the reason that version is pinned.
        say "jira: licence"
        wizard_post "$JIRA_URL/secure/SetupLicense.jspa" \
          --data "atl_token=$ATL_TOKEN" \
          --data-urlencode "setupLicenseKey=$JIRA_LICENSE_KEY"
        case "$WIZARD_URL" in
          *SetupAdminAccount*) ;;
          *) die "jira did not accept the licence key -- it moved to $WIZARD_URL.
  A rejected timebomb key is the known failure on Jira 11.x; on a version this
  script is pinned to, it means the key was truncated or has been withdrawn." ;;
        esac ;;
      SetupAdminAccount.jspa)
        say "jira: administrator $ADMIN_USER"
        wizard_post "$JIRA_URL/secure/SetupAdminAccount.jspa" \
          --data "atl_token=$ATL_TOKEN" \
          --data-urlencode "fullname=knobas seed" \
          --data-urlencode "email=$ADMIN_MAIL" \
          --data-urlencode "username=$ADMIN_USER" \
          --data-urlencode "password=$ADMIN_PASS" \
          --data-urlencode "confirm=$ADMIN_PASS" ;;
      SetupMailNotifications.jspa)
        say "jira: no outgoing mail"
        wizard_post "$JIRA_URL/secure/SetupMailNotifications.jspa" \
          --data "atl_token=$ATL_TOKEN" \
          --data "noemail=true" \
          --data "testingMailConnection=false" \
          --data-urlencode "finish=Finish" ;;
      *) unknown_step jira ;;
    esac
  done
  wizard_end

  # Belt and braces on the failure above: that exit condition is a `break`, and
  # a break on the first pass means the wizard was never walked, not that it
  # finished. Reaching the post-wizard wait in that state costs a whole cap and
  # then reports the wrong thing.
  [ "$_walked" -gt 0 ] || die "jira's wizard walk did nothing -- it left the
  wizard at $WIZARD_URL without POSTing a step. Jira has not been set up."

  wait_for_state state_is_running jira "$JIRA_URL" \
    "$JIRA_RUNNING_CAP_S" RUNNING

  _code=$(curl -sS -o /dev/null -w '%{http_code}' -u "$ADMIN_USER:$ADMIN_PASS" \
               "$JIRA_URL/rest/api/2/myself")
  [ "$_code" = "200" ] || die "jira set up but /rest/api/2/myself answered $_code"
  _v=$(curl -sS -u "$ADMIN_USER:$ADMIN_PASS" "$JIRA_URL/rest/api/2/serverInfo" | jq -r '.version')
  say "jira: RUNNING $_v, REST 200"
  record jira "$(jq -n --arg u "$JIRA_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" --arg v "$_v" \
    '{url:$u, user:$n, password:$p, version:$v}')"
}

# JIRA FIRST in `both`, so this mode agrees with the order `just atlassian-live`
# drives one product per invocation in, and for the same reason: Jira's
# post-wizard restart is the long pole and it wants the VM to itself. `both`
# can only reduce the overlap and not remove it -- by the time it runs, the
# caller has started both containers and the two JVMs are already up -- so on a
# loaded machine prefer the recipe's shape: up and seed Jira, then up and seed
# Confluence.
MODE=${1:-both}
case "$MODE" in
  both)       setup_jira; setup_confluence ;;
  jira)       setup_jira ;;
  confluence) setup_confluence ;;
  *) die "usage: ./seed-atlassian.sh [both|jira|confluence]" ;;
esac

# Named per invocation, because one product at a time is now the normal call:
# saying "both licences" after `./seed-atlassian.sh jira` was a small lie about
# which timebomb had started ticking.
say "done -- $MODE; each product's timebomb licence expires 3 hours after it is applied"
