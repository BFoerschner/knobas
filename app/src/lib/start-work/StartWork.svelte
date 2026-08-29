<!--
  The start-work stepper — `#/start-work/<ticket>` (issue #44).

  **The whole sequence is shown before anything happens.** That is the shape
  this view exists to hold: knobas proposes a branch name, a pull request title
  and body, and a target status; the reader edits any of them; and only then is
  there a *Run* button. A view that dispatched on open would be the macro this
  is explicitly not.

  Three rules a later edit must not quietly relax:

  * **Every step's outcome is on screen, always.** A step that fails says what
    the source said, in the source's own words, and the steps after it stay
    visibly untouched — that is story 16, "see what the flow already did when it
    stopped partway".
  * **Retry and Skip are per step, and only on the step the flow stopped at.**
    Offering them on every row would let the reader run the transition over a
    branch that was never created; the backend refuses that, and this does not
    ask.
  * **`queued` is drawn as neither success nor failure.** The write is with the
    source's queue and will go. There is nothing to retry and nothing has
    happened yet.
-->
<script lang="ts">
  import { getEntity, type EntityDetail } from "../ipc/entity";
  import { listEntities } from "../ipc/entity";
  import {
    createStartWork,
    demandOf,
    edited,
    fieldsOf,
    labelOf,
    type Field,
  } from "./start-work.svelte";
  import type { StartWorkStep } from "../ipc/entity";

  let {
    entityId,
    onnavigate,
    onclose,
  }: {
    /** The ticket the flow is about. */
    entityId: string;
    /** The shell owns navigation; this hands an intention back. */
    onnavigate: (hash: string) => void;
    onclose: () => void;
  } = $props();

  // Built once, from the address this view was opened at. `App.svelte` keys
  // this component on the ticket, so a second ticket is a second component
  // rather than this one being re-pointed -- which is what makes a flow's state
  // and its address the same thing.
  // svelte-ignore state_referenced_locally
  const flow = createStartWork(entityId);

  /** The ticket, for the heading. `null` while the read is in flight. */
  let ticket = $state<EntityDetail | null>(null);
  /** The repositories the reader may choose between (story 5). */
  let repos = $state<{ entity_id: string; title: string }[]>([]);
  let chosen = $state<string>("");
  /** The step whose proposal is open for editing. One at a time. */
  let editing = $state<number | null>(null);
  let draft = $state<Record<string, string>>({});

  $effect(() => {
    const id = entityId;
    void (async () => {
      try {
        ticket = await getEntity(id);
      } catch {
        // The heading degrades to the id, which is still an address the reader
        // recognises. A flow is not blocked by its title being unreadable.
        ticket = null;
      }
    })();
    void flow.load(null);
  });

  /**
   * The repositories, read once and only when there is no flow yet.
   *
   * A ticket has no repository anywhere in the data model — nothing in Jira
   * says which repo its work belongs in — so the reader picks, which is story 5
   * exactly rather than a gap in the proposal.
   */
  $effect(() => {
    if (flow.steps.length > 0 || repos.length > 0) return;
    void (async () => {
      try {
        const page = await listEntities(
          {
            sources: [],
            kinds: ["repo"],
            updated_within_days: null,
            context: null,
            order: "title_asc",
            include_deleted: false,
          },
          200,
          0,
        );
        repos = page.rows.map((row) => ({ entity_id: row.entity_id, title: row.title }));
        chosen = repos[0]?.entity_id ?? "";
      } catch {
        repos = [];
      }
    })();
  });

  /** The step the sequence is on — the only one that may be acted upon. */
  const current = $derived(flow.steps.find((step) => demandOf(step.outcome) !== "done") ?? null);
  const done = $derived(flow.steps.length > 0 && current === null);
  /**
   * Whether the ticket actually moved. The footer may only claim "the ticket
   * is In Progress" when the transition step *succeeded* -- a flow whose
   * transition was skipped is finished too, and saying the ticket moved would
   * be a false record of what happened.
   */
  const ticketMoved = $derived(
    flow.steps.some((step) => step.step === "transition" && step.outcome === "succeeded"),
  );
  /** Nothing has run yet, so the whole sequence is still a proposal. */
  const unstarted = $derived(
    flow.steps.length > 0 && flow.steps.every((step) => step.outcome === "pending"),
  );

  function openEditor(step: StartWorkStep, fields: Field[]) {
    editing = step.id;
    draft = Object.fromEntries(fields.map((field) => [field.key, field.value]));
  }

  async function save(step: StartWorkStep, fields: Field[]) {
    let payload = step.payload;
    for (const field of fields) {
      const value = draft[field.key];
      if (value !== undefined && value !== field.value) {
        payload = edited({ ...step, payload }, field.key, value);
      }
    }
    await flow.amend(step.id, payload);
    editing = null;
  }

  /** What a step's outcome says, in the reader's words rather than the wire's. */
  function says(step: StartWorkStep): string {
    switch (step.outcome) {
      case "pending":
        return "not started";
      case "running":
        return "running";
      case "succeeded":
        return "done";
      case "queued":
        return "queued — waiting for the source, it will go on its own";
      case "failed":
        return "stopped";
      case "skipped":
        return "skipped";
    }
  }
</script>

<section class="sw" aria-labelledby="sw-title">
  <header class="sw-head">
    <div>
      <h1 id="sw-title" class="sw-title">Start work on {ticket?.row.title ?? entityId}</h1>
      <p class="sw-sub">
        <button class="linkish" onclick={() => onnavigate(`#/entity/${entityId}`)}>
          {entityId}
        </button>
        · four steps across three systems, shown before any of them runs
      </p>
    </div>
    <button class="btn" onclick={onclose}>Back to the room</button>
  </header>

  {#if flow.error}
    <p class="sw-error" role="alert">{flow.error}</p>
  {/if}

  {#if !flow.loaded}
    <p class="sw-note">Reading…</p>
  {:else if flow.steps.length === 0}
    <div class="sw-pick">
      <p class="sw-note">
        Nothing has been proposed yet. Choose the repository the branch and the pull request go
        in — a ticket does not say which, so this is yours to pick.
      </p>
      {#if repos.length === 0}
        <p class="sw-note">
          No repository has been mirrored yet. Add a source that syncs repositories, then come
          back.
        </p>
      {:else}
        <label class="sw-field">
          <span class="sw-label" id="sw-repo-label">Repository</span>
          <select aria-labelledby="sw-repo-label" bind:value={chosen}>
            {#each repos as repo (repo.entity_id)}
              <option value={repo.entity_id}>{repo.title}</option>
            {/each}
          </select>
        </label>
        <button
          class="btn pri"
          disabled={flow.busy || chosen === ""}
          onclick={() => flow.load(chosen)}
        >
          Propose the sequence
        </button>
      {/if}
    </div>
  {:else}
    <ol class="sw-steps">
      {#each flow.steps as step (step.id)}
        {@const fields = fieldsOf(step)}
        {@const demand = demandOf(step.outcome)}
        {@const isCurrent = current?.id === step.id}
        <li class="sw-step {step.outcome}" aria-current={isCurrent ? "step" : undefined}>
          <header class="sw-step-h">
            <b class="sw-step-name">{labelOf(step.step)}</b>
            <span class="sw-state {demand}">{says(step)}</span>
          </header>

          {#if step.detail}
            <p class="sw-detail">{step.detail}</p>
          {/if}

          {#if editing === step.id}
            {#each fields as field (field.key)}
              <label class="sw-field">
                <span class="sw-label" id="sw-f-{step.id}-{field.key}">{field.label}</span>
                {#if field.long}
                  <textarea
                    rows="4"
                    aria-labelledby="sw-f-{step.id}-{field.key}"
                    bind:value={draft[field.key]}
                  ></textarea>
                {:else}
                  <input
                    type="text"
                    aria-labelledby="sw-f-{step.id}-{field.key}"
                    bind:value={draft[field.key]}
                  />
                {/if}
              </label>
            {/each}
            <div class="sw-acts">
              <button class="btn pri" disabled={flow.busy} onclick={() => save(step, fields)}>
                Save
              </button>
              <button class="btn" onclick={() => (editing = null)}>Cancel</button>
            </div>
          {:else}
            {#if fields.length > 0}
              <dl class="sw-props">
                {#each fields as field (field.key)}
                  <dt>{field.label}</dt>
                  <dd class="mono">{field.value}</dd>
                {/each}
              </dl>
            {/if}
            <div class="sw-acts">
              {#if step.outcome !== "succeeded" && fields.length > 0}
                <button class="btn" disabled={flow.busy} onclick={() => openEditor(step, fields)}>
                  Edit
                </button>
              {/if}
              <!--
                Retry and Skip only on the step the sequence is on. A stepper
                that offered them everywhere would invite running a later step
                over an earlier failure, which is the outcome the whole sequence
                exists to prevent.
              -->
              {#if isCurrent && step.outcome === "failed"}
                <button class="btn pri" disabled={flow.busy} onclick={() => flow.retry(step.id)}>
                  Retry this step
                </button>
              {/if}
              {#if isCurrent && demand !== "done" && step.outcome !== "running"}
                <button class="btn" disabled={flow.busy} onclick={() => flow.skip(step.id)}>
                  Skip
                </button>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ol>

    <footer class="sw-foot">
      {#if done}
        <p class="sw-note">
          {ticketMoved
            ? "Every step is finished. The ticket is In Progress."
            : "Every step is settled. The ticket was not moved."}
        </p>
      {:else if unstarted}
        <button class="btn pri" disabled={flow.busy} onclick={() => flow.run()}>
          Run the sequence
        </button>
        <span class="sw-note">Nothing has happened yet. Edit anything above first.</span>
      {:else}
        <button class="btn pri" disabled={flow.busy} onclick={() => flow.run()}>
          Continue
        </button>
        <span class="sw-note">
          Picks up at the step the flow is on. The steps that already succeeded are not run
          again.
        </span>
      {/if}
    </footer>
  {/if}
</section>

<style>
  .sw {
    padding: 20px 24px;
    max-width: 760px;
  }

  .sw-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  .sw-title {
    margin: 0;
    font: 500 18px var(--disp);
    color: var(--text);
  }

  .sw-sub {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 12px;
  }

  .linkish {
    padding: 0;
    border: 0;
    background: none;
    color: var(--link);
    font: inherit;
    cursor: pointer;
  }

  .sw-error {
    margin-bottom: 12px;
    color: var(--fail);
    font-size: 12px;
  }

  .sw-note {
    margin: 4px 0;
    color: var(--muted);
    font-size: 11.5px;
  }

  .sw-pick {
    padding: 14px 16px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
  }

  .sw-steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /*
    One left edge per outcome. A step that is *queued* and one that *failed* are
    not the same thing waiting different lengths of time -- the first needs the
    network and the second needs the reader -- so they never share an edge.
  */
  .sw-step {
    padding: 12px 14px;
    margin-bottom: 8px;
    border: 1px solid var(--hair);
    border-left-width: 3px;
    border-left-color: var(--hair2);
    border-radius: 2px;
    background: var(--panel);
  }

  .sw-step.succeeded {
    border-left-color: var(--ok);
  }

  .sw-step.failed {
    border-left-color: var(--fail);
  }

  .sw-step.queued,
  .sw-step.running {
    border-left-color: var(--amber);
  }

  .sw-step-h {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .sw-step-name {
    font: 500 13px var(--sans);
    color: var(--text);
  }

  .sw-state {
    font-size: 11.5px;
    color: var(--muted);
  }

  .sw-state.decide {
    color: var(--fail);
  }

  .sw-state.waiting,
  .sw-state.running {
    color: var(--amber);
  }

  .sw-state.done {
    color: var(--ok);
  }

  .sw-detail {
    margin: 6px 0 0;
    color: var(--text);
    font-size: 12px;
  }

  .sw-props {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 12px;
    margin: 8px 0 0;
    font-size: 11.5px;
  }

  .sw-props dt {
    color: var(--muted);
  }

  .sw-props dd {
    margin: 0;
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .mono {
    font-family: var(--mono);
  }

  .sw-field {
    display: block;
    margin: 8px 0;
  }

  .sw-label {
    display: block;
    margin-bottom: 3px;
    color: var(--muted);
    font-size: 11.5px;
  }

  .sw-field input,
  .sw-field textarea,
  .sw-pick select {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--bg);
    color: var(--text);
    font: 12px var(--mono);
  }

  .sw-acts {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }

  .sw-foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 16px;
  }
</style>
