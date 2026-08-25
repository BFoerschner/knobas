<!--
  This item's own history — spec §12.1, *"change history per asset: every
  mutation appends a line"*, over §2a's activity log as its store.

  The lines arrive with the entity (`get_entity` fetches them), so this panel
  does no reading of its own. `detail` is free-form jsonb, so it goes through
  the same §3a projection the payload does — as text, behind a disclosure.
-->
<script lang="ts">
  import type { ActivityRow } from "../ipc/entity";
  import Monogram from "../shell/Monogram.svelte";
  import { ago } from "../shell/time";
  import PayloadView from "./PayloadView.svelte";
  import { parseActor } from "./actor";
  import { projectPayload } from "./payload";

  let { activity }: { activity: ActivityRow[] } = $props();

  /** Whether a line carries anything worth opening. */
  function hasDetail(row: ActivityRow): boolean {
    return projectPayload(row.detail).length > 0;
  }
</script>

<div class="sec">
  <div class="sec-h">
    <span class="lab">History</span>
    <span class="k muted">{activity.length}</span>
  </div>

  {#if activity.length === 0}
    <div class="empty">No recorded activity for this item yet.</div>
  {:else}
    {#each activity as line (line.id)}
      <div class="row g4 hist">
        {#key line.id}
          {@const actor = parseActor(line.actor)}
          <Monogram text={actor.monogram} label={actor.label} />
          <span class="t">
            <!-- The verb is a stored string. Text, like everything else here. -->
            <b>{line.verb}</b>
            <span class="sub">{actor.label}</span>
          </span>
        {/key}
        <span class="r"></span>
        <span class="r">{ago(line.at)}</span>
      </div>
      {#if hasDetail(line)}
        <details class="hist-d">
          <summary><span class="k">what changed</span></summary>
          <PayloadView fields={projectPayload(line.detail)} />
        </details>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .hist b {
    font-weight: 500;
  }

  .hist .sub {
    margin-left: 8px;
  }

  .hist-d {
    padding: 4px 0 8px;
  }

  .hist-d summary {
    cursor: pointer;
  }
</style>
