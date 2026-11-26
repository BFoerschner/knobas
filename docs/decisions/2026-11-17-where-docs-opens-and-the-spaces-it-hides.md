# 2026-11-17: Where Docs opens, the spaces it hides, and a filter over its spaces

**Asked (Björn, on PR #2479):** "i'd like you to add the ability to change in the docs area the default open page. i'd
like it to be a certain area. Also, i'd like to just be able to like, hide areas that I'm not using? I still want it to
be searchable, just not visible (except when i unhide somehow again) so the docs page becomes usable for browsing
again, not just opening pages via launchctl." Then, the same day: "Also, I'd like to be able to filter spaces with,
like, a small filter, like, in the code pages."

An area is a Confluence space. At his size Docs holds about 5,000 pages over many spaces, and every one of them stood
in the rail and on the home, so the place to browse was the launcher.

## What was built

### Docs' home: where Docs opens

- **A space's own page or one page.** *Open Docs here* in a space's right-click menu (the rail's space, a tile's head,
  a space's own heading, the hidden fold's row) or a page's (a tree row, a link, a page's heading) makes it Docs' home;
  on the home the item reads *Stop opening Docs here*. Settings › Docs › *Docs opens on* lists Docs' own page, then
  every space, hidden ones marked, then the page chosen when a page is; a page is chosen from its own menu, since a
  list of every page would be thousands long. A toast says *Docs opens on Engineering now.* with *Undo*.
- **Opening Docs with nothing in it chosen lands there**: the Docs entry's first press, after a restart too (the
  choice is read once the database answers, before the entry is pressed), the entry pressed inside Docs (the section's
  own page is the home now), and the launcher's *Docs* row. The page opened ahead (`ahead.ts`) goes there too, since it
  follows the entry.
