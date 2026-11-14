<!--
  *Start work* on the job (#818, R-4.9): a To Do ticket assigned to the reader
  shows the five-step stepper over the BUILT flow (#44) -- create the branch
  in the suggested repository, open the pull request, link it, move the
  ticket -- and a fifth step of its own, start the timer.

  Each step shows its value, editable where the flow lets it be, and its
  state, which is the flow's: *ready* until it runs, *queued* while its write
  waits in the queue, *done* once it landed. The step the flow stopped at can
  be skipped, and retried when it failed. A queued one is checked again: that
  re-reads its write, sends it nothing more while it is open, and runs the flow
  on once it landed (#1998). The backend decides what runs next
  (`start-work.svelte.ts`'s rule); nothing here walks the steps itself.

  The proposal is made as soon as the repositories are read, in the suggested
  one: proposing dispatches nothing, and it is what puts real names in the
  fields. Picking another repository amends the two steps that name it.

  A move the source's credential cannot make (`moveRefused`, #2052) is drawn
  with the reason, and *Run all steps* skips it, which runs the flow on past
  it as *Skip* does: it is never queued, and the skipped row keeps the reason.

  Step 5 is the shell's *Start timer*, which opens the worklog draft for a
  timer already running on another ticket before it starts this one (R-4.9,
  story 62).

  **Nothing is chosen until something runs (F-130).** The steps are a look, on the
  card's sheet, so *Cancel* and `Esc` leave everything as it was; *Run all steps*
  makes the ticket the job and its toast names the one it replaces, and ends
  *Timer running.* when the same press starts the timer (F-145). Once a step has
  run, the primary says where the flow is (`runWord`) and the page's foot comes back
  with the timer (`started`).

  **A *Skip* before the run only marks the step (F-137).** `start_work_skip` settles
  a step and runs the flow on, so pressing it would be the run. The mark is this
  page's, *Include* takes it back, and *Run all steps* sends it instead of `run`, as
  it does for a refused move. There is no command that settles a step without running
  on, so one step can sit out the run: the first, or a refused move, and with a
  refused move the first's *Skip* is off and says why.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import TfKey from "../TfKey.svelte";
  import { stateWord } from "../state-word";

  import type { EntityDetail, StartWorkStep } from "../../ipc/entity";
  import type { Timer } from "../../shell/timer.svelte";
  import { toastStarted } from "../../shell/queued-toast";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { createStartWork, demandOf, editable, edited, runWord, unrun, type StartWorkApi } from "../../start-work/start-work.svelte";
  import { keyOf } from "./job";
  import { chosenJob } from "./chosen.svelte";
  import { pageVisible } from "../kept-pages.svelte";

  let {
    detail,
    repos,
    suggested,
    timer,
    api,
    onstarttimer,
    moveRefused = null,
    oncancel,
    started = $bindable(false),
    arrive = false,
  }: {
    detail: EntityDetail;
    /** Every repository, `null` while read: the dropdown's options. */
    repos: { entity_id: string; title: string }[] | null;
    /** The repository to suggest first: one the ticket already reaches. */
    suggested: string | null;
    timer: Pick<Timer, "current">;
    api: StartWorkApi;
    /** `quiet`: this press's own toast says the timer runs, so the start says nothing more (F-145). */
    onstarttimer: (entityId: string, name: string, quiet?: boolean) => void;
    /** Why the source's credential cannot move this ticket (#2052), or `null`. */
    moveRefused?: string | null;
    /** *Cancel*, while nothing has run; left out where the steps are not a look (the ticket is already the job). */
    oncancel?: (() => void) | undefined;
    /** Whether something of the flow has run, for the page's foot. */
    started?: boolean;
    /** A press brought the reader here: focus comes to the steps, since the press's own button has gone. */
    arrive?: boolean;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Its follow reads nothing while keep-alive holds the page hidden (#2249).
  const flow = createStartWork(detail.row.entity_id, api, pageVisible());
  // svelte-ignore state_referenced_locally
  const key = keyOf(detail.row.entity_id);

  onDestroy(() => flow.stop());
  void flow.load(null);

  /** The repository list, suggested first. */
  const options = $derived.by(() => {
    const list = repos ?? [];
    const first = list.find((repo) => repo.entity_id === suggested) ?? list[0];
    return first ? [first, ...list.filter((repo) => repo !== first)] : [];
  });

  /** Propose once, in the suggested repository, when there is no flow yet. */
  let proposed = false;
  $effect(() => {
    if (!flow.loaded || flow.steps.length > 0 || proposed || options.length === 0) return;
    proposed = true;
    untrack(() => void flow.load(options[0]!.entity_id));
  });

  const fresh = $derived(unrun(flow.steps));
  $effect(() => {
    started = !fresh;
  });
  const run = $derived(runWord(flow.steps, flow.busy));

  const step = (kind: StartWorkStep["step"]) => flow.steps.find((each) => each.step === kind) ?? null;
  const branch = $derived(step("create_branch"));
  const pr = $derived(step("create_pull_request"));
  const link = $derived(step("link_pull_request"));
  const move = $derived(step("transition"));
  /** The step the flow is on: the only one that can be retried or skipped. */
  const current = $derived(flow.steps.find((each) => demandOf(each.outcome) !== "done") ?? null);

  /** The op inside a step's payload. */
  function op(of: StartWorkStep | null): Record<string, unknown> {
    const payload = of?.payload as Record<string, unknown> | null | undefined;
    const body = payload ? Object.values(payload)[0] : null;
    return typeof body === "object" && body !== null ? (body as Record<string, unknown>) : {};
  }
  const text = (value: unknown) => (typeof value === "string" ? value : "");

  const repoOf = $derived(text(op(branch)["entity"]));
  const otherTimer = $derived.by(() => {
    const target = timer.current?.target;
    if (!target || target.kind !== "entity" || target.entity_id === detail.row.entity_id) return null;
    return keyOf(target.entity_id);
  });

  /** The step a *Skip* marked before the first run, by its id. Nothing is sent until *Run all steps* (F-137). */
  let markedId = $state<number | null>(null);
  /** A refused move is the one step that can sit out the run, so it takes the mark's place. */
  const marked = $derived(fresh && moveRefused === null ? markedId : null);
  /** The field of a step that won't run takes no edits. */
  const fixed = (of: StartWorkStep | null) => !of || !editable(of.outcome) || of.id === marked;

  /** What the timer step says: done once the timer runs here. */
  let timerSkipped = $state(false);
  const timerDone = $derived(
    timer.current?.target?.kind === "entity" && timer.current.target.entity_id === detail.row.entity_id,
  );

  /** A flow step's state in prototype-6's words. */
  function word(of: StartWorkStep | null): string {
    switch (of?.outcome) {
      case "succeeded":
        return "done";
      case "queued":
        return "queued";
      case "running":
        return "running";
      case "failed":
        return "failed";
      case "skipped":
        return "skipped";
      default:
        return "ready";
    }
  }

  function amend(of: StartWorkStep | null, field: string, value: string) {
    if (!of || text(op(of)[field]) === value) return;
    void flow.amend(of.id, edited(of, field, value));
  }

  /** Another repository: both steps that name one move with it. */
  async function pick(repo: string) {
    for (const of of [branch, pr]) {
      if (of && editable(of.outcome) && text(op(of)["entity"]) !== repo) {
        await flow.amend(of.id, edited(of, "entity", repo));
      }
    }
  }

  /** The steps have begun: the ticket is the job now (spec #2227), and the toast says so, what it replaced, and that the timer runs. */
  function began(timing: boolean) {
    const before = chosenJob.id;
    toastStarted(key, flow.steps.filter((each) => each.outcome === "queued").length, detail.source, undefined, before !== null && before !== detail.row.entity_id ? keyOf(before) : null, timing);
    chosenJob.choose(detail.row.entity_id);
  }

  async function runAll() {
    if (!run.can) return;
    // A move the credential cannot make is skipped, never queued. A skip runs the flow on past its step, so it is the run.
    const sitsOut = moveRefused !== null && move !== null && editable(move.outcome) ? move.id : marked;
    if (sitsOut !== null) await flow.skip(sitsOut);
    else await flow.run();
    if (flow.error !== null) return;
    const starts = !timerDone && !timerSkipped;
    // With another timer running, its draft comes first, so the timer isn't running yet and the draft's answer says so.
    const atOnce = starts && timer.current === null;
    began(atOnce);
    if (starts) onstarttimer(detail.row.entity_id, key, atOnce);
  }

  /** Why the first step's *Skip* is off with a refused move: `start_work_skip` runs on, so only one step can be settled before the run. */
  const ONE_SITS_OUT = "Only one step can sit out a run, and the move this source refuses is that one.";

  const ROWS = [
    { id: "branch", label: "Create branch" },
    { id: "pr", label: "Open pull request" },
    { id: "link", label: "Link PR to ticket" },
    { id: "status", label: "Move ticket" },
    { id: "timer", label: "Start timer" },
  ] as const;

  function flowStep(id: (typeof ROWS)[number]["id"]): StartWorkStep | null {
    return id === "branch" ? branch : id === "pr" ? pr : id === "link" ? link : id === "status" ? move : null;
  }
  function stateOf(id: (typeof ROWS)[number]["id"]): string {
    if (id === "timer") return timerDone ? "done" : timerSkipped ? "skipped" : "ready";
    const of = flowStep(id);
    return of !== null && of.id === marked ? "skipped" : word(of);
  }
</script>

<p class="tf-dim tf-swnote">Check the names, then run the steps. Each write waits in the queue until it reaches the source.</p>
{#if flow.error}<p class="tf-bad tf-small" role="alert">{flow.error}</p>{/if}
<div class="tf-steps" role="group" aria-label="Start work" tabindex="-1" {@attach (node: HTMLElement) => { if (untrack(() => arrive)) node.focus({ preventScroll: true }); }}>
  {#each ROWS as row, index (row.id)}
    {@const of = flowStep(row.id)}
    {@const state = stateOf(row.id)}
    <div class="tf-step tf-st-{state}" data-step={row.id}>
      <span class="tf-num">{index + 1}</span>
      <strong>{row.label}</strong>
      <span class="tf-state {state === 'ready' ? 'tf-dim' : ''}">{stateWord(state)}</span>
      <div class="tf-field">
        {#if row.id === "branch"}
          <input aria-label={row.label} value={text(op(of)["name"])} disabled={fixed(of)} onchange={(event) => amend(of, "name", event.currentTarget.value)} />
          <select aria-label="Repository" value={repoOf} disabled={fixed(of)} onchange={(event) => void pick(event.currentTarget.value)}>
            {#each options as repo, at (repo.entity_id)}
              <option value={repo.entity_id}>{repo.title}{at === 0 ? " (suggested)" : ""}</option>
            {/each}
          </select>
          <span class="tf-dim tf-small">from {text(op(of)["from_ref"]) || "main"}</span>
        {:else if row.id === "pr"}
          <input aria-label={row.label} value={text(op(of)["title"])} disabled={fixed(of)} onchange={(event) => amend(of, "title", event.currentTarget.value)} />
          <span class="tf-dim tf-small">draft, to {text(op(of)["base"]) || "main"}</span>
        {:else if row.id === "link"}
          <span class="tf-stepval">{text((of?.payload as Record<string, unknown> | undefined)?.['relation']) || 'implements'} <TfKey value={key} /></span>
          <span class="tf-dim tf-small">stays in knobas</span>
        {:else if row.id === "status"}
          <input aria-label={row.label} value="{stateWord(detail.status ?? 'To Do')} → {stateWord(text(op(of)['status']) || 'In Progress')}" readonly {...refusedAttrs(moveRefused)} />
          <span class="tf-dim tf-small">{detail.source.display_name} transition</span>
        {:else}
          <span class="tf-stepval"><TfKey value={key} /></span>
          <span class="tf-dim tf-small">{otherTimer ? `stops the timer on ${otherTimer} first` : ""}</span>
        {/if}
        {#if of && current?.id === of.id && (of.outcome === "failed" || of.outcome === "queued")}
          <!-- While a queued step's write is open, the button re-reads it and
               sends it nothing more (`ready_retry`'s rule 1), so it is not
               *Retry* (#1998). A write discarded or refused since settles the
               step failed and sends nothing; the answer redraws it with
               *Retry* (#2015). -->
          <button class="tf-btn tf-sm" disabled={flow.busy} onclick={() => void flow.retry(of.id)}>{of.outcome === "queued" ? "Check again" : "Retry"}</button>
        {/if}
        {#if of && fresh && current?.id === of.id}
          <!-- One button whose word changes, so a keyboard's place stays on it. -->
          <button
            class="tf-btn tf-sm tf-ghost"
            type="button"
            disabled={flow.busy}
            {...refusedAttrs(moveRefused === null ? null : ONE_SITS_OUT)}
            onclick={() => {
              if (moveRefused === null) markedId = marked === of.id ? null : of.id;
            }}>{marked === of.id ? "Include" : "Skip"}</button
          >
        {:else if of && current?.id === of.id && of.outcome !== "running"}
          <button class="tf-btn tf-sm tf-ghost" disabled={flow.busy} onclick={() => void flow.skip(of.id)}>Skip</button>
        {:else if row.id === "timer" && !timerDone && (fresh || !timerSkipped)}
          <button class="tf-btn tf-sm tf-ghost" type="button" onclick={() => (timerSkipped = !timerSkipped)}>{timerSkipped ? "Include" : "Skip"}</button>
        {/if}
      </div>
      {#if of?.detail}<p class="tf-swdetail tf-small">{of.detail}</p>{/if}
      {#if fresh && moveRefused !== null && of !== null && current?.id === of.id}<p class="tf-swdetail tf-small">{ONE_SITS_OUT}</p>{/if}
      <!-- A skipped refused move keeps its reason: *Run all steps* skipped it, not the reader. -->
      {#if row.id === "status" && moveRefused !== null && of?.outcome !== "succeeded"}<p class="tf-swdetail tf-small">{moveRefused}</p>{/if}
    </div>
  {/each}
  <footer>
    <span class="tf-dim tf-small tf-swhint">A failed step can be retried, skipped or amended.</span>
    {#if fresh && oncancel}
      <button class="tf-btn tf-ghost" type="button" onclick={oncancel}>Cancel</button>
    {:else if !fresh}
      <button class="tf-btn tf-ghost" type="button" onclick={openWriteQueue}>Show queue</button>
    {/if}
    <!-- `aria-disabled`, so a keyboard's place stays on it as it changes its words after the press. -->
    <button class="tf-btn tf-primary" class:tf-lg={fresh} type="button" aria-disabled={run.can ? undefined : "true"} onclick={() => void runAll()}>{run.word}</button>
  </footer>
</div>
