#!/usr/bin/env bash
# Sync the live products, filled by seed-scale.sh, into a knobas database of
# its own, the way Björn's app does: the debug app (target/debug/knobas-app
# --demo, the one `just demo` runs) on a profile under <root>, its sources
# added by `live-sources.sh add --scale`, its own scheduler doing the syncs.
#
#   testenv/scale-sync.sh <root> [<source id>...] [--wait S]
#
# <root> is a scratch HOME: the profile lands in
# <root>/Library/Application Support/dev.knobas.desktop/demo (database in db/),
# the app's log in <root>/Library/Logs/dev.knobas.desktop. The PostgreSQL
# binaries are the machine's (~/.theseus, linked in). Run it again with the
# next JVM product up: the sources are updated in place, and the run waits for
# a finished sync of each <source id> (default: every source the product list
# added) that started after the app did, then prints per source how long it
# took and what it wrote, the mirror's rows per source and kind, and the
# database's size. The app is quit at the end and its server stopped, so the
# directory can be copied.
#
# No demo knobas of this profile may be running; Björn's own demo profile is
# never touched (HOME is <root> for every process this starts).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
say() { echo "scale-sync: $*"; }
die() { echo "scale-sync: $*" >&2; exit 1; }

root=${1:?usage: testenv/scale-sync.sh <root> [<source id>...] [--wait S]}
shift
wait_s=7200 ids=()
while [ $# -gt 0 ]; do
  case $1 in
    --wait) wait_s=${2:?}; shift 2 ;;
    *) ids+=("$1"); shift ;;
  esac
done
mkdir -p "$root"
root=$(cd "$root" && pwd)
[ -e "$root/.theseus" ] || ln -s "$HOME/.theseus" "$root/.theseus"
data="$root/Library/Application Support/dev.knobas.desktop"
db="$data/demo/db"
pg=$HOME/.theseus/postgresql/18.6.0/bin
[ -x "$pg/psql" ] || die "no $pg/psql: run the app once so postgresql_embedded installs it"

say "building target/debug/knobas-app"
env -C "$repo" env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app --bin knobas-app --quiet
bin=$repo/target/debug/knobas-app

# The server a killed app left would be adopted; stop it so the sources are
# applied by live-sources' own start.
stop_db() { [ ! -e "$db/data/postmaster.pid" ] || "$pg/pg_ctl" -D "$db/data" stop -m fast -w >/dev/null 2>&1 || true; }
stop_db

"$here/live-sources.sh" add --data-dir "$data" --scale ||
  say "live-sources reported a product it skipped or refused (above); going on with what was added"

log=$root/app.log
# Before the app starts: its first runs begin with it.
t0=$(date -u +%Y-%m-%dT%H:%M:%SZ)
say "starting the app on $data/demo (log $log)"
HOME=$root "$bin" --demo >>"$log" 2>&1 &
app=$!
trap 'kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; stop_db' EXIT

psql_() {
  local port
  port=$(sed -n 4p "$db/data/postmaster.pid" 2>/dev/null) || return 1
  [ -n "$port" ] || return 1
  PGPASSWORD=$(cat "$db/.pgpass") "$pg/psql" -h 127.0.0.1 -p "$port" -U postgres -d knobas -X -q -t -A -F ' ' "$@"
}
until psql_ -c 'select 1' >/dev/null 2>&1; do
  kill -0 "$app" 2>/dev/null || die "the app exited before its database answered; $log"
  sleep 2
done
if [ ${#ids[@]} -eq 0 ]; then
  read -r -a ids <<<"$(psql_ -c "select string_agg(id, ' ' order by id) from knobas.source_config where enabled")"
fi
say "waiting for a finished sync of: ${ids[*]} (up to ${wait_s}s)"
deadline=$((SECONDS + wait_s))
for id in "${ids[@]}"; do
  while :; do
    row=$(psql_ -c "select outcome, round(extract(epoch from finished_at - started_at)), upserted, coalesce(left(error, 300), '')
                      from knobas.sync_run where source_id = '$id' and started_at >= '$t0' and finished_at is not null
                     order by started_at limit 1")
    if [ -n "$row" ]; then say "$id: $row (outcome, seconds, rows upserted, error)"; break; fi
    if [ $SECONDS -gt $deadline ]; then say "$id: no finished sync after ${wait_s}s"; break; fi
    kill -0 "$app" 2>/dev/null || die "the app exited mid-sync; $log"
    sleep 10
  done
done

say "every run since the app started:"
psql_ -c "select source_id, trigger, outcome, started_at, round(extract(epoch from coalesce(finished_at, now()) - started_at)) s, upserted, swept
            from knobas.sync_run where started_at >= '$t0' order by started_at"
say "the mirror, rows per source and kind:"
psql_ -c "select source_id, kind, count(*) from sync.item group by 1, 2 order by 1, 2"
say "database: $(psql_ -c "select pg_size_pretty(pg_database_size('knobas'))") in knobas; $(du -sh "$db/data" | cut -f1) on disk"
