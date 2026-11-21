---
status: accepted
---

# A theme has two parts, and the window derives what a theme leaves out

Björn, on PR #2479: "theme the whole Knobas app … or just import vscode/etc. alone just for the knobas theme without
touching code". Decided 2026-11-21 (Fable, under Björn's standing instruction;
`docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`), amending ADR-0051: **a theme may hold colours for
the window, for code, or for both. The window's colours are a fixed list of keys, each one of the stylesheet's
existing tokens. A key a theme leaves out is computed from the keys it sets. Which theme draws is four slots, and
the code slots follow the window's unless set apart.**

- **A theme holds colour and nothing else.** No length, no font, no shadow.
- **The stylesheet's token names are the contract.** A theme never adds a token. A colour drawn anywhere in the app
  is a token or is on a short list of exceptions a test reads.
- **Derivation runs in the page, in OKLCH, and writes plain colours.** The stylesheet computes none of it. knobas's
  own two themes are the empty set, and the stylesheet's fallbacks are their values.
- **Both sides' variables are on the root at once**, as ADR-0051 has it, now for every colour.
- **What was last written is kept beside the themes**, and a script in the document's head writes it back before
  the first paint. That script derives nothing.
- **The window draws an unreadable theme only after a press that says so**, and Reset needs no reading.
- **Kept on this computer**, as ADR-0051 has it. One command tells the Rust side the page colour of each side and
  the mode, for the bare window.

## Rejected

- **Renaming the tokens to role names**: thousands of edits for no pixel.
- **Two kinds of theme, one for the window and one for code**: matching them is a lookup that breaks on a rename.
- **Deriving in the stylesheet** with `color-mix()` and relative colours: the checks, the fixes and the first paint
  need the values as numbers, and a step that stops when a contrast is met cannot be written there.
- **One slot that sets both parts**: the ask was the window alone, without touching code.
- **Fixing an imported theme without a word**, and **refusing an unreadable one**: the first hides a change, the
  second takes a choice away.
- **Themes carrying fonts, radius or density**: they move what is drawn, and a slot changes with the system.

## Consequences

- One IPC command and one small file in the profile's folder. No dependency, no migration.
- An imported theme is a reading of it onto knobas's colours. Where a colour of it cannot play its part here, knobas
  derives one.
- A new colour in the app is a new key with a derivation and a check, or it is one of the tokens there are.
- A build from before this one drops a theme's window colours when it saves that theme.
