- **Migration `0013`, and a `time` module pair on both sides of the bridge, issue #278
  (2026-09-03):** M3.1's first frozen-surface touch, ratified in advance by the spec (#272)
  Björn approved — "Schema and settings. Migrations from the next free number for the timer row,
  blocks, worklogs" and "Time's IPC. One §10.8-ratified exception for a `time` module pair on both
  sides of the bridge, holding timer, heartbeat, block, worklog, draft, day and week commands,
  following the precedent of the backup module." Written with the implementing PR per the
  #175/#177/#208 pattern.

  **This supersedes one sentence in each of the #204 and #208 entries above** — "**No migration —
  `0013` is still the next free number**", true when both were written. `0013` is claimed here;
  the next free number is `0014`, and #280's worklog table takes it. The old sentences are left as
  history rather than rewritten, the treatment #53 gives the #52 sentences it supersedes.

  **The migration.** `0013_the_timer_and_its_blocks.sql` adds two tables and edits nothing.

  ```sql
  create table knobas.timer (
    only_one       boolean primary key default true constraint timer_only_one_chk check (only_one),
    entity_id      text,  label  text,
    started_at     timestamptz not null default now(),
    last_heartbeat timestamptz not null default now(),
    constraint timer_target_chk check ((entity_id is null) <> (label is null)));

  create table knobas.block (
    id bigint generated always as identity primary key,
    started_at timestamptz not null, ended_at timestamptz not null,
    entity_id text, label text, kind text not null,
    ended_by_relaunch boolean not null default false,
    worklog_id bigint,
    constraint block_target_chk check ((entity_id is null) <> (label is null)),
    constraint block_kind_chk   check (kind in ('manual','passive')),
    constraint block_span_chk   check (ended_at >= started_at));
  create index block_started_idx on knobas.block (started_at desc);
  ```

  **"At most one" is structural, not a rule a reader has to know.** The primary key is a column
  that can only ever hold `true`, so a second `insert` is a primary-key violation and there is no
  state in which two timers exist — as against "the newest row wins" or a unique index over a
  `running` flag, both of which let a second row live long enough for two surfaces to disagree
  about what the clock is on. `time::start` turns that violation into a `conflict` by asking for
  it (`on conflict (only_one) do nothing`, no row back means refused), so two starts racing
  produce one timer and one honest refusal rather than an overwrite. `tests/time_ipc.rs` pins all
  three levels: the command's `conflict`, a raw second `insert` that collides on the primary key,
  and a raw `insert` naming `only_one = false` — which is the only thing `timer_only_one_chk`
  stands in the way of, and without which the check could be deleted with every test still green.

  **The target is two nullable columns with an exactly-one check, in both tables, deliberately.**
  Not one column with a discriminator: the entity half is an entity id and has to read like every
  other entity id in this schema (`knobas.link`, `knobas.activity`, `knobas.start_work_step`), and
  a column holding sometimes-an-id-sometimes-a-sentence is one no reader could join on even in
  principle. It is the same two columns and the same check in `knobas.block`, so a block made by
  stopping a timer carries the target verbatim rather than through a translation that could
  disagree. **No foreign key on `entity_id`**, the decision `0005`, `0008` and `0009` all record:
  a block is a record of what the user did with their day, and purging the mirror or removing a
  source must not delete an afternoon.

  **A stored context is not a timer target, and the schema deliberately does not say so.** The
  rule is `knobas_app::time::vet`'s, on the way in, because it is about the `ctx:` namespace
  (`knobas_core::entity::RESERVED_NAMESPACES`) and a check constraint that parsed entity ids would
  be a second copy of that list — the copy that goes stale. `CONTEXT.md`'s *timer target* gives
  the reason: a context is a set, and time on a set has nowhere to go. **`start_timer` is the one
  enforcement**: it refuses a `ctx:` id with `invalid`, and `tests/time_ipc.rs`'s
  `a_stored_context_is_refused_as_a_target` builds the context through
  `knobas_core::context::create_adhoc` rather than inventing the string. The two frontend guards
  — ⌘T's picker filtering it out of the recents, and the launcher's *Start timer* row being
  absent on it — are defence in depth over a list that **cannot carry one today**: recents and
  search both read `sync.live_item`, and migration `0006`'s `item_entity_reserved_chk` forbids a
  `sync.item` row from naming the `ctx:` namespace at all. They are here because the rule belongs
  to the *list* rather than to that constraint two crates away — #279 draws its own candidates —
  and each is witnessed as a **refusal** rather than an absence: the fixture puts a real context
  row in the middle of the list and asserts the rows either side of it survive.

  **`worklog_id` carries no foreign key because there is no table yet.** The worklog arrives with
  #280 and takes the next free number; the column is here rather than there because a block's
  read-only rule (spec #272 story 20) is about it, and every reader written before then would
  otherwise have to be revised. #280 may add the constraint or may follow this schema's usual
  no-foreign-key rule; either way nothing here changes. Nothing in #278 writes it.

  **`kind` enumerates `passive` before anything writes one.** #282 adds the derivation; the day
  review (#279) draws the two distinguishably and would otherwise have one kind to distinguish.
  The vocabulary lives in three places — the check constraint, `time::BlockKind`, and the
  `'manual'` literal in the two writing statements — and `every_block_kind_is_one_the_schema_accepts`
  reads the migration file to keep them in step, the cross-check `knobas_core::start_work` runs
  against `0008`'s `step` vocabulary. **`0013`'s own comments credit that derivation to #281,
  and stay wrong**: sqlx checksums an applied migration, so editing one for a comment would
  fail startup on every existing database — the rule this file states for `0001`. The number
  here is the correction (#306).

  **The module pair.** `crates/knobas-app/src/time/mod.rs` holds the decisions and
  `crates/knobas-app/src/commands/time.rs` is shims over it — the arrangement `backup/` set and
  the reason is the same: a `#[tauri::command]` cannot be called from a test. Its mirror is
  `app/src/lib/ipc/time.ts`. **Every** time command lives there, including #279's and #280's block,
  worklog, draft, day and week commands; standup and Confluence reads go into the entity module as
  usual. The mirror tests live in `commands/time.rs` beside the shims, which is where
  `commands/backup.rs` — the precedent this pair rides on — keeps its own.

  **The four commands**, all in the new module:

  ```rust
  #[tauri::command] pub async fn current_timer(..) -> Result<Option<time::RunningTimer>, IpcError>;
  #[tauri::command] pub async fn start_timer(.., target: time::TimerTarget) -> Result<time::RunningTimer, IpcError>;
  #[tauri::command] pub async fn stop_timer(..) -> Result<Option<time::Block>, IpcError>;
  #[tauri::command] pub async fn timer_heartbeat(.., foreground: Option<time::TimerTarget>)
      -> Result<Option<time::RunningTimer>, IpcError>;
  ```

  **`TimerTarget` is a tagged union on the wire**, `{"kind":"entity","entity_id":…}` /
  `{"kind":"label","label":…}`, mirrored as two interfaces and a union rather than one interface
  with two optional fields. Exactly-one is what the whole feature is about; a shape that could
  carry both would put the rule in the caller, and the mirror would be the one place it was not
  stated. `RunningTimer` and `Block` are pinned by `assert_shape` against those interfaces, and
  `BlockKind`'s members are read out of the mirror rather than listed in the test — the rule
  `entity_mirror.rs` states.

  **`timer_heartbeat`'s `foreground` is taken and not yet stored, and that is deliberate.** It is
  the observation passive attribution (#282) turns into passive blocks, and #282 brings the table
  to store it in. It is on the command **now** because the frontend rule that computes it — *open
  detail, else room anchor, else none* — is part of this ticket, and adding the parameter later
  would be a second §10.8 touch on a command that already exists. *Revised by #282's entry below
  (2026-09-04): the table arrived, and the foreground is stored whenever passive attribution is on
  — `time::passive::observe` reads the setting and writes the observation on every beat. The "not
  yet" above is this entry's own moment and no longer knobas'; what stays true is that the
  parameter rode this command first so that storing it later cost no second §10.8 touch.*

  **It is not validated, and `timer_heartbeat` never refuses on it.** The value is vetted *after*
  the stamp lands and a refusal is logged, never propagated: the stamp is a statement about
  knobas being alive, and a beat lost to a malformed foreground would freeze `last_heartbeat` and
  hand the next relaunch a block hours short — the one failure the relaunch rule exists to
  prevent (`a_foreground_the_timer_could_never_run_on_does_not_cost_the_beat`). So #282 inherits
  the parameter unvalidated; what it inherits alongside it is a log line already complaining
  about every bad one.

  **No new event, and that is the acceptance criterion rather than an omission.** `start_timer`
  and `stop_timer` write activity lines with actor `user` and announce them on the existing
  `activity:new`, so the status bar's latest-change line, the shell's timer store and the digest
  (#288) all learn through the signal they already watch; the strip ticks *elapsed* client-side
  from `started_at`. A channel of the timer's own would be a second thing to keep in step with the
  first and would carry no fact the row does not already hold. The relaunch sweep signs its line
  `knobas` — the actor `knobas_core::activity` reserves for an action knobas took on its own —
  because a person reading their own name against a stop they did not make is knobas lying about
  who did what.

  **Bring-up gains one call, and its position is load-bearing.** `time::close_stranded` runs in
  `spawn_bring_up` immediately after `migrate::run` and **before** `DbState::Ready`, so the
  shell's first `current_timer` can never beat it. A stranded timer's block ends at
  `last_heartbeat`, not at `now()`: `now()` would log the hours knobas spent closed as work, which
  is the one thing a forgotten timer must not do. It is flagged `ended_by_relaunch` so #279 can
  offer *Extend to now* rather than silently shortening a block somebody really did work
  through. A sweep that
  fails is logged and bring-up continues — refusing to start over a forgotten timer would make it
  a reason knobas cannot open at all. Both halves are witnessed: `time_ipc.rs` closes a timer
  whose last heartbeat is **hours** before the sweep, so "at the heartbeat" and "at `now()`" are
  hours apart rather than within a tolerance, and `wiring.rs` reads the source to prove the call
  exists and precedes `DbState::Ready`.

  **Which barrels were appended**: one group of four lines at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, after the `commands::sources::` group;
  `pub mod time;` in `crates/knobas-app/src/commands/mod.rs` and in `lib.rs`; and
  `export * from "./time";` at the foot of `app/src/lib/ipc/index.ts`. Neither barrel is rewritten.

  **What did not change.** No existing command, DTO field or event name changes meaning. Nothing
  under `crates/knobas-source/src/**` — a timer is knobas' own and no adapter hears about it;
  `WriteOp::LogWork` is #280's growth with its own entry. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the commands' failures are
  `invalid`, `conflict` and query failures, which `IpcError`'s existing constructors and its
  `From<sqlx::Error>` already cover. No settings key — passive attribution's is #282's. The
  backup export needs no change to carry the two new tables: it dumps the whole `knobas` schema
  (design §16.12), so `knobas.timer` and `knobas.block` ride in it already; the share export's
  time toggle is #283's paperwork. `knobas_core` gains nothing — the store is in `knobas-app`
  beside the commands, which is what the module-pair exception is for.

  Ratified by the orchestrator as spec #272 and issue #278, whose acceptance criteria specify the
  migration, the module pair, the four commands, the relaunch rule, the tests and this entry.

