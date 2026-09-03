<!--
  The week timesheet — Monday to Sunday under the day strip (issue #283, spec
  #272 stories 40–44).

  **One screen, two readings.** `DayReview` above is one day as a list of
  things a person can fix; this is the same time added up, so that assigning a
  block and watching the week's unlogged total change is one glance (story 44).
  They share the address `#/time/<YYYY-MM-DD>`: the strip's date is what drives
  the highlighted column here, so moving days moves the highlight rather than
  re-reading a different week.

  **Every number is in words, and every one of them is minutes.** The day
  review records why widths are not proportional — `style-src 'self'` forbids
  an inline width — and the same rule holds a timesheet to the reading a person
  checks against their Jira anyway. Minutes with no rounding is `CONTEXT.md`'s
  timesheet: `week.ts`'s `minutesOf` floors, because the only safe direction to
  be wrong in is the one that never claims time nobody worked.

  **A weekend collapses only when it is empty**, and a weekday never does. An
  empty Wednesday is the gap somebody opened this view to find; an empty
  Saturday is the ordinary case. `visibleColumns` carries the whole rule.

  **The "no target, app open" row is last and has no actions.** It is focused
  time no block covers, so the week's total is honest (story 41) — and it has
  no target, so there is nothing *Log all* could do with it.

  **_Log all_ shows what it will send before it sends it.** Two calls, and the
  gap between them is the point: `logAllPreview` lists one line per day and
  ticket, and nothing reaches the write queue until the reader has read that
  list and pressed the second button. A bulk write into a ticketing system
  other people read is not something to discover afterwards.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    logAll as realLogAll,
    logAllPreview as realLogAllPreview,
    weekTimesheet as realWeekTimesheet,
    type PlannedWorklog,
    type Week,
    type WeekRow,
  } from "../ipc/time";
  import { latestRead } from "../shell/latest-read";
  import { targetReading } from "../shell/timer";
  import { dayKey, durationReading, horizonNote } from "./day";
  import {
    cellIsEmpty,
    columnHeading,
    daysPastHorizon,
    minutesOf,
    mondayOf,
    visibleColumns,
    weekLabel,
    weekWindows,
  } from "./week";

  /** The bridge this view needs, injectable so a test needs no Tauri. */
  interface WeekPorts {
    weekTimesheet: typeof realWeekTimesheet;
    logAllPreview: typeof realLogAllPreview;
    logAll: typeof realLogAll;
  }

  let {
    day = null,
    revision = 0,
    onchanged,
    ports,
    now = () => new Date(),
  }: {
    /** The day from the address, or `null` for today — `DayReview`'s prop. */
    day?: string | null;
    /**
     * Bumped by whatever else on this screen changed the week (#283, story
     * 44: "assigning a block and watching the week's unlogged total change is
     * one glance").
     *
     * A number rather than an event, because the shell owns the screen: the
     * day strip and this table are siblings, `update_block` deliberately
     * writes no activity line (`commands::time`'s `update_block` records why),
     * and a component reaching for its sibling's reads would be the coupling
     * the port objects exist to avoid. The parent bumps one counter and both
     * views re-read.
     */
    revision?: number;
    /** Called after this view writes, so the strip above re-reads too. */
    onchanged?: (() => void) | undefined;
    ports?: Partial<WeekPorts>;
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once at init, the decision `DayReview` records: production omits this
  // prop, and a bridge swapped mid-life would leave a week on screen that was
  // read through one set of ports and logged through another.
  const io: WeekPorts = {
    weekTimesheet: realWeekTimesheet,
    logAllPreview: realLogAllPreview,
    logAll: realLogAll,
    ...ports,
  };

  /** The day the strip is on — what the highlighted column is. */
  const key = $derived(day ?? dayKey(now()));
  /** The Monday of that day's week — what is read. */
  const monday = $derived(mondayOf(key));

  let week = $state<Week | null>(null);
  let failure = $state<string | null>(null);
  /**
   * What *Log all* is about to send, or `null` while nothing is being
   * confirmed.
   *
   * An empty array is **not** `null`: it is the answer "there is nothing to
   * log", which is a sentence the reader has to see rather than a button that
   * appears to do nothing.
   */
  let planned = $state<PlannedWorklog[] | null>(null);
  /** A write is in flight; both buttons are held until it lands. */
  let sending = $state(false);

  // `key` is passed so the column the strip is standing on is drawn even when
  // it is an empty weekend day — otherwise the reader steps onto a quiet
  // Saturday and the week loses the one column they are on.
  const columns = $derived(week === null ? [] : visibleColumns(week, key));
  /** The highlighted column, or -1 when the strip's day is in another week. */
  const highlighted = $derived(week === null ? -1 : week.days.indexOf(key));
  /**
   * The headings of this week's days past the observation horizon
   * (#337).
   *
   * Every one of them, not only the drawn columns: a collapsed weekend day is
   * still a day whose observations retention took, and `daysPastHorizon`
   * carries the reasoning.
   */
  const absent = $derived(week === null ? [] : daysPastHorizon(week));

  /** Unlogged seconds over the rows a predicate keeps. */
  function unloggedOver(keep: (row: WeekRow) => boolean): number {
    if (week === null) return 0;
    return week.rows
      .filter(keep)
      .reduce((total, row) => total + row.cells.reduce((sum, c) => sum + c.unlogged_seconds, 0), 0);
  }

  /**
   * The week's unlogged time, and how much of it has nowhere to go.
   *
   * Two numbers rather than one, and they sit beside the *Log all…* button:
   * the total is what story 41 calls an honest week, but *Log all* can only
   * touch the part with a target on it, and one number next to that button
   * would read as a promise about all of it.
   */
  const unlogged = $derived(unloggedOver(() => true));
  const unloggedWithNoTarget = $derived(unloggedOver((row) => row.target === null));

  /**
   * Read the week whenever the address names another one.
   *
   * Through `latestRead` for the reason `DayReview` uses it: clicking through
   * days puts several reads in flight with nothing sequencing them, and the
   * hand-written guard drops the rejection path in one predictable way (#107).
   */
  const readWeek = latestRead<Week>();
  $effect(() => {
    // `revision` is read for its own sake: an edit on the strip above changes
    // this week without changing its address, and story 44 asks for the
    // change to be one glance rather than a reload.
    revision;
    void load(monday);
  });

  async function load(from: string) {
    await readWeek(() => io.weekTimesheet(weekWindows(from)), {
      ok: (read) => {
        week = read;
        failure = null;
      },
      fail: (cause) => {
        failure = ipcErrorMessage(cause);
      },
    });
  }

  /** Ask what would be sent. Nothing is queued by this. */
  async function preview() {
    failure = null;
    try {
      planned = await io.logAllPreview(weekWindows(monday));
    } catch (error) {
      failure = ipcErrorMessage(error);
    }
  }

  /**
   * Send it, then re-read: every cell in the week has just changed — and so
   * has the strip above, whose blocks have just become read-only, which is
   * what `onchanged` is for.
   *
   * **`onchanged` fires on the refusal path too, and `DayReview`'s does not.**
   * The difference is not an oversight, it is what `log_all` is: a day that
   * fails does not roll back the days that succeeded (ADR-0012 — the copies
   * are knobas' record of writes that may already have landed), so a rejected
   * *Log all* has still written, usually most of a week of it. A success-only
   * bump here would leave the strip above offering *Edit* on blocks that four
   * of five days' worklogs have just made read-only. An edit is the other
   * case: it is one statement, and a refused one wrote nothing.
   */
  async function send() {
    sending = true;
    try {
      await io.logAll(weekWindows(monday));
      planned = null;
    } catch (error) {
      failure = ipcErrorMessage(error);
    } finally {
      sending = false;
    }
    await load(monday);
    onchanged?.();
  }

  /** What a row is called: the mirror's title, else the id, else the label. */
  function nameOf(row: WeekRow): string {
    if (row.target === null) return "No target, app open";
    return row.title ?? targetReading(row.target);
  }

  /** The reading for a number of seconds, or `null` when there is none. */
  function reading(seconds: number): string | null {
    return seconds === 0 ? null : durationReading(minutesOf(seconds));
  }
</script>

<section class="week" aria-label="The week">
  <div class="head">
    <h2>{week === null ? "" : weekLabel(week.days)}</h2>
    <span class="k">
      {durationReading(minutesOf(unlogged))} unlogged this week{#if unloggedWithNoTarget > 0},
        <!--
          Named separately because *Log all* cannot touch it: it has no target.
          One number beside that button would read as a promise about all of
          it.
        -->{durationReading(minutesOf(unloggedWithNoTarget))} of it with no target{/if}
    </span>
    <button class="btn sm" onclick={() => void preview()} disabled={sending}>Log all…</button>
  </div>

  {#if failure}
    <p class="empty fail">{failure}</p>
  {/if}

  {#if absent.length > 0}
    <!--
      Outside both branches below, deliberately. A week entirely past the
      horizon has no rows at all — `week::read` drops the "no target, app open"
      row when it has nothing to say — so it falls into the "nothing tracked
      this week" arm, which is the one reading that is false about it. This is
      drawn above that line in both arms, in the day review's own words.
    -->
    <p class="horizon">{horizonNote(absent.join(", "))}</p>
  {/if}

  {#if planned !== null}
    <!--
      The confirmation, and it is a *list* rather than a count: "4 worklogs"
      is not something a person can check, and the thing they need to check is
      which days and which tickets. Story 43's whole point is that a week logs
      per day rather than as one lump, and this is where a reader sees that it
      has.
    -->
    <div class="confirm" role="alertdialog" aria-label="What Log all will send">
      {#if planned.length === 0}
        <p class="empty">
          Nothing to log this week — every tracked block is either already on a worklog, a
          passive block knobas is only offering, or an ad-hoc label with no ticket to go to.
        </p>
        <button class="btn sm" onclick={() => (planned = null)}>Close</button>
      {:else}
        <p>
          This will send {planned.length} worklog{planned.length === 1 ? "" : "s"} — one per day
          and ticket, with no comment:
        </p>
        <ul class="plan">
          {#each planned as entry (`${entry.day}-${entry.entity_id}`)}
            <li>
              <span class="when">{entry.day}</span>
              <span class="txt">{entry.title ?? entry.entity_id}</span>
              <span class="ctxk">{entry.entity_id}</span>
              <span class="amt">
                {durationReading(minutesOf(entry.seconds))}
                {#if entry.blocks > 1}<i>({entry.blocks} blocks)</i>{/if}
              </span>
            </li>
          {/each}
        </ul>
        <div class="acts">
          <button class="btn sm pri" onclick={() => void send()} disabled={sending}>
            {sending ? "Sending…" : `Send ${planned.length}`}
          </button>
          <button class="btn sm ghost" onclick={() => (planned = null)} disabled={sending}>
            Cancel
          </button>
        </div>
      {/if}
    </div>
  {/if}

  {#if week !== null && week.rows.length === 0 && !failure}
    <p class="empty">
      Nothing tracked this week. Every stretch the timer runs lands on the day it started, and
      the week adds them up here.
    </p>
  {:else if week !== null}
    <table class="sheet">
      <thead>
        <tr>
          <th scope="col" class="who">Target</th>
          {#each columns as index (index)}
            <th scope="col" class={index === highlighted ? "on" : ""}>
              {columnHeading(week.days[index] ?? "", index)}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each week.rows as row (row.target === null ? "open" : JSON.stringify(row.target))}
          <tr class={row.target === null ? "open" : ""}>
            <th scope="row" class="who">
              {nameOf(row)}
              {#if row.target?.kind === "entity"}
                <span class="ctxk">{row.target.entity_id}</span>
              {/if}
              {#if row.target === null}
                <!--
                  Said in words: this row is not a target and nothing here can
                  be logged. A reader who mistook it for one would be looking
                  for a *Log* button that must not exist.
                -->
                <i>focused time no block covers</i>
              {/if}
            </th>
            {#each columns as index (index)}
              {@const cell = row.cells[index]}
              <td class={index === highlighted ? "on" : ""}>
                {#if cellIsEmpty(cell)}
                  <span class="none">—</span>
                {:else if cell !== undefined}
                  {#if reading(cell.tracked_seconds)}
                    <span class="tracked">{reading(cell.tracked_seconds)}</span>
                  {/if}
                  {#if reading(cell.offered_seconds)}
                    <!--
                      knobas' own guess, in the day strip's own word and never
                      inside the tracked total — the arrangement `DayReview`'s
                      heading already uses on the same screen.
                    -->
                    <span class="offered">{reading(cell.offered_seconds)} offered</span>
                  {/if}
                  {#if reading(cell.logged_seconds)}
                    <span class="logged">{reading(cell.logged_seconds)} logged</span>
                  {/if}
                  {#if reading(cell.held_seconds)}
                    <!--
                      Held, not logged — spec story 39. The blocks under it are
                      already spoken for, so it is neither of its neighbours and
                      says its own word.
                    -->
                    <span class="held">{reading(cell.held_seconds)} held</span>
                  {/if}
                  {#if reading(cell.unlogged_seconds)}
                    <span class="unlogged">{reading(cell.unlogged_seconds)} unlogged</span>
                  {/if}
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .week {
    margin-top: 18px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .head h2 {
    margin: 0;
    font: 600 13px var(--sans);
  }

  /*
    The day review's `.horizon`, in the same words and the same key: a fact
    about the week rather than something that went wrong, and nothing for the
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

  .confirm {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--hair2);
    border-radius: 2px;
    background: var(--raised);
  }

  .confirm p {
    margin: 0 0 6px;
  }

  .plan {
    display: grid;
    gap: 2px;
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }

  .plan li {
    display: grid;
    grid-template-columns: 96px 1fr auto auto;
    align-items: center;
    column-gap: 10px;
  }

  .acts {
    display: flex;
    gap: 6px;
  }

  .sheet {
    width: 100%;
    border-collapse: collapse;
  }

  .sheet th,
  .sheet td {
    padding: 4px 8px;
    border: 1px solid var(--hair2);
    text-align: left;
    vertical-align: top;
    font-weight: 400;
  }

  .sheet thead th {
    font: 600 11px var(--mono);
    color: var(--faint);
  }

  /* The strip's day, carried through the column it is. */
  .sheet .on {
    background: var(--raised);
  }

  .who {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .amt,
  .when {
    font: 500 11px var(--mono);
    color: var(--faint);
  }

  .amt i {
    font-style: normal;
    color: var(--muted);
  }

  .ctxk {
    display: block;
    font: 500 11px var(--mono);
    color: var(--faint);
  }

  .who i {
    display: block;
    font-style: normal;
    color: var(--faint);
  }

  .tracked,
  .offered,
  .logged,
  .held,
  .unlogged,
  .none {
    display: block;
    font: 500 11px var(--mono);
  }

  .tracked {
    color: var(--text);
  }

  .logged {
    color: var(--faint);
  }

  .offered,
  .held {
    color: var(--amber);
  }

  .unlogged {
    color: var(--muted);
  }

  .none {
    color: var(--faint);
  }

  .open th,
  .open td {
    color: var(--faint);
  }
</style>
