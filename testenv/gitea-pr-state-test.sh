#!/usr/bin/env bash
# gitea-pr-state.sh's own unit tests (issue #884), run by `just witness-unit`.
# No container: `api` is a stand-in answering the pull request JSON the test
# dictates, so the rule -- a seeded pull request the fixture says is open is
# refused, with the remedy, when Gitea holds it merged or closed -- is checked
# without the shared Gitea whose fixture it protects. `./seed-gitea.sh` against
# a Gitea with a merged seeded pull request is the live witness.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

ORG=tidewater ADMIN=knobas:pw
API_STATUS=200 API_BODY=''
api() { API_STATUS=${STUB_STATUS:-200}; API_BODY=$STUB_BODY; }
expect() { [ "$API_STATUS" = 200 ] || { echo "stub: HTTP $API_STATUS" >&2; exit 2; }; }
die() { printf '%s\n' "$*" >"$DIED"; return 1; }

# shellcheck source=testenv/gitea-pr-state.sh disable=SC1091
. "$here/gitea-pr-state.sh"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-gitea-pr-state-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
DIED=$scratch/died
failures=0 checks=0

check() {  # check <what> <body> <want: ok|refused> [<text the refusal must hold>...]
  local what=$1 want=$3
  STUB_BODY=$2; shift 3
  checks=$((checks + 1)); : >"$DIED"
  local got=ok
  gitea_require_open_pr payout-service 2 "Add payout CSV export" || got=refused
  if [ "$got" != "$want" ]; then
    echo "gitea-pr-state-test: FAIL $what: wanted $want, got $got" >&2; failures=$((failures + 1)); return
  fi
  local text
  for text in "$@"; do
    grep -qF -- "$text" "$DIED" || { echo "gitea-pr-state-test: FAIL $what: refusal lacks: $text" >&2; failures=$((failures + 1)); }
  done
}

check "an open pull request passes" '{"state":"open","merged":false}' ok
check "a merged one is refused with the remedy" '{"state":"closed","merged":true}' refused \
  "is merged" "payout-service#2" "docker volume rm" "testenv/reset"
check "a closed unmerged one is refused" '{"state":"closed","merged":false}' refused "is closed"

if [ "$failures" -ne 0 ]; then echo "gitea-pr-state-test: $failures of $checks failed" >&2; exit 1; fi
echo "gitea-pr-state-test: $checks checks passed"
