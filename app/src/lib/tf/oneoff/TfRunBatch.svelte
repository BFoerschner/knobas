<!--
  A batch (ADR-0060, section 12): a file's command on many files, one run a file. On the left a row a file with how it
  ended, failures first once they end; on the right the picked file's output, its places linked. `↑` and `↓` step.
-->
<script lang="ts">
  import type { OneOffPlace } from "../../ipc/oneoff";
  import { endedShort, stateOf, took } from "../../render/oneoff";
  import type { Run } from "../../render/oneoff.svelte";
  import TfRunLines from "./TfRunLines.svelte";

  let { run, onplace, onurl }: { run: Run; onplace: (place: OneOffPlace, wide: boolean) => void; onurl: (href: string) => void } = $props();

  /** The most rows drawn; a batch holds at most 200. */
  const ROWS = 200;
  const items = $derived.by(() => {
    void run.version;
    const all = (run.batch ?? []).map((item, at) => ({ item, at }));
    const bad = (e: typeof all[number]) => e.item.ended !== null && !(e.item.ended.how === "exit" && e.item.ended.code === 0);
    return [...all.filter(bad), ...all.filter((e) => !bad(e))];
  });
  const picked = $derived.by(() => {
    void run.version;
    return run.batch?.[run.batchAt] ?? null;
  });
  const tone = (e: Parameters<typeof stateOf>[0]) => {
    const t = stateOf(e).tone;
    return t === "muted" ? "" : t === "info" ? "tf-run" : `tf-${t}`;
  };

  function onkeydown(event: KeyboardEvent) {
    const order = items.map((e) => e.at);
    const now = order.indexOf(run.batchAt);
    if (event.key === "ArrowDown") {
      event.preventDefault();
      run.batchAt = order[Math.min(order.length - 1, now + 1)] ?? run.batchAt;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      run.batchAt = order[Math.max(0, now - 1)] ?? run.batchAt;
    }
  }
</script>

<div class="tf-oo-batch">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex: the list takes focus so the arrows step through it -->
  <div class="tf-oo-flist" tabindex="0" role="listbox" aria-label="The files it ran on" {onkeydown}>
    {#each items.slice(0, ROWS) as { item, at } (item.path)}
      <button type="button" class="tf-oo-brow" role="option" aria-selected={at === run.batchAt} aria-current={at === run.batchAt} title={item.path} onclick={() => (run.batchAt = at)}
        ><span class="tf-oo-fname">{item.path.slice(item.path.lastIndexOf("/") + 1)}</span><span class="tf-oo-bend {tone(item.ended)}"
          ><span class="tf-dot {tone(item.ended)}"></span>{item.run === null && item.ended === null ? "waiting" : endedShort(item.ended)}</span
        ><span class="tf-oo-fsize">{item.ended !== null && item.run !== null && item.run.elapsed > 0 ? took(item.run.elapsed) : ""}</span></button
      >
    {/each}
  </div>
  <div class="tf-oo-fone">
    {#if picked !== null}
      <div class="tf-oo-fhead"><b>{picked.path}</b><span class="tf-dim">{picked.ended === null ? (picked.run === null ? "waiting for its turn" : "running") : picked.ended.how === "not_started" ? picked.ended.reason : endedShort(picked.ended)}</span></div>
      <div class="tf-oo-fbody">
        {#if picked.run !== null && picked.run.lines.length > 0}
          {#key picked.path}<TfRunLines run={picked.run} {onplace} {onurl} />{/key}
        {:else}
          <div class="tf-oo-quiet"><p>{picked.ended === null ? "Nothing printed yet." : "It printed nothing."}</p></div>
        {/if}
      </div>
    {/if}
  </div>
</div>
