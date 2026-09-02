#!/bin/sh
# Set up the real TeamCity container end to end, unattended: first-start
# wizard, administrator, access token, one authorised build agent.
#
#   docker compose --profile real-teamcity up -d teamcity teamcity-agent
#   ./seed-teamcity.sh          # or ./seed --teamcity
#
# WHAT THIS IS AND WHY IT IS NOT LIKE seed-gitea.sh
#
# Only the second half of this script -- the token and the agent -- talks to
# TeamCity's documented REST API (`/app/rest`). The first half walks the
# first-start wizard, which JetBrains describes as a browser step and serves
# from `/mnt/do/*` commands and `/createAdminSubmit.html`: not an API, no
# compatibility promise, and free to change between versions. Every command
# name, field name and response shape below was read off the running container
# named in VERIFIED_TEAMCITY_IMAGE, on 2026-09-02 (TeamCity 2026.1.3, build
# 222742), by fetching each page with curl and reading the form it showed and
# the JavaScript its buttons call:
#
#   /mnt                     BS.Maintenance.FirstStart.submit(false)
#                              -> POST /mnt/do/goNewInstallation  restore=false
#   /mnt (db-settings)       BS.Maintenance.postCommandAndRefresh('goNewDatabase', form)
#                              -> POST /mnt/do/goNewDatabase      dbType=HSQLDB2
#   /mnt (licence)           postCommandAndRefresh('acceptLicenseAgreement')
#                              -> POST /mnt/do/acceptLicenseAgreement
#   /setupAdmin.html         <form action="/createAdminSubmit.html">
#                              -> POST username1, encryptedPassword1,
#                                 encryptedRetypedPassword, submitCreateUser,
#                                 publicKey
#
# Every `/mnt/do/*` command answers the literal body `OK` and anything else is
# the text the page would have shown in an alert(); the page tells the browser
# when to reload through `/mnt/get/stateRevision`, and this script does the
# same thing by re-reading `/mnt` until its stage changes. All of them need
# the `X-TC-CSRF-Token` header carrying the `tc-csrf-token` <meta> of a page
# fetched IN THE SAME SESSION -- the token is bound to the session cookie, and
# a token read off an earlier page answers 403.
#
# THE ADMINISTRATOR'S PASSWORD IS RSA-ENCRYPTED IN THE BROWSER, and a form POST
# carrying `password1` in the clear is answered with "Password is empty" (tried
# first). What the page's JavaScript does (BS.Encrypt.encryptData, /js/bs/
# encrypt.js, over the jsbn RSA in /js/crypt/rsa.js): a 1024-bit public key
# whose modulus is the form's `publicKey` hex and whose exponent is 0x10001,
# PKCS#1 type-2 padding, and the padded message is the password's bytes
# followed by ONE BYTE holding the password's length. That layout is exactly
# what `openssl pkeyutl -encrypt` with `rsa_padding_mode:pkcs1` produces from
# the message "<password><length byte>", so the browser's work is reproduced
# here with openssl and no JavaScript. The field it goes in is "encrypted" +
# the capitalised input name (BS.AbstractPasswordForm.serializeParameters),
# hence `encryptedPassword1` and `encryptedRetypedPassword`. Non-ASCII
# passwords take a different path in that code (two bytes per character, a
# shorter chunk); this script refuses them rather than guess.
#
# Hence the version guard: this script REFUSES to run against an image digest
# it was not derived on, exactly as `seed-atlassian.sh` does. A wizard
# half-completed by a script working from stale names is worse than one not
# started -- the instance looks configured and is not. If the guard fires, the
# fix is to re-derive the sequence against the new image (the recipe is
# above: curl each page, read its form and the JavaScript behind its button)
# and update both the digest and whatever moved.
#
# THE INTERNAL DATABASE (HSQLDB) IS THE DECISION for this environment. TeamCity
# calls it evaluation-only, and that is what this is: a test environment whose
# lifetime is `down -v`. A PostgreSQL beside it would buy durability across
# server upgrades, which nothing here needs, at the cost of another container
# in a Docker VM that is already tight (#264).
#
# IDEMPOTENT: the token recorded in seed-state.json is tried first, and a
# server that answers it is already set up -- the wizard is skipped and only
# the agent checks run. A set-up server with no usable recorded token (another
# worktree seeded last; see README.md, "One environment, one owner") is
# recognised by the administrator's basic-auth credentials, and a fresh token
# is minted. Only a server still in its maintenance stages gets the wizard.
set -eu
cd "$(dirname "$0")"

