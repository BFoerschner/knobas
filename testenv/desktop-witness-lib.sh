#!/usr/bin/env bash
# The parts of the desktop witness that are decisions rather than side effects
# (issue #500), kept here so `desktop-witness-test.sh` can run them without a
# screen, a bundle or a signing identity.
#
# The split is not cosmetic. Everything in `desktop-witness.sh` either builds
# something, mutates Launch Services or drives a live app, so the only witness
# it can ever have is a run on the dev Mac with somebody's screen given over to
# it. What is left over -- comparing two paths, and reading a probe's three
# lines -- is where the harness is most likely to be quietly wrong, and it
# costs nothing to test. See `testenv/README.md`, *The desktop witness (macOS)*.
#
# Sourced, never executed. The shebang is here so `just shell`'s discovery
# (`git ls-files -- testenv`, filtered by shebang) lints this file too.

# The README section every refusal below points at, in one place: a message
# that names the wrong heading is worse than one that names none.
readonly WITNESS_README_SECTION='testenv/README.md, "The desktop witness (macOS)"'

# normalise_app_path <path>
#
# The spelling of a bundle path that two lookups can be compared on. Launch
# Services hands back a directory URL, and a directory URL's path carries a
# trailing slash that `tauri build`'s path does not; `dirname`/`basename`
# arithmetic elsewhere can leave doubled separators. Neither difference means
# the paths are different bundles, and a harness that refuses to launch over
# one is a harness nobody runs twice.
#
# Deliberately textual: no `realpath`, no `cd -P`. Resolving symlinks would
# make the answer depend on the filesystem, and this function's whole point is
# that its answer depends on nothing but its argument -- which is what lets a
# test assert it. Symlink resolution, if it is ever needed, belongs at the call
# site where the paths actually exist.
normalise_app_path() {
    local path=$1
    # Collapse runs of separators, then drop trailing ones. `/` itself is not
    # a bundle path, so it needs no special case.
    while [[ $path == *//* ]]; do path=${path//\/\//\/}; done
    while [[ ${#path} -gt 1 && $path == */ ]]; do path=${path%/}; done
    printf '%s\n' "$path"
}

# paths_match <expected> <actual>
#
# True when two bundle paths name the same bundle. The comparison the harness
# makes after it asks Launch Services what `dev.knobas.desktop` resolves to:
# an answer that is not the bundle this run just built means a notification
# click, or a second `open`, would activate somebody else's copy -- the Launch
# Services caveat in the README's *Signed dev build* section, which cost the
# notification prototype a whole run.
paths_match() {
    # The right-hand side is quoted: unquoted, `[[ == ]]` reads it as a glob,
    # and a bundle path is a filename -- `*` and `?` are legal in one.
    [[ $(normalise_app_path "$1") == "$(normalise_app_path "$2")" ]]
}

# classify_readiness <probe output>
#
# Turn `ax probe`'s three lines into the one word the harness acts on. The
# probe reports facts and refuses to interpret them; this is the interpreter,
# and it is where the order of precedence lives.
#
# The order is by what a person would have to do about it, not by severity:
# a missing Accessibility grant is a one-time visit to System Settings, a
# locked screen is a password, and losing the right to post events is neither.
# `unreadable` is its own answer rather than a default of `granted`: a probe
# that printed nothing at all (a helper that failed to compile, a truncated
# pipe) must never read as permission having been granted.
classify_readiness() {
    local output=$1 trusted post_events locked
    trusted=$(tsv_field "$output" trusted)
    post_events=$(tsv_field "$output" post-events)
    locked=$(tsv_field "$output" screen-locked)

    if [[ -z $trusted || -z $post_events || -z $locked ]]; then
        printf 'unreadable\n'
    elif [[ $trusted != 1 ]]; then
        printf 'not-trusted\n'
    elif [[ $post_events != 1 ]]; then
        printf 'cannot-post-events\n'
    elif [[ $locked == 1 ]]; then
        printf 'screen-locked\n'
    else
        printf 'granted\n'
    fi
}

# tsv_field <output> <name>
#
# The value of one tab-separated line, or nothing. Every subcommand of the
# accessibility helper answers in this shape, so the harness and the drivers
# read it the same way and there is one copy of the reader to be wrong -- the
# tested one. A value may contain spaces (an accessibility description is a
# sentence), which is why the field separator is a tab and why this is `awk`
# rather than `read` or `cut -d' '`.
tsv_field() {
    printf '%s\n' "$1" | awk -F'\t' -v key="$2" '$1 == key { print $2; exit }'
}

# readiness_message <classification>
#
# What the run says on its way out. Each one names the permission by the name
# it has in System Settings -- the string somebody has to search for -- and the
# README section that says how it was granted the first time. A message that
# says only "permission denied" sends the reader to the wrong pane: this Mac
# has two TCC grants in play, Accessibility and Automation, and the witness
# needs the first and deliberately avoids the second.
readiness_message() {
    case $1 in
    not-trusted)
        printf '%s\n' \
            "the terminal running this witness is not trusted for Accessibility." \
            "  Grant it in System Settings > Privacy & Security > Accessibility," \
            "  then start a new shell (the trust is read at process start)." \
            "  See ${WITNESS_README_SECTION}."
        ;;
    cannot-post-events)
        printf '%s\n' \
            "this process may not post keyboard events (System Settings >" \
            "  Privacy & Security > Accessibility covers both; a separate" \
            "  Input Monitoring entry can also refuse it)." \
            "  See ${WITNESS_README_SECTION}."
        ;;
    screen-locked)
        printf '%s\n' \
            "the screen is locked. A synthetic keystroke does not reach an" \
            "  application behind the lock screen, and the window server answers" \
            "  every third-party window query with the application element" \
            "  instead of the window -- so a driver would read an empty tree and" \
            "  call it a failure of the app. Unlock the Mac and run this again." \
            "  See ${WITNESS_README_SECTION}."
        ;;
    unreadable)
        printf '%s\n' \
            "the accessibility probe printed nothing this harness understands." \
            "  Run 'ax probe' by hand (the compiled helper's path is above) and" \
            "  see ${WITNESS_README_SECTION}."
        ;;
    granted)
        printf '%s\n' "the desktop is drivable."
        ;;
    *)
        printf '%s\n' "unknown readiness classification: $1"
        ;;
    esac
}
