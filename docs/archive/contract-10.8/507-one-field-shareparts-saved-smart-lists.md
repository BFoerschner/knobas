- **One field on `ShareParts`, issue #507 (2026-09-08): saved smart lists in the share export.** The
  share export's parts gain **`smart_lists`**, on by default, and the dialog draws its toggle only
  once a saved list exists. Ratified in advance by `CONTEXT.md`'s **Share export** entry, whose
  sentence *"Smart lists are built-ins, so there is no toggle for them until a saved one exists"*
  predates #506 and whose #506 amendment names this ticket by number (*"the toggle, with the
  archive's `smart_lists` part behind it, is **#507**'s"*), and by spec #491's stream map row 8,
  which counts *"the share-export part"* in the same stream the orchestrator split this ticket out
  of. Written with the implementing PR, per #454's, #455's and #506's pattern. **Björn keeps the
  gate for frozen contracts and this entry is flagged for his review.** In his absence the v1.5
  loop's deputy is what exercises that gate, and its ruling is recorded on the issue and appended to
  `docs/archive/2026-09-v1-5-unattended-rulings.md` by the PR that acts on it; **no ruling had
  been posted on #507 when this entry was written**, so this sentence records the flag and claims no
  ratification — **ratified in his absence by the deputy's ruling of 2026-09-08 on #507**
  ([comment](https://github.com/BFoerschner/knobas/issues/507#issuecomment-5579888742),
  `docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the field, on
  the criterion's second clause, on both decisions taken rather than forked and on the disclosed
  walk. The two sentences stand side by side rather than the first being rewritten, the treatment
  §10.8 gives every sentence it supersedes.

  **The DTO — one boolean on an existing one, and nothing else on the wire:**

  ```rust
  pub struct ShareParts {
      pub links: bool,        // default true
      pub assets: bool,       // default true
      pub contexts: bool,     // default true
      pub notes: bool,        // default false
      pub time: bool,         // default false
      pub sources: bool,      // default true
      pub smart_lists: bool,  // new, default true
  }
  ```

  mirrored in `app/src/lib/ipc/backup.ts` as one field on `interface ShareParts` and one key on
  `shareDefaults`. **`#[serde(default)]` is already on the struct** and is what this field rides in
  on: the attribute #454 put there for exactly this case means a knobas built before today still
  decodes a payload naming `smart_lists`, and a caller that sends the six older keys gets the
  ratified answer for the seventh. That second half is **asserted by name** in
  `a_partial_payload_decodes_onto_the_defaults`, not left to the attribute: `smart_lists` is the
  first field to make #454's sentence testable, and an equality against `Default` alone would hold
  even if a later field arrived with the wrong ratified answer, since both sides of it would be
  wrong together. **No new command, no command renamed or removed, no argument
  added to one, no return type changed, no new DTO, no event, no settings key, no `Kind`, no
  reserved namespace, no `WriteOp` (a saved list is knobas-local and no source has heard of one),
  no `Capability`, and no migration** — `knobas.smart_list` is `0025`'s and exists; `0026` is still
  the next free number and this entry claims none. The `commands/` + `ipc/` module layout is
  untouched and **neither append-only barrel grows a line**: `share_export` is already in
  `generate_handler!` and `app/src/lib/ipc/index.ts` re-exports whole modules.

  **The part is one table, and it is the only part that brings no address book.** `smart_lists` →
  `smart_list`. Every other referencing part carries `entity` with it, because an entity row is the
  address of the thing the part is about; a saved smart list has no id in `knobas.entity`, nothing
  links to one and no context holds one — it is a query somebody wrote down (`0025`'s own header).
  So the table list for this part is one name long, and `ENTITY` is deliberately absent from it.

  **On by default, with links and not with notes.** A saved list is a query over the link map — the
  same map the links part carries — and not a private note about it: what it holds is the text of a
  search and the name somebody gave it, which is the kind of thing this feature exists to hand over.
  The two parts that are off by default are off because they are somebody's *contents* (a note's
  body) or somebody's *hours*, and a saved query is neither.

  **The toggle is conditional, and hiding it is not what keeps the table out of the archive.** The
  dialog draws no `smart_lists` checkbox until this knobas holds a saved list, which is
  `CONTEXT.md`'s sentence and is what makes the toggle mean something — the built-in lists are code
  and are rows nowhere, so before #506 the part would have been a checkbox about nothing. The
  consequence worth recording, because a reader will otherwise assume the cheaper mechanism:
  **`pg_dump` writes a `TABLE DATA` entry for every table its argument list names and never counts
  rows first** — asserted on this branch by
  `an_empty_saved_list_table_is_still_in_the_archive_while_the_part_is_on`, not argued in a comment,
  because it is a premise about a *tool* under a decision taken in the *webview* — so an export
  taken with the part *on* names `smart_list` whatever this knobas has saved. Hiding the toggle therefore hides nothing
  by itself, and the dialog **sends the part off** while no saved list exists (`openShare` opens the
  draft on `{ ...shareDefaults, smart_lists: savedLists }`). That is the one place a fact about the
  archive is decided in the webview, and it is decided there because the *toggle's* condition has to
  be decided there anyway: the section reads the launcher's own rail (`smart_lists`) and asks it for
  one bit, whether any list on it is `saved` — the flag #506 put on the row for this question. A
  rail that cannot be read costs the toggle and nothing else: `savedLists` stays false, no error is
  drawn over the backups, and the export is the smaller one.

  **A stored query crosses exactly as it was written.** `CONTEXT.md`'s **Smart list** rules that no
  migration ever rewrites `knobas.smart_list.query` to a newer grammar, because an upgrade is a
  parse in disguise — it commits the migration's reading of what the reader wrote — and would make
  *needs attention* a state no row can reach. An **export or an import that upgraded a stored
  query would be the same thing wearing a different hat**, and this entry records that the rule
  binds all three: a `pg_dump` table list is the one shape that cannot rewrite anything, and the
  seam test puts a row today's grammar refuses through an archive and reads the rail's verdict on
  both machines.

  **The cap is not in the archive, and that is a decision and not an omission.**
  `knobas_search::saved::create` counts the table and refuses the row past `MAX_SAVED_LISTS`, and
  nothing else enforces it — a restore is a schema dump, so a restored database holds however many
  lists the archive it came from held. **Every row arrives.** The alternative — a restore that
  dropped rows to fit today's constant — is a restore that loses data silently and would have to
  choose *which* rows to lose, and it would do it on the machine least able to notice. The cap
  reasserts itself the next time anybody saves a list, which is what makes it a bound on the board's
  cost rather than a property of the schema.

  Stated now although **nothing in the tree can make the two disagree today**: the constant has
  never moved, so every row any archive carries was written past a `create` holding the same bound,
  and the seam test reaches the state with an `insert` past `create` because that is the only way
  to reach it. It stops being a contrived state the day the number changes, and there is an open
  ticket to change it (#533, PR #540, unmerged as this was written — its measured curve would put
  the constant at 16, and an archive taken before it would then outrun the cap on the machine
  restoring it). This entry records the reading, not that ticket's outcome.

  **#540 landed first** — `MAX_SAVED_LISTS` is **16** on the commit this entry merges on, and was 64
  when the paragraph above was written. That paragraph stays as the history it is, side by side with
  this one; what it calls a contrived state is live from here, and nothing else in this entry
  changes, because the reading it records is the one that says so. The seam test asserts against
  `MAX_SAVED_LISTS` and never against a number, so it read the same at 64 and reads the same at 16.

  **What is not touched.** `crates/knobas-source/**`, `crates/knobas-http/**`,
  `crates/knobas-secrets/**`, `crates/knobas-app/src/{error,profile}.rs`, the keychain envelope
  (still version 2), `crates/knobas-db/src/backup.rs` (the archive is an argument list; `dump_tables`
  and `restore` are unchanged), and every other part's table list. `knobas.setting` is still in no
  part, so the `search.smart_list_seen` key behind a list's change badge does **not** cross: a
  restored list's badge is the recipient's own question about their own corpus, and it is the same
  answer `knobas.setting` was excluded for in #454.

  Pinned by: `commands::backup::tests::the_share_parts_serialise_the_keys_the_mirror_declares` (both
  directions, now seven keys, with the ratified default asserted); `backup::share::tests`'
  `the_defaults_are_the_ratified_ones`, `each_part_alone_is_exactly_its_own_tables` (the part
  alone is `["smart_list"]`, with no `entity` beside it) and
  `a_partial_payload_decodes_onto_the_defaults` (the six-key payload of a caller built before this
  field, decoding to `smart_lists: true`); in `crates/knobas-app/tests/backup_ipc.rs`
  over scratch databases, `a_share_export_restores_the_link_map_and_leaves_the_hours_behind` (the
  saved list crosses with its id, its name and its query),
  `switching_the_saved_lists_part_off_leaves_the_list_table_behind` (two archives from one populated
  database, so the absence is the argument list's doing and not the fixture's),
  `a_saved_query_crosses_an_archive_exactly_as_it_was_written` (a row `create` could not make,
  refused by the rail on both machines) and
  `an_archive_holding_more_saved_lists_than_the_cap_restores_all_of_them` (every row, and the next
  `create` refused); and, on the rendered side, four tests in `BackupSection.test.svelte.ts` — no
  toggle and no part with nothing saved, the toggle up and ticked with one saved, unticking it, and
  a rail read that fails. The premise under the dialog's behaviour is pinned separately by
  `an_empty_saved_list_table_is_still_in_the_archive_while_the_part_is_on`, whose other half is this
  criterion's *"with no saved list … the archive carries no list table"* read at the seam.
