# A ring takes the shape of what it rings, and the branch box is a button (2026-11-21)

The ux27 pass, PR #2479. Björn: "i think the box from branch switching kind of feels like it's different sized than
the button above? also the blue outline is kinda weird when i select it somehow sometimes. in general, the blue
selection outline around elements might seem weird, think about it if it's fitting to the style."

Ruled by a Fable design agent (claude-fable-5-2, xhigh) on the measurements and screenshots below, built as ruled.
The design system's *Motion and focus*, *Shape*, *Controls* (form controls) and `--field` row carry the rule.

## What was measured (headless Chrome on the mock corpus, 1440x868@1, 2560x1440@1.25, 5120x1440@1, light and dark)

**The branch box** was already the buttons' height (32 px) and radius (4 px). What set it apart was everything else: a
native select is as wide as its longest option, so *main* sat in a 258 px box with about 150 px of empty field beside
*New branch…* at 117.5 px; it stood on `--field` (white in light, darker than the page in dark) where the buttons stand
on `--sheet`; its name was the code face at 600 where a button's words are 500; and it drew the native chevron, larger
and heavier than *Open in*'s caret.

**"Weird when I select it"**, found by walking the keyboard focus through 21 pages in both themes (2,134 focus stops)
and by pointer cases on the repository header:

1. **The branch box rang on a mouse press.** Chromium (so WebView2 too) matches `:focus-visible` on a `<select>` after
   a mouse press, which it never does for a button, and the box drew its ring with `:focus-within`. Pressing the box,
   or picking a branch from its list, left a blue ring around it. Compare's selects, Code's *Team* and Settings'
   selects did the same.
2. **Every focused control changed shape.** The global rule set `border-radius: 3px` on whatever had focus: a pill
   (*Mine*, *Rendered*, radius 12) and a switch (radius 10) went square while focused, square tabs, rows and links
   went round, buttons went 4 to 3. `.tf-chipbtn` forced 2 px.
3. **Rings cut by the box around them**: a code fence's *Copy* (1.5 px), Compare's file list rows (2 to 4 px),
   Monitors' names and *Watches* links (3 to 4 px), a pull request's check links (4 px), a Docs page's comment box at
   its scroller's edge (3.6 px).
4. **A field drew a second frame**: a 2 px ring 2 px outside its own 1 px border.
5. Not a fault: `Esc` back to *New branch…* or *Open in…* after its dialog or menu rings the button, as the keyboard
   asks; a mouse *Cancel* draws none.

## What it is now

- **The ring** is 2 px in the surface's colour (`--focus`; `--card-ink` on a card; `--paper` inside a fill) with no
  radius of its own, so it follows each control's own corners. A control standing free wears it 2 px outside; a field
  and anything in a box that clips wears it 2 px inside, so a field's border becomes the ring. `--focus` is 5.06 to
  6.23:1 on the light surfaces and 6.29 to 7.71:1 on the dark ones, above WCAG 2.2's 3:1.
- **One place.** `tokens.css` holds `--focus-w`, `--focus-off`, `--focus-in` and the one rule; a container sets
  `--focus-o` (the boxes that clip, listed once in `chrome.css`) or `--focus-c` (cards, fills). About 20 one-off
  outlines are gone, and the components' own rings (`::after` on a pipeline row, the radio rows, a landing row, the
  assets dialogs) are drawn from the tokens. `tf-css.test.ts` refuses a focus rule that sets a radius or a literal
  outline in any stylesheet or component `<style>`.
- **A select rings only after a key.** `input-modality.ts` writes `data-input="pointer"` or `"keys"` on `<html>` from
  capture listeners (a bare modifier changes nothing, a run of one hand writes once), and only a select reads it. A
  field still rings under a press: it holds the caret.
- **Ellipsis boxes** that hold a pressable name (`.tf-mon-name`, `.tf-mon-watch`, `.tf-prgrow`) keep 4 px of room
  inside their edge and give it back outside, so nothing moves.
- **The branch box is a button that opens a list.** The native select is the box: `--sheet`, 1 px `--rule`, radius 4,
  32 px, the branch glyph 11 px in and *Open in*'s caret 11 px from the end, its name in the code face at 13 px and
  500, as wide as its name up to 282 px and then cut, the whole name in its title. *main* is now 96 px (was 258). The
  header's row is anchored at its end, so a pick grows the box leftward and *New branch…*, *Open in…* and *Refresh*
  keep their place (each stayed at x 1072.9 at 1440). Before the branch list is read the shown name is the select's
  one option, so the box is that name's width from its first draw. Stacked from the page's edge, and in a webview
  without `field-sizing`, it keeps its longest option's width as before. Compare's *Into* and *From*
  and the other selects stand in forms or beside fields and stay fields.

## Reversing it

Putting `border-radius: 3px` back on `.tf :focus-visible` reshapes every focused pill and switch again, and dropping
the `data-input` rule brings the ring back on a mouse press of a select. The branch box's old face is `code.css`'s
`.tf-bsel` block before this date: a label on `--field` around a bare select.

## Checked by

The Tab-walk scan and the pointer cases (scripts in the pass's scratch, screenshots in its report), `tf-css.test.ts`,
`input-modality.test.ts` (10 of 10 mutants killed; 12 of 12 CSS mutants killed by the stylesheet tests), press trips
`--strict`, `just moves`, `pixel-same.py`, `long-paths.mjs`. WebView2's look is owed on Windows.
