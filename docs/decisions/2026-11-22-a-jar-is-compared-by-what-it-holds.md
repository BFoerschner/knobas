# 2026-11-22: A JAR is drawn in a folder renderer's output, and compared by what it holds

**Asked (Björn, on PR #2479):** "for the folder comparisons. There a jar isn't rendered because it's shown as binary
file and I can't select a renderer." Then: "folder renderers i mean".

The shape of a JAR's comparison went to Fable (xhigh) under Björn's standing instruction, and Opus built it. This
records why it failed, the rulings and what was built.

## Why it was "binary file"

- **A generated folder's file view** (ADR-0046) filtered the JAR renderer out of a generated file's renderers: the
  archive reads named a repository's file by entity, path and commit, and a generated file has none of them. So a JAR a
  folder renderer wrote was "A binary file, not drawn.", and *Render as* had nothing to offer.
- **Every comparison** (a folder renderer's two outputs, two commits, the working tree, a pull request, Local
  changes) planned a rendered mode only for a renderer with `diff: true`. JAR and OpenAPI had `diff: false`, so a
  changed JAR was "A binary file: no lines to draw." with no *Rendered* and an empty *Render as*. Images and PDFs were
  already drawn there (side by side, swiped, onion-skinned; a PDF page by page).

## The rulings (Fable)

1. **A JAR in a comparison** is one column in the stage, like a PDF: a head line and a list of only the entries that
   differ. No side-by-side mode: two whole trees do not show what changed, and the Files tab browses one already.
2. **Reads.** Each side is listed by the backend, never through the byte read: `DiffContext.archive` (`archive_open`
   at the side's commit, `~working-tree` or `~index`) and `folder_render_archive_open` for a generated one. A side's
   listing is its `SideGot`, so absence, failure, the epoch and the foot's reading words stay as they were. A handle
   the backend let go (it keeps eight) is listed again once.
3. **Changed** is told by name, CRC-32 and size: the index carries `crcs`, aligned with `sizes` (additive). A name
   listed twice is compared by its first entry.
4. **Rows**, flat by path, 29 px: the diff card's badge, the path, its sizes (`12.4 KB → 12.9 KB`). A top-level class
   and its nested classes are one row (`and 2 nested classes`), which is also what is decompiled. A library removed
   and one added in one folder under one name is one changed row (`6.1.2 → 6.1.3`). A one-sided JAR lists every entry.
5. **The head line:** `ledger-service 1.4.0 → 1.5.0 · Spring Boot JAR · Java 17 → 21`, an arrow only where a value
   differs.
6. **An opened row** draws once its reads are in, in one frame, at its own height inside the stage's scroller, so the
   card's outside never moves; meanwhile its size's place says *Reading…* or *Decompiling…*. At most 400 lines, then
   *Show the other N lines*. Changed text: its line diff. A changed image: its two frames. A changed nested JAR: its
   own list one indent in. Other bytes: *A binary entry: 12.4 KB before, 12.9 KB after.* Added or removed: its one
   side as the JAR view draws an entry.
7. **A changed class:** *Outline | Source* at the caption's right end where a Java is found, kept in this browser
   (`knobas.diff-class`), *Source* first. Source is the line diff of the two decompiled top-level classes, under
   *Decompiled by ⟨decompiler⟩ on both sides: not the original source.* Outline is the diff of the outline's lines,
   one block per named class that differs. With no Java: no seg, the outline. With a Java too old: *Source* disabled,
   the reason its title. Decompiles run one at a time app-wide in press order; one stopped by another is asked once
   more, then the outline stands with the reason. Only a press starts a JVM, never the band's *Rendered*.
8. **Words.** The foot: `3 changed, 2 added, 1 removed; 1,198 the same`; one side's `1,204 entries, all added`.
   Nothing differs: *Every entry is the same on both sides: the archives differ only in how they are packed.*
9. **OpenAPI** has `diff: true`: two views, one a side, each its own scroller at the stage's height. It is planned
   with no read where the diff holds text that sniffs as a spec (an added or deleted file whole, a first hunk from line
   1, a YAML version key line in any hunk) or the name says so (`openapi`, `swagger` or `.oas.`, ending `.json`,
   `.yaml` or `.yml`). A side that is no spec says *Not an OpenAPI or Swagger spec on this side.*; neither: *Neither
   side is an OpenAPI or Swagger spec: Lines has the change.* Known gap: a modified spec with a plain name changed away
   from its first lines keeps *Lines* only, until the diff's backend marks specs as it lists files.

Rejected: two JAR views side by side (above); a side-by-side mode beside the list (a second surface for browsing
unchanged entries); "changed" by size alone (misses same-size edits); the outline as the default (most changes are
inside methods, so it would mostly say "the same"); OpenAPI offered on every JSON or YAML file (every config file would
gain a switch that leads to "not a spec").

## As built (Opus)

- `folder_render_archive_open(token, path)` (`folder_render/made.rs`, `archive::open_kept`): a generated archive read
  up to the archive cap and listed by the archive store's own reader, kept by the token and the path (a generated
  folder is never rewritten). The Files tab hands it to the JAR view as its `open`.
- `archive_open` reads `~index` from the checkout as it reads `~working-tree`.
- `render/jar-diff.ts` (the compare, the foot's words, the head line, the outline's lines), `tf/diff/TfJarDiff.svelte`,
  `tf/diff/class-mode.svelte.ts`; the stage (`TfDiffRendered`) draws the JAR column, the seg and the foot; the view
  (`TfDiffView`) reads each side's listing.
- Departures, each small: one-sided entries reuse `TfJarCode` and `TfJarClass` rather than lifting the JAR view's
  detail snippet; a one-sided PDF or Markdown entry draws as text or a binary note, not the PDF viewer.

## Tested

- Rust: `folder_render::made`'s generated archive (listed, kept with no second read, another token read again, a path
  outside the manifest and a non-archive refused), `archive`'s CRCs, `tests/it/working_reads.rs` for `~index`.
- Vitest: `jar-diff.test.ts`, `TfDiffJar.test.svelte.ts`, `TfDiffView.folders.test.svelte.ts` (a generated JAR by
  token), `TfDiffRendered.test.svelte.ts` (OpenAPI's planning and its words), `rendered.test.ts` (`specHint`),
  `TfRepoFiles.made.test.svelte.ts` (a generated JAR in the JAR view).
- 29 hand mutants on the decision logic (the JAR's plan against `can.archive`, the registry's `diff`, the listing
  guard, the generated folder's JAR, the CRC and size compare, families, library pairing, the words, `specHint`, the
  per-name plan cache): all killed.
