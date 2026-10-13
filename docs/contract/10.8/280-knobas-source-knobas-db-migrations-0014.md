- **`crates/knobas-source/src/**`, `crates/knobas-db/migrations/0014_the_worklog.sql` and the IPC
  command schema, issue #280 (2026-09-03):** the worklog — M3.1's
  `WriteOp` growth, the local copy of what it sends, and the two commands that draft and log it.
  One entry for the package, as the #278 entry above records is the arrangement for this
  sub-milestone.

  **The SPI, and it is two changes rather than one.**

  `WriteOp::LogWork { entity, started, seconds, comment }`, identifier `"log_work"`, ratified
  under ADR-0006 as M3's growth. `entity` is the **ticket**; `started` is a `DateTime<Utc>` and
  not a formatted string, because Jira's spelling of a worklog's start
  (`yyyy-MM-dd'T'HH:mm:ss.SSSZ`, milliseconds and a numeric offset both mandatory, `Z` refused)
  is the adapter's business and not a shape the SPI should teach every future source; `seconds`
  is the time **worked** and is deliberately not `ended - started`, because the blocks a worklog
  covers have gaps between them. `comment` may be empty. Nothing carries `adjustEstimate`: Jira's
  own default of `auto` applies, and an op that carried the choice would be asking every caller a
  question no surface in knobas puts to the user. `WriteOp::identifier`, the battery's
  `known_write_ops` probe, `knobas_sync::write_queue::target_entity` and
  `knobas_core::write_queue::PROJECTED_OPS` each gain their arm and each keeps its missing
  wildcard.

  **Declared by the Jira adapter alone** (`descriptor.rs`), and the battery's clause 5 is what
  proves Gitea, TeamCity, Confluence (#284, which landed while this was open) and the mock refuse
  it — each gained the identifier in the arm that already refuses what it does not declare, so the
  refusal is by descriptor rather than by a comment saying it would be. Those matches carry no
  wildcard, which is why a fifth adapter arriving mid-flight stopped compiling rather than
  silently accepting an op nobody had decided about. `project`'s reading for `log_work` is **liveness alone**, stated in
  that function's own docs: a worklog is a statement about hours somebody worked and nothing that
  can happen to the ticket makes those hours wrong, so a colleague's reply must not hold it; the
  ticket leaving the mirror still does.

  `Source::write` answers `Result<WriteReceipt, SourceError>` instead of `Result<(), SourceError>`.
  `WriteReceipt` is one optional field, `remote_id`, and every adapter but Jira's worklog arm
  answers `WriteReceipt::none()`. **The reason is one write op, not an appetite for return
  values:** knobas addresses what it wrote by *reading it back* — `start_work` finds the pull
  request it created in the mirror — and a worklog is the one thing knobas writes that no read
  can identify, because it is not a mirrored entity and two worklogs of the same length on the
  same day are indistinguishable from outside. The id exists exactly once, in the answer to the
  POST. `CreateTicket` still drops the key Jira hands it, and its comment now says why: the
  ticket *is* a mirrored entity, so the mirror is the answer that survives a re-send. A struct
  rather than `Option<String>` so the next thing a source has to say about a write is a field and
  not a second signature change. Delivery is unchanged and still at-least-once (ADR-0012): a
  receipt is what the source answered *this* time, and nothing treats it as an idempotency key.

  **Migration `0014`** — `knobas.worklog`: ticket, `started_at`, `seconds`, `comment`,
  `block_ids`, `write_queue_id`, `remote_id`, `created_at`. The copy is written **before** the
  write lands and stays if the write is refused, which the migration argues in place: a copy
  written only on success leaves a refused worklog with no trace of the hours it was made of, and
  its blocks back on the pile with nothing to say they were already sent once. `remote_id` is
  **not unique** — at-least-once means a re-sent worklog is a second row at Jira with a second
  id, and a unique constraint would turn that into a failed settle. It also adds the foreign key
  `0013` could not: `knobas.block.worklog_id` → `knobas.worklog(id)` `on delete set null`, the
  one knobas-owned-both-ends case in this schema, so deleting a worklog gives its blocks back
  rather than taking the afternoon with it.

  **And one column on `knobas.write_queue`: `remote_id`.** The same value as the worklog's, on the
  row that asked for it, because the two writers of the copy's id can arrive in either order:
  `log` queues, writes the copy, then flushes — but the scheduler flushes on its own tick, and a
  tick landing between the queue row and the copy settles the write while nothing names it, so the
  settle's own stamp would match no row and the id would be gone. The copy takes it off the queue
  row instead, whenever it is written. The column is **internal to the queue**: `queue_columns!`
  does not list it, so `QueuedWrite` keeps its shape and its TypeScript mirror is untouched.

  **What "read-only" means for a covered block, and where it is enforced.** In two places, and
  neither is a trigger: the *draft* offers only blocks whose `worklog_id is null`, so a logged
  afternoon cannot be logged twice; and #279's `update` and `delete` carry the same
  `worklog_id is null` in their own `where`, which is story 20's refusal. The foreign key above is
  the third leg — it makes "logged into a worklog that exists" a thing the schema knows, so a
  deleted worklog gives its blocks back instead of locking them for ever.

  That key is why **`time_ipc.rs`'s `logged_into` now writes a real worklog row** rather than
  stamping a fabricated id: a block pointing at a worklog that does not exist is exactly the state
  the key forbids. #279's helper said in as many words that this was the shape it would take once
  the worklog landed.

  **The settle is what stamps the id, in one statement.**
  `knobas_core::write_queue::sent(pool, id, remote_id)` settles the queue row, records the id on
  it, and updates the worklog naming that row — all in the same statement, so "the write
  settled" and "the copy carries the id" are one event. It names `knobas.worklog` deliberately: no transaction spans
  `Source::write` and the settle (ADR-0012), so a second statement could settle and then fail to
  stamp, leaving a worklog Jira holds and knobas cannot name with nothing left to re-read it
  from. The `update` matches nothing for every other op.

  **`knobas_sync::write_queue::queue` exists beside `submit`**, and it is not a second write
  path: it is `submit`'s first half, and the flush loop is still the only caller of
  `Source::write` (`write_choke_point.rs` is unchanged in that respect). The reason is
  **durability, not the id**: nothing may be sent to Jira before a local record of it exists, or
  a crash in the gap leaves a worklog on the ticket that knobas has no trace of and offers the
  reader to log again. So `time::worklog::log` queues, writes the copy and spends the blocks,
  then flushes. The id survives either order because of the queue column above — the ordering and
  the column answer two different questions, and both are needed.
  `sources::write_queue::submit` is now those two halves called in order and behaves exactly as
  before. `time::worklog::keep` — the copy, and the blocks it spends — is `pub` for one reason:
  it is the seam where the order of the copy and the flush stops mattering, and
  `a_settle_that_beat_the_copy_still_gives_it_the_id` has to be able to put the copy second. `HANDS_TO_THE_QUEUE` gains its third entry, `knobas-app/src/time/worklog.rs`, which
  builds the op typed rather than as a `json!` literal for the reason the #44 entry gives.

  **Two commands**, both in the `time` module pair the #278 entry ratified:

  ```rust
  #[tauri::command] pub async fn worklog_draft(.., entity_id: String, day: NaiveDate, offset_minutes: i32)
      -> Result<Option<time::worklog::Draft>, IpcError>;
  #[tauri::command] pub async fn log_work(.., entity_id: String, day: NaiveDate, offset_minutes: i32,
      started_at: DateTime<Utc>, seconds: i64, comment: String)
      -> Result<time::worklog::Worklog, IpcError>;
  ```

  **The draft reads `manual` blocks only.** `0013`'s block vocabulary has a second kind and
  #282 writes it: a `passive` block is knobas' guess at what was open on screen, not a person's
  account of an afternoon, and drafting one would put minutes nobody vouched for into a worklog
  that bills a client. `UNLOGGED_BLOCKS` carries `kind = 'manual'`, which is also what `log`
  covers, since it re-derives its blocks from that same read. #282's own surface is where a
  passive block is assigned and becomes a claim; `a_passive_block_is_not_drafted` is the guard.
  It was written before its writer existed and #282 landed while this branch was open, so it now
  stands in front of rows the derivation really produces.

  **`worklog_draft` answers `null` for two different questions on purpose**, because the caller
  does the same thing with both: the shell asks for a draft on **every** stop that closed an
  entity's block and opens the modal only if it got one. A stop on a note, on a Gitea commit or
  on a ticket whose day is already logged is not an error to apologise for. Whether a worklog can
  go somewhere is read off the source's **declared write ops** — never a kind list, which is the
  hardcoded per-adapter table §3a exists to prevent.

  **The day is the reader's, and their machine is the only thing that knows which one.**
  `day` + `offset_minutes` (east of UTC positive, the opposite sign to `getTimezoneOffset`)
  rather than a server-side `date_trunc`, which would file a Berlin evening's blocks under the
  following day. A fixed offset and not a named zone: on the two days a year a zone changes
  offset the window is an hour out at one end, the interval is editable, and a zone database on
  the bridge for those two days is not worth it. Stated here because it is a limit, not an
  oversight.

  **The interval is editable as a whole** (spec #272, story 31): the draft carries a start field
  and a minutes field, and both are the reader's — a timer started ten minutes after the work did
  is the ordinary case. **An untouched field is sent verbatim, seconds and all**, on both halves,
  rather than put through the minute-granularity field: a block is measured to the second, so
  rounding 09:00:37 down — or logging 9000s for blocks worth 9037s and marking those exact blocks
  spent — would be knobas quietly changing a fact it measured on a draft nobody edited.

  **Which blocks a worklog covers is not an argument**, and that asymmetry is the point:
  `started_at`, `seconds` and `comment` are the reader's, edited in the draft or not, and knobas
  has no business overruling a person's account of their own afternoon — but *which of knobas'
  rows are now spoken for* is re-derived by the same rule the draft used, because a caller that
  could name them could name another ticket's, or the same ones twice.

  **DTOs**: `Draft`, `Candidate`, `CandidateSource` (`mirror` | `activity` | `write` | `note` --
  two at ratification, four since #409; its own entry below carries the widening) and `Worklog`, all
  mirrored in `app/src/lib/ipc/time.ts` and pinned by `assert_shape`/`declared_union` in
  `commands/time.rs`. The mirror's two functions take `ReaderDay` and `LoggedWork` **objects**
  rather than six positional arguments — three of them adjacent strings, where a swap is silent —
  and unpack them into the same `invoke` payload the commands declare; those two interfaces are
  the frontend's own shape and cross nothing. A candidate carries its own **`bullet`**, composed in Rust: the frontend
  joins the bullets of what is ticked, so the wording of every worklog knobas *sends* is one rule
  in one language. It strips the leading `- ` when it draws a candidate's own row, because a
  checkbox list does not need a second bullet glyph; nothing it draws reaches the comment. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains its ninth member, which
  `every_write_op_variant_is_declared_in_the_mirror` requires.

  **ADR-0012's sentence is on the draft, verbatim**, beside the write-queue panel's copy of it
  (#224): a duplicated worklog is hours somebody bills twice, so the guarantee belongs on the
  surface where that write is sent. `the_draft_quotes_adr_0012s_sentence_verbatim` reads the ADR
  file and the component and fails if the two stop agreeing.

  **`Timer.press()` answers a `TimerPress` (`{did, closed}`) instead of a bare word.** A frontend
  shape, not an IPC one, and recorded because #278's entry described the old one: a stop closes a
  block and the block is what the draft opens on, and reading the timer back for it would find a
  different block whenever another surface started one in between.

  **One barrel, not both.** `crates/knobas-app/src/lib.rs`'s `generate_handler!` gains the two
  commands; `app/src/lib/ipc/index.ts` is untouched, because #278's entry already exported
  `./time` from it and the two commands land in that module.

  **What did not change.** No new event — a worklog appears in the pending-writes panel like any
  other write, on the signal that panel already watches. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched. `QueuedWrite` keeps its shape and its
  mirror: the receipt is consumed at the settle and nothing stores it on the queue row. No
  settings key. The backup export carries `knobas.worklog` already, for the reason the #278 entry
  gives. **`knobas-mockd` is untouched**, and deliberately: ADR-0013 freezes it — *nothing new* — so
  `POST api/2/issue/{key}/worklog` stays a contract verb it answers `501` to. The worklog's wire
  format is witnessed by the real Jira in `atlassian_live.rs` and, offline, by the one thing about
  it that is a string rather than a request: `a_worklogs_started_carries_milliseconds_and_a_numeric_offset`
  pins Jira's `started` pattern in the adapter's own tests. The app-level seam
  (`tests/worklog_ipc.rs`) runs against a **trait-level fake** — the `knobas-source-mock` layer
  ADR-0013 explicitly keeps — because what it asserts is knobas' own plumbing and not a claim
  about any source.

  Ratified by the orchestrator as spec #272 and issue #280, whose acceptance criteria specify the
  op, the migration, the commands, the candidates, the ADR-0012 quotation, the live test and this
  entry.

