<!--
  What ⌘T opens when there is nothing in front of the reader (#278, story 9).

  Two ways to name what the time is on, and they are deliberately different in
  kind:

  * **an ad-hoc label** — free text, first in the dialog and focused on open,
    because this is the case the picker exists for: "DB config for the
    migration" is as legal a target as a ticket, and a person who pressed ⌘T
    from an empty room usually has a sentence rather than an id;
  * **something recent** — the entities knobas last saw, so a reader who *does*
    have a ticket in mind does not have to retype its key.

  ## A stored context is refused, not merely absent

  Recents come out of `knobas.entity`, and a stored context **is** a row there
  — kind `ctx`, id `ctx:<uuid>` (`knobas_core::context`). So it arrives in this
  list like anything else, and the list is filtered through
  `timer.ts`'s `canBeTarget` rather than happening not to contain one. The
  reason is the glossary's: a context is a set, and time on a set has nowhere
  to go — the label field above is what covers "worked across the SEPA
  context". `start_timer` refuses one too; this is what keeps the reader from
  being offered a row that can only fail.
-->
<script lang="ts">
  import { launcherHome as realLauncherHome } from "../ipc";
  import type { EntityRow } from "../ipc/entity";
  import type { TimerTarget } from "../ipc/time";
  import Modal from "./Modal.svelte";
  import { candidateOf, legalCandidates, type TargetCandidate } from "./timer";

  let {
    onpick,
    onclose,
    recent,
  }: {
    /** The target the reader chose. Starting it is the shell's. */
    onpick: (target: TimerTarget) => void;
    onclose: () => void;
    /**
     * The recents the list is drawn from, injectable so a test needs no Tauri
     * bridge. Production omits it and the launcher's own board read is used —
     * the same list ⌘K shows, so the two surfaces cannot disagree about what
     * "recent" means.
     */
    recent?: () => Promise<EntityRow[]>;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init, on purpose: production omits this prop and nothing
  // changes it, and a bridge swapped mid-life would re-run the read below for
  // a dialog the reader is already choosing from.
  const read = recent ?? (() => realLauncherHome().then((home) => home.recent));

  /** Unique per instance, so two dialogs cannot share a label's `for`. */
  const labelId = `timer-label-${Math.random().toString(36).slice(2, 9)}`;

  let label = $state("");
  let candidates = $state<TargetCandidate[]>([]);
  /** What went wrong reading the recents, if anything. The label still works. */
  let failed = $state(false);

  $effect(() => {
    let live = true;
    void read()
      .then((rows) => {
        if (!live) return;
        // Filtered, not hoped over. See the header.
        candidates = legalCandidates(rows.map(candidateOf));
      })
      .catch(() => {
        if (live) failed = true;
      });
    return () => {
      live = false;
    };
  });

  /** The label as a target, or `null` while it is blank. */
  const labelTarget = $derived(
    label.trim() === "" ? null : ({ kind: "label", label: label.trim() } as const),
  );

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (labelTarget) onpick(labelTarget);
  }
</script>

<Modal title="What is the time on?" subtitle="⌘T" center {onclose}>
  {#snippet body()}
    <form class="pick" onsubmit={submit}>
      <div class="fld">
        <label class="l" for={labelId}>A label</label>
        <input
          class="inp"
          id={labelId}
          type="text"
          autocomplete="off"
          bind:value={label}
          placeholder="DB config for the migration"
        />
      </div>
      <button class="btn pri" type="submit" disabled={labelTarget === null}>Start</button>
    </form>

    {#if candidates.length > 0}
      <p class="lab recent-head">…or something recent</p>
      <ul class="recent">
        {#each candidates as candidate (candidate.entityId)}
          <li>
            <button
              class="row"
              type="button"
              onclick={() => onpick({ kind: "entity", entity_id: candidate.entityId })}
            >
              <span class="t">{candidate.title}</span>
              <span class="k">{candidate.entityId}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if failed}
      <!--
        Said rather than swallowed: the label field above still works, and a
        reader who expected their recents deserves to know why they are not
        there instead of concluding knobas has forgotten them.
      -->
      <p class="lab recent-head">Recent items could not be read. A label still works.</p>
    {/if}
  {/snippet}
</Modal>

<style>
  .pick {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 8px;
  }

  .fld .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-bottom: 4px;
  }

  .recent-head {
    margin: 14px 0 6px;
  }

  .recent {
    display: grid;
    gap: 2px;
  }

  .recent .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    column-gap: 8px;
    width: 100%;
    height: var(--row);
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 2px;
    text-align: left;
    background: transparent;
    color: var(--text);
  }

  .recent .row:hover {
    background: var(--raised);
    border-color: var(--hair2);
  }

  .recent .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recent .k {
    font: 500 11px var(--mono);
    color: var(--faint);
  }
</style>
