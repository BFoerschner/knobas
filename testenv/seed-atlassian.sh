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
# FIRST_RUN_CAP_S bounds two waits, not one: a container answering /status with
# a state a wizard can be driven from, and -- Jira only -- that container then
# actually serving the wizard, which lands about a minute later and is a
# separate question (see wait_for_jira_wizard). JIRA_RUNNING_CAP_S bounds Jira's
# post-wizard restart, which is the slow one: Jira writes its schema and
# re-initialises the whole plugin system before /status says RUNNING.
# POLL_TIMEOUT_S bounds one poll, so that a single unanswered request cannot
# outlive the cap that exists to bound the wait it sits in.
#
# MEASURED on this machine -- 12 cores, an 8 GB Docker VM, images already
# pulled, volumes empty, TeamCity stopped, and Jira starting ALONE
# (`just atlassian-live`, 2026-09-03): jira reached FIRST_RUN 56 s after `up`,
# served its first wizard step 53 s after that, and the post-wizard wait this
# cap bounds returned in **0 s** -- the walk's last step does not answer until
# Jira has restarted, so by the time the wait begins /status already says
# RUNNING. On a VM Jira has to itself, this wait is not a wait.
#
# Under load it is a different measurement entirely. On 2026-09-03, with seven
# agents working and Confluence's JVM starting beside it, that same restart did
# NOT finish inside the old 300 s cap -- twice, at this same line, each time
# killing a whole `just atlassian-live` window before a suite ran (#313 -> #314).
# 300 s was the cap and not a measurement, so the slowest start under that load
# is unknown and above it.
#
# WHICH MAKES 900 s INSURANCE, NOT A FIX. The fix is the sequencing: Jira
# starts and is seeded with the VM to itself, before Confluence exists (see the
# recipe's header), and every recipe run since -- 292 s, 399 s, 480 s, the
# 392 s one that measured the numbers above, and the 327 s re-run of the
# merged bytes -- has been nowhere near even the old cap. What kept 300 s
# survivable this long is the recipe's refusal to run while TeamCity is up;
# the pair sharing the VM with a third JVM is the case nobody has measured,
# and 900 s is the margin for it. The cap costs a working run nothing -- the
# loop breaks the moment /status says RUNNING -- so all it decides is how long
# a run that is going to fail takes to say so, and the three-hour licence
# window has ample room for that. A cap that fires here is a report about the
# machine, not a flake to widen again.
#
# WIZARD_POST_CAP_S IS THE ONE NUMBER HERE THAT NOTHING HAS MEASURED. It bounds
# a different question from the two above: not whether a product has reached a
# state, but whether a product already serving the wizard can yet *process* a
# POST to it (wizard_post's own comment says why the POST is the only honest
# test of that). Its loop has printed nothing on any run recorded since #314,
# the run that certified this line included: on 2026-09-04, from empty volumes,
# all eight wizard steps across the two products were accepted on the first
# attempt and the retry printed not one line. So the slowest real value of this
# wait is not known to be anything above zero, and 300 s is what the script was
# first written with rather than a measurement of anything.
#
# It stays at 300 s for that reason and not by inheritance: widening a cap that
# nothing has ever reached would be the same guess in a larger size, and the
# argument that made 900 s reasonable next door -- a measured start that had
# already overrun -- does not exist here. What will size this one is the first
# run on which the progress line in wizard_post actually prints; whoever sees
# it should replace this paragraph with its seconds, the way the paragraph
# above replaced its own guess.
#
# WIZARD_POST_TIMEOUT_S BOUNDS ONE POST, the way POLL_TIMEOUT_S bounds one
# poll, and for the reason that line gives: a single unanswered request must
# not outlive the cap that exists to bound the wait it sits in. Without it the
# retry above is a cap on an app that ANSWERS 500 and nothing at all on an app
# that accepts the connection and never answers -- which is the shape that hung
# the /status poll before #314 gave it a timeout, arriving through the one door
# that family of changes did not cover (#367).
#
# IT IS NOT POLL_TIMEOUT_S'S TEN SECONDS, and the difference is not caution.
# A poll is `GET /status`: it costs the product nothing, and redoing one costs
# us nothing either. A wizard POST is the step's actual work -- Jira's
# application-properties step is not quick, and this file says so where
# wizard_read explains why only the poll passes a timeout. Cutting a POST that
# is being processed is the one thing this loop must not do: the retry is safe
# only because a 500 means the step was refused rather than half-applied, and
# an ABORTED request carries no such promise -- the server may finish it after
# curl has stopped listening. So this number is not sized to the slowest POST;
# it is sized so that reaching it means the app is not answering AT ALL.
#
# Which puts the two constraints in tension, and 60 s is where they sit.
# ABOVE any POST that has answered: MEASURED-PLACEHOLDER. BELOW the overrun a
# timeout costs: the cap is checked only after curl has come back, so the last
# attempt starts just under it and runs a whole timeout past it, and the sleep
# before that attempt is on the far side of the check too. The worst case is
# therefore WIZARD_POST_CAP_S + POLL_S + this, 365 s against a 300 s cap --
# which is a bound worth writing down and is not the unbounded wait it
# replaces. If a real POST is ever cut, widen this; that would show up as a
# wizard step applied twice, not as a slow run.
#
# All seven are seconds of WALL CLOCK, not counts of anything.
FIRST_RUN_CAP_S=600
JIRA_RUNNING_CAP_S=900
WIZARD_POST_CAP_S=300
POLL_S=5
PROGRESS_EVERY_S=30
POLL_TIMEOUT_S=10
WIZARD_POST_TIMEOUT_S=60

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
#
# THE OPTIONAL TIMEOUT IS FOR POLLING, and it changes two things about the read.
# It cannot outlive the cap of the wait it sits in, and a read that fails is
# reported as no form yet at the next progress line instead of ending the run:
# under `set -eu` an unguarded `_raw=$(curl ...)` exits the script, so without
# it a single refused connection during Jira's start kills the seed with a bare
# `curl: (7)` and none of the diagnosis the wait exists to print. `state_of`
# answers UNREACHABLE for exactly the same reason, and this is that rule applied
# to the one wait that reads a page rather than /status.
#
# Only the poll passes it. Every other call here reads the answer to a POST the
# product has already accepted, where a slow response is legitimate -- Jira's
# application-properties step is not quick -- and a failed one really is fatal.
wizard_read() {  # wizard_read <url> [poll timeout seconds]
  if [ -n "${2:-}" ]; then
    # `@@$1` on failure so the fields below come out as "this url, no form",
    # which is what the caller's progress line and die message want to say.
    _raw=$(curl -sS --max-time "$2" -c "$JAR" -b "$JAR" -L \
                -w '\n@@%{url_effective}' "$1" 2>/dev/null) || _raw="@@$1"
  else
    _raw=$(curl -sS -c "$JAR" -b "$JAR" -L -w '\n@@%{url_effective}' "$1")
  fi
  WIZARD_URL=$(printf '%s' "$_raw" | tail -n 1 | sed 's/^@@//')
  PAGE=$(printf '%s' "$_raw" | sed '$d' | tr '\n' ' ')
  STEP=$(printf '%s' "$PAGE" | grep -o '<form[^>]*action="[^"]*"' | head -1 \
         | sed 's/.*action="//; s/"$//')
  ATL_TOKEN=$(printf '%s' "$PAGE" | grep -o 'name="atl_token"[^>]*value="[^"]*"' \
              | head -1 | sed 's/.*value="//; s/"$//')
}

