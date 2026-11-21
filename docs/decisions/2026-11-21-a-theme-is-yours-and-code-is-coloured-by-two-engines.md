# 2026-11-21: A theme is yours, and the highlighter is a choice

**Asked (Björn, on PR #2479):** "I'd like to be able to customize the themes. Starting with custom syntax highlighting. I
found tanstack highlight … this seems themable. it really seems to look good on diffs too because you can like dim and
highlight in different colors." Then: "research other highlighting libraries as well and choose the best one." On the
engines, each word replacing the one before: "I feel pierre really is too young", "keep pierre and have tanstack in the
back and not shiki", and "no i actually want to pick the highlighter itself". On the page: "themes go into appearance.
and also, this is just themes for code anyways."

The judgment calls went to Fable (xhigh) under Björn's standing instruction. The architecture is ADR-0051. The
highlighter's setting is the re-ruling of the same day; the page's place is `2026-11-21-settings-has-subtrees.md`.

## What was there

Shiki 4.5.0 in a worker, six kinds of token drawn by class, their colours two fixed sets in `tokens.css`. A diff tinted
its changed lines and marked changed words. No Docs code block was coloured. The window's side was forgotten on a
restart.

## The rulings

1. **Seventeen roles**, a colour a CSS variable each (`--hl-<role>`, `.tf-hl-<role>`), so a theme recolours what is
   drawn: comment, keyword, string, escape, number, constant, function, type, property, key, tag, attribute, variable,
   operator, punctuation, heading, link, and bold and italic as shapes. In a data language (JSON, YAML, properties,
   INI, TOML) a property, tag or attribute is a key. Each engine has one table from its own token names to the roles
   (`render/roles.ts`, `ROLE_MAPS`).
2. **A theme is role colours for one side.** Two slots, *While light* and *While dark*, both written on `<html>` before
   the first draw, so the system changing sides runs no script. Seven come with knobas (the knobas pair, GitHub Light
   and Dark, One Dark, Dracula, Catppuccin Mocha); a person's own is a copy. Kept on this computer, not in a backup. The
   window's side is kept across a restart now too (amending R-1.7's "for the session").
3. **Contrast is checked on every surface code is drawn on** (the sheet, a Docs block, a Markdown fence, an added and a
   removed line), warned under 4.5:1 with the surface named, and fixed by lightness alone when asked.
4. **Import** reads knobas's file, a VS Code theme and a Zed theme onto the roles. **Export** writes knobas's file.
5. **A diff's unchanged lines are quiet**: 40 % of their colour mixed into the secondary ink. *Full colour* and *Grey*
   are the other two choices. Its tints are `--diff-add` and `--diff-del`, which a theme may set.
6. **A Docs page's code block is coloured**, its find marks kept, and holds its job so a change of highlighter colours
   it again in place.
7. **The page is Settings › Appearance › Code colours**, a child of Appearance in a nav that has subtrees now. The
   window's side stays on Appearance.
8. **The highlighter is a choice** (`knobas.highlighter`): Pierre Highlights (the default), TanStack Highlight or Shiki,
   each loaded only when needed, each with an order for what it cannot draw. Under Pierre, SQL goes to TanStack.

## Measured

On Björn's PC in headless Edge 157, per 1,000 lines: Pierre 0.4 to 0.9 ms, TanStack 0.5 to 1.9 ms, Shiki 1.2 to
82.1 ms (TypeScript 82, Gradle 48, Java 21); a 20,000-line TypeScript file 21 ms, 37 ms and 2,500 ms. Heap after
fourteen languages: 6 MB, 6 MB and 43 MB. Every built-in role passes 4.5:1 on every surface of its side, and so does
its quiet mix (`theme/contrast.test.ts`).

## As built, where it differs from the rulings

- **Rust's `storage.type.rust` stays a keyword.** The ruling's Shiki table put it under *type*; in Shiki's Rust grammar
  that scope is `let`, `struct` and `enum`, which would have drawn as types. `entity.name.type` already makes Rust's types.
- **Kotlin's `entity.name.type.annotation` is an attribute**, so `@Suppress` reads as `@Override` does.
- **`properties` is a language of its own**, `ini` to Shiki and `properties` to the other two, and **a `.gradle` file is
  Groovy**; before, a properties file was Shiki's `ini` and a Gradle file had no language.
- **`CHUNK` stays 400 on every engine.** The ruling halves it on the fast engines if a slice's main-thread time passes 8 ms
  at 6x. Measured in headless Chrome on the WSL box with a stand-in view that redraws 60 rows a slice, the slowest slice
  of a 20,000-line TypeScript file at 6x was 13 to 22 ms at both 400 and 200 lines a slice, varying run to run, while 200
  doubled the slices and the time to colour the whole file (Pierre 703 ms against 1,214 ms). The size of a slice is not
  what costs; no main-thread task passed 50 ms at 6x under any engine.
- **A Docs code block, the theme page's preview and Markdown fences hold their jobs**, so a change of highlighter colours
  them again; `highlightSnippet` lets its job go once answered.

## Measured in the build

Headless Chrome on the WSL box (Ryzen 5 5600X), the built app's own worker, from asking to the first colours and to the
last slice (`S/hlprobe`): a 2,000-line TypeScript file in a new worker (the first file of a session) 39 ms under Pierre,
24 ms under TanStack and 400 ms under Shiki; warm, 8, 11 and 69 ms to first colours. A 20,000-line one: first colours in
59 to 70 ms under Pierre (it tokenizes the whole file in one call first), 75 to 91 ms under TanStack, 72 ms under Shiki
(its lines in view first), and the whole file 0.2 s, 0.2 s and 4.0 s. Throttling the page's CPU 4x and 6x does not slow the
worker. The worker's own chunk is 23.5 KB; Pierre 430 KB, Shiki's core 186 KB and TanStack 80 KB are chunks of their own.

## Not in this pass

A build's log. Colours for the rest of the window (the app-wide theme page has its slot under Appearance). Themes in a
backup. Every timing in knobas's own window on Windows.

## Reversing it

`data-diff-unchanged="full"` is the diff of before. The knobas pair writes nothing, so the stylesheet's colours are the
ones of before but what the new roles split off. Removing the page leaves the two knobas themes.
