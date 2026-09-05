# CLAUDE.md

## Agent skills

### Issue tracker

Issues are tracked as GitHub Issues on `BFoerschner/knobas` via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default label vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Planning flow

mattpocock-skills only (grilling → spec → tickets), decided 2026-08-28; do not use the
superpowers planning skills. Task tracking is GitHub Issues (milestone M2 onward); the
retired superpowers plan docs are in git history. Key documents: spec
`docs/specs/2026-08-23-knobas-design.md` · frozen-contract record `docs/contract.md` ·
roadmap `docs/roadmap.md` · agent working model `docs/agents/working-model.md`.
