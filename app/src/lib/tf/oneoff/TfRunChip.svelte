<!--
  A run still going after its dialog closed (ADR-0060, section 18): the status bar says it, *ESLint 0:42* behind a
  pulsing dot, and a press opens it again on what it printed meanwhile. Several are counted.
-->
<script lang="ts">
  import "../css/oneoff.css";
  import { clock } from "../../render/oneoff";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";

  let { store = sharedStore }: { store?: OneOffs } = $props();

  const running = $derived(store.running.filter((r) => r.id !== store.open?.id));
  let now = $state(Date.now());
  $effect(() => {
    if (running.length === 0) return;
    const tick = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(tick);
  });
</script>

{#if running.length === 1}
  {@const run = running[0]!}
  <button type="button" class="tf-oo-chip" title="{run.name} is still running: open it" onclick={() => void store.reopen(run.id)}
    ><span class="tf-dot tf-run" aria-hidden="true"></span>{run.name} {clock(now - run.started_at)}</button
  >
{:else if running.length > 1}
  <button type="button" class="tf-oo-chip" title={running.map((r) => r.name).join(", ")} onclick={() => void store.reopen(running[0]!.id)}
    ><span class="tf-dot tf-run" aria-hidden="true"></span>{running.length} commands running</button
  >
{/if}
