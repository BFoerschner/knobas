- **Migration `0006` and the IPC schema, issue #46 (2026-08-29):** Notes v1 — the first kind
  knobas **owns** rather than mirrors. Granted by the **orchestrator under delegation while Björn
  was away**, both halves at once, so the stream did not have to stop. **The merge gate**: when
  this entry was first written the merge was Björn's — the PR carries migration `0006` and this
  grant, two frozen surfaces. On 2026-08-29 Björn delegated exactly those ("let Migration and ipc
  additions be merged by fable too"), the same instruction the #42 entry above records, and the PR
  was merged by its merge-manager under it. Milestone exits and the contract battery's clauses
  were not delegated and remain his.

  **The migration.** `crates/knobas-db/migrations/0006_notes.sql`, allocated to this stream and to
  nothing else. `0005` belongs to the write queue (#42, PR #117), which was open and unmerged when
  this was written: **#117 merges first**. `knobas.note` has been in `0001` since M0 and nothing
  ever wrote to it; `0006` adds the three constraints that make it safe to start, and no table,
  column or index:

  - `note_entity_fk` — `knobas.note.id references knobas.entity(id) on delete cascade`. A note is
    an entity (`CONTEXT.md`), which is the whole of what makes it linkable: `knobas.link`'s two
    endpoints reference `knobas.entity(id)`, so before this a note was a table *beside* the address
    space and a note nothing could link to.
  - `note_id_ns_chk` — `check (id ~* '^note:')`.
  - `item_entity_reserved_chk` on **`sync.item`** —
    `check (entity_id !~* '^(note|ctx|asset|route|monitor):')`.

  **The third one is a change to the mirror's schema inside a migration called `notes`, and that is
  deliberate rather than sloppy.** It is the floor under *a note is never swept*. The sweep
  (`knobas_sync::SWEEP`) tombstones `knobas.entity` rows **through** `sync.item` — `update
  knobas.entity e ... from sync.item i where i.entity_id = e.id` — so "can a note be swept" reduces
  to "can a mirror row name a note", and this constraint answers no, for every namespace knobas
  keeps for itself. Notes are the one thing knobas holds that no source can hand back, and ADR-0003
  is the precedent for taking the tombstone trap seriously: absence proves deletion only where a run
  really returns everything, and an owned kind is absent from every run there is. "Notes are not in
  the sweep's kind list" would have been true and true only by accident. A later tidy-up that moves
  this constraint out of `sync.item` because it "belongs with the notes" removes the guarantee.

  The namespace list is `knobas_core::entity::RESERVED_NAMESPACES`, and it is **kept on one line**
  so the cross-check in that module's tests can find it — the same discipline `link_origin_chk`
  (`0003`) and the run-log vocabularies (`0002`, `0004`) get, and for the same reason. Pinned from
  both sides: `knobas_core::entity` walks the Rust list against this file, and `knobas-db`'s schema
  battery walks the live catalog (`pg_get_constraintdef`) and exercises the refusal in both
  directions — a `note:` id refused, an ordinary `jira:` id and a `notebook:` lookalike not.
  **`0007` is the next free number**; `0001`–`0006` are never edited.

  **The IPC.** Four commands, all in the **existing** `commands/entity.rs` and the existing
  `app/src/lib/ipc/entity.ts` — **no new module on either side**, the `commands/` + `ipc/` layout
  untouched, and both append-only barrels (`crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, `app/src/lib/ipc/index.ts`) appended to and not otherwise changed. `index.ts` needed no
  edit at all: it already re-exports `./entity`. Notes are entities and their refs are links, both
  of which already live in that module — the same reading, and the same words, as #52's grant.

  | command | Rust signature | TS mirror |
  | --- | --- | --- |
  | `create_note` | `(title: Option<String>, body_md: Option<String>) -> NoteDetail` | `createNote(title?, bodyMd?)` |
  | `save_note` | `(note_id: String, title: String, body_md: String) -> NoteDetail` | `saveNote(noteId, title, bodyMd)` |
  | `get_note` | `(note_id: String) -> NoteDetail` | `getNote(noteId)` |
  | `delete_note` | `(note_id: String) -> bool` | `deleteNote(noteId)` |

  *`create_note`'s row is the signature as of #46 and is left as the record it was: the command
  grew a third argument in issue #502 (2026-09-08) — see **One argument on one command — issue
  #502** below, which carries the current signature.*

  Additive on every axis: no existing command's arguments, return type or name changes, no event
  name changes, and `EntityDetail` keeps the shape #53 left it in. Two new DTOs ride on the new
  commands — `NoteDetail { note, refs, links }` and `knobas_core::note::{NoteRow, NoteRef}` — pinned
  by `crates/knobas-app/tests/entity_mirror.rs` the way every other interface in that mirror is,
  with the nullable field (`NoteRef.target`) exercised as `None`.

  **Shape decisions a later reader might undo without realising what they were for.**

  - **`NoteDetail` is not `EntityDetail`, and merging them is a bug, not a simplification.**
    `EntityDetail` is shaped around a *mirror row* — a `source`, a `payload`, a `web_url`, a
    `synced_at`. A note has none of those and cannot be given them: half the DTO would be
    placeholders a reader could not tell from real values, and `get_entity`'s own statement reads
    `sync.item`, which a note is forbidden to have a row in by the constraint above.
  - **`refs` and `links` overlap on purpose.** `refs` is the body's own list, in body order,
    **including the ones that resolve to nothing**; `links` is the panel #53 built, both
    directions. Collapsing them into one field loses the unresolved ref, which is story 10 — the
    only way a typo is ever discoverable.
  - **`save_note` answers with the whole detail** because it is what the autosave calls: the refs
    it has just reconciled are what the editor redraws its chips from, and fetching them in a
    second call would race the next keystroke.
  - **`delete_note` returns `bool`, not `()`.** `false` is "there was nothing left to delete" —
    the same distinction `unlink`'s `Option` exists to make, and for the same reason.

  Outside the frozen list, and recorded here because it is what the grant is *for*:

  - **`KindCatalog` gains a second map** (`owned`, from `knobas_core::entity::OWNED_KINDS`) beside
    the `declared` one it built from descriptors. **Merging them is a bug in two directions**:
    `is_empty()` means "no adapter has told me what kinds exist" — the open **E-Q2** state the
    product still ships in — and owned kinds are compiled in, so folding them in would make the
    catalog claim to have been told and start reporting `type:hypervisor` as unknown; and "first
    declaration wins" would let an adapter that declares `note` rename the user's own notes in the
    launcher. `is_declared` stays *descriptor-declared only*, which is what makes story 21's "no
    descriptor declares it" testable; `is_known` is the widened predicate the grammar uses.
  - **`corpus::NOTE` ships** and `corpus::ALL` is the one list the launcher searches. `Prefix::Note`
    leaves `empty_corpus`, which is the whole of turning the prefix on.
  - **`knobas-search` gains a dependency on `knobas-core`**, for `OWNED_KINDS` alone. The list lives
    beside `RESERVED_NAMESPACES` because every owned kind's id **is** its reserved namespace, and
    the sweep-safety argument crosses from one to the other.
  - **`PgSink::check` gains one refusal**: a kind knobas owns is not a source's to mirror, whatever
    its descriptor claims. `crates/knobas-sync/**` is not frozen (see below).
  - **`note` leaves the router's `RESERVED` set** (`app/src/lib/shell/router.svelte.ts`). It was
    listed with `inbox` and `time` so a later milestone's address rendered "arrives in M<n>". The
    milestone arrived. **Putting it back makes every note in the app unopenable**, and the symptom
    is a slide-over claiming the address is from a later milestone; a test pins it.

  No change to `crates/knobas-source/src/**`, `crates/knobas-http/**` or
  `crates/knobas-app/src/{error,profile}.rs` — `CoreError` gained no variant, so the
  `From<CoreError> for IpcError` match is untouched. Notes needed **no backup change**: the archive
  is schema-scoped (`--schema=knobas`), which is exactly why it was written that way.

