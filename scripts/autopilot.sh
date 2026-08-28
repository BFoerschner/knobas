#!/usr/bin/env bash
# autopilot.sh -- implement ready-for-agent issues unattended, one issue at a
# time, each in a FRESH `claude -p` session, until the queue is empty or an
# issue genuinely needs Björn.
#
# A fresh session per issue is the point, not an implementation detail: it is
# what "clear the context between issues" means in practice. The driver holds
# no state worth keeping -- everything a session needs (the flow, the standing
# rules, the machine-earned limits) loads from CLAUDE.md, the project memory,
# and docs/agents/working-model.md, and everything it produces lands in git
# and on the issue. Serial by design: the working model caps this machine at
# two concurrent Rust workers, and migrations are the #1 collision source.
#
# Usage:
#   scripts/autopilot.sh                 the whole queue, ascending
#   scripts/autopilot.sh 52 53 54        exactly these, in this order
#   scripts/autopilot.sh --dry-run ...   show what would run, run nothing
#
# An issue is runnable when it is open, labelled ready-for-agent, every issue
# in its blocked-by list is closed, and it is not an umbrella with open
# sub-issues. Non-runnable issues are skipped with a reason, not fatal.
#
# The ONLY designed stop: a session that hits a real decision comments the
# question on its issue and swaps ready-for-agent -> ready-for-human. The
# driver prints that comment and exits 2. Exit 0: queue drained. Exit 3: a
# session ended without either closing its issue or escalating -- inspect the
# log before rerunning; the driver never retries on its own.
set -euo pipefail

dry_run=false
if [ "${1:-}" = "--dry-run" ]; then
    dry_run=true
    shift
fi

root="$(git rev-parse --show-toplevel)"
cd "$root"

# One autopilot per machine: the sessions it spawns each build the workspace
# and start embedded Postgres servers, and two drivers would race on main.
lock="${TMPDIR:-/tmp}/knobas-autopilot.lock"
if ! mkdir "$lock" 2>/dev/null; then
    echo "another autopilot appears to be running (lock: $lock)" >&2
    echo "remove the directory if you are sure it is stale" >&2
    exit 1
fi
trap 'rmdir "$lock"' EXIT

logdir="${TMPDIR:-/tmp}/knobas-autopilot-logs"
mkdir -p "$logdir"
echo "session logs: $logdir"

# The blocked-by list and the sub-issue tally are only in the raw view, not in
# --json fields, so both readers parse the tab-separated header lines.
field() { # field <issue> <name>  -> the raw header line's value
    gh issue view "$1" | awk -F'\t' -v k="$2:" '$1==k{print $2}'
}

# Why not runnable, or nothing if it is.
skip_reason() {
    local n="$1" state labels done_tally b
    state="$(gh issue view "$n" --json state --jq .state)"
    [ "$state" = "CLOSED" ] && { echo "already closed"; return; }
    labels="$(gh issue view "$n" --json labels --jq '[.labels[].name]|join(" ")')"
    case " $labels " in
        *" ready-for-agent "*) ;;
        *) echo "not labelled ready-for-agent ($labels)"; return ;;
    esac
    done_tally="$(field "$n" sub-issues-completed)"
    if [ -n "$done_tally" ] && [ "${done_tally%/*}" != "${done_tally#*/}" ]; then
        echo "umbrella with open sub-issues ($done_tally)"
        return
    fi
    for b in $(field "$n" blocked-by | grep -oE '#[0-9]+' | tr -d '#'); do
        if [ "$(gh issue view "$b" --json state --jq .state)" != "CLOSED" ]; then
            echo "blocked by open #$b"
            return
        fi
    done
}

queue() {
    if [ "$#" -gt 0 ]; then
        printf '%s\n' "$@"
    else
        gh issue list --label ready-for-agent --state open \
            --json number --jq 'sort_by(.number)[].number'
    fi
}

