#!/usr/bin/env bash
# The desktop-witness driver for a pull request's *Open in…* (#2316): the
# three OS-level targets spec #2234 (*Testing Decisions*) gives the desktop
# witness, on one pull request of the testenv Gitea.
#
#   1. an application opens the pull request's **worktree** at its head
#      (#2285, ADR-0035): knobas fetches `refs/pull/<n>/head` over https into
#      `<clones root>/pull-requests/<owner>/<repo>-<n>` and the stub is run
#      with that folder as its only argument; the person's checkout keeps its
#      branch, its HEAD and its status;
#   2. *Diff tool* opens the pull request's range in the tool git is
#      configured with (#2275): the scratch clone names a stub as its
#      `diff.guitool`, and the stub sees the files the pull request changed;
#   3. *Browser* hands the pull request's page to the default browser (#2274):
#      the menu item reads the page built from the source's typed address,
#      and the default browser comes to the front after the press.
#
# Why the testenv Gitea and not the demo's mock (ruling on #2316). The mock
# draws no source view, its repositories carry no owner (`payout-service`, so
# ADR-0023's `owner/repo` refuses to build a URL from it) and its shas are
# short; the Gitea adapter has all three for real. The one thing the testenv
# Gitea lacks is https, which the worktree's fetch requires (ADR-0035's
# protocol flags): the testenv's https front (#2377, docker-compose.yml
# `https-front`) serves it at https://127.0.0.1:3443 on either target (the
# WSL box's through the tunnel), with a certificate from its own CA, which
# `just live-trust-ca` trusts for SSL in the login keychain. knobas' client
# (rustls with the platform verifier) and git both accept it, so the driver
# adds the testenv Gitea at that address -- through the real Settings, as a
# person would -- and removes it at the end. KNOBAS_WITNESS_GITEA names
# another https front.
#
# What it leaves behind: the application it added (as open-in-editor does, for
# the same reason), the clones root it set, and Settings › Checkouts' record
# of a worktree whose folder this run deleted. The source it added is removed
# with what it synced; a run that fails before that names the source to remove.
#
# Needs the testenv Gitea up and seeded from this tree (`just live-up gitea`),
# whose `./seed --env` gives the token. Run by testenv/desktop-witness.sh,
# which passes it a compiled helper, the pid of the one running instance and
# the bundle it launched.

set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}
app=${KNOBAS_WITNESS_APP:?the harness passes this}

# The accessible names this driver presses and fills, each pinned against the
# file that carries it by `just witness-unit`.
readonly SETTINGS_BUTTON='Settings'                    # TfTopBar.svelte
readonly SOURCES_SECTION='Sources'                     # settings/settings.ts
readonly ADD_A_SOURCE='+ Add a source'                 # settings/TfSources.svelte
readonly ADAPTER_GITEA='Gitea'                         # knobas-source-gitea's descriptor name
readonly ADDRESS_FIELD='Address'                       # settings/TfAddSource.svelte
readonly TOKEN_FIELD='Token'
readonly SOURCE_NAME_FIELD='Name'
readonly SOURCE_ID_FIELD='Id'
readonly TEST_CONNECTION='Test connection'
readonly ADD_SOURCE='Add source'
readonly REMOVE_SOURCE='Remove…'                       # settings/TfSourcePage.svelte
readonly DELETE_SYNCED='Delete what it synced'
readonly REMOVE_CONFIRM='Remove'
readonly CHECKOUTS_SECTION='Checkouts and Open in…'    # settings/settings.ts
readonly CHANGE_ROOT='Change…'                         # TfCheckoutsSection.svelte
readonly CLONES_ROOT_FIELD='Clones root'
readonly SAVE_ROOT='Save'
readonly EDIT_APPS='Edit applications…'
readonly ADD_APP='Add an application…'                 # openin/TfAppsManage.svelte
readonly DONE='Done'
readonly NAME_FIELD='Name'                             # openin/TfAppDialog.svelte
readonly COMMAND_FIELD='Command'
readonly PR_COMMAND_FIELD='For a pull request'
readonly ADD_BUTTON='Add application'
readonly OPEN_IN='Open in…'                            # openin/TfOpenObject.svelte
readonly DIFF_TOOL='Diff tool'
readonly BROWSER='Browser'
readonly QUERY_BOX='Search or act'                     # launcher/QueryBox.svelte

