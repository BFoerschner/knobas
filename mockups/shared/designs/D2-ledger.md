# D2 — Ledger

**Identity:** an engineer's ledger — ruled paper, columns, entries in ink, source stamps in red. Light, precise, printable. The app reads like a well-kept logbook where every ticket, commit, build and worklog is a line with a date, a reference and a stamp.

## Tokens
- Paper `#FCFCFA` (near white, *not* cream) · Panel `#F4F5F2` · Rule lines `#D7DEE8` (pale blue ruling) · Margin line `#E0857C`
- Ink `#1B2230` · Muted ink `#5F6B7A` · Faint `#9AA5B4`
- **Ink blue `#1F3A8A`** — links, selection, active, primary action
- **Pencil red `#C8372D`** — stamps, failures, attention, unread counts
- Success: ink blue check, no green. Running: a hand-drawn-looking dashed circle in ink.
- Source identity: by **stamp** (see signature), all in pencil red or ink blue, never rainbow colors.

## Type (Google Fonts)
- Display / headings: **Archivo** 600–700 (or Archivo Expanded for the app title)
- Body & UI: **Archivo** 400/500
- Long text (descriptions, pages, notes, standup protocol): **Source Serif 4** 400/600 — the "written" parts are serif, the "ledger" parts are grotesk.
- Keys, hashes, times: **IBM Plex Mono** 400, tabular.

## Shape & density
- 0 radius. Horizontal rules between rows (1 px `#D7DEE8`), a single vertical margin line in pencil red on the left of content columns (like the red margin of a ledger). Column headers as small caps / uppercase Archivo 11 px, +0.1em.
- Rows 32–36 px. Generous horizontal rhythm, tight vertical.

## Signature
**Ledger lines with rubber-stamp source badges.** Every entity row is a ledger line: date column · reference (key/hash/#) · description · status · stamp. The stamp is a rotated (−4°…+3°, seeded per item so it's stable) uppercase label in a double border, slightly translucent ink (`opacity .85`, `mix-blend-mode: multiply`), e.g. `JIRA`, `WIKI`, `GIT`, `CI`, `NOTE`; failed builds get a red `FAILED` stamp across the status cell. Confirming a suggested link "stamps" it (a 200 ms scale-in). Worklog entries are ledger lines in a running total column.

## Motion
Stamp scale-in, and page-turn-free: instant navigation. Reduced-motion removes the stamp animation.

## Avoid
Cream `#F4F1EA` + terracotta, serif display headlines, broadsheet multi-column newspaper layout, handwriting fonts, skeuomorphic paper textures. The ruling and the stamps are the only "paper" cues.
