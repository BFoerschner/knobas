---
status: accepted
---

# A token carries a role, a theme colours roles, and the engine that cuts the tokens is part of neither

Björn, on PR #2479: "I'd like to be able to customize the themes. Starting with custom syntax highlighting." Decided
2026-11-21 (Fable, xhigh, under Björn's standing instruction;
`docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`): **what the highlighter answers is
text and one of a fixed list of roles; a colour exists only as a CSS variable a theme sets; and which engine cut the
tokens is the worker's own business.**

- **The port** (`render/port.ts`) speaks `[text, role]` by line. A role is one of `render/roles.ts`'s seventeen and two
  shapes. No colour, scope or engine's own name crosses it.
- **An engine** is a table from its own token names to roles (`ROLE_MAPS`) and a way to tokenize. A setting picks the
  engine, and a fixed order a choice says who draws what it cannot.
- **A theme** is a name, a side and a colour for some roles. A role it leaves out is ink. It may set the diff's four
  colours and whether comments slant.
- **Two slots**, one a side. Their variables are both on the root, and the stylesheet picks by the side drawn, so the
  system changing sides runs no script.
- **Kept on this computer**, read before the first draw. Not in the database, not in a backup, not over IPC.
- **A theme never changes a size, a weight but the heading's, a slant but the comment's, or a surface.** Until the rest
  of the window is themed, contrast is checked against knobas's own surfaces.

## Rejected

- **A colour in the token** (Shiki's themes as they come, VS Code themes stored whole): a theme change would tokenize
  everything again, and every engine would need the theme.
- **Scopes as the unit a person colours**: thousands, different in every grammar, absent in a lexer.
- **One engine.** None of the three is fast, exact and complete alone: Shiki is 20 to 130 times slower on the languages
  read most here, Pierre draws SQL badly, TanStack knows 79 languages.
- **Highlighting in the backend** (tree-sitter through arborium, or syntect): slower than the fast engines in the page
  before a token crosses IPC. Björn on arborium: "remove arborium, i didn't see its timing was even before ipc".
- **Themes in the database**: they would land after the first draw.

## Consequences

- New dependencies for the engines (their licences and sizes in the PR's *Merge Danger*). No IPC, no migration, no crate.
- A theme imported from another editor is a reading of it onto seventeen roles, not the theme.
- When the window's own colours become part of a theme, the surfaces contrast is checked against become the theme's.
