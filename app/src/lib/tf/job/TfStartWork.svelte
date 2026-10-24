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
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  import type { EntityDetail, StartWorkStep } from "../../ipc/entity";
  import type { Timer } from "../../shell/timer.svelte";
  import { toastStarted } from "../../shell/queued-toast";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { createStartWork, demandOf, editable, edited, type StartWorkApi } from "../../start-work/start-work.svelte";
  import { keyOf } from "./job";

  let {
    detail,
    repos,
    suggested,
    timer,
    api,
    onstarttimer,
    moveRefused = null,
  }: {
    detail: EntityDetail;
    /** Every repository, `null` while read: the dropdown's options. */
    repos: { entity_id: string; title: string }[] | null;
    /** The repository to suggest first: one the ticket already reaches. */
    suggested: string | null;
    timer: Pick<Timer, "current">;
    api: StartWorkApi;
    onstarttimer: (entityId: string, title: string) => void;
    /** Why the source's credential cannot move this ticket (#2052), or `null`. */
    moveRefused?: string | null;
  } = $props();

  // svelte-ignore state_referenced_locally
  const flow = createStartWork(detail.row.entity_id, api);
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

  async function runAll() {
    // A move the credential cannot make is skipped, never queued; the skip runs the flow on past it.
    if (moveRefused !== null && move !== null && editable(move.outcome)) await flow.skip(move.id);
    else await flow.run();
    if (flow.error === null) {
      toastStarted(key, flow.steps.filter((each) => each.outcome === "queued").length, detail.source);
    }
    if (flow.error === null && !timerDone && !timerSkipped) {
      onstarttimer(detail.row.entity_id, detail.row.title);
    }
  }

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
    return word(flowStep(id));
  }
</script>

<p class="tf-dim tf-swnote">Check the names, then run the steps. Each write waits in the queue until it reaches the source.</p>
{#if flow.error}<p class="tf-bad tf-small" role="alert">{flow.error}</p>{/if}
<div class="tf-steps" aria-label="Start work">
  {#each ROWS as row, index (row.id)}
    {@const of = flowStep(row.id)}
    {@const state = stateOf(row.id)}
    <div class="tf-step tf-st-{state}" data-step={row.id}>
      <span class="tf-num">{index + 1}</span>
      <strong>{row.label}</strong>
      <span class="tf-state {state === 'ready' ? 'tf-dim' : ''}">{state}</span>
      <div class="tf-field">
        {#if row.id === "branch"}
          <input aria-label={row.label} value={text(op(of)["name"])} disabled={!of || !editable(of.outcome)} onchange={(event) => amend(of, "name", event.currentTarget.value)} />
          <select aria-label="Repository" value={repoOf} disabled={!of || !editable(of.outcome)} onchange={(event) => void pick(event.currentTarget.value)}>
            {#each options as repo, at (repo.entity_id)}
              <option value={repo.entity_id}>{repo.title}{at === 0 ? " (suggested)" : ""}</option>
            {/each}
          </select>
          <span class="tf-dim tf-small">from {text(op(of)["from_ref"]) || "main"}</span>
        {:else if row.id === "pr"}
          <input aria-label={row.label} value={text(op(of)["title"])} disabled={!of || !editable(of.outcome)} onchange={(event) => amend(of, "title", event.currentTarget.value)} />
          <span class="tf-dim tf-small">draft, to {text(op(of)["base"]) || "main"}</span>
        {:else if row.id === "link"}
          <input aria-label={row.label} value="{text((of?.payload as Record<string, unknown> | undefined)?.['relation']) || 'implements'} {key}" readonly />
          <span class="tf-dim tf-small">stays in knobas</span>
        {:else if row.id === "status"}
          <input aria-label={row.label} value="{detail.status ?? 'To Do'} to {text(op(of)['status']) || 'In Progress'}" readonly {...refusedAttrs(moveRefused)} />
          <span class="tf-dim tf-small">{detail.source.display_name} transition</span>
        {:else}
          <input aria-label={row.label} value={key} readonly />
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
        {#if of && current?.id === of.id && of.outcome !== "running"}
          <button class="tf-btn tf-sm tf-ghost" disabled={flow.busy} onclick={() => void flow.skip(of.id)}>Skip</button>
        {:else if row.id === "timer" && !timerDone && !timerSkipped}
          <button class="tf-btn tf-sm tf-ghost" onclick={() => (timerSkipped = true)}>Skip</button>
        {/if}
      </div>
      {#if of?.detail}<p class="tf-swdetail tf-small">{of.detail}</p>{/if}
      <!-- A skipped refused move keeps its reason: *Run all steps* skipped it, not the reader. -->
      {#if row.id === "status" && moveRefused !== null && of?.outcome !== "succeeded"}<p class="tf-swdetail tf-small">{moveRefused}</p>{/if}
    </div>
  {/each}
  <footer>
    <button class="tf-btn tf-primary" disabled={flow.busy || flow.steps.length === 0} onclick={() => void runAll()}>Run all steps</button>
    <span class="tf-dim tf-small">A failed step can be retried, skipped or amended.</span>
  </footer>
</div>