prompt_for() {
    local n="$1"
    cat <<PROMPT
You are the knobas autopilot session for issue #$n, running unattended. Björn
is not watching this terminal; a question printed here is a question nobody
answers, so never block on one.

Invoke the Skill tool with skill "mattpocock-skills:implement" and args "$n",
and carry the issue through the whole project flow (CLAUDE.md, project memory,
docs/agents/working-model.md): test-first implementation, \`just check\` green,
the two-axis mattpocock code-review skill (deep pass if the diff touches a
frozen surface per docs/contract.md §10.8, standard otherwise), a PR with the
mutation proof pasted, CI green, squash-merge with --delete-branch. The merge
closing issue #$n is what "done" means.

Ground rules:
- Work on your own branch in your own worktree under .worktrees/; the
  repo-root checkout stays on main -- it belongs to the driver. Remove the
  worktree and its target/ after the merge.
- \`env -u RUSTUP_TOOLCHAIN\` for every cargo invocation. Commit before going
  idle. Never edit an applied migration; new schema needs the next free
  migration number and an orchestrator-ratified issue.
- Never merge red CI. Never force-push. Never touch issues other than #$n
  beyond reading them.

Stop protocol -- the ONLY way to hand something to Björn: if a genuine
decision arises (review disagreement unresolved after 3 rounds, a
frozen-surface change no issue ratifies, red CI needing a judgment call, spec
ambiguity where the readings diverge materially), then (1) comment the precise
question on issue #$n -- what you need decided, the options, your
recommendation; (2) \`gh issue edit $n --add-label ready-for-human
--remove-label ready-for-agent\`; (3) push your branch as it stands; (4) stop.
Do not merge partial work, and do not escalate for anything you can decide
yourself.
PROMPT
}

ran=0
while :; do
    picked=""
    for n in $(queue "$@"); do
        reason="$(skip_reason "$n")"
        if [ -n "$reason" ]; then
            echo "skip #$n: $reason"
            continue
        fi
        picked="$n"
        break
    done
    if [ -z "$picked" ]; then
        echo "queue drained: nothing runnable ($ran issue(s) completed)"
        exit 0
    fi

    if $dry_run; then
        echo "would run #$picked ($(gh issue view "$picked" --json title --jq .title))"
        # Pretend it completed so the dry run walks the rest of the queue --
        # but only in explicit mode, where the list can shrink; the auto
        # queue would just re-pick the same issue forever.
        [ "$#" -eq 0 ] && exit 0
        rest=()
        for a in "$@"; do [ "$a" != "$picked" ] && rest+=("$a"); done
        set -- ${rest[@]+"${rest[@]}"}
        continue
    fi

    # A session branches from wherever main is, so main must be current and
    # the root clean before every issue -- and after every merge.
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "repo root is dirty; commit or stash before running autopilot" >&2
        exit 1
    fi
    git switch -q main
    git pull -q --ff-only

    log="$logdir/issue-$picked-$(date +%Y%m%d-%H%M%S).log"
    echo "=== issue #$picked ($(gh issue view "$picked" --json title --jq .title))"
    echo "=== log: $log"
    # The session's own judgment decides success, not its exit code: what the
    # driver trusts is the state it left on GitHub.
    claude -p "$(prompt_for "$picked")" --dangerously-skip-permissions \
        >"$log" 2>&1 || true
    tail -5 "$log"

    state="$(gh issue view "$picked" --json state --jq .state)"
    labels="$(gh issue view "$picked" --json labels --jq '[.labels[].name]|join(" ")')"
    if [ "$state" = "CLOSED" ]; then
        ran=$((ran + 1))
        echo "=== #$picked done ($ran so far)"
        git pull -q --ff-only
        continue
    fi
    case " $labels " in
        *" ready-for-human "*)
            echo ""
            echo "=== #$picked needs a decision from you:"
            gh issue view "$picked" --json comments --jq '.comments[-1].body'
            echo "=== answer on the issue, relabel it ready-for-agent, rerun."
            exit 2
            ;;
    esac
    echo "=== #$picked: session ended without closing or escalating -- inspect $log" >&2
    exit 3
done
