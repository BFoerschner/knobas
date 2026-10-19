// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * What stopping the timer on a page, a note, a repo or a label offers.
 *
 * **Two nested absences, and they are different answers**, which is why this
 * is a struct and not an `Option<Option<Suggestion>>` (serde spells both of
 * those `null`):
 *
 * * [`offer`] answering `None` is *this is not an ad-hoc block* -- its target
 *   is a ticket, and the worklog draft is what opens.
 * * `Some(AdHocBlock { suggestion: None })` is *the dialog opens and knobas
 *   has nothing to suggest* -- no rule fired, and *Keep local* is the default.
 *
 * One field, and the block's id is deliberately **not** echoed back: the
 * caller passed it in and still holds it, so a copy on the way out would be a
 * value with no reader and a promise of a guard nothing makes.
 */
export type AdHocBlock = { suggestion: Suggestion | null, };

/**
 * One stretch of time knobas owns -- `CONTEXT.md`'s **block**.
 */
export type Block = { id: number, 
/**
 * RFC 3339, UTC.
 */
started_at: string, 
/**
 * RFC 3339, UTC.
 */
ended_at: string, target: TimerTarget, kind: BlockKind, 
/**
 * *Ended when knobas closed* -- the block was closed by the relaunch
 * sweep at the last heartbeat, not by anybody pressing stop. The day
 * review offers *Extend to now* on these (#279).
 */
ended_by_relaunch: boolean, 
/**
 * The worklog this block was logged into (#280); `null` until then.
 */
worklog_id: number | null, };

/**
 * How a block came to exist. Mirrors `block_kind_chk` in migration `0013`.
 *
 * - `manual`: the timer made it -- `CONTEXT.md`'s wording, and deliberately
 *   not "a person started and stopped it": [`close_stranded`] writes a
 *   `manual` block that nobody stopped. [`Block::ended_by_relaunch`] is what
 *   tells those two apart; the kind says only where the block came from.
 * - `passive`: passive attribution recorded what was open (#282) -- knobas'
 *   own guess, and never logged anywhere on its own. Written only by
 *   [`passive::materialize`], and turned into `manual` the moment a person
 *   assigns it (`update_block`) -- nothing else in the crate writes this
 *   word, and nothing at all writes it back.
 *
 * The kinds are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/time.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type BlockKind = "manual" | "passive";

/**
 * One thing knobas saw the reader do inside a draft's interval.
 *
 * A proposal for a bullet, not a record of anything: the draft renders these
 * as checkboxes and the comment is the `bullet`s of the ticked ones, joined
 * with newlines. **The frontend does not compose a bullet** -- the wording is
 * the backend's, so that every worklog knobas has ever sent is spelled one
 * way.
 */
export type Candidate = { 
/**
 * Stable within a draft, and the key the checkbox list is drawn on --
 * `"item:jira:PAY-231"`, `"activity:4211"`, `"write:97"`, and a note's own
 * `"note:<uuid>"`, which is already in a namespace of its own. Two
 * candidates can share an entity (a commit and the activity line about
 * it; a pull request of the reader's own and the comment they left on it),
 * so the entity id alone would not do.
 */
id: string, 
/**
 * Where this candidate was seen.
 */
source: CandidateSource, 
/**
 * When, RFC 3339, UTC -- which is what puts the list in order.
 */
at: string, 
/**
 * The entity it is about, when there is one.
 */
entity_id: string | null, 
/**
 * **The line this candidate contributes to the comment, verbatim.**
 *
 * Composed here rather than in the webview so that ticking a box is the
 * only thing the frontend decides: it joins the bullets of what is
 * checked, and the wording of a bullet is one rule in one language. A
 * frontend that formatted its own would be a second spelling of every
 * comment knobas has ever sent.
 */
bullet: string, };

