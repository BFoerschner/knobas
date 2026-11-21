#!/usr/bin/env bash
# Rebuild the saved scale database (testenv/README.md, *Björn's scale*) as a
# knobas home from a `pg_dump -Fc` of it: the home the perf passes clone and
# open with the windowless backend (testenv/perf/README.md, *On a real
# database*).
#
#   testenv/scale-db-restore.sh <dump> <home> [--force]
#
# 1. The windowless backend (examples/headless_ipc.rs, a debug build of this
#    checkout, or $KNOBAS_HEADLESS_IPC) starts once on an empty <home>
#    (HOME=<home>): knobas makes its cluster as on any first start (initdb
#    with its locale, the superuser and its .pgpass, the loopback port) and
#    migrates an empty database. It is quit.
# 2. That cluster is started by hand (the profile's own binaries, password and
#    a loopback port), `knobas` dropped and created again with the locale
#    knobas gave it, the dump restored into it, and every source's interval set
#    to ten years, so no run starts by itself (*Sync now* still runs one).
# 3. The sources' credentials are written to the profile's dev-secrets.json
#    from this checkout's seed state (testenv/seed-state.json,
#    seed-state-forgejo.json, kuma-api-key; Jira and Confluence take the
#    README's admin pair): a debug build reads them there.
# 4. The backend starts again on the restored database and migrates it to this
#    checkout's head. It is quit, the mirror's rows per source and kind are
#    checked against the restored dump's, and the database is analyzed.
#
# The PostgreSQL binaries are the machine's: <home>/.theseus links to
# ~/.theseus (18.6, as knobas pins). Nothing is fetched and no product is
# asked anything: the sources keep the addresses the dump holds.
#
# Idempotent: <home> gets a scale-db-restore.json when done; a second run with
# the same dump says so and changes nothing. A <home> that exists, is not
# empty and has no such file is refused, so the script never touches a home it
# did not make. --force rebuilds a home it made. Never Björn's own profile:
# <home> may not be $HOME.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
say() { echo "scale-db-restore: $*"; }
die() { echo "scale-db-restore: $*" >&2; exit 1; }

usage="usage: testenv/scale-db-restore.sh <dump> <home> [--force]"
dump=${1:?$usage}
home=${2:?$usage}
force=
[ "${3:-}" = "" ] || [ "${3:-}" = --force ] || die "$usage"
[ "${3:-}" = --force ] && force=1

readonly INTERVAL_SECS=315360000 # ten years: the scheduler's next run is never due
readonly MARK=scale-db-restore.json

[ -r "$dump" ] || die "no dump at $dump"
dump=$(cd "$(dirname "$dump")" && pwd)/$(basename "$dump")
mkdir -p "$home"
home=$(cd "$home" && pwd -P)
real_home=$(cd "$HOME" && pwd -P)
[ "$home" != "$real_home" ] || die "<home> is your own HOME: give a scratch directory"
case "$home/" in "$real_home/Library/Application Support/"*) die "<home> is inside your own profile" ;; esac

pg=$HOME/.theseus/postgresql/18.6.0/bin
[ -x "$pg/pg_ctl" ] || die "no PostgreSQL 18.6 at $pg: start any knobas once, which installs it"
for f in seed-state.json seed-state-forgejo.json kuma-api-key; do
    [ -r "$here/$f" ] || die "no testenv/$f in this checkout: copy it from the root checkout (CLAUDE.md, Showing Björn the app)"
done

sha=$(shasum -a 256 "$dump" | cut -d' ' -f1)
if [ -e "$home/$MARK" ]; then
    if [ -z "$force" ] && [ "$(jq -r '.dump_sha256 // empty' "$home/$MARK")" = "$sha" ] &&
        [ "$(jq -r '.done // false' "$home/$MARK")" = true ]; then
        say "$home already holds this dump, migrated to $(jq -r .migration "$home/$MARK") on $(jq -r .head "$home/$MARK"): nothing to do (--force rebuilds it)"
        exit 0
    fi
    say "rebuilding $home (a home this script made)"
    rm -rf "$home/Library" "$home/restore-logs"
    rm -f "$home/.theseus" "$home/$MARK"
