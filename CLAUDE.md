# CLAUDE.md

## Tracker

GitHub Issues on `BFoerschner/knobas` via `gh` (`docs/agents/issue-tracker.md`). Labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `backlog`, `wontfix` (`docs/agents/triage-labels.md`).

## Documents

Single context: `CONTEXT.md` + `docs/adr/` (`docs/agents/domain.md`). Spec `docs/specs/2026-08-23-knobas-design.md` · frozen contract `docs/contract.md` (§10.8 is the IPC/migration ledger) · roadmap `docs/roadmap.md` · working model `docs/agents/working-model.md` · dated rulings `docs/decisions/`.

## Flow

mattpocock-skills only: grilling → to-spec → to-tickets → `/loop /ticket-loop` (`.claude/commands/ticket-loop.md`; agent briefs `docs/agents/implementer-brief.md`, `docs/agents/merge-manager-brief.md`). `triage`, `to-spec`, `to-tickets` and `handoff` refuse the Skill tool: Björn types them. A handoff lands in `$TMPDIR/handoff-*.md`, not the repo.

**Standing rules live in the skill, the briefs and `docs/`, never in memory** (Björn, 2026-10-19: memories get skipped). A new rule is a docs-only PR to the file whose reader needs it.

## Machine

- **Unattended in bypass mode: no permission prompts.** Never: `cd` chained into a compound command (absolute paths, `git -C`, `npm --prefix`, and `env -C <dir> <cmd>` for anything that must run in a directory, such as cargo); Bash writes, deletes or moves under `~/.claude/` or any `.git/` (git commands are fine; memory files via Write/Edit); shell-config edits. If a step needs one, skip it and say so. Put this rule in every agent brief.
- The Bash tool is Homebrew bash 5.3 (`CLAUDE_CODE_SHELL` in `~/.claude/settings.json`; PATH from `~/.bashrc`). macOS: no `timeout` binary, no `pgrep -c`; never call a pager. Start piped test or gate commands with `set -o pipefail;`.
- `gh pr merge` from the root checkout tries to switch it to `main`; a dirty root makes it exit 1 after the merge succeeded on GitHub.
