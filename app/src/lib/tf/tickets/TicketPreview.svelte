<!--
  The selected ticket's preview (#810, R-3.9): key, title, the facts,
  the description, *Around this ticket* one line a section, and one primary
  button whose label follows the ticket's state. The button opens the job's
  address; what *Start work* does there is #818's.
-->
<script lang="ts">
  import type { MiniBoardCard } from "../../ipc/entity";
  import { jobHash } from "../route";
  import { columnName, previewAction, when, type AroundLine } from "./tickets";

  let {
    card,
    status,
    description,
    around,
    timerOnIt,
  }: {
    card: MiniBoardCard;
    /** The ticket's status, the column it stands in. */
    status: string | null;
    /** The description, or `null` while it is read. */
    description: string | null;
    /** *Around this ticket*, or `null` while it is read. */
    around: AroundLine[] | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
  } = $props();

  const action = $derived(previewAction(card, status, timerOnIt));
</script>

<aside class="tf-preview" aria-label="Selected ticket">
  <div>
    <span class="tf-k">{card.key}</span>
    <h2>{card.title}</h2>
  </div>
  <dl class="tf-facts">
    <dt>Status</dt>
    <dd>{columnName(status)}</dd>
    <dt>Assignee</dt>
    <dd>{card.assignee ?? "Unassigned"}</dd>
    {#if card.priority}<dt>Priority</dt>
      <dd>{card.priority}</dd>{/if}
    {#if card.path}<dt>Epic</dt>
      <dd>{card.path}</dd>{/if}
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
