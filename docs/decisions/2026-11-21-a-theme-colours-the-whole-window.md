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
   is a button, a launcher action and `knobas --default-theme` (the design's ⌘⇧⌫ was dropped, below).
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

JetBrains schemes (Björn wants them last in ux29, time allowing). A chart palette, since there is no chart with series. Fonts, radius or
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

## Björn's answers (2026-11-22)

Section 11, answered, and built:

1. **Code follows the window**, and can be set apart: as built.
2. **The yellow card is a theme colour.** Any theme may set it. One that sets none but sets its Attention gets a card
   of that hue at knobas's card's lightness and chroma, so the built-ins and imports bring their own; a theme that sets
   no Attention keeps knobas's yellow, and so do knobas's own themes.
3. **Colour-blind mode**, a switch in Settings › Appearance, off at first. On, it names each pair of states that reads
   the same under protanopia, deuteranopia or tritanopia, with the roles ("Attention and Failed read the same under
   deuteranopia"), for both sides' themes, under the Theme page's States, and in a toast when such a theme is put in a
   slot. The simulation is Machado, Oliveira and Fernandes (2009), *A Physiologically-based Model for Simulation of
   Color Vision Deficiency*, IEEE TVCG 15(6), at full severity; a pair reads the same under 0.08 in OKLab. Two
   built-ins, *knobas Light for colour blindness* and *knobas Dark for colour blindness*, carry the set *Tell them
   apart* computed: light Passed `#00604F`, Attention `#905C0C`, Failed `#9B0618`; dark Passed `#52D8BD`, Attention
   `#F2BD5B`.
4. **An import arrives as published, with *Fix all*:** as built.
5. **Themes are in backups.** The store is mirrored into one `knobas.setting` row (`themes.kept`), which the database's
   dump carries; a restore puts it back. A share export carries no setting, so none carries themes.
6. **JetBrains `.icls`** (code colours only): wanted, last in ux29 if time allows.
7. **No reset key.** The button, the launcher action and `knobas --default-theme` stay.
8. **The built-ins keep their published code colours.** The nine are not lightened. On their own window a few fall
   under 4.5:1 and are marked; the editor's *Fix all* and each row's fix work on a built-in too, by making a copy with
   the fixed colours where the original drew.
9. **A selected row is a pair of theme colours of its own**, *Selected row* and *Text on a selected row*
   (`--sel-row`, `--on-sel-row`), split from *Pressed* (`--fill`, `--on-fill`, which pressed segments keep). Unset,
   it is the ink plate of today. Its row in the editor has three quick picks: *knobas (ink)*, *The theme's accent*, and
   for an import that had one, *The theme's own* (VS Code's `list.activeSelectionBackground` and `-Foreground`, Zed's
   `element.selected`), which the import offers and does not set. The pair is checked at 4.5:1 and joins the 3:1
   floor; a selected row's states (`--on-ink-*`) are worked out against it.
