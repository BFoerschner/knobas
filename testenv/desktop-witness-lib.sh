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
    # Collapse runs of separators, then drop trailing ones -- but never the
    # last character, so `/` normalises to `/` and not to the empty string.
    # `/` is not a bundle path; that guard is against a caller that passes one,
    # not a case the harness reaches.
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
# The order below is the order the branches are written in, and it is by what
# a person would have to do about it rather than by severity: a missing
# Accessibility grant is a one-time visit to System Settings, losing the right
# to post events is a second pane to go and look at, and a locked screen is
# only a password -- which is why it is reported last, since it will be gone by
# the time anyone reads a message about anything else.
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

# --- the open-in-editor driver's pure parts (issue #501) --------------------
#
# Here rather than in the driver for the reason the two functions above are:
# everything in a driver either types at a real window or reads a real
# accessibility tree, and the only witness for that is an unlocked Mac. What is
# left over -- the text of a stub, the text of a git config, and the reading of
# what the stub recorded -- is text in and text out, and `witness-unit` runs it
# on every gate.

# stub_script <record-path>
#
# An executable that writes the arguments it was given, one per line, and
# exits. The program `open-in-editor` points a command template at, so that
# "what did knobas pass?" has an answer on disk.
#
# One argument per line, because the count is half of what is being witnessed:
# a space-joined line could not tell one argument holding a space from two
# arguments, and "the checkout path and nothing else" is a claim about both.
#
# The record path is baked in rather than taken from the environment: the
# template the app stores is a command line, the app spawns it with no shell
# and passes on none of this shell's variables, so an `$RECORD` in here would
# be empty at the moment it mattered.
stub_script() {
    # An unquoted heredoc, so `$1` is this function's argument while the
    # `\n` and the `"\$@"` inside reach the file as written.
    cat <<EOF
#!/bin/sh
printf '%s\n' "\$@" > '$1'
EOF
}

# path_is_one_word <path>
#
# Whether a path can go into a command template unquoted -- which is the only
# way a driver may put one there.
#
# **Because a driver may not type a quote.** Measured on the dev Mac,
# 2026-09-08: macOS' *Smart Quotes* substitution rewrites a typed `"` as `“`
# and `”` on its way into a WebKit field, so `open-in-editor`'s quoted template
# reached the database as `“/var/…/stub” {path}` and knobas refused to run a
# program by that name -- correctly, while the driver reported the feature as
# broken. `ax type` posts characters the way a person's keyboard does, and the
# input stack rewrites some of them; the harness's answer is to type none of
# those, and to check rather than hope that it did not have to.
#
# The three the template's own grammar cannot survive unquoted: whitespace,
# which is where `knobas_core::checkout::expand` splits words, and either
# quote, which is what it groups them with. An empty path is refused too --
# there is no word in it at all.
path_is_one_word() {
    case $1 in
    '' | *[[:space:]]* | *'"'* | *"'"*) return 1 ;;
    esac
    return 0
}

# git_config <remote>
#
# The `.git/config` of a clone whose `origin` is <remote>. What the scan reads
# (`knobas_core::checkout::origin_url`), and the whole of what makes a
# directory a checkout as far as knobas is concerned -- no `git init` is run,
# because nothing in this feature runs git (ADR-0016) and a driver that did
# would be witnessing git's behaviour rather than knobas'.
git_config() {
    printf '[core]\n\tbare = false\n[remote "origin"]\n\turl = %s\n' "$1"
}

# record_is_complete <record-path>
#
# Whether the stub has finished writing: something is there, and it ends in a
# newline.
#
# The newline and not merely a non-empty file, because the newline is what
# `recorded()` waits for in `crates/knobas-app/tests/checkout_ipc.rs`, and the
# two are watching the same stub write the same file in the same shape. The
# stub's `> file` is one open and one write, so half a record is unlikely
# rather than impossible; what half a record produces is a comparison against
# half a line, reported as *the command was not given the checkout path* -- the
# feature blamed for a race in the harness, at the one moment the witness
# matters (#531).
#
# `-s` as well as the newline test, and it is load-bearing: `tail -c 1` of an
# empty file prints nothing, which is the same answer it gives for a file that
# ends in a newline. Without it the empty file the stub's `> file` leaves
# between its open and its write would read as a finished record.
record_is_complete() {
    [ -s "$1" ] && [ -z "$(tail -c 1 "$1")" ]
}

