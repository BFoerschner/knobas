#!/usr/bin/env bash
# The third desktop-witness driver (issue #503, ported to the ticket-first
# shell by #1084): the global shortcut opens the capture window over another
# application, and what is typed there becomes a note.
#
# Why this feature is witnessed here and not in a suite. A **global** shortcut
# is the one keystroke in knobas that has to arrive while knobas is not the
# application being typed at. `capture_ipc.rs` proves what is stored and what is
# reported, and `capture.test.svelte.ts` what the window does with a keystroke
# -- both against ports, with no window server, no key and no other application
# in them. The only witness for the premise is a real Mac with something else
# in front (ADR-0016).
#
# What it asserts, in order:
#
#   1. in Settings › Notes and capture, notes shown (the shortcut registers
#      nothing while they hide, D10) and a shortcut typed into the *Change…*
#      dialog (#1086) and stored by the real backend, reported as registered
#      rather than refused -- and the note count as it stands;
#   2. the job PAY-231 opened, so the capture has a foreground to attach;
#   3. **Finder frontmost**, asserted and not assumed;
#   4. the keystroke opening the capture window -- **a window of knobas' own
#      that was not there one keystroke earlier** (#548) -- with the caret in
#      its box;
#   5. a line typed, *Saved as a note.*, and *Open in knobas* pressed;
#   6. knobas frontmost again, the capture window gone, and Settings › Notes
#      and capture on screen -- where a note opens while notes hide (#1085) --
#      counting **one note more** than step 1 did.
#
# What it does **not** assert: Escape closing the window
# (`CaptureWindow.test.svelte.ts`), a combination the system refuses (needs an
# application holding one), and the note's *captured from* link -- a note has
# no view while notes hide, so no window draws its links; the link is pinned at
# `capture_ipc.rs` and `capture.test.svelte.ts`, which is where spec #491's
# stream map puts it.
#
# What it leaves behind on the demo profile: notes shown, this shortcut stored,
# and one note per run. A run that finds the shortcut already stored clears it
# first, so *Save* is never pressed on a field that would not change.
#
# Run by testenv/desktop-witness.sh, which passes it a compiled helper, the pid
# of the one running instance and the bundle it launched.

set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}

# The accessible names this driver presses, fills and reads. Each is pinned
# against the file that carries it by `just witness-unit`. The new shell
# uppercases none of them.
readonly SETTINGS_BUTTON='Settings'                       # TfTopBar.svelte
readonly NOTES_SECTION='Notes and capture'                # settings/settings.ts
readonly SHOW_NOTES='Show notes'                          # TfNotesSection.svelte
readonly CHANGE_SHORTCUT='Change capture shortcut'
readonly SHORTCUT_FIELD='Capture shortcut'
readonly CLEAR='Clear'
readonly SAVE='Save'
# Readings (values, not names) the section draws.
readonly HIDDEN_LINE='Inactive while notes are hidden.'
readonly REGISTERED_LINE='Opens a small window over whatever you are doing.'
readonly NOT_SET='Not set'
readonly YOUR_NOTES='Your notes'
readonly OPEN_CODE_VIEW='Open Code view'                  # job/TfCodeSection.svelte
readonly TICKETS_LINK='Tickets'                           # TfTopBar.svelte's nav
readonly TICKETS_FILTER='Filter by key, summary, epic or person'  # tickets/TicketsPage.svelte
# The preview's button, worded by the ticket's state (`tickets.ts`'
# `previewAction`); each is a link to the job.
readonly JOB_LINKS=('Start work' 'Resume work' 'Open job')
readonly CAPTURE_BOX='Capture'                            # capture/CaptureWindow.svelte
# The **window**'s title (`capture.rs`'s `.title("Capture")`), a different fact
# from the box's `aria-label` above though the same word (#548).
readonly CAPTURE_WINDOW='Capture'
readonly OPEN_IN_MAIN='Open in knobas'