- **A place left in Docs still wins on a return** (*Section's place*, 2026-11-13). A search left is still the entry's
  own page inside Docs, as section memory keeps it. Docs' own page, `#/docs`, stays what the crumb, *Back to Docs*
  and `Esc` lead to: it is the one place that lists every space, and the 2026-11-05 ruling made it a page of its own.
  Standing on it does not take the home's place as the entry's own page.
- **A home the mirror no longer holds** (a page deleted, a space gone) opens `#/docs` in its place, in the frame the
  read says so, with a toast: *Docs' home, Payments, is not in knobas now, so Docs opened on its own page. Settings ›
  Docs chooses another.* The choice is kept, so a page that comes back is the home again. Checked where Docs reads it,
  not when the choice is read: knowing whether a space is held needs the tree, which at his size is a 600 ms read
  cold, and the shell reads the choice at start.

### Hidden spaces

- **Hidden from browsing, never from finding.** A hidden space leaves the rail's list of spaces, the home's tiles and
  the Space picker's list. The launcher and Docs' search find its pages as before, a link into it opens, *Continue
  reading*, *For you* and the job's pages still list its pages (they are about the reader, not browsing), and the sync
  keeps mirroring it. Hiding deletes nothing.
- **The fold** follows Monitors' (#2462): after the rail's spaces and after the home's tiles, *3 hidden spaces* with
  *Search still finds their pages.*, closed until the reader opens it, kept as left for the session (one fold for the
  rail and the home). A row is the space's name, dimmed, as a link to its own page, its count, and *Unhide*. The home's
  header counts them: *42 spaces (39 hidden), 5,032 pages*, as Monitors says *4 monitors, 2 hidden*.
- **The Space picker** lists the hidden spaces last under *Hidden spaces*: hidden from browsing is not hidden from
  choosing, and a page of a hidden space needs its space in the picker to show.
- **A page of a hidden space** draws its space's tree in the rail, as any page does, with *Engineering is hidden from
  Docs.* and *Unhide* above it.
- **Hide and unhide** from a space's right-click menu (*Hide from Docs*, *Unhide*), the fold's *Unhide*, and Settings ›
  Docs › *Spaces shown in Docs*, a box per space (a box cleared hides it), which is how many go at once. Each is drawn in
  the frame it is pressed and written behind; a failed write puts it back and says so; the toast's *Undo* takes it
  back.
- **Search marks it** (the brief left it to the design): a hit in a hidden space says *in a hidden space* after its
  place in Docs' search, because the rail and the home no longer show where that page lives. The launcher does not
  mark: it is a search over everything, hiding never narrows it, and its rows name no space to mark without a read.
- **A space, not a subtree.** Björn's "areas" are spaces, and a page's subtree already closes with its fold in the rail.
  Hiding a subtree would need a second kind of fold inside every tree and a rule for its pages on the home's tiles; it
  can be added on a page's menu later without changing what is stored.

### The spaces' filter

- **Code's own bar** (`FilterBar`, its `.tf-fsearch` field, *Filter spaces*), at the top of the rail under the Space
  picker, drawn without a band of its own. Typing narrows the rail's spaces to those whose name (with its source where
  another source's space shares it) or key holds every word, as typed, with nothing read. Its count says *3 of 40
  spaces*. `Esc` clears it (`filterKeys`). Its words are kept for the session (`sessionFilter("docs-spaces")`), as
  Code's are, so a return finds them. It is drawn while the rail lists the spaces, which is what it narrows; on a page or
  a space's own page the rail is that space's tree, as before, and *All spaces* in the picker brings the list and the
  filter back (a 420 px tall window's tree lost a row to it otherwise, the press trips' `kept-docs-tree`).
- **⌘F is not the filter's.** On Docs' home ⌘F opened the launcher on Docs' search, and still does: the filter is a
  box ⌘F never takes (`filterKeys`' `rail`, a bar's `findKey: false`), since it finds nothing in the page. On a page
  ⌘F opens the page's find bar, as before.
- **The fold too** (Monitors' rule, #2462): the filter applies inside the hidden fold, which lists the hidden spaces
  the words match, and opens by itself when they match no shown space, until the reader closes it. *No space's name or
  key holds zzz.* with *Clear* when nothing matches at all.
- **Not windowed**: the rail's list is one closed fold per space, which is what the rail drew before; each space's tree
  is the windowed list (`TfDocTree`), as before. The filter only takes rows away.

## Where the choices are kept

**The database, as `knobas.setting` rows, not per-machine settings.** `docs.home` holds the home as `{kind, id, title}`
(a space as `<source>/<key>`, a page by its entity id, and its name when chosen, so Settings names a home the mirror
lost); a hidden space is a row of its own, `docs.hidden_space.<source>/<key>`, so a hide is one write and never a
read-modify-write of a list another press could race, as *Run embedded HTML*'s rows are. No migration. Why there:

- They are choices about what the mirror holds, as a hidden monitor is (`knobas.monitor_hidden`, #2447), not about this
  machine, as the code font is (`localStorage`, because it names a font installed here).
- A backup carries them, and a share export does not (`knobas.setting` is in no part of one).
- The shell reads them once the database answers (`docs_choices`, one indexed read of a few rows), before the Docs
  entry can be pressed, and the windowless backend the probes and press trips drive answers them as the app does.
- A source deleted takes its rows with it (`forget_docs_choices`), as it takes its embeds' row.

IPC, additive: `docs_choices`, `set_docs_home`, `set_docs_space_hidden`, and the generated `DocsChoices`, `DocsHome`,
`DocsHomeKind`.

## Tested

- Vitest: `TfDocs.choices.test.svelte.ts` (the rail's list and its fold, *Unhide* at once with *Undo*, a failed hide
  put back, the picker's *Hidden spaces*, a page of a hidden space and its note, the home's tiles, count and fold, a
  search hit marked, the filter by name and by key with its count, `Esc`, the fold under the filter opening by itself,
  the words kept on a return, no filter over a page's or a space's tree, ⌘F still the launcher's, a home page and a
  home space gone, a page not held that is not the
  home); `docs-choices.test.ts` (the addresses, a later press standing over an earlier answer, *Undo*, a failed write, a
  read a write overtook, the rail's split and filter); `TfShell.test.svelte.ts` (the entry's first press at a home
  space, the place left winning, inside Docs going home, `#/docs` not taking the home's place, a search kept, a page
  home, none chosen); `actions.test.ts` (the launcher's Docs row); `items.test.ts` and `thing.test.ts` (*Open Docs
  here*, *Stop opening Docs here*, a space's menu, *Hide from Docs* and *Unhide*, a space read off an address);
  `TfDocsSection.test.svelte.ts` (Settings › Docs).
- Rust: `docs_ipc.rs`, `docs_choices_are_kept_and_go_with_their_source` (written and read back, unhidden, a page home,
  ids of no space or page refused, a deleted source's rows gone and `_` taken literally).
- Mutants: see the PR comment for the list; each fails a test above.
- Timing: `testenv/perf/docs-choices.mjs` (hide, unhide, Docs' first open and return with all but three spaces hidden
  and with a home space) and the keys probe's `docs-spaces-letter` and `docs-spaces-two-letters`; the numbers are in
  the PR comment.

## Reversing it

Drop the shell's `docsOwn` (the entry back to `sectionTabs.docs ?? sectionHash("docs")`), the launcher's row back to
`sectionHash`, and `railSpaces`' hidden set from the rail, the picker and the home; the rows stay in `knobas.setting`
and do nothing. The filter is one `FilterBar` and its session state.