# What the app said when it refused, for the message at the cap: these bodies
# are HTML error pages and the sentence worth reading is a few words buried in
# markup, so tags out, whitespace squeezed, one line, first 200 characters.
post_refusal() {  # post_refusal <body file>
  if [ -s "$1" ]; then
    sed -e 's/<[^>]*>/ /g' "$1" | tr -s '[:space:]' ' ' | sed -e 's/^ *//' | cut -c1-200
  else
    printf 'nothing -- empty body'
  fi
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
#
# AND A 500 IS NOT THE ONLY WAY THAT WINDOW ANSWERS. An app can take the
# connection and never reply, which is a refusal that says nothing at all --
# the shape that hung the /status poll until #314 bounded it, and that the cap
# and the progress line here could not see for as long as the POST below
# carried no --max-time: no code came back, so no iteration happened, so
# nothing printed and nothing counted (#367). WIZARD_POST_TIMEOUT_S is what
# turns it back into an iteration, and its comment carries why it is not
# POLL_TIMEOUT_S.
#
# WHICH IS WHY THE REFUSAL IS QUOTED BACK (post_refusal, above). The whole
# content of this wait is a response body nobody sees: the sentence above is
# known only because someone read one by hand. So the last one is kept and
# printed on the way out, and the wait itself reports the step, the URL, what
# the app is doing -- answering a code, or not answering -- and how far into
# the cap it is every PROGRESS_EVERY_S: what wait_for_state prints, for the
# same reason (#314): an app that is warming and an app that is broken were
# both printing the same dot.
#
# `_p`-prefixed locals because sh has none, and a plain `_t0` here would be the
# same variable wait_for_state uses.
wizard_post() {  # wizard_post <url> <curl --data args...>
  _url=$1; shift
  _was=$STEP
  # Which container this URL is, worked out ONCE and up front: both failures
  # below end on a `docker logs` line, and a last line that is a command to run
  # beats one to hand-edit first (wait_for_state does the same with $2). Up
  # front and not inside the failure because the lookup can itself fail, and a
  # `$(...)` in a die string runs in a subshell where a `die` of its own would
  # exit nothing.
  case "$_url" in
    "$JIRA_URL"*)       _pwho=jira ;;
    "$CONFLUENCE_URL"*) _pwho=confluence ;;
    # Not reachable from this file -- all eight call sites are one of those two
    # prefixes -- and a `die` rather than a deletion because the arm has to
    # exist for `set -u`: with none, a third product added later would reach an
    # unset `$_pwho` and get an unbound-variable error in place of a diagnosis.
    # What it used to do was worse than either: print `knobas-127.0.0.1`.
    *) die "wizard_post: $_url is under neither \$JIRA_URL nor \$CONFLUENCE_URL,
  so there is no container to name if it fails. Add the product to this case
  along with the rest of its walk." ;;
  esac
  _pt0=$(date +%s)
  _pnext=$PROGRESS_EVERY_S
  _pretried=0
  while :; do
    # `|| _prc=$?` rather than a bare assignment, because --max-time gives this
    # curl a way to fail on purpose: under `set -eu` an unguarded
    # `_out=$(curl ...)` ends the whole seed on a bare `curl: (28)` with none
    # of the diagnosis below -- the same trade wizard_read's optional timeout
    # describes one function up. curl's own stderr goes with the guard, so the
    # exit code is named instead of quoted.
    _prc=0
    _out=$(curl -sS --max-time "$WIZARD_POST_TIMEOUT_S" -c "$JAR" -b "$JAR" \
                -o "$JAR.body" -w '%{http_code} %{redirect_url}' \
                -X POST "$_url" "$@" 2>/dev/null) || _prc=$?
    # ONE `case` OVER WHAT CURL DID, not two: a timeout is not an HTTP code
    # and carrying it as one -- `_code=timeout`, then a second switch with a
    # `timeout)` arm -- puts a value in `_code` that the arm below would print
    # as "answered timeout" the day someone adds a branch. What every arm that
    # keeps going leaves behind is the pair the messages need: `_plast` fills
    # the failure's "last response:" column, `_pdoing` is the progress line's
    # verb, and the timeout has to read as a sentence in both.
    #
    # `break` inside the inner `case` leaves the `while`, not the `case`.
    case "$_prc" in
      0)
        _code=${_out%% *}; _loc=${_out#* }
        case "$_code" in
          2*|3*) break ;;
          5*) _plast=$_code; _pdoing="answering $_code" ;;
          *) die "$_url answered $_code" ;;
        esac ;;
      # ONLY 28 IS RETRIED, because only 28 is "no answer yet". A refused
      # connection, a reset, an empty reply: none of those has been seen at
      # this point in the walk, and folding them into the retry is how they
      # would stay unseen. Still fatal, but named -- `curl: (7)` alone was the
      # entire message before.
      28)
        _plast="no reply in ${WIZARD_POST_TIMEOUT_S}s (curl --max-time)"
        _pdoing="not answering" ;;
      *) die "the POST to $_url failed outright (curl exit $_prc).
  step: ${_was:-none}
  This is not the warming window the retry exists for; that window answers.
  A connection refused or reset mid-walk is the container going away:
    docker logs --tail 50 knobas-$_pwho" ;;
    esac
    _pwaited=$(( $(date +%s) - _pt0 ))
    if [ "$_pwaited" -ge "$WIZARD_POST_CAP_S" ]; then
      # Read BEFORE the message is built: a `$(...)` inside a die string is
      # expanded there, and this message has to survive being the last thing
      # that happens.
      _pwhy=$(post_refusal "$JAR.body")
      die "${_was:-the wizard} never accepted a POST (${_pwaited}s of ${WIZARD_POST_CAP_S}s).
  url:           $_url
  last response: $_plast
  it said:       $_pwhy
  A 500 here is the product up but not yet able to process the step, and no
  reply at all is that same window with the request never handed back. Both
  are windows both products have and neither reports. Either one repeated to
  the cap is not a slow start: it is a step this product is refusing outright,
  or a container that is up and broken:
    docker logs --tail 50 knobas-$_pwho"
    fi
    if [ "$_pretried" -eq 0 ]; then
      _pretried=1
      say "waiting for ${_was:-the wizard} to accept a POST (cap ${WIZARD_POST_CAP_S}s)"
    fi
    sleep "$POLL_S"
    _pwaited=$(( $(date +%s) - _pt0 ))
    if [ "$_pwaited" -ge "$_pnext" ]; then
      # The step as well as the URL, and not only because the criteria say so:
      # they are the same name on every step this script knows, so a line where
      # they disagree is a POST going somewhere the rendered form did not point.
      say "  ${_was:-no step} at $_url is $_pdoing -- ${_pwaited}s of ${WIZARD_POST_CAP_S}s"
      _pnext=$(( _pwaited + PROGRESS_EVERY_S ))
    fi
  done
  [ "$_pretried" -eq 0 ] \
    || say "${_was:-the wizard} accepted the POST after $(( $(date +%s) - _pt0 ))s"

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
  _s=$(curl -sS --max-time "$POLL_TIMEOUT_S" "$1/status" 2>/dev/null | jq -r '.state // empty' 2>/dev/null) || _s=
  [ -n "$_s" ] || _s=UNREACHABLE
  printf '%s' "$_s"
}

