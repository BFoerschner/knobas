<!--
  What stopping the timer on a ticket opens (#280, spec #272 "Worklog draft and
  ad-hoc block").

  The day's blocks on that ticket, concatenated into one interval, the reader's
  own work inside it as checkboxes, and a comment generated from what is
  ticked. *Log 2h 30m to PAY-231* queues it like any other write.

  ## What this component decides, and what it does not

  It decides **which candidates are ticked** and nothing else about the words.
  A candidate carries its own `bullet`, composed by the backend, and the
  comment is those bullets joined — so the wording of a worklog is one rule in
  one language, and unticking a box cannot produce a line this file spelled
  differently from the one the draft opened with.

  The moment the reader types in the comment box, it is **theirs**: ticking
  after that would silently overwrite what they wrote, so the boxes stop
  rewriting it and say so. Editing down to empty is allowed — a worklog with no
  words is a worklog.

  The **interval is editable as a whole** (spec #272, story 31): when the work
  began and how long it lasted are both the reader's to correct — a timer
  started ten minutes after the work did is the ordinary case. What the edit
  does *not* move is which blocks the worklog covers: those are re-derived by
  the backend from the same rule the draft used, because a caller that could
  name them could name another ticket's (`knobas_app::time::worklog::log`
  records why). The day does not move either — `atClock` keeps it, so
  correcting a start cannot file the afternoon under yesterday.

  ## ADR-0012's sentence is here on purpose

  A worklog is a write, and a write in flight when knobas stops may arrive
  twice — which for a worklog means two entries on a timesheet somebody bills
  from. The write-queue panel says it where a re-send is contemplated
  (#224); this says it where a *worklog* is, because this is the surface a
  person reaches for the one write of knobas' whose duplicate costs money.
  `commands/time.rs`'s `the_draft_quotes_adr_0012s_sentence_verbatim` reads the
  ADR and this file and fails if the two ever stop agreeing.
-->
<script lang="ts">
  import { logWork as realLogWork, type Draft, type Worklog } from "../ipc/time";
  import Modal from "../shell/Modal.svelte";
  import { offsetMinutes } from "./draft";
  import { DraftForm } from "./draft-form.svelte";

  let {
    draft,
    onclose,
    onlogged,
    logWork = realLogWork,
  }: {
    /** The draft the backend answered `worklog_draft` with. */
    draft: Draft;
    onclose: () => void;
    /** The copy that now exists. Saying so on screen is the shell's. */
    onlogged: (worklog: Worklog) => void;
    /** The bridge, injectable so a test needs no Tauri. */
    logWork?: typeof realLogWork;
  } = $props();

  /** Unique per instance, so two drafts cannot share a control's `for`. */
  const uid = `worklog-${Math.random().toString(36).slice(2, 9)}`;

  /**
   * The form: the ticks, whether the comment is the reader's, and what is sent.
   * Shared with the new shell's draft (#826); the rules are explained above.
   */
  // svelte-ignore state_referenced_locally
  // Read once, at init, on purpose: a draft swapped mid-life would silently
  // discard whatever the reader had already ticked. The shell closes and reopens.
  const form = new DraftForm(draft);

  let sending = $state(false);
  /** What the backend said, if it refused. */
  let failed = $state<string | null>(null);

  function send() {
    if (sending) return;
    sending = true;
    failed = null;
    void logWork(
      draft.entity_id,
      { day: draft.day, offsetMinutes: offsetMinutes() },
      form.work,
    )
      .then((worklog) => {
        onlogged(worklog);
        onclose();
      })
      .catch((error: unknown) => {
        // Said, never swallowed: the blocks are still unlogged, and a reader
        // who is not told will close the draft believing their day is on the
        // ticket.
        failed = error instanceof Error ? error.message : String(error);
        sending = false;
      });
  }

  /** `09:00` — the clock reading, in the reader's own zone. */
  function clock(at: string): string {
    return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<Modal title="Log time to {form.key}" subtitle={draft.day} {onclose}>
  {#snippet body()}
    <p class="lab">
      The day's blocks on this ticket, concatenated — they ran until {clock(draft.ended_at)}.
    </p>

    <div class="two">
      <div class="fld">
        <label class="l" for="{uid}-began">Started</label>
        <input class="inp" id="{uid}-began" type="time" bind:value={form.began} />
      </div>
      <div class="fld">
        <label class="l" for="{uid}-mins">Time logged (minutes)</label>
        <input class="inp" id="{uid}-mins" type="number" min="1" step="1" bind:value={form.minutes} />
      </div>
    </div>

    {#if draft.candidates.length > 0}
      <p class="lab head">What you did in that time</p>
      <ul class="cands">
        {#each draft.candidates as candidate (candidate.id)}
          <li>
            <label class="cand">
              <input
                type="checkbox"
                checked={form.ticked.has(candidate.id)}
                onchange={() => form.toggle(candidate.id)}
              />
              <span class="t">{candidate.bullet.replace(/^- /, "")}</span>
            </label>
          </li>
        {/each}
      </ul>
      {#if form.edited}
        <p class="lab note">
          The comment is yours now — ticking no longer rewrites it.
        </p>
      {/if}
    {:else}
      <p class="lab head">knobas saw nothing else in that time. The comment is yours to write.</p>
    {/if}

    <div class="fld">
      <label class="l" for="{uid}-comment">Comment</label>
      <textarea
        class="inp ta"
        id="{uid}-comment"
        rows="4"
        value={form.comment}
        oninput={(event) => form.takeOver(event.currentTarget.value)}
      ></textarea>
    </div>

    {#if failed}
      <p class="fail">{failed}</p>
    {/if}
  {/snippet}

  {#snippet footer()}
    <p class="guarantee">
      A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than
      guess; it never merges or drops what you wrote.
    </p>
    <span class="spacer"></span>
    <button class="btn ghost" type="button" onclick={onclose}>Not now</button>
    <button class="btn pri" type="button" disabled={sending || form.seconds <= 0} onclick={send}>
      Log {form.reading} to {form.key}
    </button>
  {/snippet}
</Modal>

<style>
  .head {
    margin: 14px 0 6px;
  }

  .note {
    margin-top: 6px;
  }

  .fld {
    margin-top: 12px;
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .fld .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-bottom: 4px;
  }

  .ta {
    width: 100%;
    resize: vertical;
    font: 400 12px var(--mono);
  }

  .cands {
    display: grid;
    gap: 2px;
  }

  .cand {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    column-gap: 8px;
    padding: 3px 4px;
    border-radius: 2px;
  }

  .cand:hover {
    background: var(--raised);
  }

  .cand .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fail {
    margin-top: 10px;
    color: var(--fail);
    font-size: 12px;
  }

  .guarantee {
    max-width: 44ch;
    margin: 0;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.4;
  }
</style>
