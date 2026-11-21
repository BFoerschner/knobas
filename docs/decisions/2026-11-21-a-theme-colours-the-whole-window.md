# 2026-11-21: A theme colours the whole window

**Asked (Björn, on PR #2479):** "I'd like for you to have another fable agent design a way to theme the whole
Knobas app. Maybe with ways to adapt it to the code-syntax theme and the imports, or just import vscode/etc. alone
just for the knobas theme without touching code, and i dunno, just make it all themable."

The judgment calls went to Fable under Björn's standing instruction. The architecture is ADR-0055.

## What was there

A theme coloured code: seventeen roles and a diff's four colours, two slots, seven built-ins (ADR-0051). The window's
own colours were two fixed sets in `tokens.css`. 81 colours stood outside them as literals, 45 of those a second copy
of the palette for a preview. 15 rules wrote text in a surface colour on a fill, which held only while the fill was
the ink. The page's module set the side after the first paint, so a window kept dark on a light system painted one
light frame.

## The rulings

1. **A theme colours things and nothing else.** No radius, shadow, density or font.
2. **The token names stay.** Every colour token reads a variable for its side with knobas's value behind it. 22 new
   tokens, among them `--on-fill`, `--primary`, `--link` and the yellow card's states. 77 literals became tokens or
   went. 4 stay, on a list with their reasons.
3. **The window's part of a theme is 51 colours a side, any of them set.** What a theme leaves out is derived in
   OKLCH from what it sets. Two colours fill the rest.
4. **One theme, two parts**, window colours and code colours, in one library.
5. **Four slots.** The code slot follows the window's until it is set apart.
6. **Import** reads a VS Code theme's `colors`, a Zed theme's `style`, a Base16 scheme and knobas's file, and then
   asks what to use it for: the app and code, the app only, or code only.
7. **Every text colour is checked on every surface it is drawn on**, at 4.5:1, and a part at 3:1. The fix moves
   lightness alone. A filled pair's fix moves the ground.
8. **States told apart** is a quiet line with a fix that may move a hue.
9. **A theme under 3:1 for the text one needs to find the way back asks first, and is kept only on a press.** Reset
   is a button, a launcher action, ⌘⇧⌫ or Ctrl+Shift+Backspace, and `knobas --default-theme`.
10. **The first paint has the theme**, from a script in the page's head, and the bare window has its colour from the
    Rust side.
11. **Settings › Appearance › Theme** is the page. Appearance's *Theme* row is now *Mode*.

## Measured

Derived from knobas's panel and text colour alone, the other colours land a mean OKLab distance of 0.006 from
knobas's own. Of 65 VS Code themes, 50 arrive with a colour under its mark, 150 colours in all, and none fails
after *Fix all*. One is unreadable as published. 62 have two states closer than 0.08 for a colour-blind reader, as
knobas's own two themes do, at 0.018 and 0.034.

A switch is one restyle of the whole page and no layout: 17, 59 and 79 ms at 1x, 4x and 6x on a page of 2,854
elements in Chrome on Björn's PC, the same whether the theme is written as properties, an attribute or a sheet, and
the same for one side or both. 400 more custom properties on `<html>` change no restyle's time. A theme set by the
page's module missed the first paint in 5 of 5 loads at each speed. Set by a script in the head, in none.

## Not in this pass

JetBrains schemes. Themes in a backup. A chart palette, since there is no chart with series. Fonts, radius or
density in a theme. Changing knobas's own state colours for colour-blind readers.

## Reversing it

With the four slot keys removed the window is knobas's, pixel for pixel but for the three changes of the token
step. The page removed leaves the tokens, which cost nothing at rest. The head's script removed brings back the one
light frame.

## Built (2026-11-22)

Steps 1 to 9 of the design, on `ux-polish`. Where the build differs from the design:

- The editor's list has 22 rows, as the design's own table has them; its text said 21.
- The Zed table was run against Zed's own One and Ayu families at the commit the notices pin. Every colour came from
  the key the table names, none of the five themes is unreadable as published, and none fails after *Fix all*. The
  guards are the design's spike's (`app-import.ts` says so), where its prose said "as above": so a Zed page can be
  lighter than its panel, as Zed draws it.
- The first-paint case is in `testenv/perf/theme-switch.mjs` beside the switch probe, and reads the first frame's
  computed page colour rather than a screenshot of it.
- *Put knobas's themes back* in the launcher is an action that runs where it is; its address is the Theme page.

Section 11's nine questions are unanswered; each default is in one place: code follows the window (`themes.svelte.ts`,
`codeDrawn`), the card is knobas's in every built-in (`builtins.ts`), the colour-blind set is not knobas's own
(`app-derive.ts`'s `KNOBAS`), an import arrives as published with *Fix all* (`theme-import.ts`), themes stay on this
computer (`themes.svelte.ts`), JetBrains is not read (`themes.ts`), the reset key (`theme-reset.ts`), the nine code
colours (`builtins.ts`), and an import's button colours the primary button only (`app-import.ts`, `primary`).
