#!/usr/bin/env bash
# The third desktop-witness driver (issue #503): the global shortcut opens the
# capture window over another application, and what is typed there becomes a
# note with the links the room it was captured in gives it.
#
# Why this feature is witnessed here and not in a suite. A **global** shortcut
# is the one keystroke in knobas that has to arrive while knobas is not the
# application being typed at. `capture_ipc.rs` proves what is stored and what is
# reported, and `capture.test.svelte.ts` proves what the window does with a
# keystroke -- both against ports, with no window server, no key and no other
# application anywhere in them. What is left over is the whole of the feature's
# premise, and the only witness for it is a real Mac with something else in
# front (ADR-0016, and the v1.5 grilling's ruling on what a witness is).
#
# What it asserts, in order:
#
#   1. a shortcut typed into the real settings field and stored by the real
#      backend, reported as **registered** rather than refused;
#   2. a ticket opened, so the capture has a **foreground** to attach;
#   3. **Finder frontmost**, asserted and not assumed, because a capture
#      shortcut that only worked while knobas had the keyboard would pass every
#      other line of this file;
#   4. the keystroke opening the capture window, with the caret in its box;
#   5. a line typed, and *Open in knobas* pressed;
#   6. knobas frontmost again, the capture window gone, and the note open with
#      *captured from* in its links panel.
#
# What it does **not** assert, said here rather than left to be discovered:
# that Escape closes the window (the same exit as the button, minus the
# reveal -- `CaptureWindow.test.svelte.ts` drives both keys); that a combination
# the operating system refuses reads as refused (that needs an application
# holding one, which is a machine this driver cannot arrange); **`captured-in`**,
# which needs a stored room the `--demo` profile does not carry and which the
# spec pins at another seam -- see step 2; and **which entity the link points
# at** -- see the note at the foot, which says why a driver that read the whole
# window for it would pass with no link drawn. **None of the three is a debt.**
#
# Run by testenv/desktop-witness.sh, which passes it a compiled helper, the pid
# of the one running instance and the bundle it launched.

set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}

# The accessible names this driver acts on. Each is pinned against the file that
# carries it by `just witness-unit`, in the shape #500 pinned the launcher's
# `aria-label` and #501 pinned the settings fields: nothing in the app knows a
# driver exists, so an edit to one of these would otherwise fail a *correct* app
# minutes into a run with no clue in the failure.
readonly SETTINGS_BUTTON='Settings'
# The **rendered** name, because this field is named by a `<label class="lab">`
# and `.lab` is uppercased by the stylesheet -- see `rendered_label`, and the
# measurement behind it. Every other name in this file is an `aria-label`,
# which is not rendered text and arrives spelled as it is written.
SHORTCUT_FIELD=$(rendered_label 'Shortcut')
readonly SHORTCUT_FIELD
readonly SAVE_SHORTCUT='Save capture shortcut'
readonly CAPTURE_BOX='Capture'
readonly OPEN_IN_MAIN='Open in knobas'
readonly QUERY_BOX='Search or act'

# The reading a note's links panel groups a `captured-from` link under
# (`detail/relations.ts`). The **note's** side of it; the other end reads
# `captured from here`, which is why `has_reading` matches whole lines.
# Rendered, for the reason the field above is: the panel's group heading is a
# bare `<span>` inside `.row.hd`, which the stylesheet uppercases.
READING_FROM=$(rendered_label 'captured from')
readonly READING_FROM
# The heading every detail's links panel carries, `.lab` and so uppercased.
LINKS_PANEL=$(rendered_label 'Linked items')
readonly LINKS_PANEL

# The shortcut this run sets, and the keystroke that is the same combination.
#
# ⌘⌥⇧K rather than something shorter: this is registered **globally**, so it is
# taken away from every application on the machine for the length of the run,
# and three modifiers is the cheapest way to be sure the one it is taken from is
# nobody. `CmdOrCtrl` is the spelling the field's own placeholder uses and the
# one `global_hotkey` maps to ⌘ on macOS.
readonly ACCELERATOR='CmdOrCtrl+Alt+Shift+K'
readonly KEY_K=40
readonly KEY_A=0
readonly KEY_RETURN=36

# The ticket the demo profile carries, opened so the capture has a foreground.
readonly TICKET_QUERY='PAY-231'

# The application brought to the front before the keystroke. Finder, because
# every Mac has one, it is always running, and it owns no interesting keys.
readonly OTHER_APP=Finder
readonly OTHER_BUNDLE=com.apple.finder

readonly SETTLE_SECONDS=8

