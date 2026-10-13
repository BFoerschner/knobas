<!--
  The builds knobas mirrors for the commit this entity stands at (#710): a
  pull request's head commit, a branch's tip, a commit itself.

  **A panel beside the links, not a block in the source's Overview.** These
  rows are a mirror fact (`get_entity`'s `builds`, read through the declared
  `commit` and `revision` paths), not the source's view: they are drawn
  whether or not the live view answered, on every tab, and the source's own
  section stays the source's (ADR-0020). Gitea's own commit statuses are the
  view's *Checks*; these are knobas' TeamCity builds of the same commit.

  Quiet: absent when nothing ran on the commit, one line per build in the
  order the read gave (failing first, then running, then the rest), and the
  only colour the state chips' and a failing count's.
-->
<script lang="ts">
  import type { EntityRow } from "../ipc/entity";
  import EntityLine from "../shell/EntityLine.svelte";
  import { hashFor } from "../shell/router.svelte";

  let {
    builds,
    onopen,
  }: {
    builds: EntityRow[];
    /** Navigate to an address (spec §2). */
    onopen: (hash: string) => void;
  } = $props();

  const failing = $derived(builds.filter((row) => row.state?.tone === "err").length);

  function open(row: EntityRow) {
    onopen(hashFor({ view: "room", ctx: "all", detail: { kind: row.kind, entityId: row.entity_id } }));
  }
</script>

{#if builds.length > 0}
  <div class="sec">
    <div class="sec-h">
      <span class="lab">Builds</span>
      <span class="k muted">{builds.length}</span>
      {#if failing > 0}
        <span class="k fail">{failing} failing</span>
      {/if}
    </div>
    {#each builds as row (row.entity_id)}
      <EntityLine {row} onopen={open} showSource={false} />
    {/each}
  </div>
{/if}
