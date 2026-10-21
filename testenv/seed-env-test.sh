#!/usr/bin/env bash
# `./seed --env` unit test (issue #982): a state with no Gitea block exports no
# Gitea variable, not the literal `null` the probe took for a URL. Runs a copy
# of `seed` beside a scratch seed-state.json; no container is started.
set -euo pipefail
# shellcheck disable=SC2016,SC2015
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-seed-env-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cp "$here/seed" "$here/gitea-token.sh" "$scratch/"
# A curl that answers the token probe with $STUB_STATUS (the recorded token is
# probed against the instance, #1377); no container is started.
mkdir "$scratch/bin"
printf '#!/bin/sh\nprintf "%%s" "${STUB_STATUS:-200}"\n' >"$scratch/bin/curl"
chmod +x "$scratch/bin/curl"
export PATH="$scratch/bin:$PATH"

echo '{"confluence":{"url":"http://c","user":"u","password":"p"}}' >"$scratch/seed-state.json"
out=$("$scratch/seed" --env)
if grep -q 'GITEA\|null' <<<"$out"; then echo "FAIL: Confluence-only state exported Gitea or null:" >&2; echo "$out" >&2; exit 1; fi
grep -q 'export KNOBAS_CONFLUENCE_URL=http://c' <<<"$out" || { echo "FAIL: Confluence block missing" >&2; exit 1; }

echo '{"gitea":{"url":"http://g","token":"t","org":"o"}}' >"$scratch/seed-state.json"
out=$("$scratch/seed" --env)
grep -q 'export KNOBAS_GITEA_URL=http://g' <<<"$out" || { echo "FAIL: Gitea block missing" >&2; exit 1; }
# A token the instance refuses ends --env before it prints a thing, naming the
# token, the status and ./seed (#1377).
rc=0
out=$(STUB_STATUS=401 "$scratch/seed" --env 2>"$scratch/err") || rc=$?
[ "$rc" -ne 0 ] || { echo "FAIL: a 401 token was printed" >&2; exit 1; }
[ -z "$out" ] || { echo "FAIL: a refused token still printed: $out" >&2; exit 1; }
grep -q '401' "$scratch/err" && grep -q './seed' "$scratch/err" && grep -qi 'token' "$scratch/err" \
  || { echo "FAIL: refusal does not name token, 401 and ./seed:" >&2; cat "$scratch/err" >&2; exit 1; }
# No answer is refused too; a Confluence-only state probes nothing.
rc=0; STUB_STATUS=000 "$scratch/seed" --env >/dev/null 2>&1 || rc=$?
[ "$rc" -ne 0 ] || { echo "FAIL: an unanswered probe was accepted" >&2; exit 1; }
echo '{"confluence":{"url":"http://c","user":"u","password":"p"}}' >"$scratch/seed-state.json"
STUB_STATUS=401 "$scratch/seed" --env >/dev/null || { echo "FAIL: no Gitea block, yet probed" >&2; exit 1; }
echo "seed-env-test: ok"
