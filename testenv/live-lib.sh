#!/usr/bin/env bash
# The real products the live suites run against, started on this machine on
# demand and stopped again (issue #633). Sourced by `testenv/live` and by the
# `just *-live` recipes; `live-test.sh` holds the decisions below to account.
#
# BJÖRN'S DECISION, 2026-10-12: "Since we focus on one source now at a time we
# don't have to run them 24/7, so we can just spin one up when we have to
# test." So TeamCity, Jira, Confluence and Gitea run in the notebook's Docker,
# one at a time, started when a live suite needs one and stopped after it.
# This supersedes the 2026-09-06 move of the three JVM products to Hetzner for
# live suites (hetzner/README.md); ADR-0013 is unchanged -- the witness is
# still the real product. Recorded in docs/decisions/2026-10-unattended-rulings.md.
#
# Four products, and what each brings up:
#
#   gitea       knobas-gitea                          seed-gitea.sh
#   teamcity    gitea, then knobas-teamcity and its   seed-teamcity.sh (which
#               agent -- the VCS roots clone Gitea    runs the builds seed too)
#   jira        knobas-jira-db, knobas-jira           seed-atlassian.sh jira, then
#                                                     its content, twice, verified
#   confluence  knobas-confluence-db, knobas-confluence   ... the same for Confluence
#
# ONE JVM PRODUCT AT A TIME. The Docker VM here has 8 GB; Jira wants ~4 GB,
# Confluence ~2 GB, TeamCity's server and agent ~2.2 GB, and two JVMs claiming
# their heaps at once is what killed #313's windows. `live_guard` refuses a
# second one in one line that names the one that is up. Gitea is not a JVM and
# is never refused.
#
# WHAT `down` MEANS DIFFERS, ON PURPOSE. TeamCity and Gitea are *stopped*:
# their volumes hold the seeded builds and the seeded corpus, and the next `up`
# finds them seeded in seconds. Jira and Confluence are `down -v`: they run on
# 3-hour timebomb licences that a restart does not renew, so a kept volume is a
# product whose licence has already run out. Their seed-state.json block goes
# with them, so `./seed --env` stops printing a URL nothing answers.
#
# NEVER THE SERVERS. A shell that has sourced hetzner/env has a `docker` on its
# PATH that routes these containers' names to the Hetzner servers; a call made
# through it from here would start a product on a server the live suites no
# longer use. `live_guard` refuses that shell, and a docker context aimed at
# one of the servers, before anything else.

LIVE_TESTENV=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# The containers whose presence means "this product is up".
_live_main() {
  case "$1" in
    gitea) echo knobas-gitea ;;
    teamcity) echo knobas-teamcity ;;
    jira) echo knobas-jira ;;
    confluence) echo knobas-confluence ;;
    *) return 1 ;;
  esac
}

# live_needs <product> -- the products a run of this one brings up, in the
# order it brings them up.
live_needs() {
  case "$1" in
    teamcity) echo "gitea teamcity" ;;
    *) echo "$1" ;;
  esac
}

# live_running <container>
live_running() {
  docker ps --format '{{.Names}}' | grep -qx -- "$1"
}

live_is_up() {
  live_running "$(_live_main "$1")"
}

# live_guard <product> -- refuse, in one line on stderr and before any docker
# call that changes anything, a product this machine must not start now.
live_guard() {
  local product=$1 other context
  _live_main "$product" >/dev/null || {
    echo "live: unknown product '$product'; one of: gitea teamcity jira confluence" >&2
    return 1
  }
  if [ -n "${KNOBAS_TESTENV_HETZNER:-}" ] || case "$(command -v docker)" in */hetzner/bin/docker) true ;; *) false ;; esac; then
    echo "live: this shell has sourced testenv/hetzner/env, whose docker shim routes to the Hetzner servers; live products run locally now -- open a fresh shell" >&2
    return 1
  fi
  context=$(docker context show 2>/dev/null || true)
  case $context in
    knobas-*)
      echo "live: the docker context is '$context', a Hetzner server; live products run locally now -- docker context use orbstack" >&2
      return 1 ;;
  esac
  [ "$product" != gitea ] || return 0
  for other in teamcity jira confluence; do
    [ "$other" != "$product" ] || continue
    if live_is_up "$other"; then
      echo "live: $(_live_main "$other") is running; one JVM product at a time on the 8 GB Docker VM -- 'just live-down $other' first" >&2
      return 1
    fi
  done
}

# live_to_put_back <product> -- the products a run of this one starts and must
# therefore stop again, last-started first: whatever was not already up.
live_to_put_back() {
  local p out=""
  for p in $(live_needs "$1"); do
    live_is_up "$p" || out="$p${out:+ $out}"
  done
  echo "$out"
}