# NOT `${KNOBAS_TEAMCITY_URL:-...}`, unlike the Jira and Confluence seeds: the
# repo-root .env.example points KNOBAS_TEAMCITY_URL at JetBrains' public guest
# instance for `just teamcity-live`, and a shell that has it exported would
# aim this script's POSTs at a server that is not ours. The port is fixed by
# interfaces §5, and the digest guard below binds the run to the local
# container anyway.
TC_URL=http://127.0.0.1:8111
ADMIN_USER=${TEAMCITY_ADMIN_USER:-knobas}
ADMIN_PASS=${TEAMCITY_ADMIN_PASS:-knobas-dev}
STATE=seed-state.json
TOKEN_NAME=knobas-seed
AGENT_NAME=knobas-agent   # AGENT_NAME in docker-compose.yml

# The digest this script's commands and field names were read off. Keep in
# step with `pin-images.sh`; a mismatch is a refusal, not a warning.
VERIFIED_TEAMCITY_IMAGE=sha256:30267c7f633a7973af1551e6f7f7683f49029ee81145b4710c570fd8393ccaef

say() { echo "seed-teamcity: $*"; }
die() { echo "seed-teamcity: $*" >&2; exit 1; }

for tool in docker curl jq openssl; do
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
    || die "$1 is not running -- 'docker compose --profile real-teamcity up -d teamcity teamcity-agent'"
  case "$_running" in
    *"$2") ;;
    *)
      echo "seed-teamcity: REFUSING to touch $3." >&2
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
# HTTP helpers. `rest` is for /app/rest and never exits on an HTTP error: the
# callers decide, because 401 is a normal answer to "does this token still
# work". It authenticates as the bearer of $REST_TOKEN when that is set and
# as $REST_USER (user:password, basic) otherwise. Every request carries
# --max-time, because a TeamCity that has just created its administrator held
# one probe open for over a minute during derivation; a hang is not a state
# this script can act on.
REST_STATUS=0
REST_BODY=''
REST_TOKEN=''
REST_USER=''
rest() {  # rest <METHOD> <path> [curl args...]
  _m=$1; _p=$2; shift 2
  if [ -n "$REST_TOKEN" ]; then set -- "$@" -H "Authorization: Bearer $REST_TOKEN"
  else set -- "$@" -u "$REST_USER"; fi
  _raw=$(curl -sS --max-time 60 -X "$_m" -H 'Accept: application/json' \
              -w '\n%{http_code}' "$@" "$TC_URL$_p") || _raw='
000'
  REST_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  REST_BODY=$(printf '%s' "$_raw" | sed '$d')
}

# The wizard's session: a cookie jar, and the CSRF token of the page last read
# into it. The trap is for the `die` paths, which would otherwise leave the jar
# and the openssl scratch files in $TMPDIR.
WORK=
wizard_begin() {
  WORK=$(mktemp -d "${TMPDIR:-/tmp}/knobas-teamcity.XXXXXX")
  trap '[ -z "$WORK" ] || rm -rf "$WORK"' EXIT
}
wizard_end() { [ -n "$WORK" ] && rm -rf "$WORK"; WORK=; }

# Read a page into $PAGE (one line) with the session, and pull out its CSRF
# token ($CSRF), the maintenance stage it reports ($STAGE, empty once the
# server has left maintenance and /mnt redirects away), and whether that stage
# is one the server is working through on its own ($ACTIVE=true, the page's
# `BS.Maintenance.activeStage`) or one waiting for a button. $WIZARD_CODE is
# the HTTP status, which is how "no stage" is told apart: a 3xx is the
# redirect out of maintenance, anything else is a page to keep polling.
#
# A refused or dropped connection is retried here, not reported: the server
# restarts its web application between stages (and OrbStack accepts the TCP
# connection before Tomcat listens, answering nothing), and the page comes
# back a few seconds later. Sixty seconds of nothing at all is the failure.
wizard_read() {  # wizard_read <path>
  _i=0
  until WIZARD_CODE=$(curl -sS --max-time 60 -c "$WORK/jar" -b "$WORK/jar" \
                           -o "$WORK/page" -w '%{http_code}' "$TC_URL$1" 2>/dev/null); do
    _i=$((_i + 1))
    [ "$_i" -lt 12 ] || die "GET $1 answered nothing for 60s"
    sleep 5
  done
  PAGE=$(tr '\n' ' ' < "$WORK/page")
  CSRF=$(printf '%s' "$PAGE" | grep -o 'name="tc-csrf-token" content="[^"]*"' \
         | head -1 | sed 's/.*content="//; s/"$//')
  STAGE=$(printf '%s' "$PAGE" | grep -o 'Stage: [A-Z_]*' | head -1 | sed 's/^Stage: //')
  ACTIVE=$(printf '%s' "$PAGE" | grep -o 'BS.Maintenance.activeStage = [a-z]*' \
           | head -1 | sed 's/.*= //')
}