# wait_for_record <record-path> <seconds>
#
# Poll until the record is complete, or give up after <seconds>. True when it
# arrived.
#
# A deadline rather than a sleep, which is the shape `recorded()` polls in and
# for its reason: the IPC call behind the button answers when the program has
# *started*, which is the only thing a call that does not wait for a process
# can promise, so what is waited on is the stub's own output.
#
# The condition is read once more after the deadline, so a record that landed
# during the last sleep is a record rather than a failure reported one poll too
# early. That final read is also what makes a budget of 0 mean *look once*,
# which is what `witness-unit` asks it for.
wait_for_record() {
    local path=$1 seconds=$2
    local deadline=$((SECONDS + seconds))
    while [ "$SECONDS" -lt "$deadline" ]; do
        record_is_complete "$path" && return 0
        sleep 0.2
    done
    record_is_complete "$path"
}

# sole_argument <recorded text>
#
# The one argument the stub recorded, or nothing at all.
#
# Nothing for **none** and nothing for **two**, and that is the point: a driver
# that read the first line would pass on a command that had been handed the
# checkout path *and* something else, which is the failure ADR-0016 is about.
# An empty answer is what the driver reports as a failure, and it prints the
# whole recording beside it.
sole_argument() {
    printf '%s' "$1" | awk 'NF { lines[++n] = $0 } END { if (n == 1) print lines[1] }'
}

# rendered_label <visible text>
#
# The accessible name an element has when its text is styled `.lab` or sits in a
# `.row.hd` -- **the uppercase form**, because both carry
# `text-transform: uppercase` in `app/src/app.css` and WebKit names an element
# by what is *rendered*, not by what is in the markup.
#
# **Measured on the dev Mac, 2026-09-08**, in the first run of this harness that
# got past its probe. The settings field whose markup reads
# `<label class="lab" for="capture-shortcut">Shortcut</label>` arrives as
# `AXTextField title=SHORTCUT`, and the links panel's group heading -- a bare
# `<span>` inside `.row.hd` -- as `CAPTURED FROM`. An `aria-label` is **not**
# transformed, because it is not rendered text: `Save capture shortcut` arrives
# spelled exactly as it is written.
#
# Here rather than as an uppercase constant in each driver so that the rule is
# stated once and the drivers keep the source spelling visible; `witness-unit`
# pins both `text-transform` rules, so a stylesheet that stopped uppercasing
# turns the gate red instead of failing a run minutes in.
#
# ASCII only, which every one of these labels is. A `tr` over a wider alphabet
# would be a promise this cannot keep.
rendered_label() {
    printf '%s\n' "$1" | tr '[:lower:]' '[:upper:]'
}

# --- the capture driver's pure parts (issue #503) ---------------------------
#
# Here for the reason everything above is: what is left over once the typing and
# the accessibility reads are taken out is text in and text out, and
# `witness-unit` runs it on every gate.

# capture_title <typed text>
#
# The title the capture gives its note: the **first non-blank line**, trimmed.
#
# The shell's copy of `app/src/lib/capture/capture.svelte.ts`'s `split`, and it
# is a copy on purpose: the driver types a paragraph at a real window and has to
# know what the note is then called, and asking the frontend would be the driver
# reading the answer off the thing it is testing. Two implementations of one rule
# is a cost this pays knowingly, and `witness-unit` runs them against the same
# cases so a change to one is a red gate rather than a run that fails minutes in
# with a title nobody can explain.
capture_title() {
    printf '%s\n' "$1" | awk 'NF { sub(/^[ \t]+/, ""); sub(/[ \t]+$/, ""); print; exit }'
}

