#!/usr/bin/env bash
# `./seed --env` unit test (issue #982): a state with no Gitea block exports no
# Gitea variable, not the literal `null` the probe took for a URL. Runs a copy
# of `seed` beside a scratch seed-state.json; no container is started.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-seed-env-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cp "$here/seed" "$scratch/seed"

echo '{"confluence":{"url":"http://c","user":"u","password":"p"}}' >"$scratch/seed-state.json"
out=$("$scratch/seed" --env)
if grep -q 'GITEA\|null' <<<"$out"; then echo "FAIL: Confluence-only state exported Gitea or null:" >&2; echo "$out" >&2; exit 1; fi
grep -q 'export KNOBAS_CONFLUENCE_URL=http://c' <<<"$out" || { echo "FAIL: Confluence block missing" >&2; exit 1; }

echo '{"gitea":{"url":"http://g","token":"t","org":"o"}}' >"$scratch/seed-state.json"
out=$("$scratch/seed" --env)
grep -q 'export KNOBAS_GITEA_URL=http://g' <<<"$out" || { echo "FAIL: Gitea block missing" >&2; exit 1; }
echo "seed-env-test: ok"