# One maintenance command. The page's postCommandAndRefresh treats the literal
# body `OK` as success and alert()s anything else, so anything else is the
# failure message, verbatim.
mnt_do() {  # mnt_do <command> [curl --data args...]
  _cmd=$1; shift
  _out=$(curl -sS --max-time 60 -c "$WORK/jar" -b "$WORK/jar" -X POST \
              -H "X-TC-CSRF-Token: $CSRF" -w '\n%{http_code}' "$@" "$TC_URL/mnt/do/$_cmd") \
    || die "POST /mnt/do/$_cmd failed"
  _code=$(printf '%s' "$_out" | tail -n 1)
  _body=$(printf '%s' "$_out" | sed '$d')
  [ "$_code" = "200" ] && [ "$_body" = "OK" ] \
    || die "/mnt/do/$_cmd answered $_code: $_body"
}

# An unrecognised stage is a hard stop carrying the evidence needed to fix it,
# never a guess: these pages are not an API, and this is how their drift
# surfaces.
unknown_stage() {
  echo "seed-teamcity: the server is showing a maintenance stage this script does not know:" >&2
  echo "  stage: $STAGE" >&2
  echo "  page:  $(printf '%s' "$PAGE" | grep -o 'Page: [a-z-]*' | head -1)" >&2
  echo "  commands its buttons call:" >&2
  printf '%s' "$PAGE" | grep -o "postCommand[A-Za-z]*('[^']*'" | sed "s/.*('/    /; s/'$//" | sort -u >&2
  echo "  fields:" >&2
  printf '%s' "$PAGE" | grep -o '<input[^>]*name="[^"]*"[^>]*>\|<select[^>]*name="[^"]*"[^>]*>' \
    | sed 's/^/    /' | cut -c1-160 >&2
  echo "  Add a branch for it, or re-derive the sequence if the version moved." >&2
  exit 1
}

# --------------------------------------------------------------------------
# The wizard: proceed, internal database, licence. Each POST changes the
# server's state revision and the page after it; the loop re-reads /mnt until
# the stage is one with a button, drives it, and stops when /mnt redirects out
# of maintenance (no stage in the page at all). Creating the database and
# starting the application took about a minute here; the cap is ten.
walk_wizard() {
  _n=0
  _waiting=0
  while :; do
    _n=$((_n + 1))
    [ "$_n" -le 120 ] || die "still in maintenance (stage '$STAGE', HTTP $WIZARD_CODE) after 600s"
    wizard_read /mnt
    if [ "$ACTIVE" = "true" ] || { [ -z "$STAGE" ] && [ "${WIZARD_CODE%??}" != "3" ]; }; then
      # The server is working (CREATE_NEW_DB, APPLICATION_STARTING, ...), or
      # answered a page with no stage that is not the redirect out -- a 503
      # from a web application that is still coming up.
      [ "$_waiting" -eq 1 ] || { printf 'seed-teamcity: the server is working '; _waiting=1; }
      printf '[%s] ' "${STAGE:-HTTP $WIZARD_CODE}"; sleep 5; continue
    fi
    [ "$_waiting" -eq 0 ] || { echo; _waiting=0; }
    if [ -z "$STAGE" ]; then break; fi   # 3xx: redirected out of maintenance
    case "$STAGE" in
      FIRST_START_SCREEN)
        say "first start: proceed with a new installation in /data/teamcity_server/datadir"
        mnt_do goNewInstallation --data 'restore=false' ;;
      DB_SETTINGS_SCREEN)
        # The form's only visible field for the internal database is the
        # <select name="dbType">; HSQLDB2 is its "Internal (HSQLDB)" option.
        say "database: internal (HSQLDB)"
        mnt_do goNewDatabase --data 'dbType=HSQLDB2' ;;
      LICENSE_AGREEMENT_SCREEN)
        # The page's Accept button picks acceptLicenseAgreementAndSendUsageStatistics
        # when its consent checkbox is ticked (it is, by default) and this
        # command otherwise. No usage statistics from a test environment.
        say "licence agreement: accepted (TeamCity License Agreement, as shown by the server)"
        mnt_do acceptLicenseAgreement ;;
      *) unknown_stage ;;
    esac
    sleep 2
  done
}

