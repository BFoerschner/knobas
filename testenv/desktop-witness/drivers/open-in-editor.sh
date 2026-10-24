#!/usr/bin/env bash
# The second desktop-witness driver (issue #501, ported to the ticket-first
# shell by #1084): an application the reader added in Settings, picked from
# repository's *Open in…* menu, runs with the checkout path.
#
# The repository's page and not the job's Code section, measured on
# 2026-10-18: the demo profile's PAY-231 reaches its branch only through a
# *proposed* link, so the job's Code section reads *Nothing connected* until a
# reader accepts it, and accepting it would be this driver arranging another
# feature's fixture. Both places draw the one `TfOpenIn` (`openin/TfOpenIn.svelte`).
#
# Why this feature is witnessed here and not in a suite. Spawning a process is
# the one thing in knobas that leaves the application: `checkout_ipc.rs` proves
# that `open_checkout` hands a stub the right argument, against a database and
# a `Command`, with no window, no settings pane and no menu anywhere in it.
# What is left over is the whole path a person takes -- set a clones root and
# add an application in Settings › Checkouts and Open in…, open the
# repository, pick the application from its *Open in…* -- and the only witness for
# that is the signed bundle on a real Mac (ADR-0016: "the spawned editor or
# terminal observed as a process with the expected path"; ADR-0023's clones
# root and the reader's own applications).
#
# What it asserts:
#
#   1. a clones root and an application whose command is a stub, typed into
#      the real Settings and stored by the real backend -- the root and the
#      command read back off the section once its dialogs have closed;
#   2. the repository payout-service, opened from ⌘K, with its *Cloned at*
#      line naming the clone **on screen after the launcher's Return and not
#      before it** (#547);
#   3. that picking the application from *Open in…* starts the stub with the
#      checkout path as its **only** argument.
#
# (3) is the ADR's rule end to end: what the program receives is what the disk
# answered, and not a field of the mirrored repo.
#
# What it leaves behind: the demo profile keeps the application it added. No
# control removes one without pressing one of several *Edit…* buttons by
# position, so each run adds its own, named for its scratch directory, and the
# menu step picks the newest by where the menu puts it (the last application).
#
# Run by testenv/desktop-witness.sh, which passes it a compiled helper, the pid
# of the one running instance and the bundle it launched.

set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}

# The accessible names this driver presses and fills. Each is pinned against
# the file that carries it by `just witness-unit`: nothing in the app knows
# this driver exists, so an edit to one would otherwise fail a *correct* app
# minutes into a run, with no clue in the failure. The new shell uppercases
# none of them, so each is spelled as the markup spells it.
readonly SETTINGS_BUTTON='Settings'                    # TfTopBar.svelte
readonly CHECKOUTS_SECTION='Checkouts and Open in…'    # settings/settings.ts
readonly CHANGE_ROOT='Change…'                         # TfCheckoutsSection.svelte
readonly CLONES_ROOT_FIELD='Clones root'
readonly SAVE_ROOT='Save'
readonly EDIT_APPS='Edit applications…'
readonly ADD_APP='Add an application…'                 # openin/TfAppsManage.svelte
readonly DONE='Done'
readonly NAME_FIELD='Name'                             # openin/TfAppDialog.svelte
readonly COMMAND_FIELD='Command'
readonly ADD_BUTTON='Add application'
readonly OPEN_IN='Open in…'                            # openin/TfOpenIn.svelte
readonly QUERY_BOX='Search or act'                     # launcher/QueryBox.svelte

# The repository the demo profile is asked for; ⌘K groups a repository above
# its branches (`knobas_search`'s `GROUP_ORDER`).
readonly REPO_NAME='payout-service'
# `https://tidewater.example` is `knobas_source_mock`'s `MOCK_BASE`.
readonly REPO_REMOTE='https://tidewater.example/tidewater/payout-service'

readonly KEY_A=0
readonly KEY_K=40
readonly KEY_RETURN=36
readonly KEY_UP=126
# The guard on every wait below: KNOBAS_WITNESS_WAIT_S (`witness_guard`, #1961).
SETTLE_SECONDS=$(witness_guard) || exit 2
readonly SETTLE_SECONDS

