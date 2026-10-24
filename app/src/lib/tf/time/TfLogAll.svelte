<!--
  *Log all…* (#828, R-13.4, R-13.5): one worklog per day and ticket, ticked,
  each with a comment to edit, before anything is sent.

  Over existing commands only: `log_all_preview` lists the rows,
  `worklog_draft` gives each row the comment the draft would (the same
  bullets, from what knobas saw that day), and **`log_work` sends each ticked
  row** with its comment: one `log_work` per ticked row, so the dialog can
  honour both the ticks and the edited comments.

  **The week is asked again at send time.** `log_work` covers every unlogged
  block of the day on the ticket and logs the seconds it is given, so a timer
  stopped while the dialog was up would be covered at the old number. The
  fresh preview's start and seconds are what go; a row it no longer offers
  (logged from elsewhere meanwhile) is skipped.

  The comment is a two-line text area, not the prototype's one-line input: a
  draft's comment is one bullet per line, and an input drops the line breaks.

  A refused worklog does not stop the rest: each worklog is its own write,
  and the copies of the ones that went stay (ADR-0012).

  A row whose source's credential cannot log work (`refused_writes`, #2052)
  is drawn unticked, its box disabled, with the reason; it is never sent.
-->
<script lang="ts">
  import { minutesLabel } from "../../time/format";
  import { onMount } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { listSources as realListSources, type SourceSummary } from "../../ipc/sources";
  import {
    logAllPreview as realLogAllPreview,
    logWork as realLogWork,
    worklogDraft as realWorklogDraft,
    type DayWindow,
    type PlannedWorklog,
    type Worklog,
  } from "../../ipc/time";
  import { askRefused, refusalOf, type RefusedWrite, type RefusedWritesPort } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { targetReading } from "../../shell/timer";
  import type { Timer } from "../../shell/timer.svelte";
  import { readerDay } from "../../time/day";
  import { columnHeading, minutesOf } from "../../time/week";
  import type { Layers } from "../layers.svelte";
  
  interface LogAllPorts {
    logAllPreview: typeof realLogAllPreview;
    worklogDraft: typeof realWorklogDraft;
    logWork: typeof realLogWork;
    listSources: () => Promise<Pick<SourceSummary, "id" | "display_name">[]>;
    /** What the sources' credentials cannot write (#2052); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort;
  }

  let {
    days,
    timer,
    layers,
    onclose,
    ondone,
    ports,
  }: {
    /** The week's windows, Monday first. */
    days: DayWindow[];
    timer: Pick<Timer, "current">;
    layers: Layers;
    onclose: () => void;
    /** After a send: the copies made, and the first refusal or `null`. */
    ondone: (made: Worklog[], failure: string | null) => void;
    ports?: Partial<LogAllPorts>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io: LogAllPorts = {
    logAllPreview: realLogAllPreview,
    worklogDraft: realWorklogDraft,
    logWork: realLogWork,
    listSources: realListSources,
    ...ports,
  };

  interface Row {
    plan: PlannedWorklog;
    ticked: boolean;
    comment: string;
  }
  let rows = $state<Row[] | null>(null);
  let failure = $state<string | null>(null);
  let sending = $state(false);
  let names = $state<Record<string, string>>({});
  /** The worklogs the sources' credentials cannot make (#2052). */
  let refused = $state<RefusedWrite[]>([]);
  const cannot = (plan: PlannedWorklog) => refusalOf(refused, plan.entity_id, "log_work");

  const uid = `tf-logall-${Math.random().toString(36).slice(2, 9)}`;
  const id = (plan: Pick<PlannedWorklog, "day" | "entity_id">) => `${plan.day} ${plan.entity_id}`;


  onMount(() => {
    void (async () => {
      try {
        const plan = await io.logAllPreview(days);
        // The draft's comment per row; a draft that does not answer leaves it empty.
        const comments = await Promise.all(
          plan.map((entry) =>
            io.worklogDraft(entry.entity_id, readerDay(entry.day)).then(
              (draft) => draft?.comment ?? "",
              () => "",
            ),
          ),
        );
        rows = plan.map((entry, index) => ({ plan: entry, ticked: true, comment: comments[index] ?? "" }));
        const tickets = [...new Set(plan.map((entry) => entry.entity_id))];
        refused = await askRefused(tickets.map((entity) => ({ entity, op: "log_work" })), io.refusedWrites);
        for (const row of rows) if (cannot(row.plan) !== null) row.ticked = false;
      } catch (error) {
        failure = ipcErrorMessage(error);
        rows = [];
      }
    })();
    io.listSources().then(
      (sources) => (names = Object.fromEntries(sources.map((source) => [source.id, source.display_name]))),
      () => {},
    );
    return layers.add({ kind: "dialog", close: onclose });
  });

  const ticked = $derived(rows?.filter((row) => row.ticked && cannot(row.plan) === null) ?? []);
  const running = $derived(timer.current);

  const dayHeading = (day: string) => {
    const index = days.findIndex((window) => window.day === day);
    return index < 0 ? day : columnHeading(day, index);
  };

  /** `" for Jira"` when every worklog goes to one source knobas can name. */
  function goesTo(made: Worklog[]): string {
    const sources = new Set(made.map((worklog) => worklog.entity_id.slice(0, worklog.entity_id.indexOf(":"))));
    const [only] = sources;
    return sources.size === 1 && only !== undefined && names[only] ? ` for ${names[only]}` : "";
  }

  async function send() {
    if (sending || ticked.length === 0) return;
    sending = true;
    failure = null;
    const made: Worklog[] = [];
    let refused: string | null = null;
    try {
      const fresh = new Map((await io.logAllPreview(days)).map((plan) => [id(plan), plan]));
      for (const row of ticked) {
        const now = fresh.get(id(row.plan));
        if (now === undefined) continue;
        // Whole minutes, floored: the preview's seconds may carry a remainder.
        const seconds = minutesOf(now.seconds) * 60;
        if (seconds === 0) continue;
        try {
          made.push(
            await io.logWork(now.entity_id, readerDay(now.day), {
              startedAt: now.started_at,
              seconds,
              comment: row.comment,
            }),
          );
        } catch (error) {
          refused ??= ipcErrorMessage(error);
        }
      }
    } catch (error) {
      refused = ipcErrorMessage(error);
    } finally {
      sending = false;
    }
    if (made.length > 0) {
      push({ text: `Queued ${made.length} worklog${made.length === 1 ? "" : "s"}${goesTo(made)}` });
    }
    if (refused !== null) push({ text: refused, tone: "err" });
    ondone(made, refused);
  }
</script>

<div class="tf-modal" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <div class="tf-dialog tf-tm-logall" role="dialog" aria-modal="true" aria-labelledby="{uid}-title">
    <header>
      <h2 id="{uid}-title">Log all</h2>
      <p class="tf-dsub">One worklog per day and ticket, from the blocks you tracked and have not logged.</p>
    </header>
    <div class="tf-body">
      {#if failure !== null}
        <p class="tf-fail" role="alert">{failure}</p>
      {/if}
      {#if rows === null}
        <p class="tf-dim">Reading the week…</p>
      {:else if rows.length === 0}
        {#if failure === null}
          <p>
            Nothing to log this week. Every tracked block is logged, offered by knobas, or on a label with no ticket to go
            to.
          </p>
        {/if}
      {:else}
        <table class="tf-tbl">
          <thead>
            <tr><th></th><th>Day</th><th>Ticket</th><th>Time</th><th>Comment</th></tr>
          </thead>
          <tbody>
            {#each rows as row (id(row.plan))}
              {@const ticket = targetReading({ kind: "entity", entity_id: row.plan.entity_id })}
              {@const reason = cannot(row.plan)}
              <tr>
                <td
                  ><input
                    type="checkbox"
                    aria-label="Log this"
                    disabled={reason !== null}
                    title={reason ?? undefined}
                    bind:checked={row.ticked}
                  /></td
                >
                <td class="tf-nowrap">{dayHeading(row.plan.day)}</td>
                <td class="tf-nowrap"
                  ><span class="tf-mono">{ticket}</span>{#if reason !== null}<br /><small class="tf-dim">{reason}</small>{/if}</td
                >
                <td class="tf-nowrap tf-tm-num">{minutesLabel(minutesOf(row.plan.seconds))}</td>
                <td class="tf-tm-comment">
                  <textarea
                    rows="2"
                    aria-label="Comment for {ticket}, {dayHeading(row.plan.day)}"
                    bind:value={row.comment}
                  ></textarea>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if running !== null}
        <p class="tf-dim tf-note">
          {running.target.kind === "entity" ? `The running timer on ${targetReading(running.target)}` : "The running timer"}
          is not in it until you stop it.
        </p>
      {/if}
      <p class="tf-dim tf-note">Time on "No target, app open" is not offered. Assign it to a ticket in the day first.</p>
    </div>
    <footer>
      <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
      {#if rows !== null && rows.length > 0}
        <button type="button" class="tf-btn tf-primary" disabled={sending || ticked.length === 0} onclick={() => void send()}>
          Log {ticked.length} worklog{ticked.length === 1 ? "" : "s"}
        </button>
      {/if}
    </footer>
  </div>
</div>