# The seeded pull request (`seed-gitea.sh`): its title is what ⌘K is asked for.
readonly PR_TITLE='SEPA retry with exponential backoff'

readonly KEY_A=0
readonly KEY_K=40
readonly KEY_RETURN=36
readonly KEY_ESCAPE=53
readonly KEY_UP=126
SETTLE_SECONDS=$(witness_guard) || exit 2
readonly SETTLE_SECONDS
# A first sync of the Gitea source: every repository, its pull requests and
# commits. Three guards, as a slow machine draws a screen slowly too.
readonly SYNC_SECONDS=$((SETTLE_SECONDS * 3))

say() { printf 'pull-request-open-in: %s\n' "$*"; }
# The source this run added, once it has; named by `die` so a failed run says
# what to remove from the demo profile.
added_source=
die() {
    printf 'pull-request-open-in: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'pull-request-open-in:   %s\n' "$line" >&2; done
    [ -z "$added_source" ] || printf 'pull-request-open-in: the demo profile keeps the source %s: remove it in Settings › Sources.\n' \
        "$added_source" >&2
    printf 'pull-request-open-in: the focused element at that moment:\n' >&2
    { "$ax" focused "$pid" 2>&1 || true; } | sed 's/^/pull-request-open-in:   /' >&2
    printf 'pull-request-open-in: every string on knobas'"'"' windows:\n' >&2
    { "$ax" values "$pid" 2>&1 || true; } | sed 's/^/pull-request-open-in:   /' >&2
    printf 'pull-request-open-in: the window as the accessibility tree sees it:\n' >&2
    { "$ax" dump "$pid" 12 2>&1 || true; } | sed 's/^/pull-request-open-in:   /' >&2
    exit 1
}

# wait_until <description> <predicate...> -- poll for a settled screen.
wait_until() {
    local what=$1
    shift
    local polled=0
    seen=
    witness_poll "$pid" "$SETTLE_SECONDS" "$@" || polled=$?
    case $polled in
    0) return 0 ;;
    3) die "$what: knobas (pid $pid) exited while this waited${seen:+; last seen: $seen}" ;;
    *) die "$what (waited ${SETTLE_SECONDS} s, KNOBAS_WITNESS_WAIT_S)${seen:+; last seen: $seen}" ;;
    esac
}

count_of() {
    local answer
    answer=$("$ax" find "$pid" "$1" 2>/dev/null || true)
    case $answer in
    '' | *[!0-9]*) printf 'x\n' ;;
    *) printf '%s\n' "$answer" ;;
    esac
}
absent() { [ "$(count_of "$1")" = 0 ]; }
exactly_one() {
    local n
    n=$(count_of "$1")
    seen="'$1' counted $n"
    [ "$n" = 1 ]
}
screen() { "$ax" values "$pid" 2>/dev/null || true; }
reads() { has_reading "$(screen)" "$1"; }

press_when() {
    wait_until "no single '$1'" exactly_one "$1"
    "$ax" press "$pid" "$1" || die "could not press '$1'"
}
fill() {
    wait_until "no field called '$1' appeared" exactly_one "$1"
    "$ax" focus "$pid" "$1" || die "could not put the keyboard in '$1'"
    "$ax" key "$KEY_A" command
    "$ax" type "$2"
}
# click_words <words> -- a left click on the words on screen, for a radio
# whose accessible name is its label and its hint run together.
click_words() {
    visible_words() { "$ax" click-at "$pid" "$1" >/dev/null 2>&1 && return 0; seen="no single '$1' to click"; return 1; }
    wait_until "could not click '$1'" visible_words "$1"
}

