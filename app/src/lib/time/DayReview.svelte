<!--
  The day review — one day's blocks on one strip, editable (issue #279, spec
  #272 stories 18-21).

  **One strip, in time order, with the gaps drawn as gaps.** A day is a list of
  things a person can fix rather than one number, and the thing a person most
  often wants to fix is the half-hour nobody claimed. So an unaccounted stretch
  is a segment of its own — dashed, labelled, and never mistakable for a block
  of no length, which is a real thing a timer can leave behind (`day.ts`'s
  `segmentsOf` carries the whole rule).

  **Widths are not proportional, and that is a CSP decision rather than a
  design one.** `style-src 'self'` forbids inline style attributes, so a
  segment cannot carry a width computed from its duration (`house-rules.test.ts`
  enforces this, `FirstRun.svelte`'s native `<progress>` is the other way
  round it). Every segment therefore carries its span *in words* — `09:00 →
  10:15 · 1 h 15 min` — which is the reading a person checks against a
  timesheet anyway.

  **A logged block is read-only** (spec #272, story 20 — not a sentence
  `CONTEXT.md` carries; its *block* entry says only that a block remembers
  which worklog it was logged into). The strip
  says so and offers nothing; the backend refuses it as well, and if that
  refusal is what a reader hits, the sentence it came with is shown here rather
  than swallowed — "nothing happened" is not something anybody can act on.

  **Passive blocks are the second style** (#282). A passive block is knobas'
  own guess at what was open, so it says so in words as well as in colour —
  colour alone is not a reading — and the only thing it offers is *Assign…*:
  it may not be edited or deleted, because the next day read reconciles the
  day's unassigned passive blocks back to what the observations support and an edit
  would be undone under the reader's hands. Assigning is what takes it out of
  that reconciliation, by making it manual.

  **A gap offers *Assign…* too, and it is the same sentence.** "This half-hour
  was this ticket" is one thought; whether knobas already had a row to put it
  on is not the reader's problem. The two paths differ by one call —
  `update_block` on a block, `create_block` on a gap — and share a form.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    createBlock as realCreateBlock,
    dayBlocks as realDayBlocks,
    deleteBlock as realDeleteBlock,
    updateBlock as realUpdateBlock,
    type DayBlock,
    type DayRecord,
    type TimerTarget,
  } from "../ipc/time";
  import { latestRead } from "../shell/latest-read";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { targetReading } from "../shell/timer";
  import {
    clockReading,
    dayKey,
    dayLabel,
    dayBounds,
    durationReading,
    horizonNote,
    minutesBetween,
    segmentsOf,
    shiftDay,
  } from "./day";

  /**
   * The bridge this view needs, injectable so a test needs no Tauri.
   *
   * Not exported: an instance `<script>` cannot export a type in Svelte 5, and
   * a caller supplying ports writes an object literal anyway — the shape
   * `TimerPicker`'s `recent` prop and `createTimer`'s `TimerPorts` both use.
   */
  interface DayPorts {
    dayBlocks: typeof realDayBlocks;
    updateBlock: typeof realUpdateBlock;
    deleteBlock: typeof realDeleteBlock;
    createBlock: typeof realCreateBlock;
  }

  let {
    router,
    day = null,
    revision = 0,
    onchanged,
    ports,
    now = () => new Date(),
  }: {
    router: Router;
    /**
     * The day from the address, or `null` for today.
     *
     * `null` is resolved here rather than in `parseHash`, which is pure: a
     * parse that read the clock would give `#/time` a different meaning
     * depending on when it was parsed.
     */
    day?: string | null;
    /**
     * Bumped by whatever else on this screen changed this day — the week
     * timesheet's *Log all*, which makes this day's blocks read-only (#283).
     *
     * A number rather than an event, because the two views are siblings and
     * the shell owns the screen: `update_block` and `log_all` deliberately
     * write no activity line of their own, so there is no signal to listen to
     * and inventing one would be a second thing to keep in step with the
     * first.
     */
    revision?: number;
    /** Called after a successful edit, so the week below re-reads too. */
    onchanged?: (() => void) | undefined;
    ports?: Partial<DayPorts>;
    /** Injectable clock — what *Today* and *Extend to now* mean. */
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init, on purpose -- the decision `TimerPicker`'s `recent`
  // prop records: production omits this prop, nothing changes it, and a bridge
  // swapped mid-life would leave the day already on screen read through one
  // set of ports and edited through another.
  const io: DayPorts = {
    dayBlocks: realDayBlocks,
    updateBlock: realUpdateBlock,
    deleteBlock: realDeleteBlock,
    createBlock: realCreateBlock,
    ...ports,
  };

  const key = $derived(day ?? dayKey(now()));
  const today = $derived(dayKey(now()));

  let rows = $state<DayBlock[]>([]);
  /**
   * This day reaches back past what retention has swept (#337).
   *
   * Not derived from the strip, and it could not be: a day past the horizon
   * draws exactly what a day nobody had the app open on draws. It is the
   * backend's answer, from the stamp that says what knobas actually threw
   * away.
   */
  let pastHorizon = $state(false);
  /** The read failed. The header still works, so the reader can move days. */
  let failure = $state<string | null>(null);
  /**
   * Why the last edit was refused, in the backend's own words.
   *
   * Shown in the view rather than pushed as a toast, and that is the
   * acceptance criterion: "refused with a reason the view shows". The commonest
   * refusal by far is *this block is logged*, which is a fact about the block
   * on screen — a message that floats away six seconds later would leave the
   * reader looking at a row that simply refuses to change.
   */
  let refusal = $state<string | null>(null);
  /** The block whose edit form is open, if any. One at a time. */
  let editing = $state<number | null>(null);
  let form = $state({ from: "", to: "", kind: "label" as TimerTarget["kind"], value: "" });
  /**
   * A stretch the reader is naming — a passive block, or a gap.
   *
   * One type for both paths, and `id` is the whole difference: a passive block
   * has a row to rewrite and a gap does not. A named type rather than the
   * shape spelled out at each of the four places it travels through, because
   * that is how the two paths come to disagree about what an assignment is.
   */
  interface Assignment {
    /** The segment it is open on, so one form is open at a time. */
    key: string;
    from: string;
    to: string;
    /** The block to rewrite, or `null` for a gap, which has no row yet. */
    id: number | null;
  }

  /** The stretch whose *Assign…* form is open, if any (#282). */
  let assigning = $state<Assignment | null>(null);
  let assignForm = $state({ kind: "label" as TimerTarget["kind"], value: "" });

  const segments = $derived(segmentsOf(rows));

  /** Minutes over the rows a predicate keeps. */
  function total(kind: "manual" | "passive"): number {
    return rows
      .filter((entry) => entry.block.kind === kind)
      .reduce(
        (sum, entry) => sum + minutesBetween(entry.block.started_at, entry.block.ended_at),
        0,
      );
  }

  /**
   * **Tracked time is manual time, and passive time is counted beside it.**
   *
   * A passive block says *what was open — not tracked*, so a heading that
   * added it to the tracked total would contradict, in one line, every block
   * it was summing. The two numbers are the two things a person wants at a
   * glance: what they have said their day was, and what knobas is offering to
   * fill the rest with.
   */
  const tracked = $derived(total("manual"));
  const offered = $derived(total("passive"));

  /**
   * Read the day whenever the address names another one.
   *
   * Through `latestRead` rather than a token of this view's own: clicking
   * through days puts several reads in flight with nothing sequencing them,
   * and that module exists because the guard is four lines copied wrong in one
   * predictable way — the **rejection** path gets dropped, so a stale failure
   * blanks a day that has since read fine (#107).
   */
  const readDay = latestRead<DayRecord>();
  $effect(() => {
    // Read for its own sake: *Log all* below changes this day's blocks without
    // changing the address, and a strip still offering *Edit* on a block that
    // has just become read-only is a strip that disagrees with the backend.
    revision;
    void load(key);
  });

  async function load(on: string) {
    const { from, to } = dayBounds(on);
    await readDay(() => io.dayBlocks(from.toISOString(), to.toISOString()), {
      ok: (listed) => {
        rows = listed.blocks;
        pastHorizon = listed.past_horizon;
        failure = null;
      },
      fail: (cause) => {
        failure = ipcErrorMessage(cause);
      },
    });
  }

  function go(to: string) {
    editing = null;
    assigning = null;
    refusal = null;
    router.go(hashFor({ view: "time", day: to }));
  }

  /**
   * The address an entity target opens at.
   *
   * The kind-agnostic `#/entity/<id>` alias, and built with `hashFor` rather
   * than a template string: entity keys carry `#` and `/`
   * (`gitea:acme/payouts#144`), and unencoded the first truncates the fragment
   * at the browser level and the second reads as another path segment. The
   * kind is `get_entity`'s to resolve — the same route `InboxView` takes, for
   * the same reason.
   *
   * One function for both the navigation and the tooltip, so the address a
   * reader is promised and the address they are taken to cannot be built two
   * ways.
   */
  function addressOf(entityId: string): string {
    return hashFor({ view: "room", ctx: router.ctx, detail: { kind: null, entityId } });
  }


  /**
   * Run one edit, keep its refusal, and re-read the day either way.
   *
   * `onchanged` on success only, and it is story 44's whole mechanism: the
   * week below re-reads, so assigning a block and watching the week's unlogged
   * total change is one glance rather than a reload.
   */
  async function write(action: () => Promise<unknown>) {
    refusal = null;
    let wrote = false;
    try {
      await action();
      editing = null;
      assigning = null;
      wrote = true;
    } catch (error) {
      refusal = ipcErrorMessage(error);
    }
    await load(key);
    if (wrote) onchanged?.();
  }

  /**
   * *Extend to now* (story 13): the block runs to this moment, and stops
   * saying knobas chose its end.
   *
   * One call, not two: `update_block` carries the block's whole editable shape
   * and clears `ended_by_relaunch` on every success, so "the end moved" and
   * "the marker went" cannot come apart.
   */
  function extendToNow(entry: DayBlock) {
    void write(() =>
      io.updateBlock(
        entry.block.id,
        entry.block.started_at,
        now().toISOString(),
        entry.block.target,
      ),
    );
  }

  function remove(entry: DayBlock) {
    void write(() => io.deleteBlock(entry.block.id));
  }

  /**
   * Open the *Assign…* form on a passive block or on a gap.
   *
   * The field starts empty in both cases, including on a passive block that
   * already names a target: the reader is being asked *what this time was on*,
   * and pre-filling it with knobas' own guess would turn the question into a
   * confirmation of the thing the whole feature is careful not to assert.
   */
  function assign(over: Assignment) {
    refusal = null;
    editing = null;
    assigning = assigning?.key === over.key ? null : over;
    assignForm = { kind: "label", value: "" };
  }

  /**
   * Send an assignment: `update_block` where there is a row, `create_block`
   * where there is only a gap.
   *
   * The backend writes `kind: "manual"` on the first, which is what takes a
   * passive block out of the day read's reconciliation — see `time::day`'s
   * `UPDATE`. Both re-read the day, because both change what the strip is.
   */
  function saveAssignment(over: Assignment) {
    const value = assignForm.value.trim();
    const target: TimerTarget =
      assignForm.kind === "entity"
        ? { kind: "entity", entity_id: value }
        : { kind: "label", label: value };
    // The form is closed by `write` **on success only**, the way the edit form
    // is: a refusal leaves it open with what the reader typed still in it, so
    // the sentence the backend sent is beside the field it is about rather
    // than beside a form that has gone.
    void write(() =>
      over.id === null
        ? io.createBlock(over.from, over.to, target)
        : io.updateBlock(over.id, over.from, over.to, target),
    );
  }

  /** The half of a target a person types: an entity id, or the label itself. */
  function valueOf(target: TimerTarget): string {
    return target.kind === "entity" ? target.entity_id : target.label;
  }

  function edit(entry: DayBlock) {
    refusal = null;
    assigning = null;
    editing = entry.block.id;
    form = {
      from: clockReading(entry.block.started_at),
      to: clockReading(entry.block.ended_at),
      kind: entry.block.target.kind,
      value: valueOf(entry.block.target),
    };
  }

  /**
   * The instant `iso` falls on, with its clock moved to `hhmm`.
   *
   * Anchored on the field's **own** stamp rather than on the day being viewed,
   * so a block that ran through midnight keeps its end on the following day
   * when only the start is edited. A blank or unparsable field leaves the
   * stamp alone — the browser's own `type="time"` validation is what keeps one
   * out, and silently rewriting it to midnight would be worse than doing
   * nothing.
   */
  function withClock(iso: string, hhmm: string): string {
    // **A field the reader did not touch does not move the stamp.** The form
    // is minute-granular and the stamp is not, so rebuilding it from the field
    // would quietly drop the seconds off both edges of every block anybody
    // opened *Edit* on — a rounding policy by accident, which is exactly what
    // `CONTEXT.md`'s minute granularity, no rounding forbids.
    if (clockReading(iso) === hhmm) return iso;
    const [hours, minutes] = hhmm.split(":").map(Number);
    if (!Number.isFinite(hours) || !Number.isFinite(minutes)) return iso;
    const at = new Date(iso);
    at.setHours(hours!, minutes!, 0, 0);
    return at.toISOString();
  }

  function save(entry: DayBlock) {
    const target: TimerTarget =
      form.kind === "entity"
        ? { kind: "entity", entity_id: form.value.trim() }
        : { kind: "label", label: form.value.trim() };
    void write(() =>
      io.updateBlock(
        entry.block.id,
        withClock(entry.block.started_at, form.from),
        withClock(entry.block.ended_at, form.to),
        target,
      ),
    );
  }
</script>

<section class="view">
  <div class="room-bar">
    <h1>
      {dayLabel(key)}
      <span class="k">{durationReading(tracked)} tracked</span>
      {#if offered > 0}
        <span class="k offered">{durationReading(offered)} offered</span>
      {/if}
    </h1>
    <div class="days">
      <button class="btn sm" onclick={() => go(shiftDay(key, -1))} aria-label="The day before">
        ‹
      </button>
      <input
        class="inp day"
        type="date"
        aria-label="The day to review"
        value={key}
        onchange={(event) => go(event.currentTarget.value || today)}
      />
      <button class="btn sm" onclick={() => go(shiftDay(key, 1))} aria-label="The day after">›</button>
      {#if key !== today}
        <button class="btn sm" onclick={() => go(today)}>Today</button>
      {/if}
    </div>
  </div>

  <div class="view-b">
    {#if failure}
      <p class="empty fail">{failure}</p>
    {/if}
    {#if refusal}
      <!--
        The backend's own sentence, kept on screen until the next edit. Story
        20's refusal is a fact about a row the reader is looking at, not a
        passing notification.
      -->
      <p class="refusal" role="alert">{refusal}</p>
    {/if}

    {#if pastHorizon}
      <!--
        Said before the strip, and before the "nothing tracked" line below it,
        because it changes what that line means: on a day past the horizon
        knobas was not idle and the reader was not away — knobas has thrown the
        record away. The words are `horizonNote`'s, shared verbatim with the
        timesheet under this (#337).
      -->
      <p class="horizon">{horizonNote("this day")}</p>
    {/if}

    {#if segments.length === 0 && !failure}
      <p class="empty">
        Nothing tracked on this day. ⌘T starts the timer on whatever is in front of you, and
        every stretch it runs lands here.
      </p>
    {/if}

    <!--
      One form for both *Assign…* paths. A snippet rather than two copies:
      the fields are the same question, and two copies is how the gap path and
      the block path come to disagree about which halves a target may have.
    -->
    {#snippet assignment(over: Assignment)}
      <form
        class="edit"
        onsubmit={(event) => {
          event.preventDefault();
          saveAssignment(over);
        }}
      >
        <label class="l" for="assign-kind-{over.key}">On</label>
        <select class="sel-inline" id="assign-kind-{over.key}" bind:value={assignForm.kind}>
          <option value="entity">An entity</option>
          <option value="label">A label</option>
        </select>
        <input
          class="inp grow"
          type="text"
          aria-label="What this time was on"
          bind:value={assignForm.value}
        />
        <button class="btn sm pri" type="submit">Assign</button>
        <button class="btn sm ghost" type="button" onclick={() => (assigning = null)}>
          Cancel
        </button>
      </form>
    {/snippet}

    <ol class="strip">
      {#each segments as segment (segment.key)}
        {#if segment.kind === "gap"}
          <!--
            A gap, and drawn as one: no target, no actions, a dashed rule and
            the span it covers. Never a zero-length segment — `segmentsOf`
            emits a gap only where there is time in it, so a gap on screen is
            always time somebody lost.
          -->
          <li class="seg gap">
            <span class="when">
              {clockReading(segment.from, key)} → {clockReading(segment.to, key)}
            </span>
            <span class="txt">{durationReading(segment.minutes)} unaccounted</span>
            <span class="acts">
              <!--
                Story 23: a gap is the thing a person most often wants to fix,
                and *Assign…* on one writes a manual block spanning exactly it
                — the same sentence as assigning a passive block, sent through
                `create_block` because there is no row yet.
              -->
              <button
                class="btn sm"
                onclick={() =>
                  assign({
                    key: segment.key,
                    from: segment.from,
                    to: segment.to,
                    id: null,
                  })}
              >
                Assign…
              </button>
            </span>
            {#if assigning?.key === segment.key}
              {@render assignment(assigning)}
            {/if}
          </li>
        {:else}
          {@const entry = segment.block}
          {@const block = entry.block}
          {@const target = block.target}
          {@const locked = block.worklog_id !== null}
          <li class="seg block {block.kind} {locked ? 'locked' : ''}">
            <span class="when">
              {clockReading(block.started_at, key)} → {clockReading(block.ended_at, key)}
              <i>{durationReading(minutesBetween(block.started_at, block.ended_at))}</i>
            </span>

            <span class="txt">
              {#if target.kind === "entity"}
                <!--
                  Title first, id beside it. The title is the mirror's, and it
                  is `null` for a target the mirror has never held or has
                  purged — migration `0013` keeps no foreign key precisely so
                  an afternoon survives that — in which case the key is the
                  name, the same half `Detail.svelte`'s header shows.
                -->
                <button
                  class="link"
                  onclick={() => router.go(addressOf(target.entity_id))}
                  title="Open {addressOf(target.entity_id)}"
                >
                  {entry.title ?? targetReading(target)}
                </button>
                <span class="ctxk">{target.entity_id}</span>
              {:else}
                <span class="label">{target.label}</span>
              {/if}

              {#if block.kind === "passive"}
                <!--
                  Said in words and not only in colour: a passive block is
                  knobas' guess at what was open, and a reader who cannot tell
                  it from a block they made would be reading a claim they never
                  agreed to. Nothing here is ever logged on its own.
                -->
                <i class="passive">what was open — not tracked</i>
              {/if}
              {#if block.ended_by_relaunch}
                <!--
                  The honest reading of the flag: knobas stopped being alive
                  here, never "the work stopped here" (migration `0013`).
                -->
                <i class="relaunch">ended when knobas closed</i>
              {/if}
              {#if locked}
                <i class="logged">logged — read-only</i>
              {/if}
            </span>

            <span class="acts">
              {#if block.kind === "passive" && !locked}
                <!--
                  The only thing a passive block offers. It is deliberately not
                  editable or deletable: the next day read reconciles the day's
                  unassigned passive blocks back to what the observations support, so
                  an edit would be undone under the reader's hands. Assigning
                  is what takes it out of that reconciliation.
                -->
                <button
                  class="btn sm pri"
                  onclick={() =>
                    assign({
                      key: segment.key,
                      from: block.started_at,
                      to: block.ended_at,
                      id: block.id,
                    })}
                >
                  Assign…
                </button>
              {/if}
              {#if block.kind === "manual" && !locked}
                {#if block.ended_by_relaunch}
                  <button class="btn sm pri" onclick={() => extendToNow(entry)}>Extend to now</button>
                {/if}
                <button class="btn sm" onclick={() => (editing === block.id ? (editing = null) : edit(entry))}>
                  Edit
                </button>
                <button class="btn sm ghost" onclick={() => remove(entry)}>Delete</button>
              {/if}
            </span>

            {#if assigning?.key === segment.key}
              {@render assignment(assigning)}
            {/if}

            {#if editing === block.id}
              <form
                class="edit"
                onsubmit={(event) => {
                  event.preventDefault();
                  save(entry);
                }}
              >
                <label class="l" for="from-{block.id}">Start</label>
                <input class="inp t" id="from-{block.id}" type="time" bind:value={form.from} />
                <label class="l" for="to-{block.id}">End</label>
                <input class="inp t" id="to-{block.id}" type="time" bind:value={form.to} />
                <!--
                  Which half of the target, spelled out rather than guessed from
                  the text. `TimerTarget` is exactly-one on the wire, in the
                  Rust and in the schema; a field that decided for itself
                  whether "DB config: the migration" was an entity id would be
                  the one place that rule was a heuristic.
                -->
                <label class="l" for="kind-{block.id}">On</label>
                <!--
                  Switching halves clears the text unless it is the block's own
                  again: `TimerTarget` is exactly-one, and an entity id left in
                  the field after a switch to *A label* would be submitted as
                  the label a person is supposed to have written.
                -->
                <select
                  class="sel-inline"
                  id="kind-{block.id}"
                  bind:value={form.kind}
                  onchange={() => (form.value = form.kind === target.kind ? valueOf(target) : "")}
                >
                  <option value="entity">An entity</option>
                  <option value="label">A label</option>
                </select>
                <input
                  class="inp grow"
                  type="text"
                  aria-label="What the time was on"
                  bind:value={form.value}
                />
                <button class="btn sm pri" type="submit">Save</button>
                <button class="btn sm ghost" type="button" onclick={() => (editing = null)}>
                  Cancel
                </button>
              </form>
            {/if}
          </li>
        {/if}
      {/each}
    </ol>
  </div>
</section>

<style>
  .days {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .offered {
    color: var(--amber);
  }

  .refusal {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--fail);
    border-radius: 2px;
    color: var(--text);
    background: var(--raised);
  }

  /*
    Quieter than a refusal and louder than nothing: this is a fact about the
    day rather than something that went wrong, and there is nothing for the
    reader to do about it.
  */
  .horizon {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px dashed var(--hair2);
    border-radius: 2px;
    color: var(--muted);
    font: 400 12px var(--sans);
  }

  .strip {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .seg {
    display: grid;
    grid-template-columns: 148px 1fr auto;
    align-items: center;
    column-gap: 10px;
    min-height: var(--row);
    padding: 4px 8px;
    border: 1px solid var(--hair2);
    border-radius: 2px;
    background: var(--panel);
  }

  /* A gap is absence, and reads as one: no fill, a dashed edge, dimmed. */
  .seg.gap {
    border-style: dashed;
    background: transparent;
    color: var(--faint);
  }

  .seg.locked {
    background: var(--raised);
  }

  .when {
    font: 500 11px var(--mono);
    color: var(--faint);
  }

  .when i {
    color: var(--muted);
    font-style: normal;
  }

  .txt {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctxk {
    font: 500 11px var(--mono);
    color: var(--faint);
    margin-left: 6px;
  }

  /*
    The second style, and it is a *style* rather than a colour: a dashed left
    edge and a dimmed ground, so a passive block reads as provisional next to
    a manual one at a glance and still reads as one in monochrome. The gap
    above is dashed all round; this is dashed on one side, because a passive
    block is a claim about time that was used and a gap is a claim about time
    that was not.
  */
  .seg.block.passive {
    border-left: 2px dashed var(--amber);
    background: var(--raised);
  }

  .passive {
    color: var(--amber);
    margin-left: 6px;
  }

  .relaunch {
    color: var(--amber);
    margin-left: 6px;
  }

  .logged {
    color: var(--faint);
    margin-left: 6px;
  }

  .link {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--link);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .acts {
    display: flex;
    gap: 4px;
  }

  .edit {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--hair2);
  }

  .edit .l {
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
  }

  .edit .grow {
    flex: 1 1 180px;
  }

  .inp.day,
  .inp.t {
    height: 22px;
    padding: 0 4px;
    font-size: 12px;
  }
</style>
