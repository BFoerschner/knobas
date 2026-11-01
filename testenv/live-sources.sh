#!/usr/bin/env bash
# Every live product as a source on the demo profile (#2371): Gitea, Forgejo,
# Jira, Confluence, TeamCity and Uptime Kuma, through the app's own
# `knobas-app live-sources` (crud add/update + set_secret), so each credential
# lands in the demo profile's secret store, where the app reads it.
#
#   testenv/live-sources.sh add    [<knobas.app>] [--data-dir <dir>]
#   testenv/live-sources.sh remove [<knobas.app>] [--data-dir <dir>]
#
# Addresses and credentials come from `./seed --env` (and its --env-forgejo and
# --env-kuma halves), which the live target (testenv/live.env: local or the WSL
# box through the tunnel) seeded. A product with no seed or no answer at its
# address is skipped with one line; the rest are still added. `add` again
# updates each source in place: one source per product, the current token.
# `remove` deletes the six with what they synced and their credentials.
#
# Gitea and Forgejo are registered at the https front (#2377: Gitea
# https://127.0.0.1:3443, Forgejo :3445), not at the http address the seed
# names: knobas clones and fetches over https or ssh only (ADR-0023,
# ADR-0035). A front whose certificate this Mac does not trust yet is added
# all the same, with the one line that says to run `just live-trust-ca`.
#
# A bundle given runs the bundle's own binary; without it target/debug/knobas-app is
# built and run, which is the one `just demo` starts. A debug build keeps the
# demo profile's credentials in the profile's dev-secrets.json, not the
# keychain (#2384), so every debug binary and bundle reads them without a
# prompt. No demo knobas may be running: the demo profile's database is one
# process's at a time. --data-dir puts the profile, and its credential file,
# under a scratch directory instead (the recipe's own tests).

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)

say() { echo "live-sources: $*"; }
die() { echo "live-sources: $*" >&2; exit 1; }

action=${1:-}
[ "$action" = add ] || [ "$action" = remove ] \
    || die "usage: testenv/live-sources.sh add|remove [<knobas.app>] [--data-dir <dir>]"
shift
app='' data_dir=()
while [ $# -gt 0 ]; do
    case $1 in
        --data-dir) data_dir=(--data-dir "${2:?--data-dir needs a directory}"); shift 2 ;;
        '') shift ;; # the recipes' empty default
        -*) die "unknown argument '$1'" ;;
        *) app=$1; shift ;;
    esac
done

if [ -n "$app" ]; then
    bin=$app/Contents/MacOS/knobas-app
    [ -x "$bin" ] || die "no knobas-app binary in $app"
else
    say "building target/debug/knobas-app"
    env -C "$repo" env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app --bin knobas-app --quiet
    bin=$repo/target/debug/knobas-app
fi

readonly IDS=(live-gitea live-forgejo live-jira live-confluence live-teamcity live-kuma)

if [ "$action" = remove ]; then
    exec "$bin" live-sources remove --demo "${data_dir[@]}" "${IDS[@]}"
fi

# One product's variables, in a subshell so one product's names never leak
# into the next. Prints nothing when the seed has none. A seed that refuses
# says why on stderr: a state file never written is "not seeded", but a
# recorded Gitea token Gitea now refuses (`./seed --env` probes it first and
# prints nothing, Jira, Confluence and TeamCity included) is a stale
# credential, and the lines that follow must not read as products never up.
seed_env() {
    local out err status=0
    err=$(mktemp)
    out=$( (cd "$here" && ./seed "$1") 2>"$err") || status=$?
    if [ "$status" -ne 0 ] && ! grep -q 'not found' "$err"; then
        # stderr and a flag file: this runs inside the caller's `$(...)`.
        say "./seed $1 refused, so its products are skipped below: $(tr '\n' ' ' <"$err")" >&2
        say "  mint fresh credentials with just live-up <product>, then run this again" >&2
        : >"$refused"
    fi
    rm -f "$err"
    printf '%s\n' "$out"
}
refused=$(mktemp)
rm -f "$refused"

# Whether anything answers HTTP at the address: any status at all.
# -k: an https front this Mac does not trust yet still answers (forge() says so).
answers() { [ "$(curl -s -k -o /dev/null -m 5 -w '%{http_code}' "$1" || true)" != 000 ]; }

sources=()
# add <product> <url> <json>: the source if its product answers, else a line.
add() {
    if [ -z "$2" ]; then
        say "$1: skipped, not seeded (just live-up $1)"
    elif ! answers "$2"; then
        say "$1: skipped, nothing answers at $2 (just live-status)"
    else
        sources+=("$3")
    fi
}

source_json() { # id kind name url auth config secret
    jq -cn --arg id "$1" --arg kind "$2" --arg name "$3" --arg url "$4" --arg auth "$5" \
        --argjson config "$6" --argjson secret "$7" \
        '{id: $id, adapter_kind: $kind, display_name: $name, base_url: $url, auth_kind: $auth,
          config: $config, secret: $secret, sync_interval_secs: 300, enabled: true}'
}

# https_front <http address> -- the https front's address for a forge's http
# one (docker-compose.yml, `https-front`), or nothing when it has none.
https_front() {
    case $1 in
        http://127.0.0.1:3000 | http://127.0.0.1:3000/) echo https://127.0.0.1:3443 ;;
        http://127.0.0.1:3002 | http://127.0.0.1:3002/) echo https://127.0.0.1:3445 ;;
    esac
}

