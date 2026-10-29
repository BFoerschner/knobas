- **IPC schema, issue #204 (2026-08-31):** `QueuedWrite` gains `source_enabled: bool` and
  `SourceRef` (riding inside `EntityDetail`) gains `enabled: bool`, with the matching declarations
  in `app/src/lib/ipc/sources.ts` and `app/src/lib/ipc/entity.ts`. Both are **derived at read
  time, never stored** — there is nothing they could be stored *in* without being stale the moment
  the user re-enables. Additive; no existing field changes meaning, no new command, no new event,
  no migration. Written with the implementing PR per the #175/#177 pattern, citing the ruling
  below.

  **The ruling this records.** Björn, 2026-08-31, answering the two questions the #202 entry above
  left open — both **yes, close them**, as one defect with one root cause: migration `0012` gave
  `sync.live_item` a second reason to hide a row, and nothing downstream could tell "source turned
  off" from "withdrawn upstream". The bounds ratified with it: derived rather than stored, no
  migration (`WriteState`'s vocabulary unchanged — the write is genuinely *held*, only its
  explanation is new), additive on the IPC schema, and the three states a reader can be in —
  **withdrawn upstream, source turned off, present and fine** — must stay distinguishable, because
  collapsing any two recreates the defect. The carrier was left to the implementer within those
  bounds.

  **The exact wire shape**, one boolean on each of the two surfaces the ruling names:

  ```rust
  // knobas_core::write_queue — derived by the queue_columns! macro, so every
  // statement that returns a row carries it:
  pub struct QueuedWrite { /* … */ pub source_enabled: bool }

  // knobas_app::commands::entity — derived in the DETAIL statement:
  pub struct SourceRef { pub id: String, pub display_name: String,
                         pub adapter_kind: String, pub enabled: bool }
  ```

  Both read `coalesce(source_config.enabled, true)` — `false` only when a configuration row exists
  and says off. That is `0012`'s own direction, and it answers the same question: *did the user
  turn this source off*, never *was this source ever configured*. `run_once` queues writes for and
  mirrors items from unconfigured sources, and those must not claim the user turned anything off.

  **The carrier, argued rather than inherited.** The ruling offered three homes — `QueuedWrite`,
  `SourceRef`, or `knobas_sync::CredentialHealth`, which every source-listing surface already
  fetches — and noted that doing it once for every surface is worth more than doing it twice
  narrowly. The two per-row flags were chosen over the one `CredentialHealth` field deliberately:

  - **The marker must describe the same instant as the row it marks.** Both flags are computed in
    the very statement that reads the row, so a detail and its marker can never disagree.
    `CredentialHealth` is a separate fetch a caller correlates by `source_id`, which reintroduces
    at the moment of a toggle exactly the two-lists-disagreeing failure #200 was about.
  - **The parallel the acceptance names is structural.** A tombstoned entity's marker
    (`deleted_at`) rides on `EntityDetail` itself; "the way a tombstoned one does" means the
    disabled marker rides beside it, not in a second round trip.
  - **`CredentialHealth` has no row for an unconfigured source**, so absence from that list would
    have to carry meaning — the trap the `coalesce` exists to avoid.
  - The surfaces `CredentialHealth` serves (top strip, launcher board, coverage) already handle
    disabled sources through the vocabulary and need no flag; the sources settings view reads
    `SourceSummary`, which has carried `enabled` all along. It is therefore untouched — a later
    surface that genuinely needs "enabled" beside auth state needs its own grant.

  **Three shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's and #42's:

  - **`source_enabled` is on every `QueuedWrite`, not only held ones.** A pending write against a
    disabled source never flushes (the scheduler's `due` excludes the source), so without the flag
    the *Waiting* section would say "not tried yet" forever about a write nothing will ever try.
    The panel reads it in both sections.
  - **A held write with `source_enabled: false` gets neither the two-versions comparison nor
    *Send mine anyway*.** The comparison would show a target that did not change, and the button
    would park the write as silently "waiting" — the remedy is the source toggle, and the row says
    so. Edit and Discard stay: those exits still work.
  - **The two detail banners are independent, and both show when both facts hold.** A tombstoned
    entity of a disabled source is both withdrawn (upstream's doing, permanent) and hidden by the
    user's own toggle (one click from undone); folding them into one banner would tell the reader
    upstream did something the user did.

  **The proof nothing is stored** is pinned as the ruling asks: re-enabling the source clears both
  markers on the next read with no re-sync and no queue edit
  (`a_write_against_a_disabled_source_says_so_until_the_source_is_back`,
  `a_disabled_sources_entity_opens_with_the_marker_until_reenabled`), and the miss directions are
  tests of their own — a write held for a changed target does not claim the source is off, and an
  enabled source's entity carries no marker.

  **What did not change.** No migration — **`0013` is still the next free number** — and no edit
  to `write_queue_state_chk`: `WriteState` keeps its five spellings, because the queue's answer
  ("nothing sends until someone acts") is the same for both hold reasons; only the explanation
  differs, which is exactly what makes it derivable. No new command and **neither append-only
  barrel is touched** — `pending_writes`, `submit_write` and `get_entity` already existed, and
  both flags ride inside DTOs those commands already return. `SearchResponse`, every event name
  and the `commands/` + `ipc/` layout keep their shape. Nothing under
  `crates/knobas-source/src/**`, `crates/knobas-http/**` or
  `crates/knobas-app/src/{error,profile}.rs`. This closes the two "left open" clauses at the foot
  of the #202 entry above, which stand as history rather than being rewritten — the treatment #53
  gives the #52 sentences it supersedes.

  Ratified by the orchestrator as issue #204 itself, whose ruling specifies the defect, the
  bounds, the acceptance criteria and this entry.

