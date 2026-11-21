#!/usr/bin/env bash
# *Everything* finds a repository by its files, against the live forges (the
# everything-search decisions, 2026-11-20, D16; ADR-0013: the real instance is
# the witness). Gitea and Forgejo as sources on a demo profile of its own, the
# app's backend with no window (`examples/headless_ipc`) syncing them as the
# app does, and the launcher's own `search` command asked:
#
#   1. The first fill: from an empty profile until `settlebridge` finds
#      `tidewater/ledger-gateway` on each forge, shown by its class line.
#      Björn's renamed repository: its name, README and pom say nothing of
#      SettleBridge, and only its code does (`seed-gitea.sh` section 4c pushes
#      the fixture's files).
#   2. A push: a file of ledger-gateway written with a fresh word, the source
#      synced, the word found; the file changed to another word, synced, the
#      new word found and the old one gone; the file deleted, synced, gone.
#      The repository's files end as the seed left them.
#
#   testenv/live-everything.sh [--root <dir>] [--keep] [--wait S]
#
# Each step prints how long it took. <dir> is a scratch HOME, made under
# $TMPDIR and removed at the end unless --keep; Björn's own profiles are never
# touched. The forges must be seeded and answering (`just live-up gitea`,
# `just live-up forgejo`, then `testenv/live-sources.sh`'s `./seed --env`);
# the run writes to `tidewater/ledger-gateway` as the seed's admin and leaves
# its files as it found them. Exits non-zero naming the step that failed.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
say() { echo "live-everything: $(date +%H:%M:%S) $*"; }
die() { echo "live-everything: $*" >&2; exit 1; }

root='' keep='' wait_s=600
while [ $# -gt 0 ]; do
  case $1 in
    --root) root=${2:?--root needs a directory}; shift 2 ;;
    --keep) keep=1; shift ;;
    --wait) wait_s=${2:?}; shift 2 ;;
    *) die "usage: testenv/live-everything.sh [--root <dir>] [--keep] [--wait S]" ;;
  esac
done
if [ -z "$root" ]; then
  root=$(mktemp -d "${TMPDIR:-/tmp}/knobas-live-everything.XXXXXX")
else
  mkdir -p "$root"
  root=$(cd "$root" && pwd)
fi
[ -e "$root/.theseus" ] || ln -s "$HOME/.theseus" "$root/.theseus"
data="$root/Library/Application Support/dev.knobas.desktop"
db="$data/demo/db"
pg=$HOME/.theseus/postgresql/18.6.0/bin

# The forges' admin, as the seed made it, and their http addresses.
ADMIN=knobas:knobas-dev
GITEA_HTTP=${KNOBAS_GITEA_URL:-http://127.0.0.1:3000}
FORGEJO_HTTP=${KNOBAS_FORGEJO_URL:-http://127.0.0.1:3002}
REPO=tidewater/ledger-gateway
FILE=LIVE-EVERYTHING.md

now_ms() { python3 -c 'import time; print(int(time.time() * 1000))'; }

say "building the backend (examples/headless_ipc) and the live-sources binary"
env -C "$repo" env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app --features test-util --example headless_ipc --quiet
backend=$repo/target/debug/examples/headless_ipc

KNOBAS_LIVE_SOURCES="gitea forgejo" "$here/live-sources.sh" add --data-dir "$data" \
  || die "live-sources could not add both forges (above)"

port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
log=$root/backend.log
stop_db() { [ ! -e "$db/data/postmaster.pid" ] || "$pg/pg_ctl" -D "$db/data" stop -m fast -w >/dev/null 2>&1 || true; }
backend_pid=''
# shellcheck disable=SC2329 # the trap's
finish() {
  status=$?
  if [ -n "$backend_pid" ] && kill -0 "$backend_pid" 2>/dev/null; then
    curl -s -m 30 -X POST "http://127.0.0.1:$port/quit" >/dev/null 2>&1 || true
    for _ in $(seq 1 30); do kill -0 "$backend_pid" 2>/dev/null || break; sleep 1; done
    kill "$backend_pid" 2>/dev/null || true
  fi
  stop_db
  if [ -z "$keep" ]; then rm -rf "$root"; else say "kept $root"; fi
  exit "$status"
}
trap finish EXIT

t0=$(now_ms)
say "starting the backend on 127.0.0.1:$port (log $log)"
HOME=$root "$backend" --data-dir "$data" --port "$port" >>"$log" 2>&1 &
backend_pid=$!

invoke() { # invoke <json body>: the answer's `ok`, or fails
  local out
  out=$(curl -s -m 30 -X POST -H 'Content-Type: application/json' --data-binary "$1" "http://127.0.0.1:$port/invoke") || return 1
  printf '%s' "$out" | jq -e 'has("ok")' >/dev/null || { echo "$out" >&2; return 1; }
  printf '%s' "$out" | jq -c .ok
}

# The repository rows `word` finds under *Everything* on `source`, each
# `title<TAB>file path<TAB>by_text`.
repos_for() { # source word
  invoke "$(jq -cn --arg s "$1" --arg w "$2" \
    '{cmd: "search", args: {query: {raw: $w, limit: 40, titles: false,
      filters: {sources: [$s], kinds: [], updated_within_days: null, mine: false}}}}')" |
    jq -r '.groups[] | select(.kind == "repo") | .hits[] |
      [.title, (.excerpts[0].file.path // ""), (.files.by_text // 0 | tostring)] | @tsv'
}

until invoke '{"cmd": "list_sources", "args": {}}' >/dev/null 2>&1; do
  kill -0 "$backend_pid" 2>/dev/null || die "the backend exited before it answered; $log"
  sleep 1
done
say "the backend answers ($(( $(now_ms) - t0 )) ms)"

# wait_until <what> <command...>: runs it every 2 s until it succeeds, within --wait.
wait_until() {
  local what=$1 started deadline
  shift
  started=$(now_ms)
  deadline=$((SECONDS + wait_s))
  until "$@"; do
    [ "$SECONDS" -lt "$deadline" ] || die "$what: not within ${wait_s}s ($log)"
    kill -0 "$backend_pid" 2>/dev/null || die "$what: the backend exited ($log)"
    sleep 2
  done
  say "$what: $(( $(now_ms) - started )) ms"
}

found_by_its_class() { # source
  local first
  first=$(repos_for "$1" settlebridge | head -n 1)
  case $first in
    "$REPO"$'\t'*SettleBridgeServer.java$'\t'*) return 0 ;;
    *) return 1 ;;
  esac
}
finds() { repos_for "$1" "$2" | cut -f1 | grep -qx "$REPO"; }
misses() { ! finds "$1" "$2"; }