forge() { # product id name seed-flag
    # shellcheck disable=SC2034 # set by the seed's eval
    local KNOBAS_GITEA_URL='' KNOBAS_GITEA_TOKEN='' KNOBAS_GITEA_OWNER='' KNOBAS_GITEA_REPO='' KNOBAS_GITEA_SEED_STATE=''
    local url
    eval "$(seed_env "$4")"
    url=$KNOBAS_GITEA_URL
    if [ -n "$url" ]; then
        url=$(https_front "$url")
        [ -n "$url" ] || die "$1: no https front for $KNOBAS_GITEA_URL (testenv/README.md, The https front)"
        # /usr/bin/curl reads the keychain's trust settings, as the app does.
        if [ "$(curl -s -k -o /dev/null -m 5 -w '%{http_code}' "$url/api/healthz" || true)" = 200 ] &&
            ! /usr/bin/curl -fsS -o /dev/null -m 5 "$url/api/healthz" 2>/dev/null; then
            say "$1: this Mac does not trust $url's certificate yet: run 'just live-trust-ca' once"
        fi
    fi
    add "$1" "$url" "$(source_json "$2" gitea "$3" "$url" Pat \
        "$(jq -cn --arg o "$KNOBAS_GITEA_OWNER" '{owners: [$o]}')" \
        "$(jq -cn --arg v "$KNOBAS_GITEA_TOKEN" '{value: $v}')")"
}
forge gitea live-gitea 'Gitea (live)' --env
forge forgejo live-forgejo 'Forgejo (live)' --env-forgejo

the_rest() {
    # shellcheck disable=SC2034 # set by the seed's eval
    local KNOBAS_JIRA_URL='' KNOBAS_JIRA_USER='' KNOBAS_JIRA_PASSWORD='' \
        KNOBAS_CONFLUENCE_URL='' KNOBAS_CONFLUENCE_USER='' KNOBAS_CONFLUENCE_PASSWORD='' \
        KNOBAS_TEAMCITY_URL='' KNOBAS_TEAMCITY_TOKEN='' KNOBAS_GITEA_URL='' KNOBAS_GITEA_TOKEN='' \
        KNOBAS_GITEA_OWNER='' KNOBAS_GITEA_REPO='' KNOBAS_KUMA_URL='' KNOBAS_KUMA_API_KEY='' \
        KNOBAS_KUMA_USER='' KNOBAS_KUMA_PASSWORD=''
    eval "$(seed_env --env)"
    local state=$here/seed-state.json projects spaces
    projects=$(jq -c '[.jira.projects[]?.key]' "$state" 2>/dev/null || echo '[]')
    spaces=$(jq -c '[.confluence.space // empty]' "$state" 2>/dev/null || echo '[]')
    add jira "$KNOBAS_JIRA_URL" "$(source_json live-jira jira 'Jira (live)' "$KNOBAS_JIRA_URL" UserPassword \
        "$(jq -cn --arg u "$KNOBAS_JIRA_USER" --argjson p "$projects" \
            '{flavor: "datacenter", projects: $p, username: $u}')" \
        "$(jq -cn --arg v "$KNOBAS_JIRA_PASSWORD" '{value: $v}')")"
    add confluence "$KNOBAS_CONFLUENCE_URL" "$(source_json live-confluence confluence 'Confluence (live)' \
        "$KNOBAS_CONFLUENCE_URL" UserPassword \
        "$(jq -cn --arg u "$KNOBAS_CONFLUENCE_USER" --argjson s "$spaces" \
            '{flavor: "datacenter", spaces: $s, username: $u}')" \
        "$(jq -cn --arg v "$KNOBAS_CONFLUENCE_PASSWORD" '{value: $v}')")"
    add teamcity "$KNOBAS_TEAMCITY_URL" "$(source_json live-teamcity teamcity 'TeamCity (live)' \
        "$KNOBAS_TEAMCITY_URL" Pat '{}' "$(jq -cn --arg v "$KNOBAS_TEAMCITY_TOKEN" '{value: $v}')")"
}
the_rest

KNOBAS_KUMA_URL='' KNOBAS_KUMA_API_KEY='' KNOBAS_KUMA_USER='' KNOBAS_KUMA_PASSWORD=''
eval "$(seed_env --env-kuma)"
add kuma "$KNOBAS_KUMA_URL" "$(source_json live-kuma kuma 'Uptime Kuma (live)' "$KNOBAS_KUMA_URL" ApiToken '{}' \
    "$(jq -cn --arg v "$KNOBAS_KUMA_API_KEY" --arg u "$KNOBAS_KUMA_USER" --arg p "$KNOBAS_KUMA_PASSWORD" \
        '{value: $v, account: {username: $u, password: $p}}')")"

[ ${#sources[@]} -gt 0 ] || { rm -f "$refused"; die "no live product to add (lines above say why)"; }
# On stdin, never on the command line: a token in argv is in every `ps`.
printf '%s\n' "${sources[@]}" | jq -s . | "$bin" live-sources apply --demo "${data_dir[@]}"
[ ! -e "$refused" ] || { rm -f "$refused"; die "a seed refused (above); those products were not added"; }