/**
 * Where a [`Candidate`] was seen.
 *
 * Four sources because each fact is read from the table it already lives
 * in (#409):
 *
 * - `mirror`: an item in the mirror whose author is the username that
 *   item's source is configured with -- a commit, a pull request, a
 *   build, a page.
 * - `activity`: a line the reader's own actions wrote in
 *   `knobas.activity`.
 * - `write`: a comment the reader posted through knobas -- a row in
 *   `knobas.write_queue`, which is the only place a comment knobas sent
 *   exists as a fact of its own.
 * - `note`: a note the reader edited -- a row in `knobas.note`, read by
 *   its `updated_at`.
 *
 * Declared this way since #409 took it from two members to four: `ALL` is
 * generated from the same list as the variants, and the TypeScript union
 * is generated from this enum (`app/src/lib/ipc/generated/time.ts`), so a
 * fifth source arrives in both with it. The wire spelling is unchanged --
 * the macro applies the same `rename_all = "snake_case"` this enum carried
 * by hand. The sources are documented here and not on each variant because
 * the generated TypeScript carries a type's and a field's doc but not a
 * variant's (#1325).
 */
export type CandidateSource = "mirror" | "activity" | "write" | "note";

/**
 * One block as the day review draws it: the block, and what the mirror calls
 * its target.
 *
 * Two values rather than a `Block` with a title field, and the split is the
 * point: a block is knobas' own durable record and the title is the mirror's
 * current opinion of a row that may since have been renamed, purged or never
 * synced at all. Folding the second into the first would make a title look
 * like something the block remembers, and the first purged ticket would turn
 * an afternoon into a row with no name.
 */
export type DayBlock = { block: Block, 
/**
 * The entity's title from `knobas.entity`, or `null`.
 *
 * `null` for a label block — there is no entity — and for an entity the
 * mirror has never held or has purged, which migration `0013` allows on
 * purpose (no foreign key: an afternoon survives a purged mirror). A
 * blank title is `null` too, so the view has **one** question to ask
 * rather than two, and the answer to it is always "show the id instead".
 */
title: string | null, };

/**
 * One day, as the day review reads it: its blocks, and whether it reaches
 * back past the observation horizon (#337).
 *
 * A struct rather than the bare `Vec<DayBlock>` this read used to answer
 * with, because there is now something true of the **day** rather than of any
 * block on it. `RETENTION_DAYS` bounds how long observations are kept and
 * bounds nothing about which day a reader may open -- `#/time/<date>` takes
 * any date -- so a day past the horizon draws exactly the strip a day nobody
 * had the app open on draws, and the difference between *knobas has no
 * observations for this day* and *knobas has observations and they say
 * nothing* had nowhere to ride.
 */
export type DayRecord = { 
/**
 * The blocks overlapping the day, earliest first.
 */
blocks: Array<DayBlock>, 
/**
 * Whether the day reaches back past the observation horizon.
 *
 * knobas keeps a month of observations, and `#/time/<date>` takes any
 * date, so a day older than that draws exactly the strip a day nobody had
 * the app open on draws. This is what tells the two apart.
 *
 * The same question [`passive::materialize`](super::passive::materialize)
 * refuses to reconcile on, asked through the same
 * [`Horizon`](super::passive::Horizon), so the day the strip calls absent
 * is exactly the day the reconciliation left alone.
 *
 * **Passive blocks the day was already offered are still drawn**, and
 * that is not a contradiction: the observation sweep took the
 * *evidence*, not the record made from it while the evidence was there.
 * The flag says the strip cannot be added to, never that what is on it is
 * untrue.
 *
 * **It does not ask whether passive attribution is switched on**, and
 * that is deliberate. The stamp it comes from is written only where a
 * sweep actually deleted rows, so a profile that never had the feature on
 * has no stamp and no day past the horizon; one that had it on, was
 * swept, and has since switched it off really has lost those
 * observations, and the sentence stays true. Gating it on the switch
 * would make the strip stop explaining a gap at the moment a person
 * turned the recording off, which is when they are most likely to be
 * looking at one.
 */
past_horizon: boolean, 
/**
 * The day's **open stretches** (#901): focused time that no block, no
 * dismissed offer and not the running timer claims, earliest first.
 *
 * What the Day tab draws as *open* rows, "knobas was open and in front,
 * with no timer and nothing it could name". The same subtraction
 * ([`passive::open_spans`](super::passive::open_spans)) the week's
 * "no target, app open" row adds up, so the day's *open* figure, the sum
 * of these, is the week's number for the day. Empty past the horizon and
 * on a day nothing was observed.
 */
open: Array<OpenStretch>, };

