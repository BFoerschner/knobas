#!/usr/bin/env bash
# The first desktop-witness driver (issue #500): ⌘K opens the launcher, and
# Escape closes it again.
#
# Why this feature first. The harness needs a driver to be a harness at all,
# and the ⌘K overlay is the one OS-level feature of the three that already
# exists and needs no source, no editor and no terminal on the machine to
# prove. It is also the honest one to start with: the assertion is about a
# keystroke arriving at a real window and a real element taking focus, which
# is precisely what a `?fake-ipc` browser walk cannot say anything about.
#
# What it asserts, and what it deliberately does not. It asserts that after a
# synthetic ⌘K the application's focused element is the launcher's query box --
# by role and by the `aria-label` the box carries (`QueryBox.svelte`,
# "Search or act"), read out of the accessibility tree -- and that after
# Escape the focus has left it. It says nothing about what the launcher then
# shows: the board, the results and the action chain are the frontend suite's
# and the `?fake-ipc` walk's, per the deputy's ruling of 2026-09-08 on #496.
#
# Run by testenv/desktop-witness.sh, which passes it a compiled helper, the
# pid of the one running instance and the bundle it launched.

set -euo pipefail

ax=${KNOBAS_WITNESS_AX:?the harness passes this}
pid=${KNOBAS_WITNESS_PID:?the harness passes this}

# The `aria-label` on the launcher's input. Named here rather than matched
# loosely: a driver that accepted any text field would pass on the shell's
# top-strip search field, which is a different box that opens the same overlay.
readonly QUERY_BOX_LABEL='Search or act'
readonly KEY_K=40
readonly KEY_ESCAPE=53
readonly SETTLE_SECONDS=5

say() { printf 'launcher-hotkey: %s\n' "$*"; }
die() {
    printf 'launcher-hotkey: FAILED -- %s\n' "$*" >&2
    printf 'launcher-hotkey: the focused element at that moment:\n' >&2
    "$ax" focused "$pid" 2>&1 | sed 's/^/launcher-hotkey:   /' >&2
    printf 'launcher-hotkey: the window as the accessibility tree sees it:\n' >&2
    "$ax" dump "$pid" 6 2>&1 | sed 's/^/launcher-hotkey:   /' >&2
    exit 1
}

# field <name> -- one line of `ax focused`, or empty when there is no focus.
field() {
    "$ax" focused "$pid" 2>/dev/null | awk -F'\t' -v key="$1" '$1 == key { print $2; exit }'
}

# focus_is_query_box -- true while the launcher's box holds focus.
focus_is_query_box() {
    [ "$(field role)" = AXTextField ] && [ "$(field description)" = "$QUERY_BOX_LABEL" ]
}

# The negation, as a command of its own: `wait_until` runs what it is given,
# and `!` is a shell keyword rather than something an argument list can carry.
focus_left_query_box() {
    ! focus_is_query_box
}

# wait_until <description> <predicate...> -- poll for a settled screen.
# A keystroke is asynchronous twice over: the window server delivers it, and
# then Svelte's `tick()` moves the focus. Neither is instant and neither is
# slow, so this polls rather than sleeping a guessed amount.
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

say "bringing pid $pid to the front"
"$ax" activate "$pid"

# The negative half, first and deliberately: without it a launcher that was
# already open (a previous run that died before its Escape, say) would let
# every assertion below pass without ⌘K doing anything at all.
if focus_is_query_box; then
    die "the launcher's query box already had focus before ⌘K was pressed"
fi
say "the query box does not have focus yet"

say "pressing ⌘K"
"$ax" key "$KEY_K" command
wait_until "⌘K did not put focus in the launcher's query box within ${SETTLE_SECONDS} s" \
    focus_is_query_box
say "the launcher's query box has focus: role AXTextField, description '$QUERY_BOX_LABEL'"

say "pressing Escape"
"$ax" key "$KEY_ESCAPE"
wait_until "Escape did not take focus out of the launcher's query box within ${SETTLE_SECONDS} s" \
    focus_left_query_box
say "focus has left the query box"

say "ok"