elif [ -n "$(ls -A "$home")" ]; then
    die "$home exists, is not empty and was not made by this script: give a new directory"
fi

profile=$home/Library/Application\ Support/dev.knobas.desktop
demo=$profile/demo
db=$demo/db
logs=$home/restore-logs
mkdir -p "$logs"
ln -s "$HOME/.theseus" "$home/.theseus"
jq -n --arg sha "$sha" --arg dump "$dump" '{dump_sha256: $sha, dump: $dump, done: false}' > "$home/$MARK"

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

bin=${KNOBAS_HEADLESS_IPC:-}
if [ -z "$bin" ]; then
    say "building the windowless backend (debug, so it reads dev-secrets.json)"
    env -C "$repo" env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app --features test-util --example headless_ipc --quiet
    bin=$repo/target/debug/examples/headless_ipc
fi
[ -x "$bin" ] || die "no backend at $bin"
head=$(git -C "$repo" rev-parse --short HEAD)

# backend <log>: start it on <home>, wait until it answers (the database is up
# and migrated by then), quit it and wait for it to exit.
backend() {
    local log=$1 port pid
    port=$(free_port)
    HOME="$home" "$bin" --data-dir "$profile" --port "$port" > "$log" 2>&1 &
    pid=$!
    for _ in $(seq 1 600); do
        grep -q 'answering on' "$log" && break
        kill -0 "$pid" 2>/dev/null || { cat "$log" >&2; die "the backend exited before answering"; }
        sleep 1
    done
    grep -q 'answering on' "$log" || { kill "$pid"; die "the backend did not answer in 10 minutes ($log)"; }
    grep 'headless_ipc: database' "$log" | sed 's/^/  /'
    curl -s -m 30 -X POST "http://127.0.0.1:$port/quit" > /dev/null || true
    for _ in $(seq 1 120); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
    if kill -0 "$pid" 2>/dev/null; then kill "$pid"; die "the backend did not quit ($log)"; fi
    wait "$pid" || true
    [ ! -e "$db/data/postmaster.pid" ] || die "the backend quit but left its PostgreSQL running ($db/data)"
}

# The profile's cluster by hand: its binaries, its password, a loopback port.
pg_port=
pg_up() {
    pg_port=$(free_port)
    "$pg/pg_ctl" -D "$db/data" -l "$logs/pg.log" -w -t 120 \
        -o "-p $pg_port -c listen_addresses=127.0.0.1 -c unix_socket_directories=" start > /dev/null
}
pg_down() { "$pg/pg_ctl" -D "$db/data" -m fast -w stop > /dev/null; }
q() { # <database> <sql>
    PGPASSWORD=$(cat "$db/.pgpass") PGCLIENTENCODING=UTF8 \
        "$pg/psql" -h 127.0.0.1 -p "$pg_port" -U postgres -d "$1" -v ON_ERROR_STOP=1 -X -q -Atc "$2"
}
trap '[ -e "$db/data/postmaster.pid" ] && "$pg/pg_ctl" -D "$db/data" -m fast -w stop > /dev/null 2>&1; true' EXIT

say "1/4 first start: knobas makes its cluster in $db"
backend "$logs/backend-first.log"

say "2/4 restoring $(basename "$dump") into it"
pg_up
read -r provider locale collate ctype encoding < <(q postgres \
    "select datlocprovider, coalesce(datlocale, '-'), datcollate, datctype, pg_encoding_to_char(encoding)
       from pg_database where datname = 'knobas'" | tr '|' ' ')
case $provider in
    b) loc="locale_provider builtin builtin_locale '$locale'" ;;
    i) loc="locale_provider icu icu_locale '$locale'" ;;
    *) loc="locale_provider libc" ;;
