- **Migration `0015` and three commands in the `time` module pair, issue #282 (2026-09-03):**
  passive attribution — the heartbeat's foreground, stored while the setting is on, and the
  passive blocks it supports. The module pair itself is #278's ratified exception ("it is where
  **every** time command lives"); what is new here is one migration, three commands, one changed
  statement, and one read that writes.

  **The migration.** `0015_the_heartbeat_and_what_it_saw.sql` adds one table, one index on it, and
  **one partial unique index on the existing `knobas.block`** — no column, constraint or row of
  `0013`'s is altered, and no earlier migration file is touched. The third statement is called out
  rather than folded into "adds a table", because a uniqueness constraint arriving on a table that
  already has writers is the kind of addition that can refuse a write nothing refused yesterday;
  the paragraph below argues it cannot refuse an honest one.

  **`0014` is #280's and is not in the tree yet.** That stream is in flight on
  `feat/280-worklog-to-jira` and #278's entry above already allocated it ("the next free number is
  `0014`, and #280's worklog table takes it"), so this stream took `0015` and left the slot. sqlx
  applies by version and skips what it has already run, so a **development** database that took
  `0015` first takes `0014` when #280 lands, out of order and without incident; a fresh profile
  takes them in order. Nothing has shipped, so no database exists that this can matter to.

  ```sql
  create table knobas.heartbeat (
    id bigint generated always as identity primary key,
    at timestamptz not null default now(),
    entity_id text, label text,
    focused boolean not null default true,
    constraint heartbeat_target_chk check (entity_id is null or label is null));
  create index heartbeat_at_idx on knobas.heartbeat (at);
  create unique index block_passive_start_idx on knobas.block (started_at) where kind = 'passive';
  ```

  **The observation is stored and the block is derived, and that is the load-bearing choice.**
  The alternative — folding each beat into an open passive block as it arrives — puts the floor,
  the merge and the cap into a state machine spread across a write path, a row and a restart,
  where the only witness is a database. Here the write path is one `insert` with no state, and
  the three rules live in `time::passive::derive`, which takes a slice of observations and
  answers with spans: no clock, no pool, no setting, fifteen unit tests and no PostgreSQL. It is also
  the only shape in which the **cap** can be what the spec says it is: "the day's passive total
  never exceeds focused time" is a rule about a *day*, the reader's midnight is a fact only the
  webview holds (#279's entry above), and the day read is therefore the one call that is told
  what a day is.

  **At-most-one target, not exactly-one**, unlike `knobas.timer` and `knobas.block`. An
  observation may honestly say the reader had nothing in front of them; that is focused time with
  nothing to attribute, which is exactly the case the cap is about. No foreign key on
  `entity_id`, the decision `0005`, `0008`, `0009` and `0013` all record.

  **`focused` has no writer today and is here anyway** — the treatment `0013` gave the `passive`
  block kind, and for the same reason. The shell sends no beat at all from an unfocused window
  (#278, story 27), so losing focus reaches this table as an *absence* of rows and the derivation
  reads that absence as the break it is. The column exists so the rule "an unfocused observation
  is neither time nor attribution" can be stated and tested, and so that a beat sent on blur —
  the obvious next accuracy fix — needs no schema.

  **A passive block is addressed by the instant it starts at**, which is what
  `block_passive_start_idx` — the one addition to an existing table — says, and what the
  reconciliation's upsert conflicts on. Spans are
  disjoint by construction, so two passive blocks starting at the same instant is not a race to
  resolve but a statement that cannot be true. Partial, on `passive` only: two *manual* blocks
  may honestly start in the same second, and a block that has been assigned leaves the index the
  moment its kind changes — which is precisely what keeps the reconciliation from claiming it
  back.

  **The setting is a `knobas.setting` row and needs no migration**, the backup schedule's
  precedent (`0002`, comment 6): `time.passive_attribution`, a bare JSON boolean, absent meaning
  **off**. A value that no longer decodes reads as off too — the opposite resolution to
  `backup::read_setting`'s, on purpose: the backup schedule's safe failure is to keep backing up,
  and this one's is to record nothing about a person who cannot be asked.

  **Off means nothing is recorded**, not that it is recorded and not looked at: `time::heartbeat`
  reads the setting and writes no observation, and `passive::materialize` returns before it reads
  anything. Switching it off later stops the recording and the derivation and **does not delete
  what has already been offered** — nothing passive has ever reached a source, so there is
  nothing to withdraw, and a passive block on a day nobody has reviewed yet is knobas' answer to
  "what was I doing".

  **The heartbeat is otherwise untouched.** The stamp lands first and unconditionally, the way
  #278's entry insists, and the setting read and the observation insert happen after it — so a
  failure in either is reported to the caller without ever having put `last_heartbeat` at risk. A
  foreground `vet` refuses is still recorded, **with no target**: the beat happened and the
  window was focused, and losing the attribution is honest where losing the observation would put
  a hole in the timeline focused time is measured off.

  **The three new commands:**

  ```rust
  #[tauri::command] pub async fn create_block(.., started_at: DateTime<Utc>, ended_at: DateTime<Utc>,
      target: time::TimerTarget) -> Result<time::day::DayBlock, IpcError>;
  #[tauri::command] pub async fn passive_attribution(..) -> Result<bool, IpcError>;
  #[tauri::command] pub async fn set_passive_attribution(.., enabled: bool) -> Result<bool, IpcError>;
  ```

  No new DTO: `create_block` answers with #279's `DayBlock` and the two setting commands carry a
  bare boolean. `set_passive_attribution` answers with what is now **stored** rather than nothing,
  the rule `set_backup_schedule` follows on the same surface — a toggle that flipped
  optimistically would tell a reader they had opted in on the one run where the write failed.

  **`update_block` now writes `kind = 'manual'`, and that is what *Assign…* on a passive block
  is.** The moment a person states a block's target, knobas' guess has become their record and
  must stop being something the next day read reconciles away under them. The write is
  unconditional rather than a `case`, because the other direction is not a thing a caller may ask
  for: there is no parameter that could request `passive`, and `passive::materialize` is the only
  writer of that word in the crate. So *assigning a passive block makes it manual* is witnessed
  by a test, and *a manual block cannot be turned passive* is witnessed by there being no way to
  say it. No signature and no other behaviour of that command changes.

  **`day_blocks` now reconciles before it reads — a write inside a read, deliberately.**
  `passive::materialize` makes the day's *unassigned passive* rows equal to what the beats
  support, and it is here rather than in a command of its own because a separate command would be
  one every future reader of blocks had to remember to run first, and one that forgot would draw
  a day with no passive time and nothing to distinguish that from a day with none. Three things
  stop it before it writes: the setting is off; the day has **no observations at all** (every day
  before this feature existed is such a day, and a reconciliation that spoke about one would
  delete passive blocks it has no evidence either way for); or a derived span overlaps a block
  the person owns, in which case the block wins and the span is dropped **whole** — the direction
  that can only lose a suggestion and never invent one, and what stops an assignment growing a
  passive twin on the next read.

  **A visit that crosses midnight is cut at midnight**, because the derivation runs over exactly
  the interval it was handed and the cap is a per-day rule. A passive block therefore belongs to
  one day, unlike a manual one, which #279 deliberately shows on both days it touched. That is a
  suggestion knobas is making about a day rather than a record of a stretch somebody worked, and
  keeping it inside the day it is offered on is what keeps two days' reconciliations from
  fighting over one row.

  **The derivation, in one paragraph, since the spec asks reviewers to look hardest at it.** A
  beat credits its target forward for one beat window (`BEAT_WINDOW_SECONDS`, pinned to the
  shell's `HEARTBEAT_MS` by a source scan) and no further, so silence stops being work thirty
  seconds after the last thing knobas heard. Claims on one target that touch or overlap merge;
  a claim on another target, a beat with nothing in the foreground, and an unfocused beat each
  truncate the visit before them, because two things cannot both have been in the foreground.
  Visits shorter than `FLOOR_SECONDS` (120, the spec's two minutes) are dropped. Focused time is
  the same walk read a beat later — the gap between consecutive beats, clamped to the same window
  — so a session's claims add up to its focused time *plus* the tail its last beat credits
  forward, and the cap takes that back: it binds on every session, and harder whenever beats
  arrive closer together than they are sent. The cap is a rule about the **total**, so focused
  time nobody attributed pays into the same budget.

  **What did not change.** No existing DTO field or event name, and no new event: a passive block
  appears on the day read like any other block. Nothing under `crates/knobas-source/src/**` — a
  block is knobas' own and no adapter hears about one. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the failures are `invalid` and query
  failures, which `IpcError` already carries. The backup export needs no change — it dumps the
  whole `knobas` schema (design §16.12), so `knobas.heartbeat` rides in it. `knobas_core` gains
  nothing. **The worklog draft's own guard is #280's, and it is not in this tree**:
  `crates/knobas-app/src/time/worklog.rs` exists only on `feat/280-worklog-to-jira`, where
  `UNLOGGED_BLOCKS` reads `knobas.block` without narrowing on `kind`. That was harmless while
  nothing wrote a passive row and is not any more — spec #272: "*Log all* … never touches passive
  or label blocks" — so it wants `and kind = 'manual'` and an assertion beside it. It is named
  here, in the entry that makes it necessary, rather than fixed here, because that branch is
  being live-tested and is not this stream's to edit; #280's merge-manager or #283 owns it.

  **Which barrels were appended**: three lines at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, three functions at the foot of
  `app/src/lib/ipc/time.ts`, and one import and one element in
  `app/src/lib/settings/SettingsView.svelte`. No barrel is rewritten.

  Ratified by the orchestrator as spec #272 and issue #282, whose acceptance criteria specify the
  setting, the pure derivation, the strip's second style, both *Assign…* paths, the tests and
  this entry.