/**
 * One of the reader's days: the date they call it, and the interval it is.
 *
 * Both halves, because neither can be derived from the other on this side.
 * The instants are the query's; the date is what a cell is keyed and labelled
 * by, and a backend that recomputed it from the instants would be doing the
 * timezone arithmetic this shape exists to avoid.
 *
 * `weekWindows` in `app/src/lib/time/week.ts` is what builds these -- one
 * `Date` per local midnight, so a week containing a daylight-saving change is
 * still seven correct windows rather than seven equal ones.
 */
export type DayWindow = { 
/**
 * `2026-09-03`, in the reader's own reckoning.
 */
day: string, 
/**
 * The reader's local midnight, RFC 3339, UTC.
 */
from: string, 
/**
 * The next local midnight, RFC 3339, UTC. Half-open: `to` is the next
 * day's.
 */
to: string, };

/**
 * What the draft offers: one interval, its candidates, and a comment made
 * from them. What stopping the timer on a ticket opens.
 */
export type Draft = { 
/**
 * The ticket the time goes on.
 */
entity_id: string, 
/**
 * The reader's day, as they named it -- `"2026-09-03"`.
 */
day: string, 
/**
 * The first block's start, RFC 3339, UTC -- what Jira is told the work
 * began at.
 */
started_at: string, 
/**
 * The last block's end, or now while the clock runs on this ticket;
 * RFC 3339, UTC.
 * **Not** `started_at + seconds`: it is the far edge of the window, and
 * the difference between the two is the gaps.
 */
ended_at: string, 
/**
 * The time that will be logged -- the blocks' durations added up, the
 * running stretch's up to now among them.
 */
seconds: number, 
/**
 * The blocks this draft is made of, oldest first. Closed blocks only: the
 * running timer's stretch has no block yet, and is [`Self::running_since`].
 */
block_ids: Array<number>, 
/**
 * When the running timer started, RFC 3339, UTC, if it runs on this
 * ticket and started on this day (#896); `null` when the clock is
 * elsewhere or stopped. The draft then counts its stretch up to the moment
 * it was read: `ended_at` is that moment and `seconds` includes it. The
 * stretch becomes a block when the clock stops, and `log_work` re-derives
 * the blocks it covers then, so a shell logs only after stopping.
 */
running_since: string | null, 
/**
 * What knobas saw in the interval, for the reader to tick.
 */
candidates: Array<Candidate>, 
/**
 * The comment as generated from **every** candidate, which is what the
 * draft opens with. Editable, down to empty.
 */
comment: string, };

/**
 * One stretch of focused time nothing claims (#901).
 */
export type OpenStretch = { 
/**
 * RFC 3339.
 */
started_at: string, 
/**
 * RFC 3339.
 */
ended_at: string, };

/**
 * One worklog *Log all* would create, before it creates any of them.
 *
 * The confirmation's line, and the acceptance criterion is that it exists
 * *before* anything is sent: a bulk write to a ticketing system is not
 * something to discover afterwards. [`log_all`] re-derives its own work
 * rather than taking this back, for the reason
 * [`worklog::log`](super::worklog::log) does not take block ids -- a caller
 * that could name the blocks could name another ticket's.
 */
export type PlannedWorklog = { 
/**
 * `"2026-08-24"` -- the day this worklog is for.
 */
day: string, entity_id: string, 
/**
 * The mirror's title for it, or `null`. For the reader to recognise.
 */
title: string | null, 
/**
 * The first covered block's start, RFC 3339, UTC -- what the source is
 * told.
 */
started_at: string, 
/**
 * The covered blocks' durations added up, gaps excluded.
 */
seconds: number, 
/**
 * How many blocks it concatenates, so a plan of one line still says it
 * is three sittings.
 */
blocks: number, };

