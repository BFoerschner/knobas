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

# registered_copies <lsregister -dump text> <bundle id>
#
# Every path the dump records for a bundle carrying <bundle id>, one per line,
# sorted and unique. A dump is records separated by a line of dashes; a record
# prints `path:` before `identifier:`, so the path is remembered until the
# identifier line says whether it counts. The ` (0x...)` Launch Services
# appends to a path is dropped. A stale entry whose bundle is gone is still
# listed: it is Launch Services' record, and the caller decides about disk.
registered_copies() {
    printf '%s\n' "$1" | awk -v id="$2" '
        /^-+$/ { path = ""; next }
        /^path:/ {
            path = $0
            sub(/^path:[ \t]*/, "", path)
            sub(/ \(0x[0-9a-f]+\)$/, "", path)
            next
        }
        /^identifier:/ {
            val = $0
            sub(/^identifier:[ \t]*/, "", val)
            if (val == id && path != "") print path
        }' | sort -u
}

# copies_to_unregister <dump> <bundle id> <keep> <repo root>
# copies_to_leave      <dump> <bundle id> <keep> <repo root>
#
# The opt-in step's decision (#1176). Of the copies Launch Services knows,
# <keep> (the bundle under test) is neither; a copy inside <repo root> (the
# root checkout and its .worktrees/) is ours to unregister for the length of
# the run; anything else -- /Applications, a DMG mount, somebody's scratch
# directory -- is not this repo's to touch, is reported, and stays registered.
# The root test is on whole path components, so `/x/knobas-old/...` is not
# inside `/x/knobas`.
copies_to_unregister() {
    local keep root path
    keep=$(normalise_app_path "$3")
    root=$(normalise_app_path "$4")
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        path=$(normalise_app_path "$path")
        [ "$path" = "$keep" ] && continue
        case $path in
        */../* | */..) ;; # a `..` segment could climb out of the root: never ours
        "$root"/*) printf '%s\n' "$path" ;;
        esac
    done < <(registered_copies "$1" "$2")
}

copies_to_leave() {
    local keep root path
    keep=$(normalise_app_path "$3")
    root=$(normalise_app_path "$4")
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        path=$(normalise_app_path "$path")
        [ "$path" = "$keep" ] && continue
        case $path in
        */../* | */..) printf '%s\n' "$path" ;;
        "$root"/*) ;;
        *) printf '%s\n' "$path" ;;
        esac
    done < <(registered_copies "$1" "$2")
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
# Written beside the record and renamed onto it, so the record appears whole
# or not at all. `printf` may write each argument's line separately, and a
# reader that caught `--wait` alone from a command wrongly given two arguments
# would see one argument and pass. The Rust twin, `recording_stub` in
# `crates/knobas-app/tests/it/checkout_ipc.rs`, writes its record the same way
# and failed the gate once before it did (#583).
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
printf '%s\n' "\$@" > '$1.tmp'
mv '$1.tmp' '$1'
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
# What makes a record whole is `stub_script`'s rename (#583); this is what a
# whole record looks like, and the same condition `recorded()` waits for in
# `crates/knobas-app/tests/it/checkout_ipc.rs`. What half a record produces is
# a comparison against half a line, reported as *the command was not given the
# checkout path* -- the feature blamed for a race in the harness, at the one
# moment the witness matters (#531). The newline alone cannot rule that out: a
# two-line record half written ends in one too.
#
# `-s` as well as the newline test, and it is load-bearing: `tail -c 1` of an
# empty file prints nothing, which is the same answer it gives for a file that
# ends in a newline. Without it an empty file -- what a stub writing straight
# to the record leaves between its open and its write -- would read as a
# finished record.
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

# witness_guard
#
# The seconds a wait on the app runs before it fails: KNOBAS_WITNESS_WAIT_S,
# 60 by default (#1961). The guard on the harness's wait for the app's process
# after `open`, on `ax activate`, and on every wait its drivers and live-shot
# make on the app: a screen settling after a keystroke, an element a step
# reaches for. (The harness's window and first draw keep WINDOW_TIMEOUT, 180 s,
# which covers a first-run PostgreSQL provision.) A loaded machine (gates and other agents'
# builds) draws a screen slowly, and the old fixed budgets of 5 to 10 s failed
# runs with nothing wrong in the app; the guard is sized past that, and an app
# that has exited fails a wait at once (`witness_poll`), so the guard costs a
# real fault nothing. Whole seconds only: anything else is refused, exit 2,
# naming the variable, rather than read as some other number.
witness_guard() {
    local s=${KNOBAS_WITNESS_WAIT_S:-60}
    case $s in
    '' | 0* | *[!0-9]*)
        printf "KNOBAS_WITNESS_WAIT_S must be a whole number of seconds, got '%s'\n" "$s" >&2
        return 2
        ;;
    esac
    printf '%s\n' "$s"
}

# witness_poll <pid> <seconds> <predicate...>
#
# Run the predicate until it holds (0), the process <pid> has exited (3), or
# <seconds> have passed (1). The predicate runs in this shell, so one that
# keeps what it last saw in a variable leaves it for the caller's message. It
# is asked before each check of the process and the guard, so a state reached
# during the last sleep counts as reached.
witness_poll() {
    local pid=$1 seconds=$2
    shift 2
    local deadline=$((SECONDS + seconds))
    while :; do
        "$@" && return 0
        kill -0 "$pid" 2>/dev/null || return 3
        [ "$SECONDS" -lt "$deadline" ] || return 1
        sleep 0.2
    done
}

# --- the PostgreSQL a killed app leaves (#1967) -----------------------------
#
# The scripts that launch knobas quit it politely when they can (⌘Q, which
# stops the embedded PostgreSQL on the way out) and kill it when they cannot.
# A killed knobas runs no exit handler, so its server stays up on the profile's
# data directory, parented to pid 1, until something stops it; the app's next
# launch adopts it (`knobas_db::EmbeddedDb::start`), which is the product's
# answer to a crash and not a script's to rely on. These are what each script
# runs after the app has gone, killed or not. Unlike the rest of this file,
# `stop_left_postgres` acts (it signals and waits); the test drives it against
# stand-in postmasters. Sourced by `scripts/single-instance-witness.sh` too.

# demo_data_dir [home]
#
# The demo profile's PostgreSQL data directory under <home> ($HOME by
# default): `app_data_dir` for `dev.knobas.desktop`, then `Profile::new`'s
# `demo`, `db_root`'s `db` and `postgresql_embedded`'s `data`.
# `desktop-witness-test.sh` pins the three files that say so.
demo_data_dir() {
    printf '%s/Library/Application Support/dev.knobas.desktop/demo/db/data\n' "${1:-$HOME}"
}

# pg_path <data dir>
#
# <data dir> spelled as `pg_ctl` hands it to the postmaster, which is what its
# command line shows: repeated slashes collapsed, `/./` dropped, no trailing
# slash (`canonicalize_path`; symlinks are not resolved). A `$TMPDIR` that ends
# in `/` gives `.../T//x`, and the postmaster on it says `.../T/x`.
pg_path() {
    printf '%s\n' "$1" | sed -e 's|//*|/|g' -e 's|/\./|/|g' -e 's|\(.\)/$|\1|'
}

# profile_postmaster <data dir>
#
# The pid of the PostgreSQL serving <data dir>, or nothing: the pid on the
# first line of its `postmaster.pid`, when that process is alive and its
# command line is a `postgres -D <data dir>`. A lock naming a dead pid is
# stale, and a live pid that is anything else (one recycled after a reboot,
# another cluster's server) is not this directory's to stop.
profile_postmaster() {
    local data pid args
    data=$(pg_path "$1")
    [ -r "$data/postmaster.pid" ] || return 0
    pid=$(head -n 1 "$data/postmaster.pid" 2>/dev/null) || return 0
    case $pid in '' | *[!0-9]*) return 0 ;; esac
    args=$(ps -o args= -p "$pid" 2>/dev/null) || return 0
    case "$args " in
    *"postgres -D $data "*) printf '%s\n' "$pid" ;;
    esac
}

# starts_in_flight <data dir> <postmaster pid>
#
# The pids of the PostgreSQL tools still working on <data dir> other than its
# postmaster: an `initdb` or `pg_ctl` the app started, which a kill does not
# stop, or a postmaster `pg_ctl start` has not yet recorded. The directory is
# matched as a literal string (ENVIRON, not `awk -v`, so this `awk` is not in
# its own answer).
starts_in_flight() {
    ps -A -o pid= -o args= 2>/dev/null | DATA=$(pg_path "$1") SKIP=$2 awk '
        index($0, ENVIRON["DATA"]) && $1 != ENVIRON["SKIP"] && /initdb|pg_ctl|postgres/ { print $1 }'
}

# stop_left_postgres <data dir> <pid before launch> <seconds>
#
# Stop the PostgreSQL the app left on <data dir>. Run once the app is gone.
# First the starts it left in flight are waited out (up to <seconds>), so a
# server coming up behind a kill mid bring-up is stopped rather than missed.
# Then the postmaster `profile_postmaster` finds gets a fast shutdown (SIGINT,
# what `pg_ctl stop -m fast` sends) and, if it is still up after <seconds>, an
# immediate one (SIGQUIT, the app's own fallback). A postmaster that was
# serving <data dir> before the launch (<pid before launch>, from
# `profile_postmaster` then) is not this run's and is left as it was.
#
# 0 when nothing of this run's is left; 1, naming the pids and the hand stop
# on stderr, when a start outlived the guard or the postmaster both signals. Says what it stopped on stdout,
# and nothing at all when there was nothing.
stop_left_postgres() {
    local data before=$2 seconds=$3 deadline pid starting
    data=$(pg_path "$1")
    deadline=$((SECONDS + seconds))
    while starting=$(starts_in_flight "$data" "$(profile_postmaster "$data")") && [ -n "$starting" ]; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            printf 'a PostgreSQL is still starting on %s after %s s (pids %s): stop them, then pg_ctl -D "%s" stop\n' \
                "$data" "$seconds" "$(printf '%s' "$starting" | tr '\n' ' ')" "$data" >&2
            return 1
        fi
        sleep 0.2
    done
    pid=$(profile_postmaster "$data")
    [ -n "$pid" ] || return 0
    if [ "$pid" = "$before" ]; then
        printf 'left alone: pid %s was serving %s before this run\n' "$pid" "$data"
        return 0
    fi
    local signal
    for signal in INT QUIT; do
        kill -"$signal" "$pid" 2>/dev/null || true
        deadline=$((SECONDS + seconds))
        while kill -0 "$pid" 2>/dev/null && [ "$SECONDS" -lt "$deadline" ]; do sleep 0.2; done
        if ! kill -0 "$pid" 2>/dev/null; then
            printf 'stopped the PostgreSQL the app left on %s (pid %s, SIG%s)\n' "$data" "$pid" "$signal"
            return 0
        fi
    done
    printf 'the PostgreSQL the app left is still running on %s (pid %s): pg_ctl -D "%s" stop -m immediate\n' \
        "$data" "$pid" "$data" >&2
    return 1
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

# (`rendered_label`, which uppercased the rooms UI's `.lab` names the way WebKit
# renders them, went with #1084: the ticket-first shell transforms no name a
# driver uses, and `witness-unit` pins that `tf.css` keeps it so.)

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

# notes_total <ax values output>
#
# How many notes Settings › Notes and capture says there are -- a number, or
# `x` when the screen carries no notes line, or more than one (#1084).
#
# The line is `notesLine`'s (`app/src/lib/tf/settings/notes.ts`): *No notes*,
# *1 note, from 18 October*, *37 notes, the last from 12 August*. Whole lines
# only, so *Move 3 notes* on a button is not a count. **`x` and not 0**, for
# `count_of`'s reason: the capture driver asserts the count went up by one,
# and an unreadable screen read as 0 would be a count it compares against.
notes_total() {
    printf '%s\n' "$1" | awk '
        { line = $0; sub(/^[ \t]+/, "", line); sub(/[ \t]+$/, "", line) }
        line == "No notes" { n = 0; seen++; next }
        line ~ /^[0-9]+ notes?(, .*)?$/ { split(line, word, " "); n = word[1] + 0; seen++ }
        END { if (seen == 1) print n; else print "x" }
    '
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

# lands <landing> <reading...>
# lands_named <landing> <name...>
#
# Wait until the page a step lands on reads every <reading> as a whole line
# (`ax values`), or carries every <name> (`ax find`). Run from a driver: it
# uses the driver's `$ax`, `$pid` and `wait_until`.
#
# **Each call is held to the app by a DOM test** (#1968). No gate runs a
# driver, so when an app change moved where a step lands, the driver kept
# waiting for the old page and the next run went red on a correct app
# (`capture.sh` after #1504). So a driver waits on a landing only through
# these two, with `readonly` constants for arguments: `app/src/lib/shell/
# test-witness.ts` reads the call out of the driver, a test in
# `app/src/App.test.svelte.ts` replays the step and asserts the page draws
# it, and `desktop-witness-test.sh` refuses a landing that test lacks.
lands() {
    local landing=$1 reading
    shift
    for reading in "$@"; do
        wait_until "the step did not land on $landing: the screen does not read '$reading'" \
            lands_reading "$reading"
    done
}
lands_named() {
    local landing=$1 name
    shift
    for name in "$@"; do
        wait_until "the step did not land on $landing: nothing on screen is called '$name'" \
            lands_name "$name"
    done
}
# Each leaves what it last saw in `seen`, which the driver's `wait_until`
# names when the guard runs out.
lands_reading() {
    # shellcheck disable=SC2034 # the driver's wait_until reads it
    seen="no line reads '$1'"
    # shellcheck disable=SC2154 # $ax and $pid are the driver's
    has_reading "$("$ax" values "$pid" 2>/dev/null || true)" "$1"
}
lands_name() {
    local n
    # shellcheck disable=SC2154 # $ax and $pid are the driver's
    n=$("$ax" find "$pid" "$1" 2>/dev/null || true)
    # shellcheck disable=SC2034 # the driver's wait_until reads it
    seen="'$1' counted ${n:-nothing}"
    case $n in '' | *[!0-9]*) return 1 ;; esac
    [ "$n" -gt 0 ]
}

# landings_of <driver source>
#
# The landings a driver waits on, one per line, each once: the first word
# after every `lands` or `lands_named` call that opens a line.
landings_of() {
    printf '%s\n' "$1" | awk '
        $1 == "lands" || $1 == "lands_named" { if (!seen[$2]++) print $2 }
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

# --- counting windows rather than names (issue #548) ------------------------
#
# The other half of what makes a wait a witness, one attribute over. #547 was a
# wait on a name the accessibility API never answers with; #548 is a wait on a
# name it answers with **three times**, all of them right. `capture.sh` asked
# `ax find` for `exactly_one` element called `Capture` and the capture window
# carries that word on its `AXWindow` (`capture.rs`'s `.title`), its `AXWebArea`
# (`capture.html`'s `<title>`) and its `AXTextArea` (`CaptureWindow.svelte`'s
# `aria-label`) -- so the count was 3 for as long as the window was up and the
# driver reported the window it was looking at as never having appeared.
#
# The lesson is not "count differently". It is that *is that window open?* is a
# question about **windows**, and a count of named elements is a representation
# of it: widening `exactly_one` to `present` would have gone green here and gone
# green just as well on a main window that had grown an element of that name
# with no capture window anywhere. `ax windows` asks the question directly, and
# these two functions are the reading of its answer.

# window_count <ax windows output> <title>
#
# How many of the application's windows are called <title> -- a number, or `x`
# when the answer cannot be read.
#
# **`x` and not 0**, for the reason `count_of`'s sentinel exists in the drivers:
# the helper prints nothing when it can see no window, which is a locked screen,
# a quit app or a pid that was never an application, and `capture.sh`'s closing
# assertion is that the capture window has **gone**. Read as 0, an app that died
# mid-run would satisfy it -- the check that cannot fail, one line after the
# check that matters.
#
# Whole-**field** equality on the title, read out of the tab-separated line
# rather than matched over it with `grep`: a window called `Capture notes` is
# not the capture window, and a substring test is how the wrong window answers
# for the right one.
window_count() {
    printf '%s\n' "$1" | awk -F'\t' -v want="$2" '
        $1 == "window" { seen = 1; if ($2 == want) found++ }
        END { if (seen) print found + 0; else print "x" }
    '
}

# count_verdict <count>
#
# A count turned into the word `waypoint_verdict` reads: `yes`, `no` or
# `unreadable`.
#
# The bridge between a count and a waypoint, so that a driver counting windows
# and a driver reading lines refuse on the same rule. `x` -- and anything else
# that is not a number -- is `unreadable` rather than `no`, which is the whole
# reason a count comes back as `x` in the first place.
#
# **Two functions where `reading_verdict` is one**, and the number is why: a
# line is on the screen or it is not, and there is nothing else to ask about it,
# while a count of windows is also the answer to *how many*, which `capture.sh`
# asserts separately from the waypoint. Folding the count into the verdict would
# throw away the only number in the answer.
#
# **How many is not this function's question.** Any number above zero is `yes`;
# whether *exactly* one window of that name is open is a separate assertion, and
# the driver makes it separately, because two capture windows and none are two
# different faults and folding them into one word would lose that.
count_verdict() {
    case $1 in
    '' | *[!0-9]*) printf 'unreadable\n' ;;
    0) printf 'no\n' ;;
    *) printf 'yes\n' ;;
    esac
}

# key_code <key>
#
# The macOS virtual key code `ax key` takes, for a key `testenv/live-shot.sh`'s
# `key` command names (#1483): a letter or digit (ANSI layout, the codes are
# physical positions), return, escape, tab, space, delete, an arrow, or a code
# given as a number. Nothing for any other word, and status 1.
key_code() {
    local pair
    for pair in a:0 s:1 d:2 f:3 h:4 g:5 z:6 x:7 c:8 v:9 b:11 q:12 w:13 e:14 r:15 \
        y:16 t:17 1:18 2:19 3:20 4:21 6:22 5:23 9:25 7:26 8:28 0:29 o:31 u:32 \
        i:34 p:35 l:37 j:38 k:40 n:45 m:46 return:36 enter:36 tab:48 space:49 \
        delete:51 backspace:51 escape:53 left:123 right:124 down:125 up:126; do
        [ "${pair%%:*}" = "$1" ] && { echo "${pair#*:}"; return 0; }
    done
    case $1 in *[!0-9]* | '') return 1 ;; *) echo "$1" ;; esac
}

# --- the window's size, before any driver or steps file (#2056) --------------

# The narrowest window, in points, that the drivers and live-shot run in.
# Tickets hides its preview at 1240 px and below (`@media (max-width: 1240px)`
# in `app/src/lib/tf/css/tickets.css`, pinned by `desktop-witness-test.sh`),
# and the capture driver opens its job from that preview. A window is as wide
# as its web view on macOS, so points here are CSS pixels there.
readonly WITNESS_MIN_WIDTH=1241

# window_width_verdict <width> <height> <minimum width>
#
# `ok`, or the refusal naming the window's size and the minimum (status 1).
window_width_verdict() {
    case $1 in
    '' | *[!0-9]*)
        printf "could not read the window's size (got '%s')\n" "$1"
        return 1
        ;;
    esac
    if [ "$1" -ge "$3" ]; then
        printf 'ok\n'
        return 0
    fi
    printf '%s\n' "the window is $1x$2 points, narrower than the $3 the drivers need: below it Tickets hides its preview. Give it more room (an empty workspace, or a wider screen) and run again."
    return 1
}

# witness_window_size <ax> <pid> [settle seconds, default 3]
#
# The one window-size step, run by the harness before its driver and by
# live-shot before its steps file: a tiling window manager gives a new window
# whatever share of the screen is left. Under AeroSpace (its CLI answers) the
# window is fullscreened by its CG window id, which AeroSpace uses as its own,
# whatever its width, so it also holds still; the fullscreen ends with the
# window. Elsewhere a window narrower than WITNESS_MIN_WIDTH is widened through
# the accessibility helper. Then the size is read again, for up to the settle
# seconds while the window manager applies it, and printed; below the minimum
# the last line is the refusal and the status is 1.
witness_window_size() {
    local ax=$1 pid=$2 settle=${3:-3} id width height
    read -r id width height < <("$ax" window-id "$pid" 2>/dev/null) \
        || { printf 'knobas has no window on screen to size\n'; return 1; }
    printf 'window %s: %sx%s points as the window manager placed it\n' "$id" "$width" "$height"
    if command -v aerospace >/dev/null && aerospace list-workspaces --focused >/dev/null 2>&1; then
        aerospace fullscreen on --window-id "$id" --no-outer-gaps >/dev/null 2>&1 \
            || { printf 'AeroSpace refused to fullscreen window %s\n' "$id"; return 1; }
        printf 'AeroSpace: window %s fullscreen\n' "$id"
    elif [ "$width" -lt "$WITNESS_MIN_WIDTH" ]; then
        if "$ax" resize "$pid" "$WITNESS_MIN_WIDTH" "$height" >/dev/null 2>&1; then
            printf 'widened window %s from %s to %s points\n' "$id" "$width" "$WITNESS_MIN_WIDTH"
        else
            printf 'could not widen window %s through accessibility\n' "$id"
        fi
    fi
    local deadline=$((SECONDS + settle)) verdict
    while :; do
        read -r id width height < <("$ax" window-id "$pid" 2>/dev/null) || width=
        verdict=$(window_width_verdict "$width" "$height" "$WITNESS_MIN_WIDTH") && break
        [ "$SECONDS" -lt "$deadline" ] || { printf '%s\n' "$verdict"; return 1; }
        sleep 0.2
    done
    printf 'window: %sx%s points (the drivers need %s wide)\n' "$width" "$height" "$WITNESS_MIN_WIDTH"
}