# --------------------------------------------------------------------------
# The administrator. /setupAdmin.html shows the create form while no account
# exists and redirects to login.html once one does; the login page ALSO
# carries a publicKey, so the create form is recognised by its action.
rsa_encrypt_hex() {  # rsa_encrypt_hex <modulus hex> <plaintext>  -> hex ciphertext on stdout
  cat > "$WORK/rsa.cnf" <<EOF
asn1=SEQUENCE:pubkeyinfo
[pubkeyinfo]
algorithm=SEQUENCE:rsa_alg
pubkey=BITWRAP,SEQUENCE:rsapubkey
[rsa_alg]
algorithm=OID:rsaEncryption
parameter=NULL
[rsapubkey]
n=INTEGER:0x$1
e=INTEGER:0x10001
EOF
  openssl asn1parse -genconf "$WORK/rsa.cnf" -noout -out "$WORK/pub.der" 2>/dev/null \
    || die "openssl could not build a public key from the form's publicKey"
  # The message the page encrypts: the bytes, then one byte with the length
  # (pkcs1pad2 in /js/crypt/rsa.js: `ba[--n] = s.length`, then the characters).
  printf '%s' "$2" > "$WORK/msg"
  # `%b` decodes a `\0ddd` octal escape in its argument: one arbitrary byte,
  # written from sh without a variable in the format string.
  printf '%b' "\\0$(printf '%03o' "${#2}")" >> "$WORK/msg"
  openssl pkeyutl -encrypt -pubin -keyform DER -inkey "$WORK/pub.der" \
      -pkeyopt rsa_padding_mode:pkcs1 -in "$WORK/msg" 2>/dev/null \
    | od -An -tx1 -v | tr -d ' \n'
}

create_admin() {
  wizard_read /setupAdmin.html
  case "$PAGE" in
    *'action="/createAdminSubmit.html"'*) ;;
    *) return 1 ;;   # no create form: an administrator already exists
  esac
  if printf '%s' "$ADMIN_PASS" | LC_ALL=C grep -q '[^ -~]'; then
    die "the administrator password must be printable ASCII: the page encrypts non-ASCII text differently, and this script does not reproduce that path"
  fi
  [ "${#ADMIN_PASS}" -le 116 ] || die "the administrator password must be at most 116 characters (the page's RSA chunk size)"
  _pk=$(printf '%s' "$PAGE" | grep -o 'name="publicKey" value="[^"]*"' | head -1 | sed 's/.*value="//; s/"$//')
  [ -n "$_pk" ] || die "/setupAdmin.html shows the create form but no publicKey"
  _enc=$(rsa_encrypt_hex "$_pk" "$ADMIN_PASS")
  [ "${#_enc}" -eq 256 ] || die "RSA encryption produced ${#_enc} hex characters, expected 256"

  say "administrator: $ADMIN_USER"
  _out=$(curl -sS --max-time 60 -c "$WORK/jar" -b "$WORK/jar" -X POST \
              -H "X-TC-CSRF-Token: $CSRF" -w '\n%{http_code}' \
              --data-urlencode "username1=$ADMIN_USER" \
              --data "encryptedPassword1=$_enc" \
              --data "encryptedRetypedPassword=$_enc" \
              --data 'submitCreateUser=' \
              --data "publicKey=$_pk" \
              "$TC_URL/createAdminSubmit.html") || die "POST /createAdminSubmit.html failed"
  _code=$(printf '%s' "$_out" | tail -n 1)
  _body=$(printf '%s' "$_out" | sed '$d')
  # Success is `<response><redirect>...</redirect><errors /></response>`;
  # a refusal is `<response><errors><error id="...">text</error>...`.
  case "$_code:$_body" in
    200:*'<errors />'*|200:*'<errors/>'*) ;;
    *)
      echo "seed-teamcity: /createAdminSubmit.html refused the administrator ($_code):" >&2
      printf '%s' "$_body" | grep -o '<error[^>]*>[^<]*' | sed 's/^/    /' >&2
      exit 1 ;;
  esac
  return 0
}

