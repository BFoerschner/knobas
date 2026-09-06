#!/usr/bin/env bash
# Keep one Atlassian product's 3-hour timebomb licence alive. Runs ON the
# product's server, from the systemd timer install-renewer.sh sets up (every
# 150 minutes and once at boot), with PRODUCT=jira or PRODUCT=confluence.
#
# WHAT RENEWS THE LICENCE, MEASURED 2026-09-06 against the real containers:
#   * A RESTART. Jira's expiry is exactly the JVM start plus three hours:
#     started 08:22:23Z, /rest/plugins/applications/1.0/installed/jira-software/
#     license said expiryDate 11:22:33Z; a recreate earlier moved it the same
#     way. Re-applying the identical key over that REST (200, body
#     {"licenseKey": ...}) moved it not at all. So for Jira this script is a
#     restart, and nothing else.
#   * Confluence shows its expiry to the day only, so which rule it follows is
#     unmeasured. It gets both: the restart, then the key re-applied through
#     /admin/doupdatelicense.action (302 to #updateSuccessful, measured) with a
#     freshly fetched key. If the fetch fails -- Atlassian's page reshaped --
#     the restart has already happened and the failure is logged, not fatal.
#
# NOT WHILE A RUN IS ON. `just atlassian-live` touches /opt/knobas/run-in-progress
# on this server for its duration; a restart under a live suite is a connection
# refused mid-test. This waits for the marker to go (or to be 30 min stale, which
# is a killed run) for up to 20 minutes, and only then restarts. The timer's
# next tick is 150 minutes after this activation, so a wait here shortens the
# margin by the same amount and never below zero for waits under 30 minutes.
#
# STATUS: /opt/knobas/renew-status, one line, `ok <iso time> <detail>` or
# `failed <iso time> <detail>`; its mtime is when this last ran. `tunnel status`
# on the notebook prints it, and the recipe reads its age to decide whether to
# run this first.
set -euo pipefail
PRODUCT=${PRODUCT:?PRODUCT=jira|confluence}
case "$PRODUCT" in
  jira)       URL=http://127.0.0.1:8080 ;;
  confluence) URL=http://127.0.0.1:8090 ;;
  *) echo "renew-licence: unknown PRODUCT '$PRODUCT'" >&2; exit 2 ;;
esac
CONTAINER=knobas-$PRODUCT
ADMIN=${ADMIN_USER:-knobas}:${ADMIN_PASS:-knobas-dev}   # seed-atlassian.sh's defaults
STATUS=/opt/knobas/renew-status
MARKER=/opt/knobas/run-in-progress

log() { echo "renew-licence[$PRODUCT]: $*"; }
status() { printf '%s %s %s\n' "$1" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$2" > "$STATUS"; }
state() { curl -s --max-time 5 "$URL/status" | sed -n 's/.*"state" *: *"\([A-Z_]*\)".*/\1/p'; }

docker inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null | grep -q true \
  || { status failed "$CONTAINER is not running; nothing to renew"; log "$CONTAINER is not running"; exit 1; }

# 1. Not under a live run.
waited=0
while [ -e "$MARKER" ] && [ $(( $(date +%s) - $(stat -c %Y "$MARKER") )) -lt 1800 ]; do
  [ "$waited" -lt 1200 ] || { status failed "a run has been in progress for 20 min; not restarting"; log "gave up waiting for $MARKER"; exit 1; }
  [ "$waited" -eq 0 ] && log "a run is in progress ($MARKER); waiting"
  sleep 15; waited=$((waited + 15))
done

# 2. Restart, and wait for RUNNING.
t0=$(date +%s)
docker restart "$CONTAINER" >/dev/null
log "restarted $CONTAINER"
until [ "$(state)" = RUNNING ]; do
  [ $(( $(date +%s) - t0 )) -lt 600 ] || { status failed "not RUNNING 600 s after restart (state: $(state))"; log "timeout"; exit 1; }
  sleep 5
done
log "RUNNING after $(( $(date +%s) - t0 ))s"

# 3. Confluence: the key again, through the licence form.
detail="restarted, RUNNING after $(( $(date +%s) - t0 ))s"
if [ "$PRODUCT" = confluence ]; then
  if key=$(cd /opt/knobas && ./fetch-timebomb-keys.sh 2>/tmp/renew-fetch.err | sed -n "s/^export CONFLUENCE_LICENSE_KEY='\(.*\)'$/\1/p") && [ -n "$key" ]; then
    jar=$(mktemp)
    page=$(curl -sS -u "$ADMIN" -c "$jar" -b "$jar" -L "$URL/admin/license.action?os_authType=basic")
    token=$(printf '%s' "$page" | grep -o 'name="atl_token"[^>]*value="[^"]*"' | head -1 | sed 's/.*value="//; s/"$//')
    code=$(curl -sS -u "$ADMIN" -c "$jar" -b "$jar" -H 'X-Atlassian-Token: no-check' -X POST \
      "$URL/admin/doupdatelicense.action?os_authType=basic" \
      --data-urlencode "atl_token=$token" --data-urlencode "licenseString=$key" --data-urlencode "update=Save" \
      -o /dev/null -w '%{http_code} %{redirect_url}')
    rm -f "$jar"
    case "$code" in
      302*updateSuccessful) detail="$detail; key re-applied"; log "key re-applied" ;;
      *) detail="$detail; key NOT re-applied ($code)"; log "licence form answered $code" ;;
    esac
  else
    detail="$detail; key fetch failed: $(head -c 200 /tmp/renew-fetch.err | tr '\n' ' ')"
    log "key fetch failed (restart done): $(head -c 200 /tmp/renew-fetch.err | tr '\n' ' ')"
  fi
else
  # RUNNING comes before the REST does: right after a restart this GET answers
  # empty for a while (plugins still initialising), so it is polled, briefly.
  exp=''; i=0
  while [ -z "$exp" ] && [ "$i" -lt 24 ]; do
    exp=$(curl -s -u "$ADMIN" "$URL/rest/plugins/applications/1.0/installed/jira-software/license" | sed -n 's/.*"expiryDate":\([0-9]*\).*/\1/p')
    [ -n "$exp" ] || { sleep 5; i=$((i + 1)); }
  done
  [ -n "$exp" ] && detail="$detail; expires $(date -u -d @$((exp / 1000)) +%H:%M:%SZ)" || detail="$detail; expiry not readable yet"
fi
status ok "$detail"
log "$detail"
