<!--
  An embed on a page (#2440, spec #2432 *Embeds in the shell*): the HTML its
  author wrote, run in a sandboxed frame from knobas' own `knobas-render`
  origin (`render/embed.ts` says why it cannot reach the app).

  - The frame is as tall as its reporter says its document is, up to the
    window's height; past that it scrolls inside itself.
  - A link or popup from the frame opens through `onopen`, which is the
    external-open path and its scheme check.
  - With *Run embedded HTML* off for the page's source, or offline, the embed
    is a placeholder with *Open in <source>*. A frame that never says it
    loaded is one with *Open in browser*.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import type { DocEmbedContent } from "../../ipc/entity";
  import { tauriBridge } from "../../render/command";
  import { EMBED_SANDBOX, embedMessage, embedSource } from "../../render/embed";
  import { EMBED_REPORT_MS, embedsRun } from "./docs";

  let {
    content,
    sourceName,
    webUrl,
    onopen,
  }: {
    content: DocEmbedContent;
    sourceName: string;
    webUrl: string | null;
    onopen: (url: string) => void;
  } = $props();

  const runs = embedsRun();
  let online = $state(typeof navigator === "undefined" ? true : navigator.onLine);
  const source = $derived(embedSource(content.html, tauriBridge));
  const running = $derived(runs() && online && source !== null);

  let frame = $state<HTMLIFrameElement | null>(null);
  let height = $state(150);
  let reported = $state(false);
  let silent = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Each time the frame starts, it has EMBED_REPORT_MS to say it loaded.
  $effect(() => {
    if (!running) return;
    reported = false;
    silent = false;
    timer = setTimeout(() => {
      if (!reported) silent = true;
    }, EMBED_REPORT_MS);
    return () => clearTimeout(timer);
  });
  onDestroy(() => clearTimeout(timer));

  function heard(event: MessageEvent): void {
    // Only the frame's own window speaks for it.
    if (frame === null || event.source === null || event.source !== frame.contentWindow) return;
    const said = embedMessage(event.data);
    if (said === null) return;
    reported = true;
    if ("height" in said) height = Math.min(said.height, window.innerHeight);
    else onopen(said.open);
  }
</script>

<svelte:window onmessage={heard} ononline={() => (online = true)} onoffline={() => (online = false)} />

{#if running && !silent && source !== null}
  <div class="tf-embed">
    <iframe
      bind:this={frame}
      title="Embedded HTML from {sourceName}"
      sandbox={EMBED_SANDBOX}
      referrerpolicy="no-referrer"
      style:height="{height}px"
      {...source}
    ></iframe>
  </div>
{:else}
  <div class="tf-macro">
    {#if !runs()}
      <span>This part is embedded HTML. Run embedded HTML is off for {sourceName} (Settings › Sources).</span>
    {:else if !online}
      <span>This part is embedded HTML, which needs the network, and knobas is offline.</span>
    {:else}
      <span>This embedded HTML did not load.</span>
    {/if}
    {#if webUrl}
      {@const url = webUrl}
      <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}
        >{runs() && online ? "Open in browser" : `Open in ${sourceName}`}</button
      >
    {/if}
  </div>
{/if}
