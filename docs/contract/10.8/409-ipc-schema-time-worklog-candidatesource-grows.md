- **IPC schema**, issue #409 (2026-09-05): `time::worklog::CandidateSource` grows from
  `mirror` | `activity` to `mirror` | `activity` | `write` | `note`, mirrored in
  `app/src/lib/ipc/time.ts` as the widened `CandidateSource` union. No other DTO in the #280 set
  changes: `Candidate` keeps its five fields, and `Draft` and `Worklog` are untouched.

  **Ratified by Björn directly (2026-09-05)**, who holds the gate for frozen contracts and
  exercised it here rather than delegating it. Recorded because the entry was nearly not written:
  #409's own body said "no migration and no new IPC command, so no §10.8 entry", which reads the
  freeze as covering commands and migrations. It does not -- the frozen list above says "the IPC
  command and event **schema**", and #284's `EntityRow.path` is the precedent: a field added to a
  DTO, no command, no migration, and an entry all the same. A wire type changing shape is the
  thing this section exists to record, whether or not a command changed with it.

  **Why nothing narrower would do.** Spec #272's story 32 promises the worklog draft lists five
  kinds of activity as checkboxes; two of them -- comments posted through knobas, and notes
  edited -- had no producer at all (#391), because the draft read only the mirror and
  `knobas.activity` and neither holds those facts. #409's ruling is that each fact is read from
  the table it already lives in, which takes the draft from two reads to four; `CandidateSource`
  is the field that names which one a candidate came from, so it grows by exactly the two reads
  that were added. The rejected alternative -- funnelling both through `knobas.activity` so the
  enum could stay at two -- would have meant an activity line per autosave in an append-only log
  and a `NOT_WORK` that reads a write's payload as well as its verb; #391 records that argument
  in full.

  **Additive, and inert on the wire today.** The two existing members keep their spellings, so a
  frontend built before this still decodes every value it knew. Nothing in the webview reads the
  field at all -- `app/src/lib/time/WorklogDraft.svelte` draws bullets and checkboxes and never
  branches on provenance -- so the widening changes no rendering; it is declared because the
  mirror is pinned against the Rust enum and a union short by two would fail that pin. **No new
  command, no argument change, no event change**, the `commands/` + `ipc/` module layout is
  untouched and neither append-only barrel grows a line. **No migration**: both new reads are
  `select`s over tables that already exist (`knobas.write_queue` from `0005`, `knobas.note` from
  `0001`), and #409 writes nothing anywhere.

  **A fifth member cannot arrive quietly.** The enum is declared with
  `knobas_core::closed_vocabulary!` as of this issue, so `ALL` is generated from the same list as
  the variants and the mirror pin walks `ALL` -- the guarantee `WriteState` already had in
  `tests/sources_mirror.rs`. Adding a variant without adding it to `time.ts` fails the pin; the
  hand-rolled array-and-match device is for enums whose crate is frozen and cannot use the macro
  (`AuthMethod`), which this one is not.

  Pinned by: `commands::time::tests::the_candidate_sources_match_their_typescript_mirror` (the
  union, driven off `ALL`) and `the_candidate_shape_matches_its_typescript_mirror` (the carrier).
  The behaviour behind the two new members is pinned in `tests/time_ipc.rs` by
  `the_candidates_are_the_readers_own_work_inside_the_interval` (both new kinds offered inside the
  interval and refused on **both** its edges), `a_note_saved_all_afternoon_is_one_checkbox` (twenty
  autosaves, one candidate) and `every_comment_but_the_withdrawn_one_is_a_candidate` (`sent`,
  `pending`, `held` and `refused` are work; `discarded` is not, and the read is keyed on
  `queued_at` rather than on delivery).

