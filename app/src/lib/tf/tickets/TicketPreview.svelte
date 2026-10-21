<!--
  The selected ticket's preview (#810, R-3.9): key, title, the facts,
  the description, *Around this ticket* one line a section, and one primary
  button whose label follows the ticket's state. The button opens the job's
  address; what *Start work* does there is #818's.
-->
<script lang="ts">
  import type { MiniBoardCard } from "../../ipc/entity";
  import { jobHash } from "../route";
  import { columnName, epicLine, previewAction, when, type AroundLine } from "./tickets";

  let {
    card,
    description,
    around,
    timerOnIt,
  }: {
    /** The ticket; its status is its own, whatever column it stands in (#1423). */
    card: MiniBoardCard;
    /** The description, or `null` while it is read. */
    description: string | null;
    /** *Around this ticket*, or `null` while it is read. */
    around: AroundLine[] | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
  } = $props();

  const action = $derived(previewAction(card, timerOnIt));
  const epic = $derived(epicLine(card));
</script>

<aside class="tf-preview" aria-label="Selected ticket">
  <div>
    <span class="tf-k">{card.key}</span>
    <h2>{card.title}</h2>
  </div>
  <dl class="tf-facts">
    <dt>Status</dt>
    <dd>{columnName(card.status)}</dd>
    <dt>Assignee</dt>
    <dd>{card.assignee ?? "Unassigned"}</dd>
    <dt>Priority</dt>
    <dd>{card.priority ?? "None"}</dd>
    {#if epic}<dt>Epic</dt>
      <dd>{epic}</dd>{/if}
    <dt>Updated</dt>
    <dd>{when(card.updated_at)}</dd>
  </dl>
  {#if description}<p class="tf-desc">{description}</p>{/if}
  <div>
    <div class="tf-pane-h">Around this ticket</div>
    {#if around !== null && around.length > 0}
      <div class="tf-rollup">
        {#each around as line (line.section)}
          <div>
            <b>{line.section}</b><span
              >{line.count}{#if line.failing}, <span class="tf-bad">{line.failing}</span>{/if}</span
            >
          </div>
        {/each}
      </div>
    {:else if around !== null}
      <p class="tf-dim tf-small">Nothing connected yet.</p>
    {/if}
  </div>
  <div class="tf-go">
    <a class="tf-btn tf-primary" href={jobHash(card.key)}>{action.label}</a>
    <small>{action.note} <kbd>Enter</kbd></small>
  </div>
</aside>