/**
 * The timer as it is running right now.
 */
export type RunningTimer = { target: TimerTarget, 
/**
 * RFC 3339, UTC. What the strip's elapsed reading counts from.
 */
started_at: string, 
/**
 * The last moment knobas was known to be alive, RFC 3339, UTC.
 *
 * Nothing draws this. It is the stamp a stranded timer's block is closed
 * at when knobas starts again -- [`close_stranded`] is what it is for.
 */
last_heartbeat: string, };

/**
 * A ticket the ad-hoc dialog offers, and the rule that produced it.
 */
export type Suggestion = { entity_id: string, 
/**
 * What the mirror calls it, or `null`.
 *
 * Beside the id rather than inside it, and blank read as nothing -- the
 * rule [`super::day::DayBlock`] states, for the same reason: the title is
 * the mirror's current opinion of a row that may since have been renamed
 * or purged, and the view's one fallback is to show the id.
 */
title: string | null, rule: SuggestionRule, };

/**
 * Which rule produced a suggestion -- the *why* the dialog puts on screen.
 *
 * A word rather than a sentence, and the sentences live in the component.
 * The reason names knobas' own rule and has to be readable beside the ticket
 * it explains ("linked to this page", "the room you were in"), which is
 * wording that belongs with the layout it sits in; what the backend owes is
 * the fact of *which* rule fired, and that is one closed vocabulary.
 *
 * The dialog draws a sentence per member, and that is the whole of what this
 * union is for: a suggestion whose reason is not on screen is a guess the
 * reader can only trust or ignore.
 *
 * - `linked_to_target`: rule one -- a confirmed link from the block's target
 *   to this ticket.
 * - `context_anchor`: rule two -- this ticket anchors the stored context the
 *   block ran in.
 * - `last_logged`: rule three -- this ticket is the last one logged to on the
 *   block's day.
 *
 * The rules are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/time.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type SuggestionRule = "linked_to_target" | "context_anchor" | "last_logged";

/**
 * What a running timer, and therefore a block, is attributed to.
 *
 * `CONTEXT.md`'s **timer target**: one entity or one ad-hoc label. A tagged
 * union rather than two nullable fields, because "exactly one" is then a
 * thing the type says instead of a thing every reader has to check -- the
 * database says it too (`timer_target_chk`), and the two agreeing is what
 * makes the check a backstop rather than the only guard.
 *
 * - `entity`: a ticket, page, note, repo or asset -- something knobas has an
 *   id for. **A stored context is never one** (`CONTEXT.md`, *timer target*):
 *   a context is a set, and time on a set has nowhere to go. `start_timer`
 *   refuses a `ctx:` id with `invalid`, and the picker never offers one.
 * - `label`: "DB config for the migration" -- work with no entity behind it.
 *
 * The halves are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/time.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type TimerTarget = { "kind": "entity", 
/**
 * `"<namespace>:<key>"` -- `knobas_core::entity::EntityRef`.
 */
entity_id: string, } | { "kind": "label", 
/**
 * Trimmed by the backend, and never empty.
 */
label: string, };

/**
 * The week, as the timesheet draws it.
 */
export type Week = { 
/**
 * The dates asked for (`"2026-08-24"` …), in order -- the column
 * headings.
 */
days: Array<string>, 
/**
 * Targets first, in title order, then the no-target row if it has
 * anything to say.
 */
rows: Array<WeekRow>, 
/**
 * Which of [`Week::days`] reach back past what retention has swept, in
 * the same order (#337).
 *
 * **One entry per day and not one flag for the week**, because a week
 * straddling the horizon is the ordinary case: [`vet`] bounds a
 * timesheet's column *count* and says nothing about where its windows
 * sit, so any four days can be behind the horizon and the other three in
 * front of it.
 *
 * It rides on the week rather than on the "no target, app open" row for
 * the reason that row is dropped when it has nothing to say: a week
 * entirely past the horizon has no such row, and that is exactly the week
 * this most needs to speak about.
 */
past_horizon: Array<boolean>, };

