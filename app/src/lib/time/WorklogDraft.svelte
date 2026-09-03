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
  import { atClock, clockOf, offsetMinutes } from "./draft";

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
   * The ticked candidates, by id.
   *
   * A `Set` of ids rather than a flag on a copy of each candidate: the
   * candidates are the backend's list and this component has no business
   * holding a second version of them.
   */
  // svelte-ignore state_referenced_locally
  // Read once, at init, on purpose: a draft is one afternoon and this dialog
  // is opened for one, so a `draft` swapped mid-life would silently discard
  // whatever the reader had already ticked. The shell closes and reopens.
  let ticked = $state(new Set(draft.candidates.map((c) => c.id)));

  /**
   * Whether the reader has taken the comment over.
   *
   * Once they have typed, ticking a box no longer rewrites what they wrote —
   * the alternative is a checkbox silently deleting a sentence somebody
   * composed, which is the one thing a draft must not do.
   */
  let edited = $state(false);
  // svelte-ignore state_referenced_locally
  let typed = $state(draft.comment);

  // svelte-ignore state_referenced_locally
  /** Minutes, because that is the unit a person corrects a clock in. */
  let minutes = $state(Math.round(draft.seconds / 60));

  // svelte-ignore state_referenced_locally
  /** When the work began, as the time field shows it. */
  let began = $state(clockOf(draft.started_at));

  /**
   * ...and as the instant that is sent. The day never moves; see `atClock`.
   *
   * **An untouched field sends the block's own start, verbatim.** A time field
   * has no seconds, so putting every start through `atClock` would round
   * 09:00:37 down to 09:00 on a draft nobody edited — knobas quietly changing
   * a fact it measured. The reading is only replaced once it differs from what
   * the block says.
   */
  const startedAt = $derived(
    began === clockOf(draft.started_at) ? draft.started_at : atClock(draft.started_at, began),
  );

  let sending = $state(false);
  /** What the backend said, if it refused. */
  let failed = $state<string | null>(null);

  /** The comment as the ticks make it — the backend's bullets, joined. */
  const generated = $derived(
    draft.candidates
      .filter((c) => ticked.has(c.id))
      .map((c) => c.bullet)
      .join("\n"),
  );

  const comment = $derived(edited ? typed : generated);

  /**
   * ...and the length that is sent.
   *
   * **An untouched field sends the blocks' own sum, verbatim**, for the reason
   * `startedAt` above does: a block is measured to the second, so a day of two
   * stretches rarely adds up to a whole minute, and putting every draft
   * through a minutes field would log a number knobas never measured while
   * marking those exact blocks spent. The field is minutes because that is the
   * unit a person corrects a clock in, not because a worklog is one.
   */
  const seconds = $derived(
    minutes === Math.round(draft.seconds / 60)
      ? draft.seconds
      : Math.max(0, Math.round(minutes * 60)),
  );

  /** `2h 30m`, `45m` — the reading on the button. */
  const reading = $derived.by(() => {
    const whole = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (whole === 0) return `${rest}m`;
    return rest === 0 ? `${whole}h` : `${whole}h ${rest}m`;
  });

  /** `PAY-231` — the half after the first colon, as everywhere else. */
  const key = $derived(draft.entity_id.slice(draft.entity_id.indexOf(":") + 1));

  function toggle(id: string) {
    // Reassigned rather than mutated: `$state` over a `Set` tracks the binding,
    // and `.add()` on the same object re-renders nothing.
    const next = new Set(ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    ticked = next;
  }

  function send() {
    if (sending) return;
    sending = true;
    failed = null;
    void logWork(
      draft.entity_id,
      { day: draft.day, offsetMinutes: offsetMinutes() },
      { startedAt, seconds, comment },
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

<Modal title="Log time to {key}" subtitle={draft.day} {onclose}>
  {#snippet body()}
    <p class="lab">
      The day's blocks on this ticket, concatenated — they ran until {clock(draft.ended_at)}.
    </p>

    <div class="two">
      <div class="fld">
        <label class="l" for="{uid}-began">Started</label>
        <input class="inp" id="{uid}-began" type="time" bind:value={began} />
      </div>
      <div class="fld">
        <label class="l" for="{uid}-mins">Time logged (minutes)</label>
        <input class="inp" id="{uid}-mins" type="number" min="1" step="1" bind:value={minutes} />
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
                checked={ticked.has(candidate.id)}
                onchange={() => toggle(candidate.id)}
              />
              <span class="t">{candidate.bullet.replace(/^- /, "")}</span>
            </label>
          </li>
        {/each}
      </ul>
      {#if edited}
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
        value={comment}
        oninput={(event) => {
          edited = true;
          typed = event.currentTarget.value;
        }}
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
    <button class="btn pri" type="button" disabled={sending || seconds <= 0} onclick={send}>
      Log {reading} to {key}
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
