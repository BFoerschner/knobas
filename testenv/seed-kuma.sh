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
# Kuma's published port from docker-compose.yml, and `/metrics` is the one
# endpoint an API key opens (README.md, the credential table), which is what
# makes it the probe below. Hardcoded like `seed`'s own copy: the port is the
# compose file's, not the caller's, and a half-honoured override is worse than
# none -- `./seed --env-kuma` hands the suites 127.0.0.1:3001 regardless.
PROBE_URL=http://127.0.0.1:3001/metrics

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

# A key file is a credential only while the instance still answers to it. Kuma
# hands a key's clear text out exactly once, so a sibling worktree's seed that
# re-minted `knobas-seed` leaves this tree holding a well-formed key the
# instance no longer has, and `[ -s "$KEY_FILE" ]` alone cannot tell the two
# apart (issue #552). So the key is probed before it is kept, which is what
# seed-gitea.sh step 8 does for the Gitea token in this same directory.
#
# Three outcomes, not two, and the third is the point. Re-minting DESTROYS the
# instance's key, so it may only happen on an answer the instance actually
# gave: 200 keeps, 401 and 403 re-mint, and anything else -- `000` from a
# published port that is not answering, a 5xx, a proxy's 404 -- refuses. This
# is the only call the script makes over the host's port (everything else goes
# to `uptime-kuma:3001` on the compose network, and the compose healthcheck
# does not cover the forward), so a dead forward would otherwise read as a
# dead key and re-mint on a guess, 401ing every sibling worktree's copy: the
# collision README.md's "One environment, one owner at a time" describes.
#
# The name says what was measured: the instance answered 200 to the key on
# disk. It is not "a key file is here" -- that is the question this script
# stopped asking in #552, and a name that still asked it is how the next reader
# comes to believe the old answer (issue #554).
KEY_AUTHENTICATED=0
if [ -s "$KEY_FILE" ]; then
  code=$(curl -sS -o /dev/null -w '%{http_code}' --connect-timeout 5 --max-time 15 \
           -u ":$(cat "$KEY_FILE")" "$PROBE_URL" || true)
  case "$code" in
    200) KEY_AUTHENTICATED=1 ;;
    401|403)
      echo "seed-kuma: the API key in $KEY_FILE no longer authenticates ($code); minting a new one" ;;
    *)
      echo "seed-kuma: $PROBE_URL answered '$code', so the key in $KEY_FILE can be neither" >&2
      echo "seed-kuma: kept nor replaced -- re-minting on a guess would 401 every other" >&2
      echo "seed-kuma: worktree's copy. Is the port up? docker compose up -d --wait uptime-kuma" >&2
      exit 1 ;;
  esac
fi

# stdout is the protocol channel (one line: KEY=... or KEEP); the container's
# progress goes to stderr and straight through to the terminal.
#
# `KNOBAS_KEY_AUTHENTICATED` is half a contract: kuma-seed.mjs reads that exact
# name, and an unset variable reads there as "did not authenticate", so a
# rename on this side alone re-mints on every run and still passes a single
# `just kuma-live` (issue #554). Rename both halves or neither.
out=$(docker compose --profile seed run --rm -T \
        -e KNOBAS_KEY_AUTHENTICATED="$KEY_AUTHENTICATED" \
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
