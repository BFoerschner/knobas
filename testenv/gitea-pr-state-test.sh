#!/usr/bin/env bash
# gitea-pr-state.sh's own unit tests (issue #884), run by `just witness-unit`.
# No container: `api` is a stand-in answering the pull request JSON the test
# dictates, so the rule -- a seeded pull request the fixture says is open is
# refused, with the remedy, when Gitea holds it merged; reopened when Gitea holds
# it closed and takes the reopen (#2110); refused when it does not -- is checked
# without the shared Gitea whose fixture it protects. `./seed-gitea.sh` against
# a Gitea with a merged or closed seeded pull request is the live witness.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

ORG=tidewater ADMIN=knobas:pw
API_STATUS=200 API_BODY=''
PATCHED=''
api() {  # the open list answers STUB_OPEN; PATCH logs "<path> <body>" and answers STUB_PATCH
  if [ "$2" = PATCH ]; then
    PATCHED="$PATCHED$3 $4;"; API_STATUS=${STUB_PATCH%% *}; API_BODY=${STUB_PATCH#* }
  elif [ "${3#*/pulls?state=open}" != "$3" ]; then
    API_STATUS=200; API_BODY=${STUB_OPEN:-[]}
  else
    API_STATUS=${STUB_STATUS:-200}; API_BODY=$STUB_BODY
  fi
}
say() { :; }
expect() { [ "$API_STATUS" = "$2" ] || { echo "stub: HTTP $API_STATUS" >&2; exit 2; }; }
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
  checks=$((checks + 1)); : >"$DIED"; PATCHED=''
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
STUB_PATCH='201 {"state":"open"}'
check "a closed unmerged one is reopened" '{"state":"closed","merged":false}' ok
checks=$((checks + 1))
[ "$PATCHED" = '/repos/tidewater/payout-service/pulls/2 {"state":"open"};' ] || { echo "gitea-pr-state-test: FAIL the reopen sent '$PATCHED'" >&2; failures=$((failures + 1)); }
check "a merged one is never reopened" '{"state":"closed","merged":true}' refused "is merged"
checks=$((checks + 1))
[ -z "$PATCHED" ] || { echo "gitea-pr-state-test: FAIL a merged pull request was patched" >&2; failures=$((failures + 1)); }
closed_seeded='{"state":"closed","merged":false,"head":{"ref":"feature/x"},"base":{"ref":"main"}}'
STUB_OPEN='[{"number":1559,"head":{"ref":"feature/x"},"base":{"ref":"main"}},{"number":7,"head":{"ref":"feature/y"},"base":{"ref":"main"}},{"number":8,"head":{"ref":"feature/x"},"base":{"ref":"release"}}]'
check "an open pull request on its head and base is closed first" "$closed_seeded" ok
checks=$((checks + 1))
[ "$PATCHED" = '/repos/tidewater/payout-service/pulls/1559 {"state":"closed"};/repos/tidewater/payout-service/pulls/2 {"state":"open"};' ] ||
  { echo "gitea-pr-state-test: FAIL the interloper's close and the reopen sent '$PATCHED'" >&2; failures=$((failures + 1)); }
STUB_OPEN='[]'
STUB_PATCH='422 {"message":"head branch does not exist"}'
check "a closed one Gitea will not reopen is refused with its answer" '{"state":"closed","merged":false}' refused \
  "is closed" "HTTP 422" "head branch does not exist" "docker volume rm"

if [ "$failures" -ne 0 ]; then echo "gitea-pr-state-test: $failures of $checks failed" >&2; exit 1; fi
echo "gitea-pr-state-test: $checks checks passed"