# to_sources -- Settings opens on the section it last showed, Sources the
# first time: there 'Sources' is the crumb, the nav item and the heading at
# once (3 names, #2316's first run), and the section needs no press. From any
# other section the nav item is the only one.
to_sources() {
    press_when "$SETTINGS_BUTTON"
    on_some_section() { [ "$(count_of "$SOURCES_SECTION")" != x ] && [ "$(count_of "$SOURCES_SECTION")" -gt 0 ]; }
    wait_until "Settings drew no '$SOURCES_SECTION'" on_some_section
    absent "$ADD_A_SOURCE" || return 0
    press_when "$SOURCES_SECTION"
}

# The focused menu item's title and description, `|`-separated.
focused_item() {
    local focused
    focused=$("$ax" focused "$pid" 2>/dev/null || true)
    [ "$(tsv_field "$focused" role)" = AXMenuItem ] || return 1
    printf '%s|%s\n' "$(tsv_field "$focused" title)" "$(tsv_field "$focused" description)"
}
# The pull request's page draws two *Open in…*: the header's, for the pull
# request (`TfOpenObject`: its worktree, the Diff tool, its page), and the
# Local checkout card's, for the repository's checkout (`TfOpenIn`). The
# header's comes first in the page and draws last, once the source view gave
# both branches: #2316's first run pressed the only one drawn yet, the card's,
# and its application ran at the checkout.
both_open_ins() {
    local n
    n=$(count_of "$OPEN_IN")
    seen="'$OPEN_IN' counted $n"
    [ "$n" = 2 ]
}
# open_menu_at <item> -- open the header's *Open in…* and walk up to the item
# whose name starts with <item>: the menu wraps, so this reaches every item,
# and the focus is asserted by name before anything is pressed.
open_menu_at() {
    wait_until "the pull request's page does not draw both '$OPEN_IN'" both_open_ins
    "$ax" press-visible "$pid" "$OPEN_IN" 1 || die "could not press the header's '$OPEN_IN'"
    menu_focused() { focused_item >/dev/null; }
    wait_until "'$OPEN_IN' opened no menu with a focused item" menu_focused
    local tries item
    for tries in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
        item=$(focused_item || true)
        case "$item" in
        "$1"* | *"|$1"*) return 0 ;;
        esac
        "$ax" key "$KEY_UP"
        sleep 0.2
    done
    die "the '$OPEN_IN' menu has no item starting '$1' (walked $tries items; the last was '$item')."
}

# --- 0. the fixture: the testenv Gitea over https, and a clone of it ---------

readonly GITEA_HTTPS=${KNOBAS_WITNESS_GITEA:-https://127.0.0.1:3443}
case $GITEA_HTTPS in
https://*) ;;
*) die "KNOBAS_WITNESS_GITEA must be an https address, and it is '$GITEA_HTTPS'." ;;
esac
curl -fsS -o /dev/null "$GITEA_HTTPS/api/healthz" 2>/dev/null || die \
    "the testenv Gitea does not answer at $GITEA_HTTPS with a certificate this Mac trusts." \
    "Bring it up with 'just live-up gitea' (which starts the https front), then trust" \
    "the front's CA once with 'just live-trust-ca'; KNOBAS_WITNESS_GITEA names another https front."
seed_env=$(./seed --env 2>/dev/null) || die "./seed --env failed: seed the testenv Gitea from this tree ('just live-up gitea')."
eval "$seed_env"
: "${KNOBAS_GITEA_TOKEN:?./seed --env named no Gitea token}"
readonly OWNER=${KNOBAS_GITEA_OWNER:-tidewater}
readonly REPO=${KNOBAS_GITEA_REPO:-payout-service}
api() { curl -fsS -H "Authorization: token $KNOBAS_GITEA_TOKEN" "$GITEA_HTTPS/api/v1/$1"; }