# --------------------------------------------------------------------------
# Merge this product's block into seed-state.json rather than rewriting it:
# seed-gitea.sh and seed-atlassian.sh own their own keys in the same file.
record() {  # record <key> <json>
  _old='{}'
  [ -r "$STATE" ] && _old=$(cat "$STATE" 2>/dev/null) && [ -n "$_old" ] || _old='{}'
  printf '%s' "$_old" | jq --arg k "$1" --argjson v "$2" '. + {($k): $v}' > "$STATE.tmp"
  mv "$STATE.tmp" "$STATE"
}

# --------------------------------------------------------------------------
guard_image knobas-teamcity "$VERIFIED_TEAMCITY_IMAGE" TEAMCITY

# Any HTTP answer at all: the first one on a fresh volume is a 503 carrying
# the first-start page, and that is the state the wizard is driven from. curl
# prints 000 through -w when the connection was accepted and then dropped,
# which is what the port does for the seconds between the container starting
# and Tomcat listening; that is not an answer.
printf 'seed-teamcity: waiting for teamcity '
_i=0
until _code=$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' "$TC_URL/" 2>/dev/null) \
      && [ "$_code" != "000" ]; do
  _i=$((_i + 1))
  [ "$_i" -lt 120 ] || { echo; die "teamcity never answered on 8111 (600s) -- is 'docker compose --profile real-teamcity up -d teamcity teamcity-agent' running?"; }
  printf '.'; sleep 5
done
echo ' ok'

# 1. Already set up? The recorded token is the proof: a server that answers it
#    on /app/rest/server has an administrator, is past the wizard, and issued
#    this very token.
TOKEN=''
[ -r "$STATE" ] && TOKEN=$(jq -r '.teamcity.token // empty' "$STATE" 2>/dev/null || echo '')
if [ -n "$TOKEN" ]; then
  REST_TOKEN=$TOKEN
  rest GET /app/rest/server
  REST_TOKEN=''
  if [ "$REST_STATUS" = "200" ]; then
    say "already set up: the recorded token authenticates ($(printf '%s' "$REST_BODY" | jq -r .version))"
  else
    say "the recorded access token no longer authenticates ($REST_STATUS); minting a new one"
    TOKEN=''
  fi
fi

if [ -z "$TOKEN" ]; then
  # 2. Past the wizard but without a usable token? The administrator's own
  #    credentials say so. Anything but 200 or 401 here is a server still in
  #    maintenance (503 with the wizard page) or not yet listening.
  REST_USER="$ADMIN_USER:$ADMIN_PASS"
  rest GET /app/rest/server
  case "$REST_STATUS" in
    200) say "already set up: $ADMIN_USER authenticates" ;;
    401)
      # The wizard is done (the REST API answers) but the administrator is
      # not ours. The one case that is not "no account yet" is an account
      # with other credentials, and that is a stop, not an overwrite.
      wizard_begin
      create_admin || die "the server has an administrator, but it is not $ADMIN_USER/$ADMIN_PASS.
  Either export TEAMCITY_ADMIN_USER/TEAMCITY_ADMIN_PASS to match, or start over:
    docker compose --profile real-teamcity down -v"
      wizard_end ;;
    *)
      wizard_begin
      walk_wizard
      # Out of maintenance. The application is up when the create-admin page
      # (or its redirect) renders; the REST API is not usable before that.
      create_admin || die "left maintenance, but /setupAdmin.html shows no create form and $ADMIN_USER does not authenticate"
      wizard_end ;;
  esac

  # Proof, not hope: the wizard reporting success and the API answering are
  # different claims, and only the second is what a test suite needs.
  rest GET /app/rest/server
  [ "$REST_STATUS" = "200" ] || die "set up, but /app/rest/server answered $REST_STATUS for $ADMIN_USER"
  say "REST 200 for $ADMIN_USER, $(printf '%s' "$REST_BODY" | jq -r .version)"

  # 3. One access token. TeamCity shows a token's value exactly once, at
  #    creation, and refuses a second token with the same name, so a stale
  #    knobas-seed (a previous run whose seed-state.json is gone) is deleted
  #    before minting -- the Gitea seed's rule.
  rest GET /app/rest/users/current/tokens
  [ "$REST_STATUS" = "200" ] || die "listing access tokens answered $REST_STATUS"
  if [ "$(printf '%s' "$REST_BODY" | jq --arg n "$TOKEN_NAME" '[.token[]? | select(.name==$n)] | length')" != "0" ]; then
    rest DELETE "/app/rest/users/current/tokens/$TOKEN_NAME"
    [ "$REST_STATUS" = "204" ] || die "deleting the stale token $TOKEN_NAME answered $REST_STATUS"
    say "deleted the stale token $TOKEN_NAME"
  fi
  rest POST "/app/rest/users/current/tokens/$TOKEN_NAME"
  [ "$REST_STATUS" = "200" ] || die "minting the token $TOKEN_NAME answered $REST_STATUS: $REST_BODY"
  TOKEN=$(printf '%s' "$REST_BODY" | jq -r '.value // empty')
  [ -n "$TOKEN" ] || die "the token answer carried no value: $REST_BODY"
  say "minted access token $TOKEN_NAME"