say() { printf 'open-in-editor: %s\n' "$*"; }
die() {
    printf 'open-in-editor: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'open-in-editor:   %s\n' "$line" >&2; done
    # `|| true` on all three: `ax focused` exits 1 exactly when nothing has
    # focus, and under `pipefail` that would kill this before the dumps.
    printf 'open-in-editor: the focused element at that moment:\n' >&2
    { "$ax" focused "$pid" 2>&1 || true; } | sed 's/^/open-in-editor:   /' >&2
    printf 'open-in-editor: every string on knobas'"'"' windows:\n' >&2
    { "$ax" values "$pid" 2>&1 || true; } | sed 's/^/open-in-editor:   /' >&2
    printf 'open-in-editor: the window as the accessibility tree sees it:\n' >&2
    { "$ax" dump "$pid" 12 2>&1 || true; } | sed 's/^/open-in-editor:   /' >&2
    exit 1
}

# wait_until <description> <predicate...> -- poll for a settled screen.
wait_until() {
    local what=$1
    shift
    local polled=0
    witness_poll "$pid" "$SETTLE_SECONDS" "$@" || polled=$?
    case $polled in
    0) return 0 ;;
    3) die "$what: knobas (pid $pid) exited while this waited" ;;
    *) die "$what (waited ${SETTLE_SECONDS} s, KNOBAS_WITNESS_WAIT_S)" ;;
    esac
}

# How many elements carry a name, as a number or `x` -- never "whatever `ax`
# printed", because an empty answer compared with `!= 0` is *true*.
count_of() {
    local answer
    answer=$("$ax" find "$pid" "$1" 2>/dev/null || true)
    case $answer in
    '' | *[!0-9]*) printf 'x\n' ;;
    *) printf '%s\n' "$answer" ;;
    esac
}
present() { local n; n=$(count_of "$1"); [ "$n" != x ] && [ "$n" -gt 0 ]; }
absent() { [ "$(count_of "$1")" = 0 ]; }
exactly_one() { [ "$(count_of "$1")" = 1 ]; }

# What is written on knobas' windows, one string per line: a *value*, which a
# driver reads, as against a *name*, which it presses (#547).
screen() { "$ax" values "$pid" 2>/dev/null || true; }
reads() { has_reading "$(screen)" "$1"; }

# press_when <label> -- wait for exactly one element called <label>, press it.
press_when() {
    wait_until "no single '$1' (count: $(count_of "$1"))" exactly_one "$1"
    "$ax" press "$pid" "$1" || die "could not press '$1'"
}

# fill <label> <text> -- put <text> in the field called <label>, replacing
# whatever is in it (⌘A first: a field may arrive with a value).
fill() {
    wait_until "no field called '$1' appeared" exactly_one "$1"
    "$ax" focus "$pid" "$1" || die "could not put the keyboard in '$1'"
    "$ax" key "$KEY_A" command
    "$ax" type "$2"
}

launcher_answered() { screen | grep -q " match"; }

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-in-editor.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

record=$scratch/record
stub=$scratch/stub
stub_script "$record" >"$stub"
chmod +x "$stub"

# This run's application: one word after *Witness*, from the scratch name, so
# a previous run's entry never answers for this one.
readonly APP_NAME="Witness ${scratch##*.}"
readonly TEMPLATE="$stub {path}"

# The clone the scan has to find: a directory one level under the root whose
# `.git/config` names the demo repository as its `origin`. No `git init`.
clones=$scratch/clones
checkout=$clones/$REPO_NAME
mkdir -p "$checkout/.git"
git_config "$REPO_REMOTE" >"$checkout/.git/config"
say "a clone at $checkout, and a stub at $stub"

# A template needs quotes around a path with a space, and a quote typed into a
# WebKit field is rewritten by macOS' Smart Quotes (#547): refuse instead.
path_is_one_word "$stub" || die \
    "the scratch path this driver has to type into a command is not one word:" \
    "  $stub" \
    "Quotes typed into a WebKit field are rewritten by Smart Quotes, so this" \
    "refuses rather than storing a command knobas cannot run."

say "bringing pid $pid to the front"
"$ax" activate "$pid"

# --- 1. Settings › Checkouts and Open in… -------------------------------------

say "opening Settings › $CHECKOUTS_SECTION"
# Waited for: the shell finishes booting after its window appears.
press_when "$SETTINGS_BUTTON"
press_when "$CHECKOUTS_SECTION"

press_when "$CHANGE_ROOT"
fill "$CLONES_ROOT_FIELD" "$clones"
press_when "$SAVE_ROOT"
# The row draws what `checkout_settings` answers once the field has closed.
root_stored() { absent "$CLONES_ROOT_FIELD" && reads "$clones"; }
wait_until "the clones root was not stored: the row does not read $clones" root_stored
say "clones root set to $clones"