_live_compose() {
  (cd "$LIVE_TESTENV" && docker compose "$@")
}

# The timebomb keys, fetched once per shell. Before either Atlassian `up`:
# Confluence reads its key at first start.
_live_licences() {
  if [ -z "${JIRA_LICENSE_KEY:-}" ] || [ -z "${CONFLUENCE_LICENSE_KEY:-}" ]; then
    eval "$("$LIVE_TESTENV/fetch-timebomb-keys.sh")"
    export JIRA_LICENSE_KEY CONFLUENCE_LICENSE_KEY
  fi
}

# _live_start <product> -- start one product (not its needs) and seed it.
# Every seed is idempotent, so starting a product that is already up is a
# re-check that writes nothing.
_live_start() {
  local product=$1
  echo "live: $product: up and seeded ..."
  case $product in
    gitea)
      _live_compose up -d --wait gitea
      (cd "$LIVE_TESTENV" && ./seed-gitea.sh) ;;
    teamcity)
      _live_compose --profile real-teamcity up -d teamcity teamcity-agent
      (cd "$LIVE_TESTENV" && ./seed-teamcity.sh) ;;
    jira | confluence)
      _live_licences
      _live_compose --profile real-atlassian up -d --wait "$product-db" "$product"
      (cd "$LIVE_TESTENV" && ./seed-atlassian.sh "$product")
      # Twice, deliberately: the second run reaches the find-then-skip branches
      # a seed from empty never does (#396), and must write nothing.
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product")
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product")
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product" --verify) ;;
  esac
  echo "live: $product: up"
}

# live_up <product> -- the product and what it needs, guarded.
live_up() {
  local p
  live_guard "$1" || return 1
  for p in $(live_needs "$1"); do _live_start "$p"; done
}

# live_down <product> -- this product only; what it needed stays as it is.
live_down() {
  _live_main "$1" >/dev/null || { live_guard "$1"; return 1; }
  case $1 in
    gitea) _live_compose stop gitea ;;
    teamcity) _live_compose --profile real-teamcity stop teamcity teamcity-agent ;;
    jira | confluence)
      # The two services NAMED: a profile-scoped `down -v` with none named
      # takes the default profile's containers and volumes too, Gitea's
      # seeded corpus included.
      _live_compose --profile real-atlassian down -v "$1" "$1-db"
      local state=$LIVE_TESTENV/seed-state.json
      if [ -f "$state" ]; then
        jq --arg k "$1" 'del(.[$k])' "$state" >"$state.tmp" && mv "$state.tmp" "$state"
      fi ;;
  esac
  echo "live: $1: down"
}

# Commands a recipe registers to run before the products are put back, last
# registered first -- `gitea-live-capped`'s uncap is one.
LIVE_ON_EXIT=()
live_on_exit() { LIVE_ON_EXIT=("$1" ${LIVE_ON_EXIT[@]+"${LIVE_ON_EXIT[@]}"}); }

LIVE_PUT_BACK=""
_live_restore() {
  local hook p
  for hook in ${LIVE_ON_EXIT[@]+"${LIVE_ON_EXIT[@]}"}; do eval "$hook" || true; done
  LIVE_ON_EXIT=()
  for p in $LIVE_PUT_BACK; do live_down "$p" || true; done
  LIVE_PUT_BACK=""
}

# live_session <product> -- for a recipe: bring the product up, export what
# `./seed --env` prints, and arm EXIT, INT and TERM to put the machine back as
# this found it. A product that was already up -- `just live-up jira` before a
# run of `just jira-live` -- is left up; one this started is stopped.
#
# INT and TERM as well as EXIT, the `gitea-live-capped` rule: bash runs no EXIT
# trap for a signal it is not trapping, and a Ctrl-C'd run is the one that
# leaves a JVM product holding the VM. Each clears EXIT, restores once and
# re-raises. Armed before the first `up`, so an interrupt mid-seed stops what
# had been started.
live_session() {
  live_guard "$1" || exit 1
  LIVE_PUT_BACK=$(live_to_put_back "$1")
  trap '_live_restore' EXIT
  trap 'trap - EXIT INT; _live_restore; kill -INT $$' INT
  trap 'trap - EXIT TERM; _live_restore; kill -TERM $$' TERM
  live_up "$1"
  eval "$(cd "$LIVE_TESTENV" && ./seed --env)"
  if [ -n "$LIVE_PUT_BACK" ]; then
    echo "live: $1 is up for this run; $LIVE_PUT_BACK will be stopped at its end"
  else
    echo "live: $1 was already up and stays up"
  fi
}
