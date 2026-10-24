<!--
  The worklog draft in the ticket-first shell (#826, R-13.1 to R-13.3), drawn
  as prototype-6's `openWorklog` dialog over the same commands as the rooms
  UI's `WorklogDraft.svelte`: `worklog_draft` opened it, `log_work` queues it,
  and `DraftForm` holds the ticks, the comment and what is sent.

  It adds two reads the prototype draws and the draft does not carry:
  `day_blocks` for the intervals and the ticket's title (the draft names its
  blocks by id), and `list_sources` for the source names ("Goes to Jira",
  "10:02, Gitea").

  **Which buttons depends on what opened it.** After a stop: *Stop, log
  later*, *Keep running*, *Discard* and *Log*. Without one (the ad-hoc
  dialog's *Log to a ticket…*): *Discard* and *Log*, with nothing about a
  clock. A stop's draft opens over the running clock (#896); what each button
  does to it is `drafting.svelte.ts`'s, and this answers with an outcome,
  except that *Log* awaits `onbeforelog` (the clock stopping) before it sends.
  `Esc` and the backdrop are the quiet answer: a stop's draft stops the clock
  (*Stop, log later*), any other is discarded.

  ADR-0012's sentence is here as it is in the rooms UI's draft, because this
  is where a person reaches for the one write whose duplicate costs money.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { listSources as realListSources, type SourceSummary } from "../ipc/sources";
  import {
    dayBlocks as realDayBlocks,
    logWork as realLogWork,
    type Candidate,
    type DayBlock,
    type Draft,
    type Worklog,
  } from "../ipc/time";
  import { clockReading, dayBounds, dayKey, dayLabel, readerDay } from "../time/day";
  import { DraftForm } from "../time/draft-form.svelte";
  import type { DraftOutcome } from "./drafting.svelte";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";

  export interface TfWorklogDraftPorts {
    logWork: typeof realLogWork;
    dayBlocks: (from: string, to: string) => Promise<{ blocks: DayBlock[] }>;
    listSources: () => Promise<Pick<SourceSummary, "id" | "display_name">[]>;
  }

  let {
    draft,
    stopped,
    onoutcome,
    onbeforelog = () => Promise.resolve(),
    onlogged,
    layers = sharedLayers,
    ports = { logWork: realLogWork, dayBlocks: realDayBlocks, listSources: realListSources },
  }: {
    draft: Draft;
    /**
     * Whether a stop opened it, which decides the buttons. The clock still
     * runs under it until the answer stops it (#896).
     */
    stopped: boolean;
    onoutcome: (outcome: DraftOutcome) => void;
    /**
     * *Log*'s first step, awaited before `log_work`: the shell stops the clock
     * under the draft, so its stretch is a block the log can cover (#896).
     */
    onbeforelog?: () => Promise<void>;
    /** The copy that now exists; the toast is the shell's. */
    onlogged: (worklog: Worklog) => void;
    layers?: Layers;
    ports?: TfWorklogDraftPorts;
  } = $props();

  const uid = `tf-worklog-${Math.random().toString(36).slice(2, 9)}`;

  // svelte-ignore state_referenced_locally
  // Read once: a draft swapped mid-life would drop the reader's ticks.
  const form = new DraftForm(draft);

  let sending = $state(false);
  let failed = $state<string | null>(null);

  /** The draft's own blocks, oldest first, once `day_blocks` answers. */
  let blocks = $state<DayBlock[] | null>(null);
  /** Source id → display name, once `list_sources` answers. */
  let names = $state<Record<string, string>>({});

  const title = $derived(blocks?.find((row) => row.title !== null)?.title ?? null);

  const namespace = (id: string) => id.slice(0, id.indexOf(":"));
  const sourceName = (id: string) => names[namespace(id)] ?? namespace(id);

  /**
   * `"from 2 blocks today: 09:40 to 11:50, and 13:58 to now"`: a running
   * clock's stretch is one of the blocks, and it runs to now (#896).
   */
  const intervals = $derived.by(() => {
    const count = draft.block_ids.length + (draft.running_since === null ? 0 : 1);
    const when = draft.day === dayKey(new Date()) ? "today" : `on ${dayLabel(draft.day)}`;
    const head = `from ${count} ${count === 1 ? "block" : "blocks"} ${when}`;
    if (blocks === null) return head;
    const spans = blocks.map(
      ({ block }) => `${clockReading(block.started_at, draft.day)} to ${clockReading(block.ended_at, draft.day)}`,
    );
    if (draft.running_since !== null) spans.push(`${clockReading(draft.running_since, draft.day)} to now`);
    if (spans.length === 0) return head;
    const list = spans.length === 1 ? spans[0] : `${spans.slice(0, -1).join(", ")}, and ${spans.at(-1)}`;
    return `${head}: ${list}`;
  });

  /** Where a candidate was seen, as the prototype says it. */
  function seenIn(candidate: Candidate): string {
    if (candidate.source === "mirror" && candidate.entity_id !== null) return sourceName(candidate.entity_id);
    if (candidate.source === "note") return "note";
    return "you wrote";
  }

  /** Leaving without logging: a stop's draft keeps the time, any other is dropped. */
  function leave() {
    // A log in flight answers for itself; leaving now would say "unlogged".
    if (sending) return;
    onoutcome(stopped ? "later" : "discard");
  }

  function send() {
    if (sending) return;
    sending = true;
    failed = null;
    // The clock first: `log_work` covers only closed blocks.
    void onbeforelog()
      .then(() => ports.logWork(draft.entity_id, readerDay(draft.day), form.work))
      .then(
        (worklog) => {
          // Outside the rejection handler on purpose: the worklog exists now,
          // and a throw in the shell's toast must not read as a failed send.
          onoutcome("logged");
          onlogged(worklog);
        },
        (error: unknown) => {
          // Said, never swallowed: the blocks are still unlogged.
          failed = error instanceof Error ? error.message : String(error);
          sending = false;
        },
      );
  }

  let first: HTMLInputElement | undefined = $state();

  onMount(() => {
    first?.focus();
    const { from, to } = dayBounds(draft.day);
    void ports
      .dayBlocks(from.toISOString(), to.toISOString())
      .then((day) => {
        blocks = day.blocks.filter((row) => draft.block_ids.includes(row.block.id));
      })
      .catch(() => {
        // The head of the line still says how many; the times are a nicety.
      });
    void ports
      .listSources()
      .then((rows) => {
        names = Object.fromEntries(rows.map((row) => [row.id, row.display_name]));
      })
      // The names are a nicety too: without them a source reads as its id.
      .catch(() => {});
    return layers.add({ kind: "dialog", close: leave });
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<!-- The backdrop's click is the mouse twin of `Esc`, which the shell's ladder handles. -->
<div
  class="tf-modal"
  onclick={(event) => {
    if (event.target === event.currentTarget) leave();
  }}
>
  <div class="tf-dialog" role="dialog" aria-modal="true" aria-labelledby="{uid}-h">
    <header>
      <h2 id="{uid}-h">Log time to <span class="tf-mono">{form.key}</span></h2>
      {#if title}<p class="tf-dsub">{title}</p>{/if}
    </header>
    <div class="tf-body">
      <div class="tf-dur">
        <label><span>From</span><input bind:this={first} type="time" aria-label="From" bind:value={form.began} /></label>
        <label><span>Minutes</span><input type="number" min="1" step="1" aria-label="Minutes" bind:value={form.minutes} /></label>
        <b>{form.reading}</b>
        <span class="tf-dim">{intervals}</span>
      </div>

      {#if draft.candidates.length > 0}
        <div class="tf-pane-h">What you did in that time</div>
        {#each draft.candidates as candidate (candidate.id)}
          <label class="tf-ck tf-cand">
            <input
              type="checkbox"
              checked={form.ticked.has(candidate.id)}
              onchange={() => form.toggle(candidate.id)}
            />
            <span>{candidate.bullet.replace(/^- /, "")}</span>
            <span class="tf-dim">{clockReading(candidate.at)}, {seenIn(candidate)}</span>
          </label>
        {/each}
      {:else}
        <div class="tf-pane-h">knobas saw nothing else in that time. The comment is yours to write.</div>
      {/if}

      <textarea
        rows="3"
        aria-label="Worklog comment"
        value={form.comment}
        oninput={(event) => form.takeOver(event.currentTarget.value)}
      ></textarea>
      <p class="tf-dim tf-note">
        Goes to {sourceName(draft.entity_id)} through the write queue.
        {form.edited ? "The comment is yours now; ticking no longer rewrites it." : "The comment follows the boxes you tick until you edit it."}
      </p>
      <!-- prettier-ignore -->
      <p class="tf-dim tf-fine">A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than guess; it never merges or drops what you wrote.</p>
      {#if failed}<p class="tf-fail" role="alert">{failed}</p>{/if}
    </div>
    <footer>
      {#if stopped}
        <button class="tf-btn tf-ghost tf-first" type="button" disabled={sending} onclick={() => onoutcome("later")}>
          Stop, log later
        </button>
        <button class="tf-btn tf-ghost" type="button" disabled={sending} onclick={() => onoutcome("keep")}>
          Keep running
        </button>
      {/if}
      <button class="tf-btn tf-ghost" type="button" disabled={sending} onclick={() => onoutcome("discard")}>Discard</button>
      <button class="tf-btn tf-primary" type="button" disabled={sending || form.seconds <= 0} onclick={send}>
        Log {form.reading} to {form.key}
      </button>
    </footer>
  </div>
</div>