press_when "$EDIT_APPS"
press_when "$ADD_APP"
fill "$NAME_FIELD" "$APP_NAME"
fill "$COMMAND_FIELD" "$TEMPLATE"
press_when "$ADD_BUTTON"
# Adding returns to the list; *Done* closes it. The section's table then draws
# the stored list, so the command on screen with no dialog up is the backend's.
press_when "$DONE"
app_stored() { absent "$DONE" && absent "$COMMAND_FIELD" && reads "$APP_NAME" && reads "$TEMPLATE"; }
wait_until "the application was not stored: the table has no '$APP_NAME' running the stub" app_stored
say "'$APP_NAME' is stored, and runs the stub"

# --- 2. the repository -------------------------------------------------------

say "opening the launcher and asking for '$REPO_NAME'"
"$ax" key "$KEY_K" command
wait_until "⌘K did not open the launcher" present "$QUERY_BOX"
"$ax" type "$REPO_NAME"
wait_until "the launcher never answered '$REPO_NAME'" launcher_answered

# The page's *Cloned at* line carries the clone's path as a value of its own.
# Read before Return and after (#547): Settings, behind the launcher, reads the
# *root* and not the clone, so the before-read is `no` on a correct app, and
# one already there witnesses nothing.
before=$(reading_verdict "$(screen)" "$checkout")
say "the launcher has answered; '$checkout' on screen: $before"
"$ax" key "$KEY_RETURN"
go=Return
# Polled under the guard; the predicate leaves what it last read in `after`.
after_names_the_clone() {
    after=$(reading_verdict "$(screen)" "$checkout")
    [ "$after" = yes ]
}
polled=0
witness_poll "$pid" "$SETTLE_SECONDS" after_names_the_clone || polled=$?
[ "$polled" != 3 ] || die "knobas (pid $pid) exited after '$go'"
say "after '$go'; '$checkout' on screen: $after"

case $(waypoint_verdict "$before" "$after") in
witnessed) ;;
too-early)
    die "'$checkout' was on screen before '$go', so waiting for it witnesses" \
        "nothing about the page. Whatever drew it over the launcher is where to look."
    ;;
never-appeared)
    die "no *Cloned at* line naming the clone on the page for '$REPO_NAME'." \
        "demo.rs's the_demo_corpus_answers_the_checkout_the_desktop_driver_opens" \
        "covers the read under the window, so check that ⌘K's first hit is the" \
        "repository and that its page found the clone."
    ;;
*)
    die "the screen could not be read either side of '$go'" \
        "(before: $before, after: $after): no window, or a locked screen."
    ;;
esac
say "the repository is open, and it is cloned at the clone"

# --- 3. the spawn ------------------------------------------------------------

press_when "$OPEN_IN"

# The menu opens with the default focused, and the arrows wrap. Its buttons end
# *…the applications, Add an application…, Manage applications…*, so three
# steps up from the default is the last application -- the one this run added
# -- however many came before it. Asserted on the focus before Return, by name.
focus_is_a_menu_item() { [ "$(tsv_field "$("$ax" focused "$pid" 2>/dev/null || true)" role)" = AXMenuItem ]; }
wait_until "'$OPEN_IN' opened no menu with a focused item" focus_is_a_menu_item
for _ in 1 2 3; do
    "$ax" key "$KEY_UP"
    sleep 0.2
done
focused=$("$ax" focused "$pid" 2>/dev/null || true)
case "$(tsv_field "$focused" title)|$(tsv_field "$focused" description)" in
"$APP_NAME"* | *"|$APP_NAME"*) ;;
*) die "the menu item three up from the default is not '$APP_NAME'." ;;
esac
say "picking '$APP_NAME' from the menu"
"$ax" key "$KEY_RETURN"

# `open_checkout` answers when the program has *started*; what is polled for is
# the stub's own record, finished (`record_is_complete`).
if ! wait_for_record "$record" "$SETTLE_SECONDS"; then
    printf 'open-in-editor: what is at %s now:\n' "$record" >&2
    { cat "$record" 2>/dev/null || printf '(nothing at that path)\n'; } |
        sed 's/^/open-in-editor:   /' >&2
    die "the stub never finished writing a record after ${SETTLE_SECONDS} s (KNOBAS_WITNESS_WAIT_S)." \
        "A finished record ends in a newline -- the condition recorded()" \
        "waits for in crates/knobas-app/tests/it/checkout_ipc.rs."
fi

recorded=$(cat "$record")
only=$(sole_argument "$recorded")
if [ "$only" != "$checkout" ]; then
    printf 'open-in-editor: the stub recorded:\n' >&2
    printf '%s\n' "$recorded" | sed 's/^/open-in-editor:   /' >&2
    die "the command was not given the checkout path, and only that." \
        "expected exactly one argument: $checkout"
fi

say "the stub ran with one argument, and it is the checkout: $only"
say "ok"