# --------------------------------------------------------------------------
# THE WAITS. Two of them ask /status -- one for a state a wizard can be driven
# from, one for RUNNING after a wizard has been walked -- and those two are one
# loop, because everything except which states they accept is the same and the
# interesting parts (the clock, the progress line, the diagnosis) are worth
# having once. The third, wait_for_jira_wizard below, reads a page instead of a
# state and is deliberately its own loop rather than a fourth and fifth
# parameter here: folding it in would mean passing the reader, the comparison
# and the failure text as arguments too, and a five-line duplication is cheaper
# to read than that.
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
# POLL_TIMEOUT_S inside state_of's curl, so against a container that is bound
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
    wizard_read "$JIRA_URL/" "$POLL_TIMEOUT_S"
    [ "$STEP" = "$JIRA_FIRST_STEP" ] && break
    [ "$(( $(date +%s) - _t0 ))" -lt "$1" ] || die "jira never served its first setup step (${1}s).
  expected form: $JIRA_FIRST_STEP
  last url:      $WIZARD_URL
  last form:     ${STEP:-none}
  A wizard stuck on the database step means ATL_JDBC_* is not reaching the
  container (docker-compose.yml). A LATER step means this instance was already
  part-walked by an earlier run: that is the half-set-up state this file's
  header refuses to guess at, so tear the pair down and seed a fresh one rather
  than resuming it --
    docker compose --profile real-atlassian down -v jira jira-db confluence confluence-db
  the four services named. Any other form means the wizard moved, and the
  sequence has to be re-derived against the new image."
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
