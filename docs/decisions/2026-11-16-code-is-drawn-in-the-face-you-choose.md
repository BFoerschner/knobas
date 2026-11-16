# 2026-11-16: Code is drawn in the face you choose

**Asked (Björn, on PR #2479, through the orchestrator):** "use monospace fonts for all code blocks, anywhere in
pages/docs/code etc. Also, files in Source-rendering should also be defaulted to monospace if not already. Use
caskaydiacovenerdfont if we can include that." Then: "maybe add a toggle in the settings pane for it" (ligatures).
Then, replacing the font: "actually, even better, let me set the font generally in the settings pane. no need to
include fonts."

## What was decided

- **No font is added to the bundle.** JetBrains Mono (400, 600) stays the face knobas brings along. CaskaydiaCove Nerd
  Font was weighed before the third ask replaced it: Nerd Fonts v3.5.1's `CascadiaCode.zip`
  (SHA-256 `1298bf92698afa06185cf1d05e6ae05f2d8a1e8c3cb45ddf4c3035168ab342a1`), the `CaskaydiaCoveNerdFontMono`
  variant, is 1.26 MB of woff2 a weight, almost all of it the icon glyphs; nothing of it is in the tree.
- **Settings › Appearance › *Code font*** picks the family code is drawn in from the fonts installed on the machine, or
  takes a typed name. `--mono` is that family first, then JetBrains Mono, its two stand-ins and the system's
  monospace, so a name the machine lacks draws as JetBrains Mono and never in a proportional face.
- **Settings › Appearance › *Ligatures in code*** is a switch, off until set: #1445 turned ligatures off because a
  ligature shows characters a file does not hold, and that stays the default. Every rule that sets `--mono` sets
  `font-variant-ligatures: var(--mono-ligatures)`.
- **Every code element is drawn through `--mono`.** One rule at no specificity gives `pre`, `code`, `kbd`, `samp` and
  `tt` the face and the switch, so code in any rendered text takes it with no rule of its own.
- **The choice is the machine's, in the webview's `localStorage`**, not a `knobas.setting` row: it names a font
  installed on this machine, so a backup restored elsewhere should not carry it, and it has to be on `<html>` before
  the first draw (`main.ts` loads `code-face.svelte.ts` before mounting). A row read from the database lands after the
  first draw, and every line of code on the page the app opened on would draw in one face and then another.

## Where code is drawn, all through `--mono`

Docs: a Confluence code or noformat macro (`.tf-doc pre`), inline code (`.tf-doc code`). Markdown everywhere (READMEs,
pull request bodies and comments, ticket descriptions and comments, Jira's wiki markup turned into Markdown, so its
`{code}`, `{noformat}` and `{{monospace}}`): fenced blocks (`.tf-md .tf-fence pre`) and inline code (`.tf-md code`). A
file in the Code page's viewer and every file's *Source* view, Markdown and other rendered files shown as source
included (`pre.tf-code`, `TfRepoFiles`, `TfFileViewer`, a renderer's text output in `TfCommandOutput`). Diffs and
their hunks (`.tf-diff`, `.tf-was-hunk`, `.tf-pdfd-hunk`), the branch editor (`.tf-editor`). Build logs
(`.tf-logv`, `.tf-log`), test output and a failure's details (`.tf-tdetail pre`, `.tf-problem pre`, a run's and a
test's `pre`), a step that failed (`.tf-stepfail pre`). Keys, shas, branch and repository names, paths (`TfKey`'s
`.tf-mono`, `.tf-k`, `.tf-p`, file lists and trees), the launcher's key cell (`.tf-ln-mono`), keyboard keys (`kbd`),
Settings' command lines and code (`.tf-stg-run p`, `.tf-stg-det code`, `.tf-sf-mono`), a saved view's query
(`.tf-vw-source-query`, `.tf-q`), a link set's pattern, the boot screen's message, the asset pages' codes. The one
`pre` drawn in another face on purpose is a pending write's old and new text in the write queue (`.tf-wq-pre`): it is
what a person wrote in a field, kept in a `pre` for its line breaks. `code-face.test.ts` names it with that reason and
fails on any other.

Before this, a bare `pre`, `code` or `kbd` with no rule of its own (a run's failure details, a test's details, a PDF's
or an image's failure, a renderer's errors, `code` in Settings' and the asset dialogs' hints, a dialog's preview) drew
in the browser's `monospace`: Menlo on the Mac, Consolas on Windows.

## How it stays fast

- **Nothing new is loaded on start.** A chosen system font is drawn by the system, with nothing to fetch or decode.
  The bundled faces load at idle as before (`shell/faces.ts`).
- **JetBrains Mono's stand-ins** (`tokens.css`): `local("Menlo")` and `local("Consolas")`, sized to JetBrains Mono's
  advance with `size-adjust` (99.66 % and 109.13 %) and its ascent and descent with the overrides, so a line of code
  drawn before the bundled face has landed is as wide as after, and nothing moves when it lands. WebKit takes
  `size-adjust` and not yet the overrides; every code view sets a line height of its own, so they move nothing there.
- **A choice applies at once with no reload:** two custom properties on `<html>`. A ligature in a coding font keeps the
  width of the characters it joins, so turning them on moves no line.
- **The list of fonts** is the backend's (`installed_fonts`, `crates/knobas-app/src/code_font.rs`): WKWebView has no
  `queryLocalFonts`, and WebView2's asks the person first. `fontdb` 0.24 (MIT) reads the system's and the user's font
  folders, memory-mapped, names and `post` tables only; a family counts as monospace when a face says fixed pitch or
  `ttf-parser` 0.25 (MIT or Apache-2.0) finds `i`, `M`, `0`, `.` and a space at one advance, which finds the patched
  coding fonts that do not say so (CaskaydiaCove Nerd Font among them). It runs once a run on a blocking thread, the
  first time Settings › Appearance opens, and is kept; the field takes a typed name while it is read. On the Mac:
  305 families, 9 monospace, 0.23 s in a debug build. Both crates are pure Rust, no system library.

## The design pass

The setting changes the family only. Sizes stay the type scale's (`--t-sm` 13 px at 1.6 for files, diffs and fences,
`--t-xs` 12 px at 1.5 to 1.55 for logs and test output, inline code at .82 to .93 em of its text), and every code
view's line height is a number, so a view's rows are as tall in any face at 100, 125 and 200 % and a windowed list's
estimates hold. No `font-size-adjust`: most coding faces' x-heights sit between .49 and .55 em, and a person who picks a
face picks its proportions. The field draws the typed name in the current code face; the list draws names in the
interface's face, because drawing each in its own would load every font on the machine to open a list. The one
memorable thing is the specimen under the field: three lines drawn as the Source view draws a file, numbered and
highlighted, which follow the option under the keys or the pointer before it is chosen, and say when a name is not
installed. Its third line holds `0O 1lI` and the ligature pairs, so the switch below shows what it does in it at once.

## Measured

On the Mac under its lock, the windowless backend over a copy of the saved scale database, its credentials moved out.

- **The pages that draw code** (`just open-budget --real --only docs-page,pull-request,run,repo,settings`, which takes
  `docs-page-back` and `repo-builds-run` with them), at 1x, 4x and 6x at 1440x868@1 and at 4x and 6x at
  2560x1440@1.25, on this head and on the commit before it: every open within its line on both, no row moved. A first
  open's drawn time at 6x, before and after: a Docs page 145 and 144 ms, a pull request 127 and 126, a repository 137
  and 148, a run 82 and 98, Settings 80 and 113; at 2560x1440@1.25 a Docs page 145 and 153, a run 96 and 96. The
  differences go both ways from one case to the next; the cases that rose most, run again alone, came back the other
  way (a run at 2560x1440@1.25 4x 90 against 77 ms after, Settings at 1440x868 6x 101 against 80). The face is a
  family name in a custom property, so nothing else was expected.
- **The setting's own actions** (a probe in the page, from the action to the second frame after it, over the same
  backend; 1x, 4x and 6x at both sizes): Settings › Appearance opens in 40 to 57 ms, the list in 23 to 36, the option
  under ↓ draws in the specimen in 13 to 33, choosing it redraws the app in 14 to 16, and the ligature switch in 12 to
  17. Each is the frame or two it waits for: at 6x as at 1x.
- **What stayed put** (`pixel-same.py`, 9 screens shot on this head and the commit before it at 1440x900@1,
  2560x1440@1 and 1920x1080@2: a Docs page, a file's source, a diff, a run, a ticket, Settings › Appearance, Inbox,
  Tickets, Monitors): only Settings › Appearance changed, by its two new rows; the other eight differ by 45 to 420
  pixels in the corner the same build differs by between two captures. The fake products' code was already in
  JetBrains Mono, so their pages draw as before.
