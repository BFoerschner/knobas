- **One argument on one command — issue #502 (2026-09-08): a new note is born with its links.**

  A note started from a room carries `captured-in` to that room's context and `captured-from` to
  the **foreground**, as `CONTEXT.md`'s **Passive attribution** defines that word and as the
  heartbeat computes it --- the open detail, else the room's anchor, else nothing (spec #491, v1.5
  stream 5, stories 40--43; `CONTEXT.md`, **Capture**). One frozen surface, additive, and both the
  ticket and the stream map name it in advance: the ticket's own words are *"`create_note` grows an
  optional list of links to draw at creation (§10.8 entry)"* and its third criterion asks for this
  entry by name; #491's stream map row 5 says *"`create_note` grows its two links"*. **Björn keeps
  the gate for frozen contracts and this entry is flagged for his review.** What `captured-from`
  names when no detail is open was a fork, and the deputy's ruling of 2026-09-08 on #502
  ([comment](https://github.com/BFoerschner/knobas/issues/502#issuecomment-5576885970),
  `docs/archive/2026-09-v1-5-unattended-rulings.md`) ratifies the reading recorded here --- the
  defined term, not the ticket's narrower phrasing of its first rung.

  **The IPC schema — one argument and one input DTO, on a command that already exists:**

  ```rust
  #[tauri::command]
  pub async fn create_note(title: Option<String>, body_md: Option<String>,
                           links: Option<Vec<NoteLinkInput>>) -> Result<NoteDetail, IpcError>;

  pub struct NoteLinkInput { pub target_id: String, pub relation: String }
  ```

  mirrored in `app/src/lib/ipc/entity.ts` as `createNote(title?, bodyMd?, links?)` and
  `interface NoteLinkInput { target_id: string; relation: string }`. Absent `links` and an empty
  list are the same answer, which is what lets a caller with nothing to attach say so either way
  and what makes this additive rather than a version of the command: `serde` reads a missing
  `Option` as `None`, so every existing caller decodes unchanged. (*New note* sends `[]` from a
  derived room; #503's capture window may send neither.)

  **A list of pairs, not two named fields.** `capturedIn` and `capturedFrom` would put the
  *capture's* vocabulary into the command, and the command's job is *draw these links with the
  note* --- the same latitude `create_link` takes when it accepts a relation rather than knowing
  what `blocks` means (§5a: relations are open). The two words live where the rest of the relation
  vocabulary lives, `app/src/lib/detail/relations.ts`, which is also where they join the curated
  menu with both of their readings (`captured in` / `captured here`, `captured from` /
  `captured from here`) --- the ticket asks for that, and an uncurated relation reads the same word
  from both ends, so a context's own panel would say the room was captured in the note. #503's
  capture window passes the same two through the same argument, and a third caller with a third
  relation needs no further change here.

  **What the two relations name.** `captured-in` is the room's **context**, so it is `null` for
  every derived room (`CONTEXT.md`, **Room**) and, where there is one, it is the **membership
  write**: ADR-0008's seed is *"every confirmed link touching the context's own `ctx:` entity"*.
  `captured-from` is the foreground, and in a promoted room with nothing open those two draw the
  context and its anchor --- two facts that share a name in the panel, the same split the timer in
  that room already makes when it runs on the anchor and not on the context (`contexts.ts`,
  *"Never the context's own id"*). The ladder has one spelling, `shell/timer.ts`'s
  `roomForeground`, called by `App.svelte`'s heartbeat foreground and by `Room.svelte`'s
  `bornWith`, so what a capture attaches equals what the heartbeat would send at that instant.

  **The four rules the argument carries**, all of them stated because each is a thing a later
  reader could reasonably decide the other way:

  1. **Origin is `manual`, not `implied`.** `implied` is `knobas_core::note::reconcile_refs`'
     population and nothing else's, and both link panels refuse to unlink an `implied` row out of a
     note with the words *"This link comes from a `[[reference]]` in the note"*
     (`notes/NoteView.svelte`, `detail/Detail.svelte`). That sentence is false of a capture link,
     and under it the reader could never withdraw one. A capture link is an ordinary link: drawn
     because the reader wrote a note there, withdrawn from the panel like any other. It is also
     outside `withdraw_refs_other_than`'s reach, so a note's first autosave does not take it ---
     but **that is the relation's doing as much as the origin's**: the clause is scoped to
     `(this note, relation `references`, origin `implied`)` and a born link fails both halves, so
     neither can be shown to matter by removing it alone, and what
     `a_born_link_survives_the_notes_first_autosave` pins is the conjunction.
  2. **They are drawn in the note's own transaction** (`knobas_core::note::create` grows a
     `&[BornLink]`), which is the whole of the ticket's title. A note that existed for a moment
     without its `captured-in` link would be, for that moment, a thought belonging to no context.
  3. **A malformed target is `invalid`; a well-formed target with no `knobas.entity` row draws no
     link and the note is written anyway.** The first is a caller bug and deterministic; the second
     is nothing a reader can act on, and refusing there would make *New note* a button that stays
     broken while they can do nothing about it. The precedent is the **heartbeat**, not
     `reconcile_refs`: `commands::time`'s `heartbeat` writes the observation with no target when
     the foreground is one the timer could never run on --- *"losing the attribution is honest,
     losing the observation is not"*. The note is the observation and a born link is the
     attribution. (`reconcile_refs` shares the `select ... from knobas.entity` and not the
     precedent: a ref that resolves to nothing is *shown back*, because that is how a typo is
     found.)

     **Which case this is, exactly**, because the obvious guess is wrong: an id *no row ever
     carried*. A withdrawn entity has a row --- a purge tombstones and never deletes
     (`knobas_sync::config`'s `PURGE_ITEMS`; `CONTEXT.md`'s **Purge** carries *delete* on its
     *Avoid* list for this reason), and a context is never deleted at all --- so a born link to
     something the source dropped **is** drawn and the panel shows it marked, which is what
     `LinkEnd.deleted_at` exists for. What reaches this clause is a caller handing an id it did
     not read from a row, or one remembered across a database that changed under it: #503's
     capture window keeps a room and a foreground between sessions.

     **The answer codes.** `create_note` can now answer **`invalid`**, which it could not before
     --- for a malformed target and for a blank relation, the two caller bugs --- and it answers
     nothing else new: no `not_found`, no `conflict`. Recorded because #440 settled that the set
     of codes a frozen command can answer with is part of what §2 pins.
  4. **The relation is folded to lower case**, like every other relation this module writes, so
     `Captured-In` and `captured-in` cannot become two group headers neither of which sees the
     other --- and a **blank** relation is `invalid` rather than `related`. `create_link`'s
     relation is an `Option` whose absence means *nobody named one*, which `DEFAULT_RELATION`
     answers; here the reader saw no dialog, so `""` is a caller bug and a link labelled `related`
     that nobody asked for is the wrong sentence in the panel rather than an error anybody could
     find.

  **One thing this does not do, named because a reader of the log will meet it.** Neither the note
  nor its links write an activity line --- nothing in `knobas_core::note` ever has, for the note or
  for its `[[ref]]` links. A born link is `manual`, so the panel *does* let a reader withdraw one,
  and that writes an `unlinked` line whose `linked` partner never existed, against the pairing
  `link_detail` promises. Recording a line here would announce a note's links while the note's own
  birth stayed silent; the fix, if it is wanted, is a line for the birth covering the note and both
  links at once, which is a ticket rather than a clause. **The class predates this ticket**: a
  `monitored-by` link drawn by the estate apply is `Origin::Imported` and withdrawable from the
  same panel, whose only refusal is `implied`, and it was never announced by a `linked` line
  either (`assets/mod.rs`, `knobas-sync/src/attach.rs`), so what #502 adds is a second population
  to a milestone-old class. Ruled not owed here by the deputy on 2026-09-08 and filed as **#524**,
  outside v1.5 for Björn to place. **#524 closed it on 2026-09-11**: `note::create` now writes one
  `created` line inside this same transaction, naming every born link by the `link_id` a later
  `unlinked` line carries, `note::delete` writes one `deleted` line, and a save still writes none
  (#409) --- no migration, no IPC change, and nothing in this entry's frozen surfaces moved.

  **No field on the note, and no membership write.** `CONTEXT.md`'s **Capture** says the first;
  the second is ADR-0008, whose seed is *"every confirmed link touching the context's own `ctx:`
  entity"* --- the `captured-in` link **is** the explicit add, so `context::member_ids` answers
  with the note without a second statement anywhere. `NoteDetail`, `NoteRow`, `NoteRef` and
  `LinkEntry` are untouched: the links come back through the `links` list the note view already
  draws.

  **What is not touched.** **No migration** --- `knobas.link` is `0001`'s and the born links are
  ordinary rows. **No new command, no new module, no barrel line**: `create_note` has been in
  `crates/knobas-app/src/lib.rs`'s handler list since #46 and `app/src/lib/ipc/index.ts` already
  re-exports `entity.ts`, so the `commands/` + `ipc/` module layout is unchanged and neither
  append-only barrel grows. No event. Nothing in `crates/knobas-source/src/**` --- no `WriteOp`
  grows and no descriptor gains a slot, and a link is never written back to a source
  (`CONTEXT.md`, **Link**). Nothing in `crates/knobas-http/**`;
  `crates/knobas-app/src/{error,profile}.rs` untouched; the keychain envelope unchanged at
  version 2. The **glossary** gains one cross-reference and no entry: `CONTEXT.md`'s **Capture**
  (added 2026-09-07 by #494) already states this behaviour, including *"The in-app New note
  attaches the same two, by the same mechanism"*, and its *"the foreground entity if there was
  one"* now says which word that is --- *"the word as [Passive attribution](#passive-attribution)
  defines it, so a promoted room with nothing open gives its anchor"* --- as the ruling directs.
  Nothing here reaches past `sync.live_item`, so **Live item**'s census of three readers is
  unchanged.

  Pinned by: five tests in `crates/knobas-app/tests/entity.rs` over a real PostgreSQL ---
  `a_note_born_in_a_stored_room_carries_both_links_and_is_a_member` (both relations, both origins,
  the note as the `from` end, `context::member_ids`, and the backlink from the ticket),
  `a_note_born_with_only_a_foreground_carries_only_that_link` (the two are independent),
  `a_note_born_with_no_links_is_born_with_none` and
  `a_born_link_is_refused_for_a_bad_address_and_skipped_for_an_absent_one` (the two refusals, the
  skip, and the tombstoned target that is drawn rather than skipped) and
  `a_born_link_survives_the_notes_first_autosave` (both links and the membership, after a save of a
  body naming no ref --- the flow *New note* itself opens, since `NoteView.svelte` autosaves 700 ms
  after the first keystroke); three in `app/src/lib/shell/timer.test.ts` for the foreground ladder, its `canBeTarget`
  fall-through and the scan that keeps it spelled once;
  `entity_mirror.rs`'s `the_note_link_input_shape_matches_its_typescript_mirror`, which round-trips
  the input DTO so a Rust-only field cannot hide; and, on the frontend, seven in
  `shell/Room.test.svelte.ts` for what *New note* sends --- four for the rooms (a stored room, over
  an open detail, the three derived rooms, and over a detail in a derived room) and three for the
  ruled second rung (a promoted room with nothing open, an ad-hoc one, and the one place the two
  rungs compete: a detail open over a promoted room, where the detail wins) --- plus two in
  `detail/relations.test.ts` for the two readings of each word and the grouping they produce on the
  context's own panel.

