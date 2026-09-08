#!/usr/bin/env bash
# The second desktop-witness driver (issue #501): the button on a repo or
# branch detail runs the configured command, and hands it the checkout path.
#
# Why this feature is witnessed here and not in a suite. Spawning a process is
# the one thing in knobas that leaves the application: `checkout_ipc.rs` proves
# that `open_checkout` hands a stub the right argument, and it proves it
# against a database and a `Command`, with no window, no settings pane and no
# button anywhere in it. What is left over is the whole path a person takes --
# type a clones root and a command into Settings, open a repo, press the
# button -- and the only witness for that is the signed bundle on a real Mac
# (ADR-0016: "the spawned editor or terminal observed as a process with the
# expected path").
#
# What it asserts:
#
#   1. a clones root and an *Open in VS Code* template, typed into the real
#      settings fields and stored by the real backend;
#   2. a repo or branch detail that finds the clone this driver put under that
#      root -- either will do, because a branch draws its repository's
#      checkout;
#   3. that pressing the button starts the stub the template names, with the
#      checkout path as its **only** argument.
#
# (3) is the ADR's rule end to end: what the program receives is what the disk
# answered, and not a field of the mirrored repo.
#
# Run by testenv/desktop-witness.sh, which passes it a compiled helper, the pid
# of the one running instance and the bundle it launched.

set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}

# The accessible names this driver acts on. Each is pinned against the file
# that carries it by `just witness-unit`, in the shape #500 pinned the
# launcher's `aria-label`: nothing in the app knows this driver exists, so an
# edit to one of these would otherwise fail a *correct* app, minutes into a
# run, with no clue in the failure.
readonly SETTINGS_BUTTON='Settings'
readonly SAVE_CLONES_ROOT='Save clones root'
readonly SAVE_VSCODE='Save Open in VS Code'
readonly OPEN_BUTTON='Open in VS Code'
# The label the settings row and the *Reset* button are both worded from. The
# button's `aria-label` carries it verbatim; the field's name is the **rendered**
# text and the stylesheet uppercases it, which is why the two are separate
# constants.
readonly VSCODE_LABEL='Open in VS Code'
# **Rendered** names (#503's measurement, 2026-09-08): a `<label class="lab">`
# and a `<span class="lab">` are uppercased by `app/src/app.css`, and WebKit
# names an element by what is rendered. Written as it appears in the markup and
# transformed here, so the source spelling stays readable and one function
# states the rule -- see `rendered_label`. As typed, these two were `Directory`
# and `Checkout`, which no element on screen carries, so this driver could not
# have got past its first `fill`.
CLONES_ROOT_FIELD=$(rendered_label 'Directory')
readonly CLONES_ROOT_FIELD
VSCODE_FIELD=$(rendered_label "$VSCODE_LABEL")
readonly VSCODE_FIELD
CHECKOUT_PANEL=$(rendered_label 'Checkout')
readonly CHECKOUT_PANEL

# The repository the demo profile is asked for, and the remote a clone of it
# carries. `https://tidewater.example` is `knobas_source_mock`'s `MOCK_BASE`.
readonly REPO_QUERY='payout-service'
readonly REPO_REMOTE='https://tidewater.example/tidewater/payout-service'

readonly KEY_A=0
readonly KEY_K=40
readonly KEY_RETURN=36
readonly SETTLE_SECONDS=8
readonly SPAWN_SECONDS=20

say() { printf 'open-in-editor: %s\n' "$*"; }
die() {
    printf 'open-in-editor: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'open-in-editor:   %s\n' "$line" >&2; done
    # `|| true` on both, and load-bearing rather than defensive: `ax focused`
    # exits 1 exactly when nothing has focus, which is one of the likelier ways
    # to arrive here, and under `pipefail` that would kill this function before
    # the tree dump -- so the one run with the diagnosis in reach would print
    # half of it.
    printf 'open-in-editor: the focused element at that moment:\n' >&2
    { "$ax" focused "$pid" 2>&1 || true; } | sed 's/^/open-in-editor:   /' >&2
    printf 'open-in-editor: the window as the accessibility tree sees it:\n' >&2
    { "$ax" dump "$pid" 8 2>&1 || true; } | sed 's/^/open-in-editor:   /' >&2
    exit 1
}

