#!/bin/sh
# Configure the Uptime Kuma baseline through socket.io (Kuma v2 has no REST API
# for configuration). The work is done by kuma-seed.mjs inside a one-shot
# node container on the compose network; this script is the host half.
#
# The container gets the repo READ-ONLY. The API key comes back on its stdout
# rather than being written into the mount, because a container writing to a
# bind mount leaves root-owned files behind on Linux.
set -eu
cd "$(dirname "$0")"

KEY_FILE=kuma-api-key
HAVE=0
[ -s "$KEY_FILE" ] && HAVE=1

# Three of the monitors ping the Hetzner servers by IP, and the IPs are in
# hetzner/hosts.env, which provision.sh writes and .gitignore keeps out of the
# repo. Not because the addresses are secret -- hetzner/estate.json commits the
# same three, and the M4 spec (#427) says plainly that it carries no secret --
# but because the file is one account's provisioning output and belongs to
# whoever ran provision.sh. monitors.json carries `${KNOBAS_HETZNER_*_IP}`
# placeholders and they are resolved here, in the container's environment.
#
# Refusing without the file beats seeding a partial list: the monitor list IS
# the estate (M4 spec, issue #427), and a Kuma missing the servers is one whose
# green means less than it looks. `hosts.env` is `VAR=value` lines only, so a
# plain `.` is safe; shellcheck cannot follow a path that exists on one machine.
HOSTS=hetzner/hosts.env
[ -r "$HOSTS" ] || {
  echo "seed-kuma: $HOSTS not found -- monitors.json pings the three Hetzner servers by IP." >&2
  echo "seed-kuma: run ./hetzner/provision.sh (README.md, 'On Hetzner instead of the laptop')." >&2
  exit 1; }
# shellcheck source=/dev/null
. "./$HOSTS"

# stdout is the protocol channel (one line: KEY=... or KEEP); the container's
# progress goes to stderr and straight through to the terminal.
out=$(docker compose --profile seed run --rm -T \
        -e KNOBAS_HAVE_KEY="$HAVE" \
        -e KNOBAS_HETZNER_TEAMCITY_IP="${KNOBAS_HETZNER_TEAMCITY_IP:-}" \
        -e KNOBAS_HETZNER_JIRA_IP="${KNOBAS_HETZNER_JIRA_IP:-}" \
        -e KNOBAS_HETZNER_CONFLUENCE_IP="${KNOBAS_HETZNER_CONFLUENCE_IP:-}" \
        kuma-seed)

case "$out" in
  KEEP)
    echo "seed-kuma: kept the existing API key in $KEY_FILE"
    ;;
  KEY=uk*)
    printf '%s\n' "${out#KEY=}" > "$KEY_FILE"
    chmod 600 "$KEY_FILE"
    echo "seed-kuma: wrote a new API key to $KEY_FILE"
    ;;
  *)
    # Anything else means the container printed something unexpected on stdout
    # -- a compose message, an npm line, a partial failure. Refusing here beats
    # writing garbage into kuma-api-key and failing mysteriously later.
    echo "seed-kuma: unexpected output from kuma-seed (wanted 'KEEP' or 'KEY=uk...'):" >&2
    printf '%s\n' "$out" >&2
    exit 1
    ;;
esac

echo "seed-kuma: Kuma now holds exactly the monitors monitors.json names"