# ⌘⌥⇧K: registered globally for the length of the run, so three modifiers.
readonly ACCELERATOR='CmdOrCtrl+Alt+Shift+K'
readonly KEY_K=40
readonly KEY_A=0

readonly JOB_QUERY='PAY-231'
readonly JOB_SUMMARY='Retry failed SEPA payouts'          # fixtures/tidewater/work.json
readonly OTHER_APP=Finder
readonly OTHER_BUNDLE=com.apple.finder
# The guard on every wait below: KNOBAS_WITNESS_WAIT_S (`witness_guard`, #1961).
SETTLE_SECONDS=$(witness_guard) || exit 2
readonly SETTLE_SECONDS

say() { printf 'capture: %s\n' "$*"; }
die() {
    printf 'capture: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'capture:   %s\n' "$line" >&2; done
    # `|| true` on each: `ax focused` exits 1 when nothing has focus, and under
    # `pipefail` that would kill this before the dumps.
    printf 'capture: whatever is frontmost:\n' >&2
    { "$ax" frontmost 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: the focused element at that moment:\n' >&2
    { "$ax" focused "$pid" 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: the windows knobas has open:\n' >&2
    { "$ax" windows "$pid" 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: every string on knobas'"'"' windows:\n' >&2
    { "$ax" values "$pid" 2>&1 || true; } | sed 's/^/capture:   /' >&2
    printf 'capture: the windows as the accessibility tree sees them:\n' >&2
    { "$ax" dump "$pid" 12 2>&1 || true; } | sed 's/^/capture:   /' >&2
    exit 1
}

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
present() { local n; n=$(count_of "$1"); [ "$n" != x ] && [ "$n" -gt 0 ]; }
absent() { [ "$(count_of "$1")" = 0 ]; }
# Keeps the count it read in `seen`, which `wait_until` names when it runs out.
exactly_one() {
    local n
    n=$(count_of "$1")
    seen="'$1' counted $n"
    [ "$n" = 1 ]
}

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

# How many of knobas' windows are titled '$CAPTURE_WINDOW', or `x` (#548).
capture_windows() { window_count "$("$ax" windows "$pid" 2>/dev/null || true)" "$CAPTURE_WINDOW"; }
capture_window_gone() { [ "$(capture_windows)" = 0 ]; }

is_frontmost() { [ "$(frontmost_bundle "$("$ax" frontmost 2>/dev/null || true)")" = "$1" ]; }

screen() { "$ax" values "$pid" 2>/dev/null || true; }
reads() { has_reading "$(screen)" "$1"; }
unread() { ! reads "$1"; }

# Tickets previews the filtered card: its summary on screen, and exactly one of
# the preview's job links.
job_link() {
    local name
    for name in "${JOB_LINKS[@]}"; do
        if exactly_one "$name"; then printf '%s\n' "$name"; return 0; fi
    done
    return 1
}
previews_the_job() { reads "$JOB_SUMMARY" && job_link >/dev/null; }

caret_in_the_box() {
    local focused
    focused=$("$ax" focused "$pid" 2>/dev/null || true)
    [ "$(tsv_field "$focused" role)" = AXTextArea ] || return 1
    [ "$(tsv_field "$focused" description)" = "$CAPTURE_BOX" ] ||
        [ "$(tsv_field "$focused" title)" = "$CAPTURE_BOX" ]
}

# --- 1. Settings › Notes and capture -----------------------------------------

say "bringing pid $pid to the front"
"$ax" activate "$pid"

say "opening Settings › $NOTES_SECTION"
press_when "$SETTINGS_BUTTON"
press_when "$NOTES_SECTION"
wait_until "the section never drew its '$SHOW_NOTES' switch" exactly_one "$SHOW_NOTES"

# The switch's state is read from the help line under the shortcut, which is
# the section's own word for it: hidden, it says so.
if reads "$HIDDEN_LINE"; then
    say "notes are hidden; turning them on"
    "$ax" press "$pid" "$SHOW_NOTES" || die "could not press '$SHOW_NOTES'"
    wait_until "the section still says notes are hidden after '$SHOW_NOTES'" unread "$HIDDEN_LINE"
fi

# A shortcut a previous run stored is cleared first: *Save* is disabled on a
# field that would not change, and the row reading *Not set* before Save is
# what makes its keys after Save mean this run's write.
#
# The field is found by the focus and not by `ax find`: measured on 2026-10-18,
# three elements answer to '$SHORTCUT_FIELD' while the dialog is up -- the
# dialog (`aria-labelledby` its heading), the heading, and the field -- each
# correctly. `TfDialog` puts the caret in its first field, so the field is the
# focused AXTextField of that name, role *and* name, like the capture box.
caret_in_the_field() {
    local focused
    focused=$("$ax" focused "$pid" 2>/dev/null || true)
    [ "$(tsv_field "$focused" role)" = AXTextField ] || return 1
    [ "$(tsv_field "$focused" description)" = "$SHORTCUT_FIELD" ] ||
        [ "$(tsv_field "$focused" title)" = "$SHORTCUT_FIELD" ]
}
#
# And the row is read with the dialog **closed**: the dialog is `aria-modal`,
# and while it is up the accessibility tree is the dialog and nothing behind it
# (measured the same day: no *Not set* in `ax values` with the row on screen).
if ! reads "$NOT_SET"; then
    say "a shortcut is already stored; clearing it"
    press_when "$CHANGE_SHORTCUT"
    wait_until "the shortcut dialog never opened with the caret in its field" caret_in_the_field
    press_when "$CLEAR"
    wait_until "the shortcut was not cleared: the row never read '$NOT_SET'" reads "$NOT_SET"
fi
say "the row reads '$NOT_SET'; setting $ACCELERATOR"
press_when "$CHANGE_SHORTCUT"
wait_until "the shortcut dialog never opened with the caret in its field" caret_in_the_field
"$ax" key "$KEY_A" command
"$ax" type "$ACCELERATOR"
press_when "$SAVE"
registered() { absent "$SHORTCUT_FIELD" && unread "$NOT_SET" && reads "$REGISTERED_LINE"; }
wait_until "the shortcut was not stored and registered: the row does not read '$REGISTERED_LINE'" \
    registered
say "the shortcut $ACCELERATOR is stored and registered"

notes_before=$(notes_total "$(screen)")
[ "$notes_before" != x ] || die "the section carries no single notes line to count from"
say "notes before the capture: $notes_before"

# --- 2. something in front of the reader -------------------------------------

# From Tickets and not from ⌘K: on 2026-10-18 the demo profile's launcher
# answered `PAY-231` with pages, commits and a branch and `#PAY-231` with
# nothing. Tickets filtered by the key previews the one card, and the
# preview's button is a link to its job.
say "opening the job $JOB_QUERY from Tickets, so the capture has a foreground"
press_when "$TICKETS_LINK"
fill "$TICKETS_FILTER" "$JOB_QUERY"
wait_until "Tickets never previewed '$JOB_QUERY' ($JOB_SUMMARY)" previews_the_job
go=$(job_link) || die "the preview offers none of: ${JOB_LINKS[*]}"
"$ax" press "$pid" "$go" || die "could not press '$go'"
job_is_open() { absent "$TICKETS_FILTER" && present "$OPEN_CODE_VIEW"; }
wait_until "no job page for '$JOB_QUERY' opened" job_is_open
say "the job is open, so something is in front of the reader"

# --- 3. somebody else's screen -----------------------------------------------

say "bringing $OTHER_APP to the front"
# `open -a` and not `ax activate` alone: macOS refuses to hand the screen to an
# application a background process merely activates (measured 2026-10-18:
# *did not become frontmost*). But `open -a` on a Finder with no window open
# makes one, and a run left it on the reader's screen the same day -- so a
# Finder that had no window gets the one this run made closed again on exit.
other_pid=$(pgrep -x "$OTHER_APP" | head -1 || true)
[ -n "$other_pid" ] || die "$OTHER_APP is not running, so there is nothing to put in front"
if ! "$ax" windows "$other_pid" >/dev/null 2>&1; then
    close_the_window_we_made() {
        # `ax close-window` closes exactly one window or none: with a second
        # (one the reader opened meanwhile) nothing is closed.
        "$ax" close-window "$other_pid" ||
            say "could not close the $OTHER_APP window this run opened"
    }
    trap close_the_window_we_made EXIT
fi
open -a "$OTHER_APP"
"$ax" activate "$other_pid" || die "could not bring $OTHER_APP to the front"
wait_until "$OTHER_APP never became frontmost, so the keystroke below would not have been global" \
    is_frontmost "$OTHER_BUNDLE"
say "$OTHER_APP has the screen; knobas is behind it"

# --- 4. the shortcut, from somebody else's screen ----------------------------

# Windows counted on both sides of the keystroke (#548): none, then one.
say "pressing $ACCELERATOR"
before=$(count_verdict "$(capture_windows)")
say "before the keystroke, windows titled '$CAPTURE_WINDOW': $before"
"$ax" key "$KEY_K" command option shift
# Polled under the guard; the predicate leaves what it last counted in
# `count` and `after`.
a_capture_window_is_open() {
    count=$(capture_windows)
    after=$(count_verdict "$count")
    [ "$after" = yes ]
}
polled=0
witness_poll "$pid" "$SETTLE_SECONDS" a_capture_window_is_open || polled=$?
[ "$polled" != 3 ] || die "knobas (pid $pid) exited after $ACCELERATOR"
say "after the keystroke, windows titled '$CAPTURE_WINDOW': $after ($count)"

case $(waypoint_verdict "$before" "$after") in
witnessed) ;;
too-early)
    die "a window titled '$CAPTURE_WINDOW' was already open before the shortcut," \
        "so waiting for one witnesses nothing about the shortcut."
    ;;
never-appeared)
    die "the capture window never appeared." \
        "The shortcut was reported as registered and $OTHER_APP had the screen," \
        "so look between the window server and capture::open_window."
    ;;
