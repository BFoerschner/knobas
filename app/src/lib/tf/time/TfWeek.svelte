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
  import { everyWhileShown } from "../while-shown.svelte";
  import { minutesLabel } from "../../time/format";

  import { ipcErrorMessage } from "../../ipc";
  import { getEntity as realGetEntity } from "../../ipc/entity";
  import { pendingWrites as realPendingWrites } from "../../ipc/sources";
  import { weekTimesheet as realWeekTimesheet, type Week } from "../../ipc/time";
  import { targetReading } from "../../shell/timer";
  import type { Timer } from "../../shell/timer.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { MONTHS, dayKey, shiftDay } from "../../time/day";
  import { columnHeading, mondayOf, weekWindows } from "../../time/week";
  import { entityAddress } from "../address";
  import { heldNotes, weekView, type WeekCellView } from "./week-view";

  interface WeekPorts {
    weekTimesheet: typeof realWeekTimesheet;
    pendingWrites: typeof realPendingWrites;
    /** Reads a target's kind, which the week's rows do not carry. */
    getEntity: typeof realGetEntity;
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
  const io: WeekPorts = { weekTimesheet: realWeekTimesheet, pendingWrites: realPendingWrites, getEntity: realGetEntity, ...ports };

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
   * the wrong thing.
   */
  let kinds = $state<Record<string, string>>({});
  const asked = new Set<string>();
  $effect(() => {
    for (const row of shown?.rows ?? []) {
      if (row.target === null || row.target.kind !== "entity") continue;
      const id = row.target.entity_id;
      if (asked.has(id)) continue;
      asked.add(id);
      io.getEntity(id).then(
        (detail) => (kinds[id] = detail.row.kind),
        () => asked.delete(id),
      );
    }
  });

  function heading(key: string): string {
    const [year, month, day] = key.split("-").map(Number);
    return `Week of ${day} ${MONTHS[(month ?? 1) - 1]} ${year}`;
  }

  const weekAfter = $derived(shiftDay(monday, 7));
  const after = $derived(weekAfter === thisMonday ? "#/time/week" : `#/time/week/${weekAfter}`);

  /** The bar's three shares, as percentages of the cell's whole. */
  function shares(cell: WeekCellView): [number, number, number] {
    const whole = cell.logged + cell.held + cell.toLog;
    if (whole === 0) return [0, 0, 0];
    const pc = (part: number) => Math.round((part / whole) * 10_000) / 100;
    return [pc(cell.logged), pc(cell.held), pc(cell.toLog)];
  }
</script>

<div class="tf-tm-nav">
  <a class="tf-btn tf-sm tf-ghost" href="#/time/week/{shiftDay(monday, -7)}" aria-label="Week before">‹</a>
  <h2>{heading(monday)}</h2>
  {#if monday < thisMonday}
    <a class="tf-btn tf-sm tf-ghost" href={after} aria-label="Week after">›</a>
    <a class="tf-btn tf-sm" href="#/time/week">This week</a>
  {:else}
    <span class="tf-btn tf-sm tf-ghost tf-hidden" aria-hidden="true">›</span>
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
      <span>{note} <button class="tf-btn tf-sm" onclick={() => onqueue()}>Open the queue</button></span>
    </div>
  {/each}

  <div class="tf-tm-scroll">
    <table class="tf-tbl tf-tm-week">
      <thead>
        <tr>
          <th>On</th>
          {#each ready.week.days as day, index (day)}
            <th class={index > 4 ? "tf-tm-we" : ""}><a href="#/time/{day}">{columnHeading(day, index)}</a></th>
          {/each}
          <th>Week</th>
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
              <td class="tf-tm-c {cell.weekend ? 'tf-tm-we' : ''}">
                {#if cell.tracked === 0 && cell.note === null}
                  <span class="tf-dim">–</span>
                {:else}
                  {@const [logged, held, toLog] = shares(cell)}
                  <a href="#/time/{cell.day}">
                    <b>{minutesLabel(cell.tracked)}</b>
                    {#if cell.bar}
                      <svg class="tf-tm-bar" width="72" height="4" aria-hidden="true">
                        <rect class="tf-tm-bar-bed" width="100%" height="100%" />
                        <rect class="tf-tm-bar-lg" width="{logged}%" height="100%" />
                        <rect class="tf-tm-bar-hd" x="{logged}%" width="{held}%" height="100%" />
                        <rect class="tf-tm-bar-un" x="{logged + held}%" width="{toLog}%" height="100%" />
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
          <td>Every day</td>
          {#each shown.dayTotals as total, index (index)}
            <td class="tf-tm-c {index > 4 ? 'tf-tm-we' : ''}"><b>{total > 0 ? minutesLabel(total) : "–"}</b></td>
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