# 1. The first fill, both forges at once: the scheduler syncs every source as
# it starts, and each run's tree and files passes run after it.
for source in live-gitea live-forgejo; do
  wait_until "first fill, $source: settlebridge finds $REPO by its class" found_by_its_class "$source"
  say "  $(repos_for "$source" settlebridge | head -n 1 | tr '\t' ' ')"
done

# 2. A push, on each forge.
contents() { # http method path [json]: the forge's contents API as its admin
  local url=$1 method=$2 path=$3
  if [ $# -gt 3 ]; then
    curl -s -f -m 30 -u "$ADMIN" -X "$method" -H 'Content-Type: application/json' -d "$4" "$url/api/v1/repos/$REPO/contents/$path"
  else
    curl -s -f -m 30 -u "$ADMIN" -X "$method" "$url/api/v1/repos/$REPO/contents/$path"
  fi
}
word() { python3 -c 'import random, string; print("".join(random.choice(string.ascii_lowercase) for _ in range(12)))'; }
sync_now() { invoke "$(jq -cn --arg s "$1" '{cmd: "sync_now", args: {sourceId: $s}}')" >/dev/null; }
push_round() { # source http
  local source=$1 url=$2 first second sha started
  first=zq$(word) second=zq$(word)
  if contents "$url" GET "$FILE?ref=main" >/dev/null 2>&1; then
    die "$REPO already holds $FILE on $source: a killed run left it; delete it and run again"
  fi
  started=$(now_ms)
  contents "$url" POST "$FILE" "$(jq -cn --arg c "$(printf 'Marker: %s\n' "$first" | base64)" \
    '{branch: "main", message: "live-everything: a fresh word", content: $c}')" >/dev/null \
    || die "$source: writing $FILE"
  say "$source: pushed $FILE saying $first ($(( $(now_ms) - started )) ms)"
  sync_now "$source"
  wait_until "$source: the pushed word found after a sync" finds "$source" "$first"

  sha=$(contents "$url" GET "$FILE?ref=main" | jq -r .sha)
  contents "$url" PUT "$FILE" "$(jq -cn --arg c "$(printf 'Marker: %s\n' "$second" | base64)" --arg s "$sha" \
    '{branch: "main", message: "live-everything: another word", content: $c, sha: $s}')" >/dev/null \
    || die "$source: changing $FILE"
  say "$source: changed $FILE to say $second"
  sync_now "$source"
  wait_until "$source: the new word found after a sync" finds "$source" "$second"
  misses "$source" "$first" || die "$source: the old word $first is still found"
  say "$source: the old word is gone"

  sha=$(contents "$url" GET "$FILE?ref=main" | jq -r .sha)
  contents "$url" DELETE "$FILE" "$(jq -cn --arg s "$sha" \
    '{branch: "main", message: "live-everything: the word removed", sha: $s}')" >/dev/null \
    || die "$source: deleting $FILE"
  say "$source: deleted $FILE"
  sync_now "$source"
  wait_until "$source: the deleted file's word gone after a sync" misses "$source" "$second"
}
push_round live-gitea "$GITEA_HTTP"
push_round live-forgejo "$FORGEJO_HTTP"

# The renamed repository is still found by its class after the round.
for source in live-gitea live-forgejo; do
  found_by_its_class "$source" || die "$source: settlebridge no longer finds $REPO first"
done
say "passed: both forges, the first fill and a push's update ($(( $(now_ms) - t0 )) ms in all)"