*)
    die "knobas' windows could not be read either side of the shortcut" \
        "(before: $before, after: $after): a quit app or a locked screen."
    ;;
esac
[ "$count" = 1 ] || die "$count windows are titled '$CAPTURE_WINDOW', and the shortcut opens one."

wait_until "the capture window opened without the caret in its box" caret_in_the_box
say "the capture window is up, with the caret in it"

# --- 5. the thought ----------------------------------------------------------

typed='Retry storm from the capture witness
the queue backs up at 09:00'
say "typing a capture whose title will be '$(capture_title "$typed")'"
"$ax" type "$typed"
wait_until "the capture never became a note: the window still says nothing is kept" \
    reads "Saved as a note."

# The page *Open in knobas* lands on, read before the press and after it: the
# job is in the main window now, so '$YOUR_NOTES' is not on screen yet.
landed_before=$(reading_verdict "$(screen)" "$YOUR_NOTES")
say "pressing '$OPEN_IN_MAIN'"
"$ax" press "$pid" "$OPEN_IN_MAIN" || die "could not press '$OPEN_IN_MAIN'"

# --- 6. the note -------------------------------------------------------------

wait_until "knobas never came forward after '$OPEN_IN_MAIN'" is_frontmost dev.knobas.desktop
wait_until "the capture window is still open after '$OPEN_IN_MAIN'" capture_window_gone
say "knobas is frontmost again and the capture window has gone"

lands open-in-knobas "$YOUR_NOTES"
landed=$(waypoint_verdict "$landed_before" yes)
[ "$landed" = witnessed ] || die "'$YOUR_NOTES' was $landed_before before '$OPEN_IN_MAIN', so landing on it witnesses nothing."

one_more() { [ "$(notes_total "$(screen)")" = $((notes_before + 1)) ]; }
wait_until "the section does not count one note more than $notes_before (it reads $(notes_total "$(screen)"))" one_more
say "Settings › $NOTES_SECTION is open and counts $((notes_before + 1)) notes: the capture is stored"
say "ok"
