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

  **Drawn in the ticket-first look (#1425).** A `TfDialog`, so it sits on the
  shell's layer stack; *Queued writes*, the status bar's word; each row named
  in words by `writeWords`, mono only for keys, paths and bodies.
-->
<script lang="ts">
  import { keyOf, minutesLabel } from "../time/format";
  import { getEntity } from "../ipc/entity";
  import type { QueuedWrite } from "../ipc/sources";
  import { clockReading, dayKey, dayLabel } from "../time/day";
  import { minutesOf } from "../time/week";
  import { layers as sharedLayers, type Layers } from "../tf/layers.svelte";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { patience } from "./patience.svelte";
  import { ago } from "./time";
  import {
    blockers,
    demandOf,
    editableBody,
    movedFile,
    namedByTitle,
    readSnapshot,
    unconfirmedSend,
    withBody,
    withdrawnWorklog,
    writeWords,
    type WithdrawnWorklog,
    type WriteQueue,
  } from "./write-queue.svelte";

  let {
    queue,
    now = new Date(),
    onclose,
    layers = sharedLayers,
    nameOf = (entityId: string) => getEntity(entityId).then((detail) => detail.row.title || null),
  }: {
    queue: WriteQueue;
    /** Injectable clock, so "3 minutes ago" is testable rather than waited for. */
    now?: Date;
    onclose: () => void;
    /** The shell's layer stack, which `Esc` reads. Tests build their own. */
    layers?: Layers;
    /**
     * A monitor's, page's or build configuration's title, for a row that has
     * no key a person reads (`writeWords`). `null`, or a failed read, leaves
     * the row saying what kind of thing it is.
     */
    nameOf?: (entityId: string) => Promise<string | null>;
  } = $props();

  const wait = patience(() => !queue.answered && queue.error === null);

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
  /** The held or refused write each waiting write is stuck behind (#1473). */
  const blockedBy = $derived(blockers(queue.rows));
  /**
   * The Waiting group's line: the promise that these go on their own holds
   * only for a write nothing above holds up.
   */
  const waitingNote = $derived.by(() => {
    const blocked = waiting.filter((row) => blockedBy.has(row.id)).length;
    if (blocked === 0) return "These go on their own as soon as their source can take them.";
    if (blocked === waiting.length) {
      return "These wait for a write above that needs your decision. Each goes once that write is sent or discarded.";
    }
    return "Unless a row says it waits for a write above, these go on their own as soon as their source can take them.";
  });

  /** Titles looked up so far, by entity id; each id is asked once. */
  let names = $state<Record<string, string>>({});
  const asked = new Set<string>();
  $effect(() => {
    for (const row of queue.rows) {
      const id = row.entity_id;
      if (!namedByTitle(row) || asked.has(id)) continue;
      asked.add(id);
      nameOf(id).then(
        (title) => {
          if (title) names[id] = title;
        },
        () => {},
      );
    }
  });

  /** Why a pending write has not gone, in the reader's words rather than the wire's. */
  function waitingBecause(row: QueuedWrite): string {
    // First: one item's writes go in order, so nothing else moves this write
    // until the one above is sent or discarded, source or no source (#1473).
    const blocker = blockedBy.get(row.id);
    if (blocker !== undefined) {
      return blocker.state === "held" ? "Waits for the held write above." : "Waits for the refused write above.";
    }
    // Before the per-attempt reasons: the scheduler never flushes a disabled
    // source, so whatever the last attempt said, nothing will move until the
    // source is back on — and "not tried yet" forever would be this surface
    // going silent about it (issue #204).
    if (!row.source_enabled) return "Its source is turned off. Re-enable it in Sources to send this.";
    if (row.attempted_at === null) return "Not tried yet.";
    switch (row.wait_reason) {
      case "unauthorized":
        return "The credential was rejected.";
      case "unreachable":
        // A keychain that would not open climbs the same ladder as a silent
        // server, and the queue's detail is how the two are told apart (#1741).
        return row.detail?.startsWith("keychain:")
          ? "The keychain would not open. Unlock it and this is tried again on its own."
          : "The server did not answer.";
      default:
        return "Waiting.";
    }
  }

  function openEditor(row: QueuedWrite) {
    editing = row.id;
    draft = editableBody(row.payload) ?? "";
  }

  async function save(row: QueuedWrite) {
    const payload = withBody(row.payload, draft, row.held_snapshot);
    if (payload === null) return;
    // Only a skipped amend (the store was busy) keeps the editor and its draft.
    if (await queue.amend(row.id, payload)) editing = null;
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

  /** `2 h 15 min` -- minute-granular and never rounded up (CONTEXT.md). */
  function worked(seconds: number): string {
    return minutesLabel(minutesOf(seconds));
  }
</script>

<!--
  ADR-0012's canonical sentence, written once and rendered twice -- under the
  retry control, and in the withdrawal dialog. A second typed copy is a second
  place for it to drift from the decision record, which is the reason
  `write-queue.test.svelte.ts` stopped holding one of its own.
-->
{#snippet guarantee()}
  <p class="tf-wq-guarantee">
    A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than
    guess; it never merges or drops what you wrote.
  </p>
{/snippet}

<!-- A row's heading: what the write does, and when it was queued. -->
{#snippet heading(row: QueuedWrite)}
  <div class="tf-wq-row-h">
    <b>{#each writeWords(row, (id) => names[id] ?? null) as part, index (index)}{#if part.code}<span class="tf-mono">{part.text}</span>{:else}{part.text}{/if}{/each}</b>
    <span class="tf-wq-when">Queued {ago(row.queued_at, now)}</span>
  </div>
{/snippet}

<TfDialog
  title="Queued writes"
  subtitle={queue.rows.length === 0
    ? "Nothing owed"
    : `${queue.counts.pending} waiting · ${queue.counts.held + queue.counts.refused} need you`}
  className="tf-wq-dialog"
  {layers}
  {onclose}
>
  {#snippet body()}
    {#if queue.error}
      <p class="tf-fail" role="alert">{queue.error}</p>
    {/if}

    {#if queue.rows.length === 0 && !queue.answered}
      <!-- Unread, or the read failed (the reason is above): neither is an
           empty queue. -->
      {#if queue.error === null && wait.slow}
        <p class="tf-dim">Reading the queue…</p>
      {/if}
    {:else if queue.rows.length === 0}
      <p class="tf-dim">Nothing is queued. Every edit you have made has reached its source.</p>
    {/if}

    {#if decisions.length > 0}
      <section class="tf-wq-sec">
        <h3 class="tf-wq-h tf-wq-decide">
          Needs your decision
          <span class="tf-wq-n">{decisions.length}</span>
        </h3>
        <p class="tf-wq-note">
          knobas is holding these. Nothing sends them, and nothing discards them, until you say
          so.
        </p>
        {#each decisions as row (row.id)}
          {@const before = readSnapshot(row.target_snapshot)}
          {@const nowSide = readSnapshot(row.held_snapshot)}
          {@const body = editableBody(row.payload)}
          {@const file = movedFile(row)}
          {@const unconfirmed = row.source_enabled ? unconfirmedSend(row) : null}
          <article class="tf-wq-row tf-wq-{row.state}">
            {@render heading(row)}

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
              <p class="tf-wq-why">
                knobas is holding this because its source is turned off. Re-enable it in Sources
                to send it — or edit or discard it here.
              </p>
            {:else if unconfirmed !== null}
              <!--
                Sent, and never answered (#1125): the write may have landed.
                Sending it again on its own could make a second ticket or
                comment, so the reader checks the source first.
              -->
              <p class="tf-wq-why">
                knobas sent this, but {unconfirmed.source} did not answer, so it may have arrived.
                Check {unconfirmed.source}, then send it again or discard it.
              </p>
              {#if unconfirmed.detail}<pre class="tf-wq-pre">{unconfirmed.detail}</pre>{/if}
            {:else if row.state === "held" && file !== null}
              <!--
                A commit over a file that moved (#822, #845): the queue
                projected only the repository's liveness when it was queued,
                so "when you queued it" has nothing to show. The two versions
                that matter are the reader's file and the source's.
              -->
              <p class="tf-wq-why">The file changed on the source after you queued this.</p>
              <div class="tf-wq-sides">
                <div>
                  <span class="tf-wq-side">Yours, <span class="tf-mono">{file.path}</span></span>
                  <pre class="tf-wq-pre">{file.mine}</pre>
                </div>
                <div>
                  <span class="tf-wq-side">On the source now</span>
                  <pre class="tf-wq-pre">{file.theirs}</pre>
                </div>
              </div>
            {:else if row.state === "held"}
              <p class="tf-wq-why">The target changed after you queued this.</p>
              <div class="tf-wq-sides">
                <div>
                  <!--
                    The version, where the op has one (#286). A page edit is the
                    one write whose two sides can read alike while differing in
                    what matters, so the number goes in the label rather than
                    leaving the reader to spot it.
                  -->
                  <span class="tf-wq-side">
                    When you queued it{before.version === null ? "" : ` — version ${before.version}`}
                  </span>
                  {#if before.raw !== null}
                    <pre class="tf-wq-pre">{before.raw}</pre>
                  {:else if before.live}
                    <pre class="tf-wq-pre">{before.text ?? ""}</pre>
                  {:else}
                    <p class="tf-wq-gone">Not in the mirror</p>
                  {/if}
                </div>
                <div>
                  <span class="tf-wq-side">
                    As it stands now{nowSide.version === null ? "" : ` — version ${nowSide.version}`}
                  </span>
                  {#if nowSide.raw !== null}
                    <pre class="tf-wq-pre">{nowSide.raw}</pre>
                  {:else if nowSide.live}
                    <pre class="tf-wq-pre">{nowSide.text ?? ""}</pre>
                  {:else}
                    <p class="tf-wq-gone">The source withdrew it</p>
                  {/if}
                </div>
              </div>
            {:else}
              <p class="tf-wq-why">The source refused this write. It will not be retried.</p>
              {#if row.detail}<pre class="tf-wq-pre">{row.detail}</pre>{/if}
            {/if}

            {#if editing === row.id}
              <div class="tf-wq-edit">
                <span class="tf-wq-side" id="wq-edit-label-{row.id}">Your write</span>
                <textarea aria-labelledby="wq-edit-label-{row.id}" bind:value={draft} rows="4"></textarea>
              </div>
            {:else if body !== null && file === null}
              <pre class="tf-wq-pre tf-wq-mine">{body}</pre>
            {/if}

            <div class="tf-wq-acts">
              {#if editing === row.id}
                <button class="tf-btn tf-sm tf-primary" disabled={queue.busy} onclick={() => save(row)}>
                  Send this instead
                </button>
                <button class="tf-btn tf-sm tf-ghost" onclick={() => (editing = null)}>Cancel</button>
              {:else}
                {#if row.state === "held" && row.source_enabled}
                  <button class="tf-btn tf-sm" disabled={queue.busy} onclick={() => queue.apply(row.id)}>
                    {unconfirmed === null ? "Send mine anyway" : "Send again"}
                  </button>
                {/if}
                {#if body !== null}
                  <button class="tf-btn tf-sm" disabled={queue.busy} onclick={() => openEditor(row)}>
                    Edit…
                  </button>
                {/if}
                <button class="tf-btn tf-sm tf-danger" disabled={queue.busy} onclick={() => withdraw(row)}>
                  Discard
                </button>
              {/if}
            </div>
          </article>
        {/each}
      </section>
    {/if}

    {#if waiting.length > 0}
      <section class="tf-wq-sec">
        <h3 class="tf-wq-h">
          Waiting
          <span class="tf-wq-n">{waiting.length}</span>
        </h3>
        <p class="tf-wq-note">{waitingNote}</p>
        {#each waiting as row (row.id)}
          {@const body = editableBody(row.payload)}
          <article class="tf-wq-row tf-wq-pending">
            {@render heading(row)}
            <p class="tf-wq-why">{waitingBecause(row)}</p>
            {#if body !== null}<pre class="tf-wq-pre tf-wq-mine">{body}</pre>{/if}
            <div class="tf-wq-acts">
              <button class="tf-btn tf-sm tf-danger" disabled={queue.busy} onclick={() => withdraw(row)}>
                Cancel
              </button>
            </div>
          </article>
        {/each}
      </section>
    {/if}
  {/snippet}

  {#snippet footer()}
    <!--
      *Flush now* is impatience, not a decision: the scheduler already retries
      on its own tick. It is deliberately incapable of releasing a held write,
      and the line beside it says which rows it moves so that pressing it is
      never mistaken for answering a conflict. It is still the footer's primary,
      after a ghost *Close*, since it is the one thing the panel itself does
      (the Controls section's dialog footer, #2289).

      Beneath it, the guarantee knobas cannot keep, in ADR-0012's words: no
      transaction spans the send and the settle, so a write in flight when
      knobas stopped goes again. It sits by the retry control because this is
      where a re-send is contemplated, and it is unconditional because it is
      true of the queue, not of any row in it.
    -->
    <div class="tf-wq-foot tf-first">
      <p>Retries the waiting writes. Held writes are untouched.</p>
      {@render guarantee()}
    </div>
    <button class="tf-btn tf-ghost" onclick={onclose}>Close</button>
    <button class="tf-btn tf-primary" disabled={queue.busy || queue.flushing} onclick={() => queue.flush(null)}>Flush now</button>
  {/snippet}
</TfDialog>

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
  <TfDialog title="Withdraw this worklog?" {layers} onclose={() => (withdrawing = null)}>
    {#snippet body()}
      <p>
        <b>{worked(log.seconds)}</b> logged to <b class="tf-mono">{keyOf(log.entity)}</b>, starting
        {dayLabel(dayKey(new Date(log.started)))} at {clockReading(log.started)} — {log.seconds}
        seconds.
      </p>
      {#if log.comment}<pre class="tf-wq-pre tf-wq-mine">{log.comment}</pre>{/if}
      <p>
        <b>knobas cannot tell whether Jira already took it.</b> Nothing in the queue separates a
        write that arrived and was never marked sent from one that never left — not the state it
        is in, and not how many times it has been tried. Withdrawing it makes this time
        <b>unlogged</b> again and offers it back to <i>Log all</i>, so if Jira did take it,
        logging it a second time puts the same {worked(log.seconds)} on {keyOf(log.entity)}
        twice.
      </p>
      {@render guarantee()}
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (withdrawing = null)}>Keep it queued</button>
      <button class="tf-btn tf-primary tf-danger" disabled={queue.busy} onclick={confirmWithdrawal}>
        Discard the worklog
      </button>
    {/snippet}
  </TfDialog>
{/if}