# The pull request as Gitea has it: what every assertion below compares with.
pr_json=$(api "repos/$OWNER/$REPO/pulls?state=open&limit=50" |
    jq --arg t "$PR_TITLE" 'map(select(.title == $t)) | .[0] // empty') || true
[ -n "$pr_json" ] || die "the testenv Gitea has no open pull request '$PR_TITLE' in $OWNER/$REPO."
PR_NUMBER=$(jq -r .number <<<"$pr_json")
PR_HEAD=$(jq -r .head.sha <<<"$pr_json")
readonly PR_NUMBER PR_HEAD
readonly PR_PAGE="$GITEA_HTTPS/$OWNER/$REPO/pulls/$PR_NUMBER"
readonly CLONE_URL="$GITEA_HTTPS/$OWNER/$REPO.git"
changed=$(api "repos/$OWNER/$REPO/pulls/$PR_NUMBER/files" | jq -r '.[] | select(.status != "removed") | .filename')
[ -n "$changed" ] || die "Gitea lists no files for pull request #$PR_NUMBER."
say "pull request #$PR_NUMBER of $OWNER/$REPO, head $PR_HEAD"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-pr-open-in.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
suffix=$(printf '%s' "${scratch##*.}" | tr '[:upper:]' '[:lower:]')
readonly SOURCE_ID="witness-$suffix"
readonly SOURCE_NAME="Witness Gitea $suffix"
readonly APP_NAME="Witness $suffix"

record=$scratch/record
stub=$scratch/stub
stub_script "$record" >"$stub"
diff_record=$scratch/diff-record
diff_stub=$scratch/diff-stub
# git difftool --dir-diff hands the tool two folders holding the changed files
# and removes them when it returns: the stub lists the right-hand one first.
cat >"$diff_stub" <<STUB
#!/bin/sh
{ printf 'left %s\nright %s\n' "\$1" "\$2"; ( cd "\$2" && find . ! -type d | sed 's|^\./||' | sort ); } > '$diff_record.tmp'
mv '$diff_record.tmp' '$diff_record'
STUB
chmod +x "$stub" "$diff_stub"
path_is_one_word "$stub" || die "the scratch path '$stub' is not one word, and a driver may not type quotes (#547)."

# The person's checkout: a real clone, since a worktree is added to it.
clones=$scratch/clones
checkout=$clones/$REPO
git clone --quiet "$CLONE_URL" "$checkout" || die "git could not clone $CLONE_URL."
git -C "$checkout" config diff.tool witness
git -C "$checkout" config diff.guitool witness
git -C "$checkout" config difftool.prompt false
git -C "$checkout" config difftool.witness.cmd "$diff_stub \"\$LOCAL\" \"\$REMOTE\""
checkout_state() {
    printf 'branch %s\nhead %s\n' "$(git -C "$checkout" symbolic-ref -q HEAD || echo detached)" \
        "$(git -C "$checkout" rev-parse HEAD)"
    git -C "$checkout" status --porcelain
}
before_state=$(checkout_state)
worktree=$(pr_worktree "$clones" "$OWNER" "$REPO" "$PR_NUMBER")
say "a clone at $checkout, a stub at $stub"

say "bringing pid $pid to the front"
"$ax" activate "$pid"
# knobas' window, while it is in front: AeroSpace focuses a window by this id.
window=$("$ax" window-id "$pid" 2>/dev/null | cut -f1) || window=
# to_front -- knobas back in front once the browser has it. `ax activate`
# alone is refused then (#2316's first run: macOS's cooperative activation
# lets the app in front keep it, and AeroSpace had the browser on another
# workspace), so AeroSpace focuses the window and Launch Services opens the
# running bundle, as a click in the Dock would, before `activate` checks.
to_front() {
    if [ -n "$window" ] && command -v aerospace >/dev/null; then
        aerospace focus --window-id "$window" >/dev/null 2>&1 || true
    fi
    KNOBAS_WITNESS_WAIT_S=5 "$ax" activate "$pid" 2>/dev/null && return 0
    open "$app"
    "$ax" activate "$pid" || die "knobas (pid $pid) would not come back to the front after '$BROWSER'."
}