# wait_until <description> <predicate...> -- poll for a settled screen.
# A keystroke is asynchronous twice over: the window server delivers it, and
# then Svelte's `tick()` redraws. Neither is instant and neither is slow, so
# this polls rather than sleeping a guessed amount.
wait_until() {
    local what=$1
    shift
    local deadline=$((SECONDS + SETTLE_SECONDS))
    while [ "$SECONDS" -lt "$deadline" ]; do
        if "$@"; then return 0; fi
        sleep 0.2
    done
    die "$what"
}

# How many elements carry a label, as a number and never as "whatever `ax`
# printed". `ax find` writes nothing on stdout when it fails, and an empty
# answer compared with `!= 0` is *true* -- so a broken helper would make every
# "it appeared" below pass. The count is read once and matched against digits.
count_of() {
    local answer
    answer=$("$ax" find "$pid" "$1" 2>/dev/null || true)
    case $answer in
    '' | *[!0-9]*) printf 'x\n' ;;
    *) printf '%s\n' "$answer" ;;
    esac
}
present() { local n; n=$(count_of "$1"); [ "$n" != x ] && [ "$n" -gt 0 ]; }
exactly_one() { [ "$(count_of "$1")" = 1 ]; }

# fill <label> <text> -- put <text> in the field called <label>, replacing
# whatever is in it.
#
# ⌘A before typing, because these fields arrive with a value: the clones root
# may hold a previous run's, and the command field holds this platform's
# default. Typing without selecting first would append to it, and the
# resulting template would be a program name nothing resolves -- a failure
# that looks like the feature's rather than the driver's.
fill() {
    local label=$1 value=$2
    wait_until "no field called '$label' appeared within ${SETTLE_SECONDS} s" exactly_one "$label"
    "$ax" focus "$pid" "$label" || die "could not put the keyboard in '$label'"
    "$ax" key "$KEY_A" command
    "$ax" type "$value"
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-in-editor.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

record=$scratch/record
stub=$scratch/stub
stub_script "$record" >"$stub"
chmod +x "$stub"

# The clone the scan has to find: a directory one level under the root whose
# `.git/config` names the demo repository as its `origin`. No `git init` --
# nothing in this feature runs git, and a driver that did would be witnessing
# git rather than knobas.
clones=$scratch/clones
checkout=$clones/$REPO_QUERY
mkdir -p "$checkout/.git"
git_config "$REPO_REMOTE" >"$checkout/.git/config"
say "a clone at $checkout, and a stub at $stub"

say "bringing pid $pid to the front"
"$ax" activate "$pid"

# --- 1. the settings ---------------------------------------------------------

say "opening Settings"
# Waited for, and not pressed straight away. The shell finishes booting after
# its window appears -- the harness waits for a *window*, which the boot screen
# also is -- so a press issued the instant the app is frontmost finds nothing.
# Measured on 2026-09-08: `ax press` answered *0 elements labelled 'Settings'*
# on a run whose own failure dump, a second later, showed the button.
wait_until "the top strip's '$SETTINGS_BUTTON' button never appeared" \
    exactly_one "$SETTINGS_BUTTON"
"$ax" press "$pid" "$SETTINGS_BUTTON" || die "could not press the '$SETTINGS_BUTTON' button"

fill "$CLONES_ROOT_FIELD" "$clones"
"$ax" press "$pid" "$SAVE_CLONES_ROOT" || die "could not press '$SAVE_CLONES_ROOT'"
say "clones root set to $clones"

# The stub's path is **quoted inside the template**: `mktemp -d` answers under
# `$TMPDIR`, and a `TMPDIR` with a space in it would otherwise make the first
# word of the template something that is not the stub -- a driver failure
# wearing the feature's clothes. `expand` groups a quoted word into one
# argument, which is exactly what this needs.
fill "$VSCODE_FIELD" "\"$stub\" {path}"
"$ax" press "$pid" "$SAVE_VSCODE" || die "could not press '$SAVE_VSCODE'"
say "the '$VSCODE_FIELD' command now runs the stub"

# Both Saves answer the fresh state, and a stored template stops being the
# platform's default -- which is what puts a *Reset* button beside it. That is
# the cheapest observable proof that the write landed, and it is asserted
# rather than assumed: a Save that silently failed would otherwise be
# discovered three steps later as "the editor did not open".
# `$VSCODE_LABEL` and not `$VSCODE_FIELD`: this is the *Reset* button's
# `aria-label`, which carries the label verbatim, while the field beside it is
# named by its rendered -- uppercased -- text.
wait_until "the command was not stored: no '$SAVE_VSCODE' row offers to reset it" \
    present "Reset $VSCODE_LABEL"
say "the template is stored (the field offers to reset it)"

# --- 2. the repo detail ------------------------------------------------------

say "opening the launcher and asking for '$REPO_QUERY'"
"$ax" key "$KEY_K" command
wait_until "⌘K did not open the launcher within ${SETTLE_SECONDS} s" present 'Search or act'
"$ax" type "$REPO_QUERY"
"$ax" key "$KEY_RETURN"

# The checkout panel is what a repo or a branch detail has and no other kind
# does, so its heading is the honest test for "a repo or branch detail is open".
# Either will do: the panel a branch draws is its **repository's** checkout
# (`knobas_app::checkout`'s `repo_of` finds the repo whose entity id the branch's
# extends), so the button below is the same button either way -- and the
# launcher's group order puts branches above repositories, so a ⌘K-and-Return
# on this query may well land on one.
#
# **Until #537 this is where a run stopped, and not for a fault of the
# feature**: `knobas_source_mock::items` walked past the fixture's `repos` and
# `branches`, so `--demo` had no repo entity at all. It carries three of each
# now, and `crates/knobas-app/tests/demo.rs`'s
# `the_demo_corpus_answers_the_checkout_the_desktop_driver_opens` holds
# everything below the window: the entity read answering under this query, and
# `checkout::view` matching a clone whose remote is $REPO_REMOTE. So a failure
# here is about the window -- the launcher, the routing, or the panel -- and
# the message says where to look rather than blaming the corpus.
deadline=$((SECONDS + SETTLE_SECONDS))
while [ "$SECONDS" -lt "$deadline" ] && ! present "$CHECKOUT_PANEL"; do sleep 0.2; done
if ! present "$CHECKOUT_PANEL"; then
    die "no repo or branch detail opened for '$REPO_QUERY'." \
        "The demo corpus does carry the repository (#537), and the read behind" \
        "this panel is covered by demo.rs's" \
        "the_demo_corpus_answers_the_checkout_the_desktop_driver_opens -- so" \
        "what failed is between the launcher and the panel: check that ⌘K's" \
        "first hit for this query is a repo or a branch, and that the tree" \
        "below carries a heading named '$CHECKOUT_PANEL'."
fi
say "a repo or branch detail is open, and it has a checkout panel"

# --- 3. the spawn ------------------------------------------------------------

wait_until "no '$OPEN_BUTTON' button on the detail within ${SETTLE_SECONDS} s" \
    exactly_one "$OPEN_BUTTON"
say "pressing '$OPEN_BUTTON'"
"$ax" press "$pid" "$OPEN_BUTTON" || die "could not press '$OPEN_BUTTON'"

# The spawn is asynchronous by construction: `open_checkout` answers when the
# program has *started*, so what is polled for is the stub's own output --
# finished, in the sense `record_is_complete` gives the word, and not merely
# begun. The wait lives in the lib because `witness-unit` covers the lib and
# this is a decision rather than a side effect (#500's ruling, #531).
if ! wait_for_record "$record" "$SPAWN_SECONDS"; then
    printf 'open-in-editor: what is at %s now:\n' "$record" >&2
    { cat "$record" 2>/dev/null || printf '(nothing at that path)\n'; } |
        sed 's/^/open-in-editor:   /' >&2
    die "the stub never finished writing a record after ${SPAWN_SECONDS} s." \
        "A record is finished when it ends in a newline -- the condition" \
        "recorded() waits for in crates/knobas-app/tests/checkout_ipc.rs."
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
