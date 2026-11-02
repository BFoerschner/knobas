<!--
  An embed on a page (#2440, #2441; spec #2432 *Embeds in the shell*): the
  HTML its author wrote, run in a sandboxed frame from knobas' own
  `knobas-render` origin, or the page a URL names, framed in the same
  sandbox (`render/embed.ts` says why neither can reach the app).

  - An HTML frame is as tall as its reporter says its document is, up to the
    window's height; past that it scrolls inside itself. A URL frame reports
    nothing and is `EMBED_URL_HEIGHT` tall, up to the window's. The height is
    the frame's `height` attribute: the app's policy refuses inline styles.
  - A link or popup from an HTML frame opens through `onopen`, which is the
    external-open path and its scheme check, only behind the reader's own
    click; any other is asked of the reader (`openVerdict`, #2454).
  - With *Run embedded HTML* off for the page's source, or offline, the embed
    is a placeholder with *Open in <source>*. A frame that never says it
    loaded (an HTML frame's report, a URL frame's `load`) is one with *Open in
    browser*. A site that refuses to be framed still fires `load`, and the
    app cannot see into another origin to tell, so a URL frame always has
    *Open in browser* under it.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import type { DocEmbedContent } from "../../ipc/entity";
  import { tauriBridge } from "../../render/command";
  import { EMBED_SANDBOX, embedMessage, embedOpens, embedSource, embedUrl, openVerdict } from "../../render/embed";
  import { EMBED_REPORT_MS, EMBED_URL_HEIGHT, embedsRun } from "./docs";

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
  const source = $derived(content.kind === "url" ? embedUrl(content.url) : embedSource(content.html, tauriBridge));
  const running = $derived(runs() && online && source !== null);
  const framed = $derived(content.kind === "url" && source !== null ? content.url : null);
  const noun = $derived(content.kind === "url" ? "an embedded page" : "embedded HTML");
  const title = $derived(framed === null ? `Embedded HTML from ${sourceName}` : `Embedded page from ${new URL(framed).host}`);
  // Off or offline the way out is the source's page (*Open in <source>*);
  // a frame that did not load opens the framed page itself where there is one.
  const away = $derived(runs() && online ? (framed ?? webUrl) : webUrl);

  let frame = $state<HTMLIFrameElement | null>(null);
  let reportedHeight = $state(150);
  const height = $derived(framed === null ? reportedHeight : Math.min(EMBED_URL_HEIGHT, window.innerHeight));
  let reported = $state(false);
  let silent = $state(false);
  let asked = $state<string | null>(null);
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
    if ("height" in said) reportedHeight = Math.min(said.height, window.innerHeight);
    else wants(said.open);
  }

  function wants(url: string): void {
    const now = Date.now();
    const activated = typeof navigator !== "undefined" && navigator.userActivation?.isActive === true;
    const verdict = openVerdict({ activated, now, lastOpen: embedOpens.last, asking: asked !== null });
    if (verdict === "open") {
      embedOpens.last = now;
      onopen(url);
    } else if (verdict === "ask") asked = url;
  }

  function answer(open: boolean): void {
    const url = asked;
    asked = null;
    if (open && url !== null) onopen(url);
  }
</script>

<svelte:window onmessage={heard} ononline={() => (online = true)} onoffline={() => (online = false)} />

{#if running && !silent && source !== null}
  <div class="tf-embed">
    {#if asked !== null}
      <div class="tf-macro">
        <span>This embed wants to open {asked}.</span>
        <button type="button" class="tf-btn tf-sm" onclick={() => answer(true)}>Open</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => answer(false)}>Dismiss</button>
      </div>
    {/if}
    <iframe
      bind:this={frame}
      {title}
      sandbox={EMBED_SANDBOX}
      referrerpolicy="no-referrer"
      {height}
      onload={() => {
        if (framed !== null) reported = true;
      }}
      {...source}
    ></iframe>
    {#if framed !== null}
      {@const url = framed}
      <div class="tf-macro">
        <span>A page that refuses to be framed draws blank here.</span>
        <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}>Open in browser</button>
      </div>
    {/if}
  </div>
{:else}
  <div class="tf-macro">
    {#if !runs()}
      <span>This part is {noun}. Run embedded HTML is off for {sourceName} (Settings › Sources).</span>
    {:else if !online}
      <span>This part is {noun}, which needs the network, and knobas is offline.</span>
    {:else}
      <span>This {noun.replace(/^an /, "")} did not load.</span>
    {/if}
    {#if away}
      {@const url = away}
      <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}
        >{runs() && online ? "Open in browser" : `Open in ${sourceName}`}</button
      >
    {/if}
  </div>
{/if}