# --- 1. Settings › Sources: the testenv Gitea, over https --------------------

say "adding $GITEA_HTTPS as '$SOURCE_NAME' in Settings › $SOURCES_SECTION"
to_sources
lands_named settings-sources "$ADD_A_SOURCE"
press_when "$ADD_A_SOURCE"
click_words "$ADAPTER_GITEA"
fill "$ADDRESS_FIELD" "$GITEA_HTTPS"
fill "$TOKEN_FIELD" "$KNOBAS_GITEA_TOKEN"
fill "$SOURCE_NAME_FIELD" "$SOURCE_NAME"
fill "$SOURCE_ID_FIELD" "$SOURCE_ID"
press_when "$TEST_CONNECTION"
# *Add source* is enabled by a passing test of exactly what is typed.
add_enabled() { [ "$("$ax" find "$pid" "$ADD_SOURCE" 2>/dev/null)" = 1 ] && "$ax" press "$pid" "$ADD_SOURCE" >/dev/null 2>&1 && absent "$ADDRESS_FIELD"; }
wait_until "the source was not added: '$TEST_CONNECTION' did not pass, or '$ADD_SOURCE' did nothing" add_enabled
added_source=$SOURCE_ID
wait_until "Settings › $SOURCES_SECTION does not list '$SOURCE_NAME'" reads "$SOURCE_NAME"
say "'$SOURCE_NAME' is a source"

# --- 2. Settings › Checkouts: the clones root and the application ------------

press_when "$CHECKOUTS_SECTION"
lands_named settings-checkouts "$CHANGE_ROOT" "$EDIT_APPS"
press_when "$CHANGE_ROOT"
fill "$CLONES_ROOT_FIELD" "$clones"
press_when "$SAVE_ROOT"
root_stored() { absent "$CLONES_ROOT_FIELD" && reads "$clones"; }
wait_until "the clones root was not stored: the row does not read $clones" root_stored
press_when "$EDIT_APPS"
press_when "$ADD_APP"
fill "$NAME_FIELD" "$APP_NAME"
fill "$COMMAND_FIELD" "$stub {path}"
fill "$PR_COMMAND_FIELD" "$stub {path}"
press_when "$ADD_BUTTON"
press_when "$DONE"
app_stored() { absent "$DONE" && absent "$COMMAND_FIELD" && reads "$APP_NAME"; }
wait_until "the application '$APP_NAME' was not stored" app_stored
say "clones root $clones; '$APP_NAME' runs the stub for a pull request"

# --- 3. the pull request, once the source has synced it ----------------------

readonly QUERY="source:$SOURCE_ID kind:pr $PR_TITLE"
hits() { screen | sed -n 's/^\([0-9][0-9]*\) matches.*/\1/p' | head -n 1; }
deadline=$((SECONDS + SYNC_SECONDS))
while :; do
    "$ax" key "$KEY_K" command
    lands_named cmd-k "$QUERY_BOX"
    "$ax" key "$KEY_A" command
    "$ax" type "$QUERY"
    answered() { [ -n "$(hits)" ]; }
    wait_until "the launcher never answered '$QUERY'" answered
    [ "$(hits)" = 0 ] || break
    [ "$SECONDS" -lt "$deadline" ] || die "'$QUERY' still matches nothing after ${SYNC_SECONDS} s: the source never synced the pull request."
    "$ax" key "$KEY_ESCAPE"
    sleep 3
