<!--
  The worklog draft in the ticket-first shell (#826, R-13.1 to R-13.3), drawn
  as prototype-6's `openWorklog` dialog over the same commands as the rooms
  UI's `WorklogDraft.svelte`: `worklog_draft` opened it, `log_work` queues it,
  and `DraftForm` holds the ticks, the comment and what is sent.

  It adds two reads the prototype draws and the draft does not carry:
  `day_blocks` for the intervals and the ticket's title (the draft names its
  blocks by id), and `list_sources` for the source name ("Goes to Jira").

  **Which buttons depends on what opened it.** After a stop: *Stop, don't log*,
  which drops the draft and not the time (F-140), then *Stop, log later*, *Cancel, keep
  running* and *Log*. Without one (the ad-hoc dialog's *Log to a ticket…*):
  *Cancel* and *Log*, with nothing about a clock. A stop's draft opens over the running clock (#896); what each button
  does to it is `drafting.svelte.ts`'s, and this answers with an outcome,
  except that *Log* awaits `onbeforelog` (the clock stopping) before it sends.
  `Esc` and the × are the quiet answer, and a quiet answer changes nothing: a stop's
  draft goes back to the running clock (*Cancel, keep running*; Björn, 2026-11-06,
  reversing #896's *Stop, log later*), any other is left. The scrim does nothing,
  as on every dialog with a footer.

  **A comment the reader edited outlives the dialog** (the key table, `drafts`): for its ticket
  and day, through `Esc`, the ×, *Cancel, keep running* and *Stop, log later*, each of which
  leaves the time to be logged later. *Log* ends it, and so do *Stop, don't log* and a
  plain *Cancel*, which are the explicit discards. ⌘Enter is `TfDialog`'s, and presses *Log*. `Enter` in *From* or *Minutes*
  presses it too, because taking the offered time is the common case (F-363).

  A worklog the ticket's source's credential cannot make (`refused_writes`,
  #2052) draws *Log* disabled with the reason, and stops no clock for it.

  ADR-0012's sentence is here as it is in the rooms UI's draft, because this
  is where a person reaches for the one write whose duplicate costs money.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import TfKey from "./TfKey.svelte";

  import { listSources as realListSources, type SourceSummary } from "../ipc/sources";
  import {
    dayBlocks as realDayBlocks,
    logWork as realLogWork,
    type DayBlock,
    type Draft,
    type Worklog,
  } from "../ipc/time";
  import { drafts } from "./drafts.svelte";
  import { askRefused, refusalOf, refusedAttrs, type RefusedWritesPort } from "../shell/refused-writes";
  import { clockReading, dayBounds, dayKey, dayLabel, readerDay } from "../time/day";
  import { DraftForm } from "../time/draft-form.svelte";
  import type { DraftOutcome } from "./drafting.svelte";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import TfDialog from "./settings/TfDialog.svelte";

  export interface TfWorklogDraftPorts {
    logWork: typeof realLogWork;
    dayBlocks: (from: string, to: string) => Promise<{ blocks: DayBlock[] }>;
    listSources: () => Promise<Pick<SourceSummary, "id" | "display_name">[]>;
    /** What the source's credential cannot write (#2052); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort;
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

  // svelte-ignore state_referenced_locally
  // Read once: a draft swapped mid-life would drop the reader's ticks.
  const form = new DraftForm(draft);
  // svelte-ignore state_referenced_locally
  const commentKey = `worklog:${draft.entity_id}:${draft.day}`;
  if (drafts.has(commentKey)) form.takeOver(drafts.get(commentKey));

  function typed(words: string) {
    form.takeOver(words);
    drafts.put(commentKey, words);
  }

  /** An explicit discard: the time's answer, and the words with it. */
  function discard() {
    drafts.forget(commentKey);
    onoutcome("discard");
  }

  let sending = $state(false);
  let failed = $state<string | null>(null);
  /** Why the source's credential cannot take this worklog (#2052), or `null`. */
  let refused = $state<string | null>(null);

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

  /** Leaving without answering: a stop's draft leaves the clock running, any other is dropped. */
  function leave() {
    // A log in flight answers for itself; leaving now would say "unlogged".
    if (sending) return;
    onoutcome(stopped ? "keep" : "discard");
  }

  function send() {
    if (sending || refused !== null) return;
    sending = true;
    failed = null;
    // The clock first: `log_work` covers only closed blocks.
    void onbeforelog()
      .then(() => ports.logWork(draft.entity_id, readerDay(draft.day), form.work))
      .then(
        (worklog) => {
          // Outside the rejection handler on purpose: the worklog exists now,
          // and a throw in the shell's toast must not read as a failed send.
          drafts.forget(commentKey);
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

  /** `Enter` in a single field takes what it holds. A modified one is `TfDialog`'s. */
  function logOnEnter(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.isComposing || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    if (form.seconds > 0) send();
  }

  onMount(() => {
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
    void askRefused([{ entity: draft.entity_id, op: "log_work" }], ports.refusedWrites).then(
      (answer) => (refused = refusalOf(answer, draft.entity_id, "log_work")),
    );
  });
</script>

{#snippet heading()}Log time to <span class="tf-mono">{form.key}</span>{/snippet}

<!-- `Esc` and the × leave quietly, through `TfDialog`'s `onclose`. -->
<!-- Wide enough that *Nothing to log yet* stands in the footer's row with the four buttons, so emptying Minutes moves nothing. -->
<TfDialog why={form.seconds <= 0 ? "Nothing to log yet." : null} title={heading} subtitle={title} className="tf-logdlg" onclose={leave} {layers}>
  {#snippet body()}
      <div class="tf-formgrid">
        <label for="tf-log-from">From</label>
        <div class="tf-dur"><input id="tf-log-from" type="time" aria-label="From" onkeydown={logOnEnter} bind:value={form.began} /></div>
        <label for="tf-log-minutes">Minutes</label>
        <div class="tf-dur">
          <input id="tf-log-minutes" type="number" min="1" step="1" aria-label="Minutes" onkeydown={logOnEnter} bind:value={form.minutes} />
          <b>{form.reading}</b>
        </div>
        <p class="tf-dim tf-note">{intervals}</p>

        {#if draft.candidates.length > 0}
          <fieldset class="tf-cands">
            <legend class="tf-pane-h">What you did in that time</legend>
            {#each draft.candidates as candidate (candidate.id)}
              <label class="tf-ck tf-cand">
                <input
                  type="checkbox"
                  checked={form.ticked.has(candidate.id)}
                  onchange={() => form.toggle(candidate.id)}
                />
                <span>{candidate.bullet.replace(/^- /, "")}</span>
                <span class="tf-dim">{clockReading(candidate.at)}</span>
              </label>
            {/each}
          </fieldset>
        {:else}
          <p class="tf-pane-h">knobas saw nothing else in that time. The comment is yours to write.</p>
        {/if}

        <label for="tf-log-comment">Comment</label>
        <textarea
          id="tf-log-comment"
          rows={Math.min(6, Math.max(3, draft.candidates.length))}
          aria-label="Worklog comment"
          value={form.comment}
          oninput={(event) => typed(event.currentTarget.value)}
        ></textarea>
        <p class="tf-dim tf-note">
          {form.edited ? "The comment is yours now; ticking no longer rewrites it." : "The comment follows the boxes you tick until you edit it."}
        </p>
      </div>
      <!-- One thought: where the worklog goes, and what that road can do to it (ADR-0012). -->
      <!-- prettier-ignore -->
      <p class="tf-dim tf-fine">Goes to {sourceName(draft.entity_id)} through the write queue. A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than guess; it never merges or drops what you wrote.</p>
      {#if refused !== null}<p class="tf-fail">{refused}</p>{/if}
      {#if failed}<p class="tf-fail" role="alert">{failed}</p>{/if}
  {/snippet}
  {#snippet footer()}
      {#if stopped}
        <button class="tf-btn tf-ghost tf-first" type="button" disabled={sending} onclick={discard}>Stop, don't log</button>
        <button class="tf-btn tf-ghost" type="button" disabled={sending} onclick={() => onoutcome("later")}>
          Stop, log later
        </button>
        <button class="tf-btn tf-ghost" type="button" disabled={sending} onclick={() => onoutcome("keep")}>
          Cancel, keep running
        </button>
      {:else}
        <button class="tf-btn tf-ghost" type="button" disabled={sending} onclick={discard}>Cancel</button>
      {/if}
      <button class="tf-btn tf-primary" type="button" {...refusedAttrs(refused)} disabled={sending || form.seconds <= 0} onclick={send}>
        Log {form.reading} to <TfKey value={form.key} />
      </button>
  {/snippet}
</TfDialog>
