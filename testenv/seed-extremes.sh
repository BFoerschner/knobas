#!/usr/bin/env bash
# The worst shapes a real team's data has, on the live products, beside the
# Tidewater seed and Björn's scale estate, never in them (the extremes pass of
# PR #2479, 2026-11-13; testenv/README.md, *The extremes*):
#
#   testenv/seed-extremes.sh <jira|confluence|gitea|teamcity> [--workers W]
#   testenv/seed-extremes.sh <product> --remove      # the scope deleted again
#   just seed-extremes <product> [--remove]
#
# Each product's extremes have a scope of their own -- Jira project XTRM,
# Confluence space XTRM, Gitea org knobas-extremes, TeamCity project Extremes
# -- so no live suite and no profile that syncs the seed or the scale estate
# sees them. A run is idempotent: it reads what is there and adds what is
# missing. It holds the product's live lock (testenv/README.md, *One
# environment, one owner at a time*) for its whole length, so it never writes
# into a product while that product's suite runs.
#
# The product must be up and seeded (`just live-up <product>`): the address
# and credentials come from this worktree's seed-state.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

die() { echo "seed-extremes: $*" >&2; exit 1; }

product=${1:-}
case $product in
  jira | confluence | gitea | teamcity) shift ;;
  *) die "usage: testenv/seed-extremes.sh <jira|confluence|gitea|teamcity> [--workers W] [--remove]" ;;
esac

command -v python3 >/dev/null || die "python3 is required (the stdlib only)"
# KNOBAS_EXTREMES_STATE names another seed state (the products' addresses and credentials), for a worktree whose own
# lacks a product's block because another run rewrote it.
state=${KNOBAS_EXTREMES_STATE:-$here/seed-state.json}
[ -r "$state" ] || die "$state not found: just live-up $product first (it seeds and writes it)"

# The target's docker (DOCKER_HOST for a remote one) and the locks.
# shellcheck source=testenv/live-lib.sh disable=SC1091
. "$here/live-lib.sh"

live_lock_traps
if [ "$product" = gitea ]; then
  live_lock_hold "seed-extremes-$product" gitea
else
  live_lock_hold "seed-extremes-$product" jvm "$product"
fi

# -I: no user site, no current directory on the path; the modules are found
# by the entry point itself. Not exec'd: the traps give the lock back.
python3 -I "$here/extremes/seed.py" "$product" --state "$state" "$@"