/**
 * One target's numbers on one day, in seconds.
 *
 * Seconds on the wire and minutes on the screen: `CONTEXT.md`'s timesheet is
 * minute-granular with no rounding, and rounding is the *view's* to do once,
 * on a number that never lost its precision getting there. A backend that
 * handed over minutes would have rounded seven times per row and the week's
 * total would not be the total of the week.
 */
export type WeekCell = { 
/**
 * The day's manual blocks on this target, added up. On the "no target"
 * row, the focused time no block covered.
 *
 * Passive blocks are not in it: tracked time is what the reader said
 * their day was, which is the same rule the day review's heading states.
 */
tracked_seconds: number, 
/**
 * The day's **passive** blocks on this target -- what knobas is offering,
 * which is never tracked and never logged.
 *
 * Beside `tracked_seconds` rather than inside it, and present rather than
 * dropped: the strip above this draws these blocks, and a week in which
 * they were simply absent would disagree with it about the same
 * afternoon. Nothing logs one until a person assigns it, which is why it
 * pays into no other column here.
 */
offered_seconds: number, 
/**
 * Worklog seconds whose write is `pending` or `sent`.
 *
 * **A pending worklog counts as logged** (spec #272 story 39): the number
 * is about what the reader did, not about sync timing.
 */
logged_seconds: number, 
/**
 * Worklog seconds whose write is waiting on a person -- `held`,
 * `refused`, `discarded`, or a copy whose queue row is gone.
 *
 * Shown as *held* rather than folded into either neighbour: the blocks
 * under it already carry a worklog id, so *Log all* will not offer them
 * again, and drawing it as unlogged would invite logging one afternoon
 * twice.
 */
held_seconds: number, 
/**
 * `tracked - logged - held`, floored at zero.
 */
unlogged_seconds: number, };

/**
 * One row of the timesheet: a target, or the row that has none.
 */
export type WeekRow = { 
/**
 * The target, or `null` for the **"no target, app open"** row.
 *
 * That row is focused time no block covers, so that the week's total is
 * honest (spec story 41). It is present only when there is something to
 * say -- a week knobas was shut for has no such row at all, which is how
 * "the app was closed" reads differently from "the app was open and
 * idle".
 *
 * `null` rather than a third `TimerTarget` variant: that enum is what a
 * timer may be started on and what a block stores, and "nothing, but the
 * window was in front of you" is neither. It is a row the timesheet
 * makes, not a target anything can be attributed to -- which is also why
 * [`plan`] skips it.
 */
target: TimerTarget | null, 
/**
 * The entity's title from `knobas.entity`, or `null`.
 *
 * `null` for a label row and for the no-target row -- neither has an
 * entity -- and for an entity the mirror never held or has purged, which
 * is the split [`super::day::DayBlock`] records: a block is knobas' own
 * record and a title is the mirror's current opinion of it.
 */
title: string | null, 
/**
 * One cell per requested day, in the order they were requested.
 */
cells: Array<WeekCell>, };

/**
 * knobas' copy of a worklog it has sent, or is still sending.
 */
export type Worklog = { id: number, entity_id: string, 
/**
 * RFC 3339, UTC.
 */
started_at: string, seconds: number, comment: string, 
/**
 * The blocks this worklog was made of. They are read-only from now on:
 * each carries `worklog_id` pointing back here.
 */
block_ids: Array<number>, 
/**
 * The write that carries it. Read its state with `pending_writes`.
 */
write_queue_id: number | null, 
/**
 * What Jira called it, once Jira has answered -- `null` while the write
 * is still owed, and after one that was refused.
 */
remote_id: string | null, 
/**
 * RFC 3339, UTC.
 */
created_at: string, };