esac
say "  knobas's database: provider $provider, locale $locale, collate $collate, ctype $ctype, $encoding"
q postgres "drop database knobas"
q postgres "create database knobas template template0 encoding '$encoding' $loc lc_collate '$collate' lc_ctype '$ctype'"
PGPASSWORD=$(cat "$db/.pgpass") "$pg/pg_restore" -h 127.0.0.1 -p "$pg_port" -U postgres -d knobas \
    --no-owner --exit-on-error -j 4 "$dump"
counts="select source_id || ' ' || kind || ' ' || count(*) from sync.item group by source_id, kind order by 1"
q knobas "$counts" > "$logs/items-restored.txt"
dumped=$(q knobas "select coalesce(max(version), 0) from public._sqlx_migrations")
q knobas "update knobas.source_config set sync_interval_secs = $INTERVAL_SECS"
pg_down
say "  restored at migration $dumped, $(awk '{n += $3} END {print n}' "$logs/items-restored.txt") items"

say "3/4 credentials from the seed state into $demo/dev-secrets.json"
gitea=$(jq -r '.gitea.token' "$here/seed-state.json")
forgejo=$(jq -r '.gitea.token' "$here/seed-state-forgejo.json")
teamcity=$(jq -r '.teamcity.token // empty' "$here/seed-state.json")
jira=$(jq -r '.jira.password // "knobas-dev"' "$here/seed-state.json")
confluence=$(jq -r '.confluence.password // "knobas-dev"' "$here/seed-state.json")
kuma=$(cat "$here/kuma-api-key")
umask 077
jq -n --arg gitea "$gitea" --arg forgejo "$forgejo" --arg teamcity "$teamcity" \
    --arg jira "$jira" --arg confluence "$confluence" --arg kuma "$kuma" '
    def env(kind; secret): {v: 2, kind: kind, secret: secret} | tojson;
    {"source:live-gitea": env("pat"; $gitea),
     "source:live-forgejo": env("pat"; $forgejo),
     "source:live-jira": env("user_password"; $jira),
     "source:live-confluence": env("user_password"; $confluence),
     "source:live-kuma": ({v: 2, kind: "api_token", secret: $kuma,
                          account: {username: "knobas", password: "knobas-dev"}} | tojson)}
    + (if $teamcity == "" then {} else {"source:live-teamcity": env("pat"; $teamcity)} end)' \
    > "$demo/dev-secrets.json"
umask 022

say "4/4 the backend on the restored database: migrations to $head's"
backend "$logs/backend-migrate.log"
pg_up
q knobas "$counts" > "$logs/items-migrated.txt"
migration=$(q knobas "select max(version) from public._sqlx_migrations")
diff "$logs/items-restored.txt" "$logs/items-migrated.txt" > /dev/null ||
    die "the mirror's rows changed in the migration: diff $logs/items-restored.txt $logs/items-migrated.txt"
q knobas "vacuum analyze" > /dev/null
q knobas "select id || ' ' || base_url from knobas.source_config order by id" | sed 's/^/  /'
total=$(awk '{n += $3} END {print n}' "$logs/items-migrated.txt")
size=$(q knobas "select pg_size_pretty(pg_database_size('knobas'))")
pg_down
jq --arg head "$head" --argjson from "$dumped" --argjson to "$migration" --argjson items "$total" \
    --arg at "$(date -u +%FT%TZ)" '. + {head: $head, dumped_at_migration: $from, migration: $to, items: $items,
    sync_interval_secs: '"$INTERVAL_SECS"', finished_at: $at, done: true}' "$home/$MARK" > "$home/$MARK.tmp"
mv "$home/$MARK.tmp" "$home/$MARK"
say "done: $home, migrations $dumped -> $migration, $total items ($size), rows per source and kind in $logs/items-migrated.txt"
