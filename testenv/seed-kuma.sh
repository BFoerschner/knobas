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

# The key's name on the instance, one per worktree (issue #1484), derived as
# gitea-token.sh derives the Gitea token's (#684): `knobas-seed-<worktree>-
# <checksum of this directory's real path>`. The Kuma container is shared by
# every worktree on the machine, while `kuma-api-key` is git-ignored and lives in
# one of them, and until #1484 every tree minted under the one name
# `knobas-seed`: a tree whose copy stopped working deleted that key and added a
# new one, which 401ed every other tree's copy, and the next of them to seed did
# the same. Now kuma-seed.mjs keeps, replaces or adds the key of THIS name only;
# another tree's key, and the pre-#1484 `knobas-seed`, are never touched.
#
# `KNOBAS_KEY_NAME` is the other half of a contract across the container
# boundary, like `KNOBAS_KEY_AUTHENTICATED` below: kuma-seed.mjs refuses to run
# without it rather than fall back to a shared name.
KEY_NAME=$(
  dir=$(pwd -P)
  sum=$(printf '%s' "$dir" | cksum | cut -d ' ' -f 1)
  printf 'knobas-seed-%s-%s\n' "$(basename "$(dirname "$dir")")" "$sum"
)

# A key file is a credential only while the instance still answers to it. Kuma
# hands a key's clear text out exactly once, so a key file whose key was
# deleted on the instance (a reset volume, a hand-deleted key, or before #1484 a
# sibling worktree's re-mint) is well-formed and worthless, and
# `[ -s "$KEY_FILE" ]` alone cannot tell the two apart (issue #552). So the key
# is probed before it is kept, which is what seed-gitea.sh step 8 does for the
# Gitea token in this same directory.
#
# Three outcomes, not two, and the third is the point. Re-minting DESTROYS the
# instance's key, so it may only happen on an answer the instance actually
# gave: 200 keeps, 401 and 403 re-mint, and anything else -- `000` from a
# published port that is not answering, a 5xx, a proxy's 404 -- refuses. This
# is the only call the script makes over the host's port (everything else goes
# to `uptime-kuma:3001` on the compose network, and the compose healthcheck
# does not cover the forward), so a dead forward would otherwise read as a
# dead key and re-mint on a guess, throwing away a key that still works (and,
# before #1484, every sibling worktree's copy with it).
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
      echo "seed-kuma: kept nor replaced -- re-minting on a guess would throw away a key" >&2
      echo "seed-kuma: that may still work. Is the port up? docker compose --env-file images.env up -d --wait uptime-kuma" >&2
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
#
# KUMA_WAIT_S is the guard on each of the seed's socket waits (kuma-seed.mjs,
# #1961); unset here, it goes in empty and the seed takes its own default.
# The two files go in as a tar on stdin: the container mounts nothing, so the
# engine may be another machine's (docker-compose.yml, kuma-seed). Plain
# ustar, and no AppleDouble files: busybox tar in the node image reads that.
out=$(COPYFILE_DISABLE=1 tar --format ustar -c kuma-seed.mjs monitors.json | docker compose --env-file images.env --profile seed run --rm -T \
        -e KNOBAS_KEY_AUTHENTICATED="$KEY_AUTHENTICATED" \
        -e KNOBAS_KEY_NAME="$KEY_NAME" \
        -e KUMA_WAIT_S="${KUMA_WAIT_S:-}" \
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
