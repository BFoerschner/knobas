<!--
  What a site's rectangle says (decisions §14, §16): the words for the site's state, drawn under the native view, so
  hiding the view never uncovers a blank. Quiet on purpose: the paper, a centred block at most 560 px wide.

  Live, or covered by a knobas layer: the page's title, its host, "Still open, as you left it." The view covers these
  while it is on screen.
-->
<script lang="ts">
  import type { SiteSetting, SiteStatus } from "../../ipc/site";

  let {
    name,
    status,
    setting,
    covered,
    onopen,
    onbrowse,
    oncontinue,
    ondecline,
    onreload,
  }: {
    name: string;
    status: SiteStatus | null;
    setting: SiteSetting | undefined;
    covered: boolean;
    onopen: () => void;
    onbrowse: () => void;
    oncontinue: () => void;
    ondecline: () => void;
    onreload: () => void;
  } = $props();

  const hostOf = (url: string | null | undefined): string => {
    if (!url) return "";
    try {
      return new URL(url).host;
    } catch {
      return "";
    }
  };

  const host = $derived(hostOf(status?.url) || hostOf(setting?.own) || hostOf(setting?.base));
  const asked = $derived(hostOf(status?.asks_origin));

  const failure = $derived.by(() => {
    const f = status?.failure;
    if (!f) return "Nothing loaded.";
    switch (f.kind) {
      case "timeout":
        return "Nothing loaded in 10 seconds.";
      case "certificate":
        return `${host}'s certificate is not trusted on this computer.`;
      case "not_found":
        return `No computer answers to ${host}.`;
      case "refused":
        return `${host} refused the connection.`;
      default:
        return f.detail;
    }
  });

  let continueButton = $state<HTMLButtonElement>();
  $effect(() => {
    if (status?.phase === "asks" && !covered) continueButton?.focus({ preventScroll: true });
  });
</script>

<div class="tf-veil" data-state={setting?.offered === false ? "unoffered" : status === null ? "closed" : covered ? "covered" : status.phase}>
  <div class="tf-veil-in">
    {#if setting?.offered === false}
      <h2>{name} opens in your browser on this system</h2>
      <div class="tf-veil-acts"><button type="button" class="tf-btn" onclick={onbrowse}>Open in browser</button></div>
    {:else if status === null}
      <h2>{name} is not open</h2>
      <p>{setting?.kept_since ? "Its sign-in is kept." : "You will sign in on its own page, once."}</p>
      <div class="tf-veil-acts"><button type="button" class="tf-btn tf-primary" onclick={onopen}>Open {name}</button></div>
    {:else if status.phase === "asks" && !covered}
      <h2>{name} wants to continue at another address</h2>
      <div class="tf-veil-big">{asked}</div>
      <p>Usually this is where you sign in. Until you allow it, knobas shows only {name}'s own addresses here.</p>
      <div class="tf-veil-acts">
        <button type="button" class="tf-btn tf-primary" bind:this={continueButton} onclick={oncontinue}>Continue here</button>
        <button type="button" class="tf-btn" onclick={ondecline}>Open in browser</button>
      </div>
      <small>Settings › Sources › {name} lists what you allowed.</small>
    {:else if status.phase === "opening" && !covered}
      <h2>Opening {name}</h2>
      <div class="tf-veil-at">{host}</div>
    {:else if status.phase === "failed" && !covered}
      <h2>{host || name} did not answer</h2>
      <p>{failure}</p>
      <div class="tf-veil-acts">
        <button type="button" class="tf-btn" onclick={onreload}>Reload</button>
        <button type="button" class="tf-btn" onclick={onbrowse}>Open in browser</button>
      </div>
    {:else if status.phase === "stopped" && !covered}
      <h2>The page stopped</h2>
      <p>{name}'s page ended by itself. What it held unsaved is gone.</p>
      <div class="tf-veil-acts"><button type="button" class="tf-btn" onclick={onreload}>Reload</button></div>
    {:else}
      <h2>{status.title || name}</h2>
      <div class="tf-veil-at">{host}</div>
      <p>Still open, as you left it.</p>
    {/if}
  </div>
</div>
