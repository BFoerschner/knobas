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

  **A logged block is read-only** (`CONTEXT.md`, *block*; story 20). The strip
  says so and offers nothing; the backend refuses it as well, and if that
  refusal is what a reader hits, the sentence it came with is shown here rather
  than swallowed — "nothing happened" is not something anybody can act on.

  **Passive blocks arrive with #281.** The kind switch below is here now with
  one arm that acts, because story 22 is that the two are *distinguishable* on
  one strip, and a switch added later would be a rendering rule discovered
  after the strip already existed.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    dayBlocks as realDayBlocks,
    deleteBlock as realDeleteBlock,
    updateBlock as realUpdateBlock,
    type DayBlock,
    type TimerTarget,
  } from "../ipc/time";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { targetReading } from "../shell/timer";
  import {
    clockReading,
    dayKey,
    dayLabel,
    dayBounds,
    durationReading,
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
  }

  let {
    router,
    day = null,
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
    ...ports,
  };

  const key = $derived(day ?? dayKey(now()));
  const today = $derived(dayKey(now()));

  let rows = $state<DayBlock[]>([]);
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

  const segments = $derived(segmentsOf(rows));

  const tracked = $derived(
    rows.reduce(
      (total, entry) => total + minutesBetween(entry.block.started_at, entry.block.ended_at),
      0,
    ),
  );

  /**
   * Read the day whenever the address names another one.
   *
   * Tokened rather than cancelled: two reads can be in flight when a reader
   * clicks through days quickly, and the one that answers last must not be the
   * one that wins if it is not the one that was asked for last.
   */
  let reads = 0;
  $effect(() => {
    void load(key);
  });

  async function load(on: string) {
    const mine = ++reads;
    const { from, to } = dayBounds(on);
    try {
      const listed = await io.dayBlocks(from.toISOString(), to.toISOString());
      if (mine !== reads) return;
      rows = listed;
      failure = null;
    } catch (error) {
      if (mine !== reads) return;
      failure = ipcErrorMessage(error);
    }
  }

  function go(to: string) {
    editing = null;
    refusal = null;
    router.go(hashFor({ view: "time", day: to }));
  }

  /**
   * Open the entity a block was on.
   *
   * The kind-agnostic `#/entity/<id>` alias, and through `hashFor` rather than
   * a template string: a block's target is an entity id, entity keys carry `#`
   * and `/` (`gitea:acme/payouts#144`), and unencoded the first truncates the
   * fragment at the browser level. The kind is `get_entity`'s to resolve — the
   * same route `InboxView` takes, for the same reason.
   */
  function open(entityId: string) {
    router.go(hashFor({ view: "room", ctx: router.ctx, detail: { kind: null, entityId } }));
  }

  /** Where a block's target goes when it is clicked, or `null` for a label. */
  function addressOf(target: TimerTarget): string | null {
    return target.kind === "entity"
      ? hashFor({ view: "room", ctx: router.ctx, detail: { kind: null, entityId: target.entity_id } })
      : null;
  }

  /** Run one edit, keep its refusal, and re-read the day either way. */
  async function write(action: () => Promise<unknown>) {
    refusal = null;
    try {
      await action();
      editing = null;
    } catch (error) {
      refusal = ipcErrorMessage(error);
    }
    await load(key);
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

  function edit(entry: DayBlock) {
    refusal = null;
    editing = entry.block.id;
    form = {
      from: clockReading(entry.block.started_at),
      to: clockReading(entry.block.ended_at),
      kind: entry.block.target.kind,
      value:
        entry.block.target.kind === "entity"
          ? entry.block.target.entity_id
          : entry.block.target.label,
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

    {#if segments.length === 0 && !failure}
      <p class="empty">
        Nothing tracked on this day. ⌘T starts the timer on whatever is in front of you, and
        every stretch it runs lands here.
      </p>
    {/if}

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
            <span class="when">{clockReading(segment.from)} → {clockReading(segment.to)}</span>
            <span class="txt">{durationReading(segment.minutes)} unaccounted</span>
          </li>
        {:else}
          {@const entry = segment.block}
          {@const block = entry.block}
          {@const target = block.target}
          {@const locked = block.worklog_id !== null}
          <li class="seg block {block.kind} {locked ? 'locked' : ''}">
            <span class="when">
              {clockReading(block.started_at)} → {clockReading(block.ended_at)}
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
                  onclick={() => open(target.entity_id)}
                  title="Open {addressOf(target)}"
                >
                  {entry.title ?? targetReading(target)}
                </button>
                <span class="ctxk">{target.entity_id}</span>
              {:else}
                <span class="label">{target.label}</span>
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
                <select class="sel-inline" id="kind-{block.id}" bind:value={form.kind}>
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

  .refusal {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--fail);
    border-radius: 2px;
    color: var(--text);
    background: var(--raised);
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
