<!--
  The Time view's Week tab (#828, R-13.4), ported from the rooms UI's week
  timesheet over the same reads: `week_timesheet` for the rows and
  `pending_writes` for why a worklog is held.

  A row per target and a column per day, Monday first, the weekend shaded;
  each cell its tracked minutes, a logged / held / unlogged bar and what is
  left. `week-view.ts` decides the numbers; this asks the bridge and draws.

  **The bar is SVG**, for the Day strip's reason: `style-src 'self'` refuses
  the inline `flex` the prototype uses, and SVG's `x` and `width` are
  attributes that take percentages.
-->
<script lang="ts">
  import { pageVisible } from "../kept-pages.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { everyWhileShown } from "../while-shown.svelte";
  import { minutesLabel } from "../../time/format";

  import { ipcErrorMessage } from "../../ipc";
  import { entityBriefs as realEntityBriefs } from "../../ipc/entity";
  import { pendingWrites as realPendingWrites } from "../../ipc/sources";
  import { weekTimesheet as realWeekTimesheet, type Week } from "../../ipc/time";
  import { targetReading } from "../../shell/timer";
  import type { Timer } from "../../shell/timer.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { MONTHS, dayKey, shiftDay } from "../../time/day";
  import { columnHeading, mondayOf, weekWindows } from "../../time/week";
  import { entityAddress } from "../address";
  import { goAcross } from "../opener";
  import { namePage } from "../page-name.svelte";
  import { heldNotes, weekView, type WeekCellView } from "./week-view";

  interface WeekPorts {
    weekTimesheet: typeof realWeekTimesheet;
    pendingWrites: typeof realPendingWrites;
    /** Reads the targets' kinds, which the week's rows do not carry: every new one in one read. */
    entityBriefs: typeof realEntityBriefs;
  }

  let {
    databaseReady = true,
    week = null,
    timer,
    revision = 0,
    onqueue = openWriteQueue,
    ports,
    now = () => new Date(),
  }: {
    /** Whether the database answers yet: nothing is read before it does. */
    databaseReady?: boolean;
    /** A day of the week from the address, or `null` for this week. */
    week?: string | null;
    timer: Pick<Timer, "current">;
    /** Bumped by whatever else changed the week: a draft logged, *Log all*, an edit on the Day. */
    revision?: number;
    /** *Open the queue*. */
    onqueue?: () => void;
    ports?: Partial<WeekPorts>;
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, as the Day tab's ports are.
  const io: WeekPorts = { weekTimesheet: realWeekTimesheet, pendingWrites: realPendingWrites, entityBriefs: realEntityBriefs, ...ports };

  /** The clock, moved every thirty seconds so the running timer's minutes move. */
  // svelte-ignore state_referenced_locally
  let clock = $state(now());
  // Paused while keep-alive holds the page hidden, read at once on its return (#2249).
  everyWhileShown(pageVisible(), 30_000, () => (clock = now()));

  const thisMonday = $derived(mondayOf(dayKey(clock)));
  const monday = $derived(mondayOf(week ?? dayKey(clock)));

  let loaded = $state<{ monday: string; week: Week; held: string[] } | null>(null);
  let failure = $state<string | null>(null);
  let issued = 0;

  async function load(from: string) {
    const mine = (issued += 1);
    try {
      const windows = weekWindows(from);
      const [read, writes] = await Promise.all([
        io.weekTimesheet(windows),
        // Why a worklog is held is an extra: the cell still says *held* without it.
        io.pendingWrites().catch(() => []),
      ]);
      if (mine !== issued) return;
      loaded = { monday: from, week: read, held: heldNotes(writes, read.days) };
      failure = null;
    } catch (cause) {
      if (mine !== issued) return;
      failure = ipcErrorMessage(cause);
    }
  }

  // The running timer's start is read too: a stop turns its stretch into a block.
  const runningSince = $derived(timer.current?.started_at ?? null);
  $effect(() => {
    revision;
    runningSince;
    if (databaseReady) void load(monday);
  });

  const ready = $derived(loaded !== null && loaded.monday === monday ? loaded : null);
  const shown = $derived(ready === null ? null : weekView(ready.week, timer.current, clock));

  /**
   * The kind of each entity target, read once per id (#1676): a week row
   * carries an entity id and no kind, and the address is the kind's (a ticket
   * by its entity id, a page at its page). Until the read answers, or when it
   * fails, the key is drawn as plain text rather than as a link that may open
   * the wrong thing. Every id not asked yet goes in one read (the query audit,
   * 2026-11-12: one `get_entity` a row, 8 KB each); an id the mirror does not
   * hold is asked again with the next.
   */
  let kinds = $state<Record<string, string>>({});
  const asked = new Set<string>();
  $effect(() => {
    const ids: string[] = [];
    for (const row of shown?.rows ?? []) {
      if (row.target === null || row.target.kind !== "entity") continue;
      const id = row.target.entity_id;
      if (asked.has(id) || ids.includes(id)) continue;
      ids.push(id);
    }
    if (ids.length === 0) return;
    for (const id of ids) asked.add(id);
    io.entityBriefs(ids).then(
      (briefs) => {
        for (const brief of briefs) kinds[brief.entity_id] = brief.kind;
        for (const id of ids) if (!(id in kinds)) asked.delete(id);
      },
      () => {
        for (const id of ids) asked.delete(id);
      },
    );
  });

  function heading(key: string): string {
    const [year, month, day] = key.split("-").map(Number);
    return `Week of ${day} ${MONTHS[(month ?? 1) - 1]} ${year}`;
  }

  const weekAfter = $derived(shiftDay(monday, 7));
  const after = $derived(weekAfter === thisMonday ? "#/time/week" : `#/time/week/${weekAfter}`);

  // So the day it opens says *Back to Week*, not the section's name.
  namePage(() => "Week");

  /**
   * A day opened from the week is opened to come back from: the two are one page by their address, so this says so,
   * and `Esc` and the bar's *Back to* return here (F-345). ⌘-click and the like stay the browser's.
   */
  function openDay(event: MouseEvent, day: string) {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    goAcross(`#/time/${day}`);
  }

  /** The bar's three shares, as percentages of the cell's whole. */
  function shares(cell: WeekCellView): [number, number, number] {
    const whole = cell.logged + cell.held + cell.toLog;
    if (whole === 0) return [0, 0, 0];
    const pc = (part: number) => Math.round((part / whole) * 10_000) / 100;
    return [pc(cell.logged), pc(cell.held), pc(cell.toLog)];
  }
</script>

<div class="tf-tm-nav">
  <a class="tf-btn tf-sm tf-ghost" href="#/time/week/{shiftDay(monday, -7)}" aria-label="Week before"><svg class="tf-navchev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 3.5 5.5 8l4.5 4.5" /></svg></a>
  <h2>{heading(monday)}</h2>
  {#if monday < thisMonday}
    <a class="tf-btn tf-sm tf-ghost" href={after} aria-label="Week after"><svg class="tf-navchev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" /></svg></a>
    <a class="tf-btn tf-sm" href="#/time/week">This week</a>
  {:else}
    <span class="tf-btn tf-sm tf-ghost tf-hidden" aria-hidden="true"><svg class="tf-navchev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" /></svg></span>
  {/if}
</div>

{#if failure}
  <p class="tf-tm-fail" role="alert">{failure}</p>
{/if}

{#if ready !== null && shown !== null}
  <div class="tf-tm-sum">
    <div><b>{minutesLabel(shown.figures.tracked)}</b><span>tracked</span></div>
    <div><b>{minutesLabel(shown.figures.logged)}</b><span>logged</span></div>
    <div class={shown.figures.toLog > 0 ? "tf-warn" : ""}><b>{minutesLabel(shown.figures.toLog)}</b><span>to log</span></div>
    <div class={shown.figures.held > 0 ? "tf-warn" : ""}><b>{minutesLabel(shown.figures.held)}</b><span>held</span></div>
  </div>

  {#each ready.held as note (note)}
    <div class="tf-tm-held">
      <span class="tf-dot tf-warn"></span>
      <span><TfKeyed text={note} /> <button class="tf-btn tf-sm" onclick={() => onqueue()}>Open the queue</button></span>
    </div>
  {/each}

  <div class="tf-tm-scroll">
    <table class="tf-tbl tf-tm-week">
      <thead>
        <tr>
          <th>Ticket or label</th>
          {#each ready.week.days as day, index (day)}
            <th class={[index > 4 && "tf-tm-we", day === dayKey(clock) && "tf-tm-today"]} aria-current={day === dayKey(clock) ? "date" : undefined}><a href="#/time/{day}" onclick={(event) => openDay(event, day)}>{columnHeading(day, index)}</a></th>
          {/each}
          <th class="tf-tm-tot">Week</th>
        </tr>
      </thead>
      <tbody>
        {#each shown.rows as row, rowIndex (rowIndex)}
          <tr>
            <td class="tf-tm-t">
              {#if row.target === null}
                <span class="tf-dim">No target, app open</span>
                <small>Focused time no block covers. Assign it in the day.</small>
              {:else if row.target.kind === "label"}
                {row.target.label} <span class="tf-dim">(label)</span>
              {:else}
                {@const kind = kinds[row.target.entity_id]}
                {#if kind !== undefined}
                  <a href={entityAddress(kind, row.target.entity_id)}><span class="tf-mono">{targetReading(row.target)}</span></a>
                {:else}
                  <span class="tf-mono">{targetReading(row.target)}</span>
                {/if}
                {row.title ?? ""}
              {/if}
            </td>
            {#each row.cells as cell (cell.day)}
              <td class={["tf-tm-c", cell.weekend && "tf-tm-we", cell.day === dayKey(clock) && "tf-tm-today"]}>
                {#if cell.tracked === 0 && cell.note === null}
                  <span class="tf-dim">–</span>
                {:else}
                  {@const [logged, held, toLog] = shares(cell)}
                  <a href="#/time/{cell.day}" onclick={(event) => openDay(event, cell.day)}>
                    <b>{minutesLabel(cell.tracked)}</b>
                    {#if cell.bar}
                      <svg class="tf-tm-bar" width="72" height="4" aria-hidden="true">
                        <rect class="tf-tm-bar-bed" width="100%" height="100%" />
                        <rect class="tf-tm-bar-lg" width="{logged}%" height="100%" />
                        <rect class="tf-tm-bar-hd" x="{logged}%" width="{held}%" height="100%" />
                        <rect class="tf-tm-bar-un" x="{logged + held}%" width="{toLog}%" height="100%" />
                        {#if held > 0 && toLog > 0}<rect class="tf-tm-bar-seam" x="{logged + held}%" width="1" height="100%" />{/if}
                      </svg>
                    {/if}
                    {#if cell.note !== null}<small>{cell.note}</small>{/if}
                  </a>
                {/if}
              </td>
            {/each}
            <td class="tf-tm-c tf-tm-tot"><b>{minutesLabel(row.total)}</b></td>
          </tr>
        {/each}
        <tr class="tf-tm-total">
          <td>Total</td>
          {#each shown.dayTotals as total, index (index)}
            <td class={["tf-tm-c", index > 4 && "tf-tm-we", ready.week.days[index] === dayKey(clock) && "tf-tm-today"]}><b>{total > 0 ? minutesLabel(total) : "–"}</b></td>
          {/each}
          <td class="tf-tm-c tf-tm-tot"><b>{minutesLabel(shown.figures.tracked)}</b></td>
        </tr>
      </tbody>
    </table>
  </div>

  <p class="tf-tm-foot">
    Minutes, not rounded. Offered time stays in the day until you add it to a ticket, so it is not counted here. A
    worklog waiting in the queue counts as logged.
  </p>
{/if}