# has_reading <ax values output> <reading>
#
# Whether the screen carries <reading> as a **whole line**.
#
# Whole line, and that is the entire content of this function. The links panel
# groups by a relation's *reading*, and every reading a capture draws has a
# longer one containing it -- `captured in` / `captured here`, and, the one that
# matters, `captured from` / `captured from here`. Those longer forms are the
# readings from the **other** end of the same link, which is what a *context's*
# panel shows about a note. A substring test would therefore let a driver
# standing on the wrong detail report a pass, and it is precisely the class of
# check this milestone keeps catching: one that measures a representation of the
# thing rather than the thing.
#
# `awk` over a whole line with the surrounding whitespace stripped, because a
# rendered line arrives with whatever the layout put around it and none of that
# is a difference in what it says.
has_reading() {
    printf '%s\n' "$1" | awk -v want="$2" '
        { line = $0; sub(/^[ \t]+/, "", line); sub(/[ \t]+$/, "", line) }
        line == want { found = 1 }
        END { exit found ? 0 : 1 }
    '
}

# frontmost_bundle <ax frontmost output>
#
# The bundle identifier of whatever has the screen, or nothing.
#
# One line of the same tab-separated shape every `ax` subcommand answers in, so
# there is one reader of it and it is the tested one.
frontmost_bundle() {
    tsv_field "$1" bundle
}

# --- what makes a wait a witness (issue #547) -------------------------------
#
# A driver waits for the screen to say something, and then reports the step
# before the wait as having happened. That reasoning holds only if the screen
# did **not** already say it -- a wait that was true one keystroke earlier
# witnesses nothing, and passes just as green on a step that did nothing at
# all. #547 is the other half of the same class: a wait that could never be
# true, on a name the accessibility API does not answer with, which reports a
# working feature as broken. The two functions below are the rule for both, in
# the one place a driver can be tested without a screen.

# reading_verdict <ax values output> <reading>
#
# What the screen says about one whole line: `yes`, `no` or `unreadable`.
#
# **Three answers and not two**, which is the whole reason this is not a bare
# `has_reading`. `ax values` prints nothing at all when it can see no window --
# a locked screen, a quit app, a helper that failed -- and exits 0 either way,
# so an empty answer compared with "does it contain this line" is *no*. Read as
# *no*, that empty answer is a **pass** for the before-half of a waypoint: the
# check that is supposed to prove a wait was false beforehand would be
# satisfied by a helper that had stopped working, which is exactly the check
# that cannot fail. `count_of`'s `x` sentinel in the drivers is the same guard
# on the same hazard, one attribute over.
#
# Nothing at all, rather than whitespace-only: `$( )` strips trailing newlines,
# and `ax values` drops empty values, so a screen with anything written on it
# answers with a non-empty string.
reading_verdict() {
    local values=$1 reading=$2
    if [ -z "$values" ]; then
        printf 'unreadable\n'
        return
    fi
    if has_reading "$values" "$reading"; then
        printf 'yes\n'
    else
        printf 'no\n'
    fi
}

# waypoint_verdict <before> <after>
#
# Whether a wait witnessed the step it follows, given what the screen said just
# before that step and just after it. One word: `witnessed`, `too-early`,
# `never-appeared` or `unreadable`.
#
# The order of the branches is the order of what a reader has to do about it,
# and `too-early` outranks everything the after-reading could say. A waypoint
# that was already true is not a weaker witness than one that never arrived --
# it is not a witness at all, and it is the failure that hides, because the run
# it makes is *green*. Which of the two directions the after-reading then took
# is a detail of a wait that had already stopped meaning anything.
#
# `unreadable` is its own answer rather than folded into `never-appeared`, for
# the reason `classify_readiness` keeps it: a helper that answered nothing must
# never read as a measurement, in either direction.
waypoint_verdict() {
    case $1:$2 in
    yes:*) printf 'too-early\n' ;;
    no:yes) printf 'witnessed\n' ;;
    no:no) printf 'never-appeared\n' ;;
    *) printf 'unreadable\n' ;;
    esac
}
