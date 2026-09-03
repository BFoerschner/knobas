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
# a real instance, `docker compose down -v`. It is not a long-lived seeded
# environment like Gitea's.
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

# Waits for a state the wizard can be driven from, named positively: the
# earlier spelling ("not UNREACHABLE and not STARTING") accepted the empty
# answer of a container still binding its port.
wait_for_first_run() {  # wait_for_first_run <name> <url>
  printf 'seed-atlassian: waiting for %s ' "$1"
  _i=0
  while :; do
    case "$(state_of "$2")" in
      FIRST_RUN|RUNNING) break ;;
    esac
    _i=$((_i + 1))
    [ "$_i" -lt 120 ] || { echo; die "$1 never reached FIRST_RUN or RUNNING (600s)"; }
    printf '.'; sleep 5
  done
  echo " $(state_of "$2")"
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
  wait_for_first_run confluence "$CONFLUENCE_URL"

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
  wait_for_first_run jira "$JIRA_URL"

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
  wizard_read "$JIRA_URL/"
  _n=0
  while [ "$_n" -lt 12 ]; do
    _n=$((_n + 1))
    # Out of the wizard is the exit condition: the last step lands on
    # WelcomeToJIRA.jspa, which has forms of its own that are not setup steps.
    case "$WIZARD_URL" in *"/secure/Setup"*) ;; *) break ;; esac
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

  printf 'seed-atlassian: waiting for jira to finish starting '
  _i=0
  until [ "$(state_of "$JIRA_URL")" = "RUNNING" ]; do
    _i=$((_i + 1))
    [ "$_i" -lt 60 ] || { echo; die "jira never reached RUNNING (300s)"; }
    printf '.'; sleep 5
  done
  echo ' ok'

  _code=$(curl -sS -o /dev/null -w '%{http_code}' -u "$ADMIN_USER:$ADMIN_PASS" \
               "$JIRA_URL/rest/api/2/myself")
  [ "$_code" = "200" ] || die "jira set up but /rest/api/2/myself answered $_code"
  _v=$(curl -sS -u "$ADMIN_USER:$ADMIN_PASS" "$JIRA_URL/rest/api/2/serverInfo" | jq -r '.version')
  say "jira: RUNNING $_v, REST 200"
  record jira "$(jq -n --arg u "$JIRA_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" --arg v "$_v" \
    '{url:$u, user:$n, password:$p, version:$v}')"
}

case "${1:-both}" in
  both)       setup_confluence; setup_jira ;;
  jira)       setup_jira ;;
  confluence) setup_confluence ;;
  *) die "usage: ./seed-atlassian.sh [both|jira|confluence]" ;;
esac

say "done -- both licences expire 3 hours after they were applied"
