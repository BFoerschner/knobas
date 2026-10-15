<!--
  The Time view's Day tab (#827, R-13.4), ported from the rooms UI's day
  review over the same reads and writes.

  Five figures, the strip with a now line, then a row per block with its state
  and actions. `day-view.ts` decides which row is which; this asks the bridge
  and draws.

  **The strip is SVG.** Its geometry is a share of the day per block, and
  `style-src 'self'` refuses the inline `left` and `width` the prototype uses
  (`house-rules.test.ts`). SVG's `x` and `width` are attributes, not style, and
  take percentages.

  **Yellow is the running timer's alone** (design system: yellow means "the
  ticket you are on"). Only the `running` row carries `tf-tm-run`.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    adHocBlock as realAdHocBlock,
    createBlock as realCreateBlock,
    dayBlocks as realDayBlocks,
    deleteBlock as realDeleteBlock,
    updateBlock as realUpdateBlock,
    weekTimesheet as realWeekTimesheet,
    type AdHocBlock,
    type DayBlock,
    type TimerTarget,
  } from "../../ipc/time";
  import { push } from "../../shell/toasts.svelte";
  import { targetReading } from "../../shell/timer";
  import type { Timer } from "../../shell/timer.svelte";
  import { clockReading, dayBounds, dayKey, dayLabel, shiftDay } from "../../time/day";
  import { atClock, clockOf, offsetMinutes } from "../../time/draft";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { dayFigures, dayRows, hm, place, stripWindow, type DayRow } from "./day-view";
  import TfBlockDialog, { type BlockForm } from "./TfBlockDialog.svelte";

  interface DayPorts {
    dayBlocks: typeof realDayBlocks;
    adHocBlock: typeof realAdHocBlock;
    weekTimesheet: typeof realWeekTimesheet;
    updateBlock: typeof realUpdateBlock;
    createBlock: typeof realCreateBlock;
    deleteBlock: typeof realDeleteBlock;
  }

  let {
    databaseReady = true,
    day = null,
    timer,
    ontimer,
    onlog,
    revision = 0,
    onchanged,
    ports,
    now = () => new Date(),
    layers = sharedLayers,
  }: {
    /** Whether the database answers yet: nothing is read before it does. */
    databaseReady?: boolean;
    /** The day from the address, or `null` for today. */
    day?: string | null;
    timer: Pick<Timer, "current">;
    /** Stop the timer into its worklog draft, as the top bar's chip does. */
    ontimer: (() => void) | undefined;
    /** Open the worklog draft for a ticket and day. */
    onlog?: ((entityId: string, day: string) => void) | undefined;
    /** Bumped by whatever else changed the day: a draft logged, *Log all*. */
    revision?: number;
    /** Called after a write here, so other time surfaces re-read. */
    onchanged?: (() => void) | undefined;
    ports?: Partial<DayPorts>;
    now?: () => Date;
    layers?: Layers;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, as `DayReview`'s ports are: a bridge swapped mid-life would read
  // the day through one set and edit it through another.
  const io: DayPorts = {
    dayBlocks: realDayBlocks,
    adHocBlock: realAdHocBlock,
    weekTimesheet: realWeekTimesheet,
    updateBlock: realUpdateBlock,
    createBlock: realCreateBlock,
    deleteBlock: realDeleteBlock,
    ...ports,
  };

  /** The clock, moved every thirty seconds so the running row and now line move. */
  // svelte-ignore state_referenced_locally
  let clock = $state(now());
  onMount(() => {
    const tick = setInterval(() => (clock = now()), 30_000);
    return () => clearInterval(tick);
  });

  const key = $derived(day ?? dayKey(clock));
  const today = $derived(dayKey(clock));

  interface Loaded {
    day: string;
    blocks: DayBlock[];
    pastHorizon: boolean;
    offers: Record<number, AdHocBlock | null>;
    openSeconds: number;
  }
  let loaded = $state<Loaded | null>(null);
  let failure = $state<string | null>(null);
  let issued = 0;

  /**
   * The day, what `ad_hoc_block` says of each unlogged block, and the day's
   * focused time no block covers, as one answer so they cannot disagree.
   *
   * An offer that fails is left out, and the row falls back on the target's
   * shape; the week read failing leaves *open* at zero. Neither is worth
   * blanking the day for.
   */
  async function read(on: string): Promise<Loaded> {
    const { from, to } = dayBounds(on);
    const window = { day: on, from: from.toISOString(), to: to.toISOString() };
    const [record, openSeconds] = await Promise.all([
      io.dayBlocks(window.from, window.to),
      io
        .weekTimesheet([window])
        .then((week) => week.rows.find((row) => row.target === null)?.cells[0]?.tracked_seconds ?? 0)
        .catch(() => 0),
    ]);
    const offers: Record<number, AdHocBlock | null> = {};
    const when = { day: on, offsetMinutes: offsetMinutes(from) };
    await Promise.all(
      record.blocks
        .filter((entry) => entry.block.worklog_id === null)
        .map((entry) =>
          io.adHocBlock(entry.block.id, when).then(
            (offer) => void (offers[entry.block.id] = offer),
            () => {},
          ),
        ),
    );
    return { day: on, blocks: record.blocks, pastHorizon: record.past_horizon, offers, openSeconds };
  }

  async function load(on: string) {
    const mine = (issued += 1);
    try {
      const answer = await read(on);
      if (mine !== issued) return;
      loaded = answer;
      failure = null;
    } catch (cause) {
      if (mine !== issued) return;
      failure = ipcErrorMessage(cause);
    }
  }

  // The running timer's start is read too: a stop turns the running row into
  // a block, and the day has to read it back.
  const runningSince = $derived(timer.current?.started_at ?? null);
  $effect(() => {
    revision;
    runningSince;
    if (databaseReady) void load(key);
  });

  const ready = $derived(loaded !== null && loaded.day === key ? loaded : null);
  const rows = $derived(
    ready === null
      ? []
      : dayRows({ day: key, blocks: ready.blocks, offers: ready.offers, running: timer.current, now: clock }),
  );
  const figures = $derived(dayFigures(rows, ready?.openSeconds ?? 0));
  const window = $derived(stripWindow(key, rows));
  const hours = $derived(Array.from({ length: window.to - window.from + 1 }, (_, index) => window.from + index));
  const nowAt = $derived(key === today ? place(key, window, clock.toISOString(), clock.toISOString()).left : null);
  const noRecord = $derived(ready !== null && ready.pastHorizon && ready.blocks.length === 0);

  let selected = $state<string | null>(null);
  function select(row: DayRow) {
    selected = row.key;
    document.getElementById(`tf-tm-row-${row.key}`)?.scrollIntoView({ block: "nearest" });
  }

  const STRIP_CLASS: Record<DayRow["state"], string> = {
    logged: "tf-tm-logged",
    unlogged: "tf-tm-manual",
    label: "tf-tm-label",
    running: "tf-tm-run",
    offered: "tf-tm-passive",
    untracked: "tf-tm-gap",
  };

  const hourLabel = (hour: number) => String(hour).padStart(2, "0");
  const span = (row: DayRow) =>
    `${clockReading(row.from, key)} to ${row.state === "running" ? "now" : clockReading(row.to, key)}`;
  const durationOf = (row: DayRow) => hm(Math.floor(row.ms / 60_000));
  const keyOf = (entityId: string) => entityId.slice(entityId.indexOf(":") + 1);

  /** The ticket an offered block is linked to, when that is not the block's own. */
  function linkedTo(row: DayRow): string | null {
    const suggestion = row.suggestion;
    if (suggestion === null || suggestion.rule !== "linked_to_target") return null;
    const own = row.target?.kind === "entity" ? row.target.entity_id : null;
    return suggestion.entity_id === own ? null : suggestion.entity_id;
  }

  /**
   * What the mirror calls a row's target. The running timer is no block yet, so
   * it borrows the title from a block of the day on the same target.
   */
  function titleOf(row: DayRow): string {
    if (row.block !== null) return row.block.title ?? "";
    const target = row.target;
    if (target?.kind !== "entity") return "";
    const same = ready?.blocks.find(
      (entry) => entry.block.target.kind === "entity" && entry.block.target.entity_id === target.entity_id,
    );
    return same?.title ?? "";
  }

  // ---- writes ----

  /** The dialog that is up: edit a block, assign a stretch, or add a block. */
  let dialog = $state<
    | { mode: "edit"; row: DayRow }
    | { mode: "assign"; row: DayRow }
    | { mode: "add" }
    | null
  >(null);
  let refusal = $state<string | null>(null);

  async function write(action: () => Promise<unknown>, said: string): Promise<boolean> {
    try {
      await action();
    } catch (error) {
      refusal = ipcErrorMessage(error);
      if (dialog === null) push({ text: refusal, tone: "err" });
      return false;
    }
    dialog = null;
    refusal = null;
    push({ text: said });
    await load(key);
    onchanged?.();
    return true;
  }

  const entity = (entityId: string): TimerTarget => ({ kind: "entity", entity_id: entityId });

  /** *Add to <ticket>* on an offered block, *Log to <ticket>* on a label block. */
  function giveTo(row: DayRow, entityId: string, said: string) {
    const block = row.block;
    if (block === null) return;
    void write(() => io.updateBlock(block.block.id, row.from, row.to, entity(entityId)), said);
  }

  function extendToNow(row: DayRow) {
    const block = row.block;
    if (block === null) return;
    void write(
      () => io.updateBlock(block.block.id, row.from, now().toISOString(), block.block.target),
      "Extended the block to now",
    );
  }

  /** A field the reader did not touch leaves the stamp alone, seconds and all. */
  function moved(iso: string, hhmm: string): string {
    return clockOf(iso) === hhmm ? iso : atClock(iso, hhmm);
  }

  function save(form: BlockForm) {
    const current = dialog;
    if (current === null) return;
    refusal = null;
    if (current.mode === "add") {
      const midnight = dayBounds(key).from.toISOString();
      void write(
        () => io.createBlock(atClock(midnight, form.from), atClock(midnight, form.to), form.target),
        "Added the block",
      );
      return;
    }
    const row = current.row;
    if (current.mode === "assign") {
      const said = `Made a block on ${targetReading(form.target)}, ${clockReading(row.from)} to ${clockReading(row.to)}`;
      const block = row.block;
      void write(
        () =>
          block === null
            ? io.createBlock(row.from, row.to, form.target)
            : io.updateBlock(block.block.id, row.from, row.to, form.target),
        said,
      );
      return;
    }
    const block = row.block;
    if (block === null) return;
    void write(
      () => io.updateBlock(block.block.id, moved(row.from, form.from), moved(row.to, form.to), form.target),
      "Saved the block",
    );
  }

  function remove() {
    const current = dialog;
    if (current === null || current.mode !== "edit" || current.row.block === null) return;
    const id = current.row.block.block.id;
    void write(() => io.deleteBlock(id), "Deleted the block");
  }

  function open(next: NonNullable<typeof dialog>) {
    refusal = null;
    dialog = next;
  }
</script>

<div class="tf-tm-nav">
  <a class="tf-btn tf-sm tf-ghost" href="#/time/{shiftDay(key, -1)}" aria-label="Day before">‹</a>
  <h2>{dayLabel(key)}</h2>
  {#if key < today}
    <a class="tf-btn tf-sm tf-ghost" href="#/time/{shiftDay(key, 1)}" aria-label="Day after">›</a>
  {:else}
    <span class="tf-btn tf-sm tf-ghost tf-hidden" aria-hidden="true">›</span>
  {/if}
  {#if key !== today}
    <a class="tf-btn tf-sm" href="#/time">Today</a>
  {/if}
</div>

{#if failure}
  <p class="tf-tm-fail" role="alert">{failure}</p>
{/if}

{#if noRecord}
  <div class="tf-tm-empty">
    <p>
      <b>No record for this day.</b> knobas keeps what it saw in front of you for about a month, and has cleared
      this day. Blocks you tracked yourself are kept, and there were none.
    </p>
  </div>
{:else if ready !== null}
  <div class="tf-tm-sum">
    <div><b>{hm(figures.tracked)}</b><span>tracked</span></div>
    <div><b>{hm(figures.logged)}</b><span>logged</span></div>
    <div class={figures.toLog > 0 ? "tf-warn" : ""}><b>{hm(figures.toLog)}</b><span>to log</span></div>
    <div><b>{hm(figures.offered)}</b><span>offered by knobas</span></div>
    <div><b>{hm(figures.open)}</b><span>open, app in front</span></div>
  </div>

  {#if ready.pastHorizon}
    <p class="tf-tm-note">
      <b>No record of what was in front of you.</b> knobas keeps it for about a month, and has cleared this day. Blocks
      you tracked yourself are kept.
    </p>
  {/if}

  <svg class="tf-tm-strip" width="100%" height="84" role="group" aria-label="The day's blocks">
    <defs>
      <pattern id="tf-tm-hatch" width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect class="tf-tm-hatch" width="5" height="10" />
      </pattern>
    </defs>
    <rect class="tf-tm-bed" x="0" y="22" width="100%" height="62" rx="6" />
    {#each hours as hour (hour)}
      {@const x = `${Math.round(((hour - window.from) / (window.to - window.from)) * 10_000) / 100}%`}
      <line class="tf-tm-hr" x1={x} x2={x} y1="22" y2="84" />
      <text class="tf-tm-hl" {x} y="14" text-anchor="middle">{hourLabel(hour)}</text>
    {/each}
    {#each rows as row (row.key)}
      {@const at = place(key, window, row.from, row.to)}
      <svg
        class="tf-tm-b {STRIP_CLASS[row.state]} {selected === row.key ? 'tf-on' : ''}"
        x="{at.left}%"
        y="30"
        width="{at.width}%"
        height="46"
        overflow="visible"
        role="button"
        tabindex="0"
        aria-label="{span(row)}, {row.target ? targetReading(row.target) : 'untracked'}"
        onclick={() => select(row)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            select(row);
          }
        }}
      >
        <title>{span(row)}</title>
        <rect width="100%" height="100%" rx="4" />
        <svg width="100%" height="100%">
          <text x="6" y="50%" dominant-baseline="central">{row.target ? targetReading(row.target) : ""}</text>
        </svg>
      </svg>
    {/each}
    {#if nowAt !== null}
      <rect class="tf-tm-now" x="{nowAt}%" y="18" width="2" height="70">
        <title>Now, {clockOf(clock.toISOString())}</title>
      </rect>
    {/if}
  </svg>

  <div class="tf-tm-legend">
    <span><i class="tf-tm-manual"></i>You tracked</span>
    <span><i class="tf-tm-logged"></i>Logged</span>
    <span><i class="tf-tm-run"></i>Timer running</span>
    <span><i class="tf-tm-passive"></i>Offered: what was in front of you</span>
    <span><i class="tf-tm-gap"></i>Untracked: no block covers it</span>
    <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "add" })}>+ Add a block</button>
  </div>

  {#if rows.length === 0}
    <div class="tf-tm-empty">
      <p>Nothing tracked on this day. ⌘T starts the timer on whatever is in front of you, and every stretch it runs lands here.</p>
    </div>
  {:else}
    <div class="tf-tm-list">
      {#each rows as row (row.key)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <!-- A row click selects; the strip's blocks are the keyboard's way to the same. -->
        <div
          id="tf-tm-row-{row.key}"
          class="tf-tm-row {selected === row.key ? 'tf-on' : ''}"
          data-state={row.state}
          onclick={(event) => {
            if (!(event.target as HTMLElement).closest("button")) selected = row.key;
          }}
        >
          <span class="tf-tm-when">{span(row)}<b>{durationOf(row)}</b></span>
          <span class="tf-tm-what">
            {#if row.state === "untracked"}
              Nothing tracked
              <small>No block covers this time. Assign it to a ticket if it was work.</small>
            {:else if row.target !== null}
              {#if row.state === "offered"}<span class="tf-dim">knobas saw</span>{/if}
              {#if row.target.kind === "entity"}
                <span class="tf-mono">{targetReading(row.target)}</span>
                {titleOf(row)}
              {:else}
                {row.target.label} <span class="tf-dim">(label)</span>
              {/if}
              {#if row.state === "offered"}
                <small>
                  {targetReading(row.target)} was in front of you.
                  {#if linkedTo(row)}It is linked to {keyOf(linkedTo(row) ?? "")}.{/if}
                </small>
              {:else if row.state === "label"}
                <small>
                  {row.target.kind === "label" ? "A label, not a ticket." : "Not a ticket."} Jira only takes time on a ticket.
                </small>
              {:else if row.block?.block.ended_by_relaunch}
                <small>Ended when knobas closed.</small>
              {/if}
            {/if}
          </span>
          <span class="tf-tm-state">
            {#if row.state === "logged"}<span class="tf-chip tf-ok">logged</span>
            {:else if row.state === "unlogged"}<span class="tf-chip tf-warn">unlogged</span>
            {:else if row.state === "running"}<span class="tf-chip tf-info">timer running</span>
            {:else if row.state === "label"}<span class="tf-chip">needs a ticket</span>
            {:else if row.state === "offered"}<span class="tf-chip">offered</span>
            {:else}<span class="tf-chip">untracked</span>{/if}
          </span>
          <span class="tf-tm-acts">
            {#if row.state === "unlogged" && row.target?.kind === "entity"}
              {@const on = row.target.entity_id}
              {#if row.block?.block.ended_by_relaunch}
                <button class="tf-btn tf-sm" onclick={() => extendToNow(row)}>Extend to now</button>
              {/if}
              <button class="tf-btn tf-sm" onclick={() => onlog?.(on, key)}>Log…</button>
              <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "edit", row })}>Edit</button>
            {:else if row.state === "running"}
              <button class="tf-btn tf-sm" onclick={() => ontimer?.()}>Stop and log…</button>
            {:else if row.state === "label"}
              {#if row.suggestion}
                {@const to = row.suggestion.entity_id}
                <button
                  class="tf-btn tf-sm"
                  onclick={() => giveTo(row, to, `Assigned to ${keyOf(to)}. It is in Log all now.`)}
                >
                  Log to {keyOf(to)}
                </button>
                <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "assign", row })}>Other ticket…</button>
              {:else}
                <button class="tf-btn tf-sm" onclick={() => open({ mode: "assign", row })}>Assign…</button>
              {/if}
              <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "edit", row })}>Edit</button>
            {:else if row.state === "offered"}
              {#if row.suggestion}
                {@const to = row.suggestion.entity_id}
                <button
                  class="tf-btn tf-sm"
                  onclick={() => giveTo(row, to, `Added ${durationOf(row)} to ${keyOf(to)}`)}
                >
                  Add to {keyOf(to)}
                </button>
              {/if}
              <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "assign", row })}>Assign…</button>
            {:else if row.state === "untracked"}
              <button class="tf-btn tf-sm" onclick={() => open({ mode: "assign", row })}>Assign…</button>
            {/if}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  <p class="tf-tm-foot">
    Passive blocks come from what was in front of you while knobas had the focus. They are offers: nothing is logged
    until you add one to a ticket. <a href="#/settings/time">Settings › Time</a>
  </p>
{/if}

{#if dialog !== null}
  {@const row = dialog.mode === "add" ? null : dialog.row}
  <TfBlockDialog
    mode={dialog.mode}
    from={row ? clockOf(row.from) : "09:00"}
    to={row ? clockOf(row.to) : "10:00"}
    target={dialog.mode === "edit" ? row?.target ?? null : null}
    span={row ? span(row) : null}
    {refusal}
    {layers}
    onsave={save}
    ondelete={remove}
    onclose={() => (dialog = null)}
  />
{/if}
