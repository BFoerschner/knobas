#!/bin/sh
# Assert docker-compose.yml's published ports against the M1 interfaces doc §5
# port map. Six streams read that map; a silent edit here is a silent break
# there, and `docker compose config -q` is happy with any port at all.
#
# This reads the RESOLVED compose model (`config --format json`), not the YAML
# text. Grepping the file for "8210" would pass on a port that a profile, an
# override file or a ${VAR} turned into something else -- the resolved model is
# the thing that actually gets bound.
#
# Needs docker and jq. Starts nothing and pulls nothing.
set -eu
cd "$(dirname "$0")"

for tool in docker jq; do
  command -v "$tool" >/dev/null || { echo "check-ports: $tool is required" >&2; exit 1; }
done

fail=0
note() { echo "check-ports: $*" >&2; fail=1; }

# mktemp, not a fixed /tmp name: several agents and several worktrees share one
# /tmp on this machine, and a predictable scratch path there is how one stream
# silently reads or overwrites another's file.
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT INT TERM

# show_diff <expected> <actual>
show_diff() {
  printf '%s\n' "$1" > "$tmpdir/expected"
  printf '%s\n' "$2" > "$tmpdir/actual"
  diff -u "$tmpdir/expected" "$tmpdir/actual" >&2 || true
}

# service|host_ip|published|target|protocol, one line per published port.
resolved() {
  docker compose "$@" config --format json \
    | jq -r '.services | to_entries[] | .key as $s
             | (.value.ports // [])[]
             | "\($s)|\(.host_ip)|\(.published)|\(.target)|\(.protocol)"' \
    | sort
}

# `config --format json` on an invalid file prints its error and emits nothing,
# which would arrive at the comparisons below as an EMPTY port map. That still
# fails -- but it blames the port map for what is really a schema error, and
# the next person debugs the wrong file. Name it here instead. (Found by
# mutation MP-4, which deleted a `ports:` entry and got exactly that.)
docker compose config -q 2>&1 \
  || { echo "check-ports: docker-compose.yml is not a valid compose file (default profile); fix that first" >&2; exit 1; }
docker compose --profile seed --profile real-teamcity --profile real-atlassian config -q 2>&1 \
  || { echo "check-ports: docker-compose.yml is not a valid compose file (with all profiles enabled); fix that first" >&2; exit 1; }
docker compose -f docker-compose.yml -f docker-compose.capped.yml config -q 2>&1 \
  || { echo "check-ports: docker-compose.capped.yml does not overlay docker-compose.yml cleanly; fix that first" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 1. The default `docker compose up` set. Anything opt-in that leaks into this
#    list is a ~10 GB surprise for whoever runs it.
# ---------------------------------------------------------------------------
expected_default='gitea|127.0.0.1|3000|3000|tcp
mockd|127.0.0.1|8200|8200|tcp
mockd|127.0.0.1|8210|8210|tcp
mockd|127.0.0.1|8212|8212|tcp
uptime-kuma|127.0.0.1|3001|3001|tcp'

actual_default=$(resolved)
if [ "$actual_default" != "$expected_default" ]; then
  note "default profile port map does not match interfaces §5:"
  show_diff "$expected_default" "$actual_default"
fi

# ---------------------------------------------------------------------------
# 2. Every profile together. The opt-in services' ports are part of the same
#    §5 map and collide with the defaults if anyone gets one wrong.
# ---------------------------------------------------------------------------
expected_all='confluence|127.0.0.1|8090|8090|tcp
gitea|127.0.0.1|3000|3000|tcp
jira|127.0.0.1|8080|8080|tcp
mockd|127.0.0.1|8200|8200|tcp
mockd|127.0.0.1|8210|8210|tcp
mockd|127.0.0.1|8212|8212|tcp
teamcity|127.0.0.1|8111|8111|tcp
uptime-kuma|127.0.0.1|3001|3001|tcp'

actual_all=$(resolved --profile seed --profile real-teamcity --profile real-atlassian)
if [ "$actual_all" != "$expected_all" ]; then
  note "all-profiles port map does not match interfaces §5:"
  show_diff "$expected_all" "$actual_all"
fi

# ---------------------------------------------------------------------------
# 2b. The capped-Gitea overlay (docker-compose.capped.yml). It changes one
#     environment variable and must change nothing else -- an override file is
#     a second, easily forgotten way for the §5 map to drift, and `config -q`
#     above is happy with any port at all.
# ---------------------------------------------------------------------------
actual_capped=$(resolved -f docker-compose.yml -f docker-compose.capped.yml)
if [ "$actual_capped" != "$expected_default" ]; then
  note "the capped overlay changes the port map, which it must not:"
  show_diff "$expected_default" "$actual_capped"
fi

# ---------------------------------------------------------------------------
# 3. No port is reserved any more. 8211 (a Confluence DC mock) was unreserved
#    2026-09-03 by ADR-0013 -- the real container on 8090 is the witness --
#    and 8213 (a low-code-runtime stub) 2026-09-06, when that feature left
#    the plan (roadmap §2 M4). Both are ordinary free ports.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# 4. Nothing may listen off-host. These are developer credentials on an
#    unauthenticated Kuma; a 0.0.0.0 bind puts them on the coffee-shop wifi.
# ---------------------------------------------------------------------------
offhost=$(printf '%s\n' "$actual_all" | awk -F'|' '$2 != "127.0.0.1" { print }')
if [ -n "$offhost" ]; then
  note "these ports are not bound to 127.0.0.1:"
  printf '%s\n' "$offhost" >&2
fi

if [ "$fail" -ne 0 ]; then
  echo "check-ports: FAILED" >&2
  exit 1
fi
echo "check-ports: ok -- default, opt-in and capped-overlay port maps match interfaces §5, all on 127.0.0.1"
