#!/usr/bin/env bash
# Fill a live product to the size of Björn's real sources: ~20,000 Jira
# issues, ~5,000 Confluence pages, ~4,000 TeamCity objects, ~10,000 Gitea
# items. BESIDE the Tidewater seed, never in it: own Jira projects (SCL*),
# Confluence spaces (SCL*), a TeamCity project tree (Scale), Gitea orgs
# (scale-*), so every live suite still finds exactly what the seed made.
#
#   testenv/seed-scale.sh <jira|confluence|teamcity|gitea|forgejo> [--count N] [--workers W]
#   testenv/seed-scale.sh kuma [--count N]          # 200 monitors named scale-*; ./seed-kuma.sh leaves them
#   testenv/seed-scale.sh <product> --plan-only     # the plan's numbers, no server
#   just seed-scale <product> [--count N]
#
# The product must be up and seeded first (`just live-up <product>`): the
# scale seed reads its address and credentials from seed-state.json
# (seed-state-forgejo.json for Forgejo), as the live suites do.
#
# THE ESTATE IS A PLAN, NOT A LOOP. testenv/scale/*.py draw every issue,
# page, build and pull request from a generator keyed on a fixed seed, the
# product and the item's own index, and date them over the two years ending
# 2026-11-01. A rerun makes the same estate. --count is the size; the default
# is Björn's. Each product's phases are idempotent and recorded on the server
# (a Jira project property, a Confluence space property, a TeamCity project
# parameter, a Gitea Actions variable), so a rerun on a filled product reads
# a few markers and adds nothing; a run killed mid-phase picks the phase up
# where it stopped.
#
# DATES the products' APIs refuse to take (Jira's created/updated/resolved,
# a Confluence page's created/modified) are written into the product's own
# PostgreSQL with `docker exec <db> psql`, then reindexed; on a remote target
# that docker call goes to the WSL box like every other (live-lib.sh).
#
# testenv/README.md, "Björn's scale", says what each product gets and what a
# run measured.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

die() { echo "seed-scale: $*" >&2; exit 1; }

product=${1:-}
case $product in
  jira | confluence | teamcity | gitea | forgejo | kuma) shift ;;
  *) die "usage: testenv/seed-scale.sh <jira|confluence|teamcity|gitea|forgejo|kuma> [--count N] [--workers W] [--plan-only]" ;;
esac

# Uptime Kuma configures over socket.io only, so its monitors are added by
# node in the kuma-seed container, as kuma-monitor.sh adds its own: the script
# goes in as a tar on stdin, nothing is mounted.
if [ "$product" = kuma ]; then
  n=200
  [ "${1:-}" != --count ] || n=${2:?--count needs a number}
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  out=$(COPYFILE_DISABLE=1 tar --format ustar -C "$here/scale" -c kuma-scale.mjs |
    env -C "$here" docker compose --env-file images.env --profile seed run --rm -T -e SCALE_MONITORS="$n" kuma-seed sh -c '
      mkdir -p /tmp/k && tar -x -C /tmp/k &&
      npm --prefix /tmp/k i --no-save --silent socket.io-client@4 &&
      node /tmp/k/kuma-scale.mjs')
  [ "$out" = OK ] || die "kuma-scale.mjs did not finish: $out"
  exit 0
fi

command -v python3 >/dev/null || die "python3 is required (the stdlib only)"
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' || die "python3 3.9 or newer is required"

state=$here/seed-state.json
[ "$product" != forgejo ] || state=$here/seed-state-forgejo.json
[ -r "$state" ] || die "$state not found: just live-up $product first (it seeds and writes it)"

# The target's docker (DOCKER_HOST for a remote one), for the date rewrites.
# shellcheck source=testenv/live-lib.sh disable=SC1091
. "$here/live-lib.sh"

# -I: no user site, no current directory on the path; the modules are found
# by the entry point itself.
exec python3 -I "$here/scale/seed.py" "$product" --state "$state" "$@"
