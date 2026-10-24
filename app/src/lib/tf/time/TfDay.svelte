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

  **A worklog the source's credential cannot make** (`refused_writes`,
  #2052): an unlogged row's *Log…* is drawn disabled with the reason, asked
  once the rows are drawn, and opens no draft.

  **Yellow is the running timer's alone** (design system: yellow means "the
  ticket you are on"). Only the `running` row carries `tf-tm-run`.
-->
<script lang="ts">
  import { keyOf, minutesLabel } from "../../time/format";
  import { onMount } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    adHocBlock as realAdHocBlock,
    createBlock as realCreateBlock,
    dayBlocks as realDayBlocks,
    deleteBlock as realDeleteBlock,
    dismissBlock as realDismissBlock,
    updateBlock as realUpdateBlock,
    type TimerTarget,
  } from "../../ipc/time";
  import { launcherHome as realLauncherHome, search as realSearch } from "../../ipc/search";
  import type { SessionPorts } from "../../launcher/session.svelte";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { targetReading } from "../../shell/timer";
  import type { Timer } from "../../shell/timer.svelte";
  import { clockReading, dayBounds, dayKey, dayLabel, shiftDay } from "../../time/day";
  import { atClock, clockOf } from "../../time/draft";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { dayFigures, dayRows, fitLabel, offerRuns, place, stripWindow, type DayRow, type OfferRun } from "./day-view";
  import { readDay, type DayRecord } from "./day-read";
  import TfBlockDialog, { type BlockForm } from "./TfBlockDialog.svelte";

  interface DayPorts {
    dayBlocks: typeof realDayBlocks;
    adHocBlock: typeof realAdHocBlock;
    updateBlock: typeof realUpdateBlock;
    createBlock: typeof realCreateBlock;
    deleteBlock: typeof realDeleteBlock;
    dismissBlock: typeof realDismissBlock;
    /** The block dialog's ticket picker searches with the launcher's reads (#1429). */
    search: SessionPorts["search"];
    launcherHome: SessionPorts["launcherHome"];
    /** What the sources' credentials cannot write (#2052); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort;
  }

  let {
    databaseReady = true,
    day = null,
    timer,
    ontimer,
    onlog,
    revision = 0,
    onchanged,
    job = null,
    ports,
    now = () => new Date(),
    layers = sharedLayers,
    stripWidth,
    measureLabel,
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
    /** The shell's current job, offered on open rows as *Add to <key>* (#1027). */
    job?: { key: string; entityId: string } | null;
    ports?: Partial<DayPorts>;
    now?: () => Date;
    layers?: Layers;
    /** How wide the strip is in pixels: what a caller says, or what the element measures. */
    stripWidth?: () => number;
    /** How wide a strip label draws, `plain` for a label target's sans face; the canvas by default. */
    measureLabel?: (text: string, plain: boolean) => number;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, as `DayReview`'s ports are: a bridge swapped mid-life would read
  // the day through one set and edit it through another.
  const io: DayPorts = {
    dayBlocks: realDayBlocks,
    adHocBlock: realAdHocBlock,
    updateBlock: realUpdateBlock,
    createBlock: realCreateBlock,
    deleteBlock: realDeleteBlock,
    dismissBlock: realDismissBlock,
    search: (query) => realSearch(query),
    launcherHome: () => realLauncherHome(),
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

  let loaded = $state<DayRecord | null>(null);
  let failure = $state<string | null>(null);
  let issued = 0;

  async function load(on: string) {
    const mine = (issued += 1);
    try {
      const answer = await readDay(io, on);
      if (mine !== issued) return;
      loaded = answer;
      failure = null;
      // A delete that has been sent is in this answer now, or refused and back.
      held = held.filter((hold) => !hold.sent);
    } catch (cause) {
      if (mine !== issued) return;
      failure = ipcErrorMessage(cause);
    }
  }

  // The running timer's start is read too: a stop turns the running row into
  // a block, and the day has to read it back.
  const runningSince = $derived(timer.current?.started_at ?? null);

  /**
   * Blocks *Delete block* took and has not written yet (#1429): off the day at
   * once, sent when the toast's *Undo* has had its time. `sent` once the write
   * has landed, so the next read, which no longer has the block, lets it go.
   */
  let held = $state<{ id: number; sent: boolean }[]>([]);
  $effect(() => {
    revision;
    runningSince;
    if (databaseReady) void load(key);
  });

  const ready = $derived(loaded !== null && loaded.day === key ? loaded : null);
  const rows = $derived(
    ready === null
      ? []
      : dayRows({
          day: key,
          blocks: ready.blocks.filter((entry) => !held.some((hold) => hold.id === entry.block.id)),
          offers: ready.offers,
          open: ready.open,
          running: timer.current,
          now: clock,
        }),
  );
  const figures = $derived(dayFigures(rows));
  /** The worklogs the sources' credentials cannot make (#2052), asked after the rows are drawn. */
  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so a re-read that changes no unlogged ticket asks nothing again. */
  const logAsks = $derived(
    JSON.stringify(
      [...new Set(rows.flatMap((row) => (row.state === "unlogged" && row.target?.kind === "entity" ? [row.target.entity_id] : [])))].map(
        (entity) => ({ entity, op: "log_work" }),
      ),
    ),
  );
  $effect(() => askInto(JSON.parse(logAsks) as WriteAsk[], io.refusedWrites, (answer) => (refused = answer)));
  /** Each run of offered rows to one ticket, by its first row's key (#1108). */
  const runAt = $derived(new Map(offerRuns(rows).map((run) => [run.rows[0]!.key, run])));
  /**
   * Past the observation horizon what was in front of you is absent, not zero
   * (`CONTEXT.md`), so the two figures made from it say so.
   */
  const unknown = $derived({
    offered: ready?.pastHorizon ?? false,
    open: ready?.pastHorizon ?? false,
  });
  const frame = $derived(stripWindow(key, rows, key === today ? clock : null));
  const hours = $derived(Array.from({ length: frame.to - frame.from + 1 }, (_, index) => frame.from + index));
  const nowAt = $derived(key === today ? place(key, frame, clock.toISOString(), clock.toISOString()).left : null);
  const noRecord = $derived(ready !== null && ready.pastHorizon && ready.blocks.length === 0);

  // ---- strip labels (#1429) ----

  let stripEl = $state<SVGSVGElement | null>(null);
  let measured = $state(0);
  const stripPx = $derived(stripWidth === undefined ? measured : stripWidth());

  /**
   * Measure the strip as it changes size. A `ResizeObserver` where there is
   * one, which the window has, so a pane opening beside the view counts too.
   * jsdom has none, and a suite that mounts the Day without `stripWidth` (the
   * Time view's, the shell's) falls back on the window's resizes.
   */
  $effect(() => {
    const element = stripEl;
    if (element === null || stripWidth !== undefined) return;
    const measure = () => {
      measured = element.getBoundingClientRect().width;
    };
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  });

  let ink: CanvasRenderingContext2D | null | undefined;
  /**
   * How wide a string draws in the face `time.css` gives a strip label: 600
   * at `--t-2xs`, mono for an entity's key and sans for a label. The face is
   * read once per label, not once per character tried. With no canvas nothing
   * can be measured, and an unmeasured label is not drawn.
   */
  function measurer(plain: boolean): (text: string) => number {
    if (measureLabel !== undefined) return (text) => measureLabel(text, plain);
    if (stripEl === null) return () => Infinity;
    ink ??= document.createElement("canvas").getContext("2d");
    const pen = ink;
    if (pen === null) return () => Infinity;
    const style = getComputedStyle(stripEl);
    const font = `600 ${style.getPropertyValue("--t-2xs").trim()} ${style.getPropertyValue(plain ? "--sans" : "--mono").trim()}`;
    return (text) => {
      pen.font = font;
      return pen.measureText(text).width;
    };
  }

  /** Six pixels of padding at each end of a block, as `x="6"` starts the text. */
  const LABEL_PAD = 12;
  function stripLabel(row: DayRow, widthPct: number): string | null {
    const room = (widthPct / 100) * stripPx - LABEL_PAD;
    if (room <= 0) return null;
    if (row.target === null) return fitLabel("Assign…", room, measurer(true));
    return fitLabel(targetReading(row.target), room, measurer(row.target.kind === "label"));
  }

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
    open: "tf-tm-gap",
  };

  const hourLabel = (hour: number) => String(hour).padStart(2, "0");
  const span = (row: DayRow) =>
    `${clockReading(row.from, key)} to ${row.state === "running" ? "now" : clockReading(row.to, key)}`;
  const durationOf = ({ ms }: { ms: number }) => minutesLabel(Math.floor(ms / 60_000));

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
    await reread();
    return true;
  }

  /** `onchanged` bumps the revision this view re-reads on, so one read, not two. */
  async function reread() {
    if (onchanged) onchanged();
    else await load(key);
  }

  /** How long *Undo* is offered on *Delete block*, and so how long the delete waits: the toast's life. */
  const UNDO_MS = 6500;

  const entity = (entityId: string): TimerTarget => ({ kind: "entity", entity_id: entityId });

  /** *Add to <ticket>* on an offered block, *Log to <ticket>* on a label block. */
  function giveTo(row: DayRow, entityId: string, said: string) {
    const block = row.block;
    if (block === null) return;
    void write(() => io.updateBlock(block.block.id, row.from, row.to, entity(entityId)), said);
  }

  /**
   * *Add <duration> to <ticket>* over a run (#1108): each row's block goes to
   * the ticket over its own stretch, in turn, so the record keeps a block per
   * visit and nothing between them is claimed. A write that fails stops the
   * rest, and the day re-reads either way: the rows already given are given.
   */
  async function takeRun(run: OfferRun) {
    const to = entity(run.entityId);
    const done = await write(async () => {
      for (const row of run.rows) {
        if (row.block !== null) await io.updateBlock(row.block.block.id, row.from, row.to, to);
      }
    }, `Added ${durationOf(run)} to ${keyOf(run.entityId)}`);
    if (!done) await reread();
  }

  /** *Add to <job>* on an open row (#1027): a block on the job over exactly the stretch. */
  function addOpen(row: DayRow, entityId: string, said: string) {
    void write(() => io.createBlock(row.from, row.to, entity(entityId)), said);
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
      const said = `Made a block on ${targetReading(form.target)}, ${span(row)}`;
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

  /** *Dismiss* on an offered block (#900): knobas stops offering that stretch. */
  function dismiss(row: DayRow) {
    const block = row.block;
    if (block === null) return;
    void write(() => io.dismissBlock(block.block.id), "Dismissed. knobas will not offer it again.");
  }

  /**
   * *Delete block* (#1429): the block leaves the day at once and the delete is
   * held for the toast's life, then sent; *Undo* cancels a write that never
   * happened, so the block keeps its id, its context and its worklog mark (the
   * #835 pattern). The timer outlives the view, as the Proposed links view's
   * do, so leaving the day does not lose the delete; closing the window inside
   * those seconds does, and the block is simply there next time.
   */
  function remove() {
    const current = dialog;
    if (current === null || current.mode !== "edit" || current.row.block === null) return;
    const id = current.row.block.block.id;
    dialog = null;
    refusal = null;
    held = [...held, { id, sent: false }];
    const release = () => (held = held.filter((hold) => hold.id !== id));
    const pending = setTimeout(async () => {
      try {
        await io.deleteBlock(id);
        held = held.map((hold) => (hold.id === id ? { ...hold, sent: true } : hold));
      } catch (error) {
        release();
        push({ text: ipcErrorMessage(error), tone: "err" });
      }
      await reread();
    }, UNDO_MS);
    push({
      text: "Deleted the block",
      ms: UNDO_MS,
      action: {
        label: "Undo",
        run: () => {
          clearTimeout(pending);
          release();
        },
      },
    });
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
    <div><b>{minutesLabel(figures.tracked)}</b><span>tracked</span></div>
    <div><b>{minutesLabel(figures.logged)}</b><span>logged</span></div>
    <div class={figures.toLog > 0 ? "tf-warn" : ""}><b>{minutesLabel(figures.toLog)}</b><span>to log</span></div>
    <div><b>{unknown.offered ? "–" : minutesLabel(figures.offered)}</b><span>offered by knobas</span></div>
    <div><b>{unknown.open ? "–" : minutesLabel(figures.open)}</b><span>open, app in front</span></div>
  </div>

  {#if ready.pastHorizon}
    <p class="tf-tm-note">
      <b>No record of what was in front of you.</b> knobas keeps it for about a month, and has cleared this day. Blocks
      you tracked yourself are kept.
    </p>
  {/if}

  <svg class="tf-tm-strip" width="100%" height="84" role="group" aria-label="The day's blocks" bind:this={stripEl}>
    <defs>
      <pattern id="tf-tm-hatch" width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect class="tf-tm-hatch" width="5" height="10" />
      </pattern>
    </defs>
    <rect class="tf-tm-bed" x="0" y="22" width="100%" height="62" rx="6" />
    {#each hours as hour (hour)}
      {@const x = `${Math.round(((hour - frame.from) / (frame.to - frame.from)) * 10_000) / 100}%`}
      <line class="tf-tm-hr" x1={x} x2={x} y1="22" y2="84" />
      <text class="tf-tm-hl" {x} y="14" text-anchor="middle">{hourLabel(hour)}</text>
    {/each}
    {#each rows as row (row.key)}
      {@const at = place(key, frame, row.from, row.to)}
      {@const label = stripLabel(row, at.width)}
      <svg
        class="tf-tm-b {STRIP_CLASS[row.state]} {selected === row.key ? 'tf-on' : ''}"
        x="{at.left}%"
        y="30"
        width="{at.width}%"
        height="46"
        overflow="visible"
        role="button"
        tabindex="0"
        aria-label="{span(row)}, {row.target ? targetReading(row.target) : 'open'}"
        onclick={() => select(row)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            select(row);
          }
        }}
      >
        <title>{span(row)}{row.target ? `, ${targetReading(row.target)}` : ""}</title>
        <rect width="100%" height="100%" rx="4" />
        {#if label !== null}
          <text
            class={row.target === null || row.target.kind === "label" ? "tf-tm-plain" : ""}
            x="6"
            y="50%"
            dominant-baseline="central"
            aria-hidden="true">{label}</text
          >
        {/if}
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
    <span><i class="tf-tm-gap"></i>Open: app in front, no target</span>
    <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "add" })}>+ Add a block</button>
  </div>

  {#if rows.length === 0}
    <div class="tf-tm-empty">
      <p>Nothing tracked on this day. ⌘T starts the timer on whatever is in front of you, and every stretch it runs lands here.</p>
    </div>
  {:else}
    <div class="tf-tm-list">
      {#each rows as row (row.key)}
        {@const run = runAt.get(row.key)}
        {#if run}
          <div class="tf-tm-group">
            <span>
              {run.rows.length} offered rows go to <span class="tf-mono">{keyOf(run.entityId)}</span>,
              {clockReading(run.from, key)} to {clockReading(run.to, key)}.
              <small>Each is its own ticket or linked to it. One action adds them all.</small>
            </span>
            <button class="tf-btn tf-sm" onclick={() => void takeRun(run)}>Add {durationOf(run)} to {keyOf(run.entityId)}</button>
          </div>
        {/if}
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
            {#if row.state === "open"}
              No target, app open
              <small>knobas was open and in front, with no timer and nothing it could name.</small>
            {:else if row.target !== null}
              {#if row.state === "offered"}<span class="tf-dim">knobas saw</span>{/if}
              {#if row.target.kind === "entity"}
                <span class="tf-mono">{targetReading(row.target)}</span>
                {titleOf(row)}
              {:else}
                {row.target.label} <span class="tf-dim">(label)</span>
              {/if}
              {#if row.state === "offered"}
                {@const linked = linkedTo(row)}
                <small>
                  {targetReading(row.target)} was in front of you.
                  {#if linked !== null}It is linked to {keyOf(linked)}.{/if}
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
            {:else}<span class="tf-chip">open</span>{/if}
          </span>
          <span class="tf-tm-acts">
            {#if row.state === "unlogged" && row.target?.kind === "entity"}
              {@const on = row.target.entity_id}
              {#if row.block?.block.ended_by_relaunch}
                <button class="tf-btn tf-sm" onclick={() => extendToNow(row)}>Extend to now</button>
              {/if}
              {@const cannot = refusalOf(refused, on, "log_work")}
              <button class="tf-btn tf-sm" {...refusedAttrs(cannot)} onclick={() => cannot === null && onlog?.(on, key)}>Log…</button>
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
              <button class="tf-btn tf-sm tf-ghost" onclick={() => dismiss(row)}>Dismiss</button>
            {:else if row.state === "open"}
              {#if job !== null}
                {@const to = job.entityId}
                <button
                  class="tf-btn tf-sm"
                  onclick={() => addOpen(row, to, `Made a block on ${job.key}, ${span(row)}`)}
                >
                  Add to {job.key}
                </button>
                <button class="tf-btn tf-sm tf-ghost" onclick={() => open({ mode: "assign", row })}>Assign…</button>
              {:else}
                <button class="tf-btn tf-sm" onclick={() => open({ mode: "assign", row })}>Assign…</button>
              {/if}
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
    title={dialog.mode === "edit" && row ? titleOf(row) : null}
    span={row ? span(row) : null}
    {refusal}
    {layers}
    ports={{ search: io.search, launcherHome: io.launcherHome }}
    onsave={save}
    ondelete={remove}
    onclose={() => (dialog = null)}
  />
{/if}
