# Mockup agent brief

You are building **one** clickable HTML mockup of *knobas*, a Rust + Tauri desktop app that unifies Jira, Confluence, Gitea, TeamCity (and similar sources) with local full-text search, cross-source links, local notes, write-back, time tracking, an attention inbox and standup generation. The user will compare 25 mockups (5 interaction paradigms × 5 visual designs) and pick a direction; yours is one cell of that matrix.

## Process
1. Invoke the `frontend-design:frontend-design` skill and follow it: plan tokens (palette, type, layout, signature) *within* your design brief, critique the plan, then build.
2. Read, in this order: `mockups/shared/dataset.md`, `mockups/shared/screens.md`, your paradigm brief (`mockups/shared/paradigms/P?-*.md`), your design brief (`mockups/shared/designs/D?-*.md`).
3. Write exactly one file at the output path you were given. Do not create other files. Do not run `git` commands (the orchestrator commits).
4. Self-check against the QA list below, fix, then report.

## Hard constraints
- **One self-contained `.html` file**, vanilla HTML/CSS/JS. No frameworks, no bundlers, no CDN scripts, no external images. Inline SVG for icons/diagrams. Google Fonts via `<link>` are allowed (with real fallbacks).
- Must work opened from `file://` in Chrome/Safari.
- **Desktop application, not a website.** Design for a 1440×900 window, usable down to 1100 px wide. App chrome, panes, toolbars, status bars, keyboard focus. No marketing hero, no landing-page sections, no page scroll of the whole app (panes scroll internally).
- **All eight surfaces** from `screens.md`, placed where your paradigm brief says. Hash-based routing (`#/inbox`, `#/ticket/PAY-231`, …) or equivalent in-page state; browser back should work where it's cheap.
- Use the dataset verbatim (keys, names, statuses, times). No lorem ipsum, no placeholder "Item 1". Today is 2026-08-22 14:32.
- Keyboard: `Cmd/Ctrl+K` → search, `Cmd/Ctrl+T` → timer toggle, `Esc` closes overlays. Visible focus rings. `prefers-reduced-motion` respected.
- Persistent elements visible on every surface: timer with context + elapsed; inbox count; sync state (one glance: 3 sources ok, TeamCity 401).
- Mutations are optimistic and in-page: status changes, comments appended, links confirmed, worklog logged, build triggered, branch/PR created, source saved. Toasts are fine. Never `alert()`/`confirm()`/`prompt()`.
- No git-branch-based time attribution anywhere (the user rejected it).
- Size: aim for 1200–2000 lines. Depth on the 8 surfaces beats extra features.
- Copy: plain, specific, sentence case, active verbs ("Log 2h 44m to PAY-231", not "Submit"). Errors say what happened and what to do.

## File header (the index is generated from it — keep the exact format)
```html
<!--
knobas mockup
paradigm: P3 Graph
design: D2 Ledger
thesis: <one line — what this paradigm makes effortless>
thesis: <one line — how this design expresses it>
thesis: <one line — the single signature element>
-->
```

## QA before you report
- Open the file mentally as the user: can they reach all 8 surfaces by clicking? Does PAY-231 → PR #142 → build #1187 → page → note navigation work?
- Timer visible on every surface? Inbox count? Sync state?
- Does `Cmd/Ctrl+K` open search? `Cmd/Ctrl+T` toggle the timer? `Esc` close overlays?
- Any `alert(`? Any lorem? Any entity not in the dataset? Any external script? Fix.
- If a headless Chrome is available (`"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu --screenshot=<scratch>/shot.png --window-size=1440,900 file://<path>`), take a screenshot and look at it; also run `--dump-dom` and check for JS errors by wrapping your init in try/catch that writes to a hidden element. Fix what you see.
- Check CSS specificity collisions (a `.panel` rule overriding a `.timer` padding, etc.).

## Report (your final message — raw data, not prose for a human)
- path, line count
- paradigm + design, the three thesis lines
- which surfaces are full screens vs panels/overlays
- known gaps or compromises
