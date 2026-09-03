<!--
  The write queue, and the conflict surface (issue #42, stories 3 and 10-18).

  Two jobs, and the second is the reason the first exists.

  **What knobas still owes.** A list, not a count: every write that has not
  reached its source, what it is, what it targets, when it was made, and why it
  has not gone.

  **What needs an answer.** A write whose target changed after it was queued is
  *held*, and knobas will not send it until the reader chooses. Those rows are
  a separate section at the top with their own heading, their own colour and
  their own verb — not a differently-shaded member of one list. Story 18 is
  "distinguishable at a glance", and a badge on row nine of twelve is not that.

  **A worklog's withdrawal asks first.** Discarding any other write is
  immediate, as it always was; discarding a queued worklog opens a
  confirmation, because since #328 that withdrawal gives the blocks back and
  the same hour can then be logged a second time (issue #331). The rule for
  *which* lives in `withdrawnWorklog`, beside its reasons.

  **There is no timeout and there is nothing here that could become one.** No
  auto-apply, no bulk "send everything", no timer. *Flush now* asks the queue
  to retry what is merely waiting; it cannot release a held write, and the
  backend would not offer it one. `write-queue.test.svelte.ts` runs the clock
  forward over a held row and asserts it is untouched.
-->
<script lang="ts">
  import type { QueuedWrite } from "../ipc/sources";
  import { clockReading, dayKey, dayLabel, durationReading } from "../time/day";
  import { minutesOf } from "../time/week";
  import Modal from "./Modal.svelte";
  import { ago } from "./time";
  import {
    demandOf,
    editableBody,
    readSnapshot,
    withBody,
    withdrawnWorklog,
    type WithdrawnWorklog,
    type WriteQueue,
  } from "./write-queue.svelte";

  let {
    queue,
    now = new Date(),
    onclose,
  }: {
    queue: WriteQueue;
    /** Injectable clock, so "3 minutes ago" is testable rather than waited for. */
    now?: Date;
    onclose: () => void;
  } = $props();

  /** The row whose edit box is open, if any. One at a time. */
  let editing = $state<number | null>(null);
  let draft = $state("");

  /**
   * The worklog a withdrawal is being asked about, if any (issue #331).
   *
   * The row's id rather than the row: a refresh lands between the question
   * and the answer routinely, and what is confirmed must be the write the
   * reader was shown, not whichever object the list holds a moment later.
   */
  let withdrawing = $state<{ id: number; log: WithdrawnWorklog } | null>(null);

  const decisions = $derived(queue.rows.filter((row) => demandOf(row.state) === "decide"));
  const waiting = $derived(queue.rows.filter((row) => demandOf(row.state) === "waiting"));

  /** Why a pending write has not gone, in the reader's words rather than the wire's. */
  function waitingBecause(row: QueuedWrite): string {
    // Before the per-attempt reasons: the scheduler never flushes a disabled
    // source, so whatever the last attempt said, nothing will move until the
    // source is back on — and "not tried yet" forever would be this surface
    // going silent about it (issue #204).
    if (!row.source_enabled) return "its source is turned off — re-enable it in Sources to send this";
    if (row.attempted_at === null) return "not tried yet";
    switch (row.wait_reason) {
      case "unauthorized":
        return "the credential was rejected";
      case "unreachable":
        return "the server did not answer";
      default:
        return "waiting";
    }
  }

  function openEditor(row: QueuedWrite) {
    editing = row.id;
    draft = editableBody(row.payload) ?? "";
  }

  async function save(row: QueuedWrite) {
    const payload = withBody(row.payload, draft);
    if (payload === null) return;
    await queue.amend(row.id, payload);
    editing = null;
  }

  /**
   * Withdraw a write -- after asking, when the withdrawal is a worklog's.
   *
   * Every other op goes straight through, which is what it always did.
   * {@link withdrawnWorklog} is where the *which* lives, and why.
   */
  function withdraw(row: QueuedWrite) {
    const log = withdrawnWorklog(row);
    if (log === null) {
      void queue.discard(row.id);
      return;
    }
    withdrawing = { id: row.id, log };
  }

  function confirmWithdrawal() {
    const asked = withdrawing;
    if (asked === null) return;
    withdrawing = null;
    void queue.discard(asked.id);
  }

  /** `PAY-231` -- the half after the first colon, as everywhere else. */
  function ticketKey(entity: string): string {
    return entity.slice(entity.indexOf(":") + 1);
  }

  /** `2 h 15 min` -- minute-granular and never rounded up (CONTEXT.md). */
  function worked(seconds: number): string {
    return durationReading(minutesOf(seconds));
  }
</script>

<!--
  ADR-0012's canonical sentence, written once and rendered twice -- under the
  retry control, and in the withdrawal dialog. A second typed copy is a second
  place for it to drift from the decision record, which is the reason
  `write-queue.test.svelte.ts` stopped holding one of its own.
-->
{#snippet guarantee()}
  <p class="wq-guarantee">
    A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than
    guess; it never merges or drops what you wrote.
  </p>
{/snippet}

<Modal
  title="Pending writes"
  subtitle={queue.rows.length === 0
    ? "nothing owed"
    : `${queue.counts.pending} waiting · ${queue.counts.held + queue.counts.refused} need you`}
  wide
  {onclose}
>
  {#snippet body()}
    {#if queue.error}
      <p class="wq-error" role="alert">{queue.error}</p>
    {/if}

    {#if queue.rows.length === 0}
      <div class="empty">
        Nothing is queued. Every edit you have made has reached its source.
      </div>
    {/if}

    {#if decisions.length > 0}
      <section class="wq-sec">
        <h3 class="wq-h decide">
          Needs your decision
          <span class="lab">{decisions.length}</span>
        </h3>
        <p class="wq-note">
          knobas is holding these. Nothing sends them, and nothing discards them, until you say
          so.
        </p>
        {#each decisions as row (row.id)}
          {@const before = readSnapshot(row.target_snapshot)}
          {@const nowSide = readSnapshot(row.held_snapshot)}
          {@const body = editableBody(row.payload)}
          <article class="wq-row {row.state}">
            <header class="wq-row-h">
              <b class="wq-op">{row.op}</b>
              <span class="wq-target">{row.entity_id}</span>
              <span class="wq-when">queued {ago(row.queued_at, now)}</span>
            </header>

            {#if row.state === "held" && !row.source_enabled}
              <!--
                Held, but not because anything changed (issue #204): the user
                turned the source off, its items left the mirror's view, and
                the queue held the write it could no longer measure. The
                two-versions comparison would show a target that did not
                change, and *Send mine anyway* would park the write as
                "waiting" forever — the scheduler never flushes a disabled
                source. So neither is offered; the remedy is the source
                toggle, and the row says so.
              -->
              <p class="wq-why">
                knobas is holding this because its source is turned off. Re-enable it in Sources
                to send it — or edit or discard it here.
              </p>
            {:else if row.state === "held"}
              <p class="wq-why">The target changed after you queued this.</p>
              <div class="grid2">
                <div>
                  <!--
                    The version, where the op has one (#286). A page edit is the
                    one write whose two sides can read alike while differing in
                    what matters, so the number goes in the label rather than
                    leaving the reader to spot it.
                  -->
                  <span class="wq-side">
                    When you queued it{before.version === null ? "" : ` — version ${before.version}`}
                  </span>
                  {#if before.raw !== null}
                    <pre class="log">{before.raw}</pre>
                  {:else if before.live}
                    <pre class="log">{before.text ?? ""}</pre>
                  {:else}
                    <p class="wq-gone">not in the mirror</p>
                  {/if}
                </div>
                <div>
                  <span class="wq-side">
                    As it stands now{nowSide.version === null ? "" : ` — version ${nowSide.version}`}
                  </span>
                  {#if nowSide.raw !== null}
                    <pre class="log">{nowSide.raw}</pre>
                  {:else if nowSide.live}
                    <pre class="log">{nowSide.text ?? ""}</pre>
                  {:else}
                    <p class="wq-gone">the source withdrew it</p>
                  {/if}
                </div>
              </div>
            {:else}
              <p class="wq-why">
                The source refused this write. It will not be retried.
              </p>
              {#if row.detail}<pre class="log">{row.detail}</pre>{/if}
            {/if}

            {#if editing === row.id}
              <div class="wq-edit">
                <span class="lab" id="wq-edit-label-{row.id}">Your write</span>
                <textarea
                  class="inp"
                  aria-labelledby="wq-edit-label-{row.id}"
                  bind:value={draft}
                  rows="4"
                ></textarea>
              </div>
            {:else if body !== null}
              <pre class="log wq-mine">{body}</pre>
            {/if}

            <footer class="wq-acts">
              {#if editing === row.id}
                <button class="btn pri" disabled={queue.busy} onclick={() => save(row)}>
                  Send this instead
                </button>
                <button class="btn ghost" onclick={() => (editing = null)}>Cancel</button>
              {:else}
                {#if row.state === "held" && row.source_enabled}
                  <button class="btn" disabled={queue.busy} onclick={() => queue.apply(row.id)}>
                    Send mine anyway
                  </button>
                {/if}
                {#if body !== null}
                  <button class="btn" disabled={queue.busy} onclick={() => openEditor(row)}>
                    Edit…
                  </button>
                {/if}
                <button class="btn danger" disabled={queue.busy} onclick={() => withdraw(row)}>
                  Discard
                </button>
              {/if}
            </footer>
          </article>
        {/each}
      </section>
    {/if}

    {#if waiting.length > 0}
      <section class="wq-sec">
        <h3 class="wq-h">
          Waiting
          <span class="lab">{waiting.length}</span>
        </h3>
        <p class="wq-note">These go on their own as soon as their source can take them.</p>
        {#each waiting as row (row.id)}
          {@const body = editableBody(row.payload)}
          <article class="wq-row pending">
            <header class="wq-row-h">
              <b class="wq-op">{row.op}</b>
              <span class="wq-target">{row.entity_id}</span>
              <span class="wq-when">queued {ago(row.queued_at, now)}</span>
            </header>
            <p class="wq-why">{waitingBecause(row)}</p>
            {#if body !== null}<pre class="log wq-mine">{body}</pre>{/if}
            <footer class="wq-acts">
              <button class="btn danger" disabled={queue.busy} onclick={() => withdraw(row)}>
                Cancel
              </button>
            </footer>
          </article>
        {/each}
      </section>
    {/if}
  {/snippet}

  {#snippet footer()}
    <!--
      *Flush now* is impatience, not a decision: the scheduler already retries
      on its own tick. It is deliberately incapable of releasing a held write,
      and the label says which rows it moves so that pressing it is never
      mistaken for answering a conflict.

      Beneath it, the guarantee knobas cannot keep, in ADR-0012's words: no
      transaction spans the send and the settle, so a write in flight when
      knobas stopped goes again. It sits by the retry control because this is
      where a re-send is contemplated, and it is unconditional because it is
      true of the queue, not of any row in it.
    -->
    <div class="wq-foot">
      <span class="lab">Retries the waiting writes. Held writes are untouched.</span>
      {@render guarantee()}
    </div>
    <span class="spacer"></span>
    <button class="btn" disabled={queue.busy} onclick={() => queue.flush(null)}>Flush now</button>
    <button class="btn ghost" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<!--
  The consent moment on the one withdrawal that can bill an hour twice
  (issue #331). Not a generic "are you sure": the panel's other discards are
  ordinary, and a confirmation on all of them would teach the reader to click
  through this one.

  Three things, in the order the reader needs them. **What is being
  withdrawn**, so the question is about a worklog they recognise rather than
  about a queue row. **What knobas cannot promise**, in plain words: the
  write may already be at Jira -- `knobas_core::write_queue::discard` names
  the window in its own doc comment, one HTTP round-trip wide and not
  crash-only -- and since #328 the withdrawal hands the same hour back to
  *Log all*. **ADR-0012's sentence**, verbatim, because the guarantee behind
  all of that is written down once and quoted, never paraphrased.

  It asks on every open worklog, not only a pending one. `withdrawnWorklog`
  carries the reasoning: a refusal can be a worklog Jira accepted but did not
  name, and a hold can be one that arrived before the settle was lost, so the
  state is not evidence of anything the dialog would need.

  No end instant is shown. `WriteOp::LogWork` carries `started` and `seconds`
  and nothing else, and `seconds` is worked time rather than the span it sits
  in -- so an end computed here would be knobas inventing a fact about the
  worklog in the middle of telling the reader what it cannot be sure of.
-->
{#if withdrawing}
  {@const log = withdrawing.log}
  <Modal title="Withdraw this worklog?" center onclose={() => (withdrawing = null)}>
    {#snippet body()}
      <p>
        <b>{worked(log.seconds)}</b> logged to <b>{ticketKey(log.entity)}</b>, starting
        {dayLabel(dayKey(new Date(log.started)))} at {clockReading(log.started)} — {log.seconds}
        seconds.
      </p>
      {#if log.comment}<pre class="log wq-mine">{log.comment}</pre>{/if}
      <p>
        <b>knobas cannot tell whether Jira already took it.</b> Nothing in the queue separates a
        write that arrived and was never marked sent from one that never left — not the state it
        is in, and not how many times it has been tried. Withdrawing it makes this time
        <b>unlogged</b> again and offers it back to <i>Log all</i>, so if Jira did take it,
        logging it a second time puts the same {worked(log.seconds)} on {ticketKey(log.entity)}
        twice.
      </p>
      {@render guarantee()}
    {/snippet}
    {#snippet footer()}
      <span class="spacer"></span>
      <button class="btn ghost" onclick={() => (withdrawing = null)}>Keep it queued</button>
      <button class="btn danger" disabled={queue.busy} onclick={confirmWithdrawal}>
        Discard the worklog
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .wq-error {
    margin-bottom: 10px;
    color: var(--fail);
    font-size: 12px;
  }

  .wq-sec + .wq-sec {
    margin-top: 18px;
  }

  .wq-h {
    display: flex;
    align-items: center;
    gap: 8px;
    font: 500 13px var(--sans);
    color: var(--text);
  }

  /*
    The decisions heading carries the colour, not just the rows under it: the
    reader's eye reaches the heading first, and it is the heading that has to
    say "this section is different in kind".
  */
  .wq-h.decide {
    color: var(--amber);
  }

  .wq-note {
    margin: 2px 0 8px;
    color: var(--muted);
    font-size: 11.5px;
  }

  .wq-row {
    padding: 10px 12px;
    margin-bottom: 8px;
    border: 1px solid var(--hair);
    border-left-width: 3px;
    border-radius: 2px;
    background: var(--bg);
  }

  /*
    Three states, three left edges. A held write and a merely pending one are
    not the same thing waiting different lengths of time -- one needs the
    reader and the other needs the network -- so they never share an edge.
  */
  .wq-row.held {
    border-left-color: var(--amber);
  }

  .wq-row.refused {
    border-left-color: var(--fail);
  }

  .wq-row.pending {
    border-left-color: var(--hair2);
  }

  .wq-row-h {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
  }

  .wq-op {
    font: 500 11px var(--mono);
    color: var(--text);
  }

  .wq-target {
    font: 400 11.5px var(--mono);
    color: var(--link);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wq-when {
    margin-left: auto;
    color: var(--faint);
    font: 400 11px var(--mono);
    white-space: nowrap;
  }

  .wq-why {
    margin: 6px 0;
    font-size: 12px;
    color: var(--muted);
  }

  /* `.grid2` is the sheet's own two-column pair; only the gap below it is ours. */
  .grid2 {
    margin-bottom: 8px;
  }

  .wq-side {
    display: block;
    margin-bottom: 3px;
    color: var(--faint);
    font: 400 11px var(--mono);
  }

  .wq-gone {
    margin: 0;
    padding: 8px 10px;
    border: 1px dashed var(--hair2);
    border-radius: 2px;
    color: var(--faint);
    font: 400 11.5px var(--mono);
  }

  .wq-mine {
    border-color: var(--hair2);
    color: var(--text);
  }

  .wq-edit {
    display: block;
    margin: 6px 0;
  }

  .wq-edit .lab {
    display: block;
    margin-bottom: 3px;
  }

  .wq-acts {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }

  /*
    The footer is one flex row, and its only text was the uppercase label.
    Two prose sentences do not survive that treatment, so the label and the
    guarantee stack in a column of their own and the buttons keep the row.
  */
  .wq-foot {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .wq-guarantee {
    margin: 0;
    color: var(--muted);
    font-size: 11.5px;
  }
</style>