say() { printf 'capture: %s\n' "$*"; }
die() {
    printf 'capture: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'capture:   %s\n' "$line" >&2; done
    # `|| true` on all three, and load-bearing rather than defensive: `ax
    # focused` exits 1 exactly when nothing has focus, which is one of the
    # likelier ways to arrive here, and under `pipefail` that would kill this
    # function before the dumps -- so the one run with the diagnosis in reach
    # would print a third of it.
    printf 'capture: whatever is frontmost:\n' >&2
    { "$ax" frontmost 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: the focused element at that moment:\n' >&2
    { "$ax" focused "$pid" 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: every string on knobas'"'"' windows:\n' >&2
    { "$ax" values "$pid" 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: the windows as the accessibility tree sees them:\n' >&2
    { "$ax" dump "$pid" 8 2>&1 || true; } | sed 's/^/capture:   /' >&2
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
absent() { [ "$(count_of "$1")" = 0 ]; }
exactly_one() { [ "$(count_of "$1")" = 1 ]; }

# fill <label> <text> -- put <text> in the field called <label>, replacing
# whatever is in it.
#
# ⌘A before typing, because a settings field arrives with a value and typing
# without selecting first would append to it -- producing a shortcut string
# nothing parses, which is a failure that looks like the feature's rather than
# the driver's.
fill() {
    local label=$1 value=$2
    wait_until "no field called '$label' appeared within ${SETTLE_SECONDS} s" exactly_one "$label"
    "$ax" focus "$pid" "$label" || die "could not put the keyboard in '$label'"
    "$ax" key "$KEY_A" command
    "$ax" type "$value"
}

# is_frontmost <bundle-id> -- whether that application has the screen.
is_frontmost() {
    [ "$(frontmost_bundle "$("$ax" frontmost 2>/dev/null || true)")" = "$1" ]
}

# What is written on knobas' windows right now, one string per line.
#
# `ax values` and not `ax find`: a heading, a status line and a note's title are
# **values**, not accessible names, and `find` counts names. The two are kept
# apart in the helper for the reason its own docs give.
screen() { "$ax" values "$pid" 2>/dev/null || true; }

# reads <line> -- the screen carries <line> as a whole line.
reads() { has_reading "$(screen)" "$1"; }

# The settings section's own word for "the operating system handed the
# combination over". The other branch reads *Not registered: …*, which is a
# different whole line, so this cannot pass on a refusal.
has_registered() { reads "Registered."; }

# A detail is open over the room: the launcher has closed and a links panel is
# on screen, which every detail has and a room has none of.
ticket_is_open() { absent "$QUERY_BOX" && reads "$LINKS_PANEL"; }

# The launcher has finished searching for what was typed.
#
# Waited for rather than assumed, because the search is a round trip and Return
# on a list that has not arrived selects nothing. Measured on 2026-09-08: a
# driver that typed and pressed Return in the same breath left the launcher
# open with its query in it and four matches underneath, and reported the
# feature as broken. The line is the launcher's own count, matched loosely
# because the timing in it changes every run.
launcher_answered() { screen | grep -q " match"; }

# The caret is in the capture window's box.
#
# The role **and** the name, because either alone would pass on the wrong thing:
# the app is full of text fields, and `AXTextArea` alone would accept the note
# editor behind the capture window.
caret_in_the_box() {
    local focused
    focused=$("$ax" focused "$pid" 2>/dev/null || true)
    [ "$(tsv_field "$focused" role)" = AXTextArea ] || return 1
    [ "$(tsv_field "$focused" description)" = "$CAPTURE_BOX" ] ||
        [ "$(tsv_field "$focused" title)" = "$CAPTURE_BOX" ]
}

# The capture window's own report that the row exists. Its other two states
# read *Nothing is kept until you type.* and *<a refusal>*, both different
# whole lines.
note_was_written() { reads "Saved as a note."; }


# --- 1. the shortcut ---------------------------------------------------------

say "bringing pid $pid to the front"
"$ax" activate "$pid"

say "opening Settings"
# Waited for, and not pressed straight away. The shell finishes booting after
# its window appears -- the harness waits for a *window*, which the boot screen
# also is -- so a press issued the instant the app is frontmost finds nothing.
# Measured on 2026-09-08: `ax press` answered *0 elements labelled 'Settings'*
# on a run whose own failure dump, a second later, showed the button.
wait_until "the top strip's '$SETTINGS_BUTTON' button never appeared" \
    exactly_one "$SETTINGS_BUTTON"
"$ax" press "$pid" "$SETTINGS_BUTTON" || die "could not press the '$SETTINGS_BUTTON' button"

fill "$SHORTCUT_FIELD" "$ACCELERATOR"
"$ax" press "$pid" "$SAVE_SHORTCUT" || die "could not press '$SAVE_SHORTCUT'"

# *Registered.* is the section's own word for "the operating system handed the
# combination over", and it is what turns the rest of this run into a witness of
# the feature rather than of a keystroke going nowhere. A refusal is the other
# branch and reads *Not registered: …*, which the values dump in `die` shows.
wait_until "the shortcut was not registered: the section never said so" \
    has_registered
say "the shortcut $ACCELERATOR is stored and registered"

# --- 2. something in front of the reader -------------------------------------
#
# A ticket opened in *All work*, so the capture has a **foreground** to attach.
#
# **`captured-in` is not asserted by this run, and it is owed to nothing.** It
# needs a *stored* room, and the `--demo` profile carries no context at all: a
# reader makes one, and making one here would mean driving another feature's tab
# strip to arrange this feature's fixture -- a first attempt did exactly that on
# 2026-09-08 and the field closed under the driver before its Return.
# **Spec #491's stream map puts the links' witness elsewhere on purpose**: row 5
# is *"app IPC suite for the links; desktop automation for the shortcut and
# window"*, and the links are pinned where it says -- `capture_ipc.rs` (the
# recorded pair, over a scratch database) and `capture.test.svelte.ts` (the pair
# becoming the links). Asserting `CAPTURED FROM` below is already one link more
# than that row asks of this driver. See testenv/README.md, *What is still not
# witnessed*, and the deputy's ruling of 2026-09-08 on #503, part 5.

say "opening the ticket $TICKET_QUERY, so the capture has a foreground"
"$ax" key "$KEY_K" command
wait_until "⌘K did not open the launcher within ${SETTLE_SECONDS} s" present "$QUERY_BOX"
"$ax" type "$TICKET_QUERY"
wait_until "the launcher never answered for '$TICKET_QUERY'" launcher_answered
"$ax" key "$KEY_RETURN"
wait_until "no detail for '$TICKET_QUERY' opened" ticket_is_open
say "a detail is open, so something is in front of the reader"

# --- 3. somebody else's screen -----------------------------------------------

say "bringing $OTHER_APP to the front"
open -a "$OTHER_APP"
other_pid=$(pgrep -x "$OTHER_APP" | head -1 || true)
[ -n "$other_pid" ] || die "$OTHER_APP is not running, so there is nothing to put in front"
"$ax" activate "$other_pid" || die "could not bring $OTHER_APP to the front"
wait_until "$OTHER_APP never became frontmost, so the keystroke below would not have been global" \
    is_frontmost "$OTHER_BUNDLE"
say "$OTHER_APP has the screen; knobas is behind it"

# --- 4. the shortcut, from somebody else's screen ----------------------------

say "pressing $ACCELERATOR"
"$ax" key "$KEY_K" command option shift
wait_until "the capture window never appeared" exactly_one "$CAPTURE_BOX"
# The caret, and not merely the window: the whole promise is that the next
# keystroke is the note, and a window that opened without focus would swallow
# the sentence typed below into whatever was behind it.
wait_until "the capture window opened without the caret in its box" caret_in_the_box
say "the capture window is up, with the caret in it"

# --- 5. the thought ----------------------------------------------------------

typed='Retry storm from the capture witness
the queue backs up at 09:00'
title=$(capture_title "$typed")
say "typing a capture whose title will be '$title'"
"$ax" type "$typed"

wait_until "the capture never became a note: the window still says nothing is kept" \
    note_was_written

say "pressing '$OPEN_IN_MAIN'"
"$ax" press "$pid" "$OPEN_IN_MAIN" || die "could not press '$OPEN_IN_MAIN'"

# --- 6. the note, and its two links ------------------------------------------

wait_until "knobas never came forward after '$OPEN_IN_MAIN'" is_frontmost dev.knobas.desktop
wait_until "the capture window is still open after '$OPEN_IN_MAIN'" absent "$CAPTURE_BOX"
say "knobas is frontmost again and the capture window has gone"

# The title `capture_title` says this typing produces, on screen -- which is the
# note view's own name field, and nothing else draws it: the capture window that
# held those words has closed by now.
wait_until "the note '$title' never opened in the main window" reads "$title"
say "the note is open in the main window, titled '$title'"

wait_until "the note's links panel does not read '$READING_FROM', so the capture attached no foreground" \
    reads "$READING_FROM"
say "the links panel reads '$READING_FROM': the capture attached what was in front of the reader"

# **Which entity the link points at is not asserted here, and that is a decision
# rather than an omission.** The ticket's key is on the room behind the
# slide-over, so a driver reading the whole window for it would pass with no
# link drawn at all -- a check measuring a representation of the thing. What the
# ends are is `capture_ipc.rs`' (the recorded pair) and
# `capture.test.svelte.ts`' (the pair becoming the links); what only this run
# can say is that a keystroke sent to Finder produced a note here with one.

say "ok"