fi

# 4. The agent. It registers on its own (SERVER_URL) and shows up unauthorised;
#    authorising is a supported REST call. Wait for the registration first --
#    the agent container starts after the server and takes a moment.
# From here on, everything is done as the bearer of the token that is about
# to be recorded -- so a token that cannot do these things is never recorded.
REST_TOKEN=$TOKEN
printf 'seed-teamcity: waiting for the agent %s to register ' "$AGENT_NAME"
_i=0
while :; do
  rest GET "/app/rest/agents?locator=name:$AGENT_NAME,authorized:any,connected:any"
  [ "$REST_STATUS" = "200" ] && [ "$(printf '%s' "$REST_BODY" | jq -r '.count // 0')" -ge 1 ] && break
  _i=$((_i + 1))
  [ "$_i" -lt 60 ] || { echo; die "no agent named $AGENT_NAME registered in 300s -- is knobas-teamcity-agent running? 'docker compose --profile real-teamcity up -d teamcity-agent'"; }
  printf '.'; sleep 5
done
echo ' ok'

# The authorized flag is a text/plain resource: GET answers `true`/`false`,
# PUT with a text/plain body sets it and answers the new value.
_authorized=$(curl -sS --max-time 60 -H "Authorization: Bearer $TOKEN" -H 'Accept: text/plain' \
                   "$TC_URL/app/rest/agents/name:$AGENT_NAME/authorized") || die "reading the agent's authorized flag failed"
if [ "$_authorized" = "true" ]; then
  say "agent $AGENT_NAME is already authorised"
else
  _authorized=$(curl -sS --max-time 60 -H "Authorization: Bearer $TOKEN" -X PUT \
                     -H 'Content-Type: text/plain' -H 'Accept: text/plain' --data 'true' \
                     "$TC_URL/app/rest/agents/name:$AGENT_NAME/authorized") || die "authorising the agent failed"
  [ "$_authorized" = "true" ] || die "PUT .../authorized answered '$_authorized'"
  say "authorised agent $AGENT_NAME"
fi

printf 'seed-teamcity: waiting for %s to be connected and authorised ' "$AGENT_NAME"
_i=0
while :; do
  rest GET "/app/rest/agents?locator=connected:true,authorized:true"
  if [ "$REST_STATUS" = "200" ] \
     && [ "$(printf '%s' "$REST_BODY" | jq -r --arg n "$AGENT_NAME" '[.agent[]? | select(.name==$n)] | length')" = "1" ]; then
    break
  fi
  _i=$((_i + 1))
  [ "$_i" -lt 60 ] || { echo; die "$AGENT_NAME never showed up as connected and authorised (300s)"; }
  printf '.'; sleep 5
done
echo " ok ($(printf '%s' "$REST_BODY" | jq -r .count) connected, authorised agent(s))"

_v=$(curl -sS --max-time 60 -H "Authorization: Bearer $TOKEN" -H 'Accept: application/json' "$TC_URL/app/rest/server" | jq -r '.version // "unknown"')
record teamcity "$(jq -n --arg u "$TC_URL" --arg n "$ADMIN_USER" --arg p "$ADMIN_PASS" --arg t "$TOKEN" --arg v "$_v" \
  '{url:$u, user:$n, password:$p, token:$t, version:$v}')"
say "wrote $STATE"
say "done"
echo
echo "# Environment for the TeamCity adapter's live suite -- eval \"\$(./seed --env)\" prints these too"
echo "export KNOBAS_TEAMCITY_URL=$TC_URL"
echo "export KNOBAS_TEAMCITY_TOKEN=$TOKEN"