done
absent "$OPEN_IN" || die "an '$OPEN_IN' is on screen before the pull request is opened, so waiting for one witnesses nothing."
"$ax" key "$KEY_RETURN"
# The header's *Open in…* is drawn only once the source view gave the
# pull request both branches (`TfPr.svelte`).
wait_until "the pull request's page drew no header '$OPEN_IN' after Return" both_open_ins
say "pull request #$PR_NUMBER is open, with its '$OPEN_IN'"

# --- 4. the worktree (#2285) -------------------------------------------------

open_menu_at "$APP_NAME"
# The item shows the command it runs, filled with the worktree: the header's menu.
item=$(focused_item)
case $item in
*"$worktree"*) ;;
*) die "the '$APP_NAME' item does not run at the worktree $worktree: '$item'." ;;
esac
"$ax" key "$KEY_RETURN"
wait_for_record "$record" "$SETTLE_SECONDS" || die "the application's stub never finished a record."
only=$(sole_argument "$(cat "$record")")
[ "$only" = "$worktree" ] || die "the application was not given the worktree, and only that." \
    "expected: $worktree" "recorded: $(tr '\n' ' ' <"$record")"
at=$(git -C "$worktree" rev-parse HEAD 2>/dev/null || true)
[ "$at" = "$PR_HEAD" ] || die "the worktree is at '$at', not the pull request's head $PR_HEAD."
fetched_ref "$(cat "$(git -C "$worktree" rev-parse --git-path FETCH_HEAD)" 2>/dev/null || true)" \
    "refs/pull/$PR_NUMBER/head" "$CLONE_URL" ||
    die "the worktree's FETCH_HEAD does not record refs/pull/$PR_NUMBER/head from $CLONE_URL."
after_state=$(checkout_state)
[ "$after_state" = "$before_state" ] || die "the person's checkout changed." \
    "before: $(tr '\n' ' ' <<<"$before_state")" "after:  $(tr '\n' ' ' <<<"$after_state")"
say "the stub ran at $worktree, fetched over https and at $PR_HEAD; the checkout is as it was"

# --- 5. the Diff tool (#2275) ------------------------------------------------

open_menu_at "$DIFF_TOOL"
"$ax" key "$KEY_RETURN"
wait_for_record "$diff_record" "$SETTLE_SECONDS" || die "the diff tool git is configured with never ran."
while IFS= read -r file; do
    has_reading "$(cat "$diff_record")" "$file" || die "the diff tool's right-hand folder lacks $file, which pull request #$PR_NUMBER changes." \
        "recorded: $(tr '\n' ' ' <"$diff_record")"
done <<<"$changed"
say "git's diff tool ran on the pull request's range, with every file it changes"

# --- 6. Browser (#2274) ------------------------------------------------------

browser=$(tsv_field "$("$ax" default-browser)" bundle)
[ -n "$browser" ] || die "Launch Services names no default browser."
open_menu_at "$BROWSER"
item=$(focused_item)
case $item in
*"$PR_PAGE"*) ;;
*) die "the '$BROWSER' item does not name $PR_PAGE: '$item'." ;;
esac
"$ax" key "$KEY_RETURN"
browser_in_front() { [ "$(frontmost_bundle "$("$ax" frontmost 2>/dev/null || true)")" = "$browser" ]; }
witness_poll "$pid" "$SETTLE_SECONDS" browser_in_front || die "the default browser ($browser) did not come to the front after '$BROWSER'."
say "'$BROWSER' handed $PR_PAGE to $browser"
to_front

# --- 7. remove the source ----------------------------------------------------

to_sources
lands_named settings-sources "$ADD_A_SOURCE"
click_words "$SOURCE_NAME"
press_when "$REMOVE_SOURCE"
click_words "$DELETE_SYNCED"
press_when "$REMOVE_CONFIRM"
gone() { absent "$REMOVE_CONFIRM" && ! reads "$SOURCE_NAME"; }
wait_until "'$SOURCE_NAME' is still a source after '$REMOVE_CONFIRM'" gone
added_source=
say "'$SOURCE_NAME' removed, with what it synced"
say "ok"
