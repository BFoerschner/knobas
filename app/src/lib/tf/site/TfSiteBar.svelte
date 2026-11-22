<!--
  A site's bar (decisions §16): one row, 40 px, where a page's breadcrumb sits, so the site reads as one more knobas
  page whose body someone else draws. It never wraps and never changes height.

  Left to right: the site's history (back, forward, reload or stop), the nameplate (whose site this is, which is also
  whose sign-in the view carries), the address in the code face (text, not a field: knobas's bar is no place to type
  an address), and the actions: *Open in browser*, *Back to ⟨name⟩* with two `Esc` caps where `Esc` has somewhere to
  go, and the close.

  Two other forms in the same row. **It asks** (decisions §10) when a close, a reload or another address would throw
  away changes the page has not saved: the sentence where the address was, the answers where the actions were, the
  first keeping the page and taking focus, `Esc` the same as it. **It says** what happened to a link, for 6 seconds
  (`site.svelte.ts`).

  The first `Esc` (decisions §9) draws the first cap down while the second still counts; nothing else changes.

  *Fill sign-in* (ux29) stands before *Open in browser* while a web sign-in is saved and the page is on the source's
  own origin; what came of a fill is said where the address was, as a link's fate is. On another origin, a sign-in
  provider's, the warning chip says knobas does not fill it there.
-->
<script lang="ts">
  import { tick } from "svelte";

  import type { SiteSetting, SiteStatus } from "../../ipc/site";
  import { openInBrowser } from "../../shell/external-links"; // browser on purpose: the bar's *Open in browser*
  import { keyHint } from "../../shell/platform";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { EscTargetOf } from "../escape";
  import { backStep } from "../page-name.svelte";
  import { answerAnyway, answerKeep, BRIDGE_ACTIONS, closeSite, reloadSite, siteBack, siteForward, type SiteActionPorts } from "./actions";
  import { siteStore, type SiteStore } from "./site.svelte";
  import type { UrlMatch } from "../../ipc/entity";
  import { entityAddress } from "../address";

  let {
    source,
    name,
    status,
    setting,
    back,
    store = siteStore,
    actions = BRIDGE_ACTIONS,
    match = null,
  }: {
    source: string;
    name: string;
    status: SiteStatus | null;
    setting: SiteSetting | undefined;
    back: EscTargetOf | null;
    store?: SiteStore;
    /** What the bar's buttons do; the bridge's when omitted. */
    actions?: SiteActionPorts;
    /** The mirrored item the address names: *Open in knobas* goes to its page (decisions §15). */
    match?: UrlMatch | null;
  } = $props();

  /** The nameplate and *Back to*'s name are cut past this many characters, the whole name in the title. */
  const NAME_MAX = 28;
  const cut = (text: string) => (text.length > NAME_MAX ? `${text.slice(0, NAME_MAX - 1)}…` : text);

  const question = $derived(store.asking.get(source) ?? null);
  const said = $derived(store.said.get(source) ?? null);
  const armed = $derived(store.armed.has(source));
  const step = backStep();

  const readable = (path: string) => {
    try {
      return decodeURI(path);
    } catch {
      return path;
    }
  };

  /** The address in parts: the scheme only for plain http, the host, then the rest. */
  const at = $derived.by(() => {
    const url = status?.url ?? null;
    if (url === null) return null;
    try {
      const parsed = new URL(url);
      return {
        plain: parsed.protocol === "http:" ? "http://" : "",
        host: parsed.host,
        // Read as a browser shows it: `%20` as a space. Text, never markup.
        rest: `${parsed.pathname === "/" && parsed.search === "" ? "" : readable(parsed.pathname)}${parsed.search}${parsed.hash}`,
        whole: url,
      };
    } catch {
      return null;
    }
  });

  const sentence = $derived.by(() => {
    if (question === null) return "";
    if (question.kind === "address") {
      const what = question.name ?? "that page";
      return `This page has changes it has not saved. Open ${what} here anyway?`;
    }
    return "This page has changes it has not saved.";
  });
  const answers = $derived.by(() => {
    if (question === null) return null;
    if (question.kind === "close") return { keep: "Keep it open", away: "Close anyway" };
    if (question.kind === "reload") return { keep: "Keep it", away: "Reload anyway" };
    return { keep: "Keep this page", away: "Open here anyway" };
  });

  let keepButton = $state<HTMLButtonElement>();
  // The keyboard onto the answer that keeps the page.
  $effect(() => {
    if (question === null) return;
    void tick().then(() => keepButton?.focus({ preventScroll: true }));
  });

  function onaskkey(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    answerKeep(source, actions, store);
  }

  function goBack(event: MouseEvent) {
    if (back === null || step === null) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    step(back.hash, back.opened);
  }

  /** Select the address on a press: it is text to copy, not a field. */
  function selectAddress(event: MouseEvent) {
    const el = event.currentTarget;
    if (!(el instanceof HTMLElement)) return;
    const range = document.createRange();
    range.selectNodeContents(el);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }

  /** What came of a fill, said where the address is; `null` when nothing is to say. */
  const signInNote = $derived.by(() => {
    switch (status?.sign_in) {
      case "not_taken":
        return `${name} did not take the saved sign-in.`;
      case "second_factor":
        return `${name} asks for a code. That one is yours to type.`;
      case "no_form":
        return "No sign-in form on this page.";
      case "ambiguous":
        return "This form has two password fields. knobas fills only a sign-in form.";
      default:
        return null;
    }
  });
  const filling = $derived(status?.sign_in === "filling");

  function fill() {
    const run = actions.fill;
    if (run === undefined) return;
    run(source).catch((cause: unknown) => push({ text: `Could not fill ${name}'s sign-in: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }

  function browse() {
    const url = status?.url ?? setting?.base ?? null;
    if (url !== null) openInBrowser(url); // browser on purpose: the bar's *Open in browser*
  }
</script>

{#if question !== null && answers !== null}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions: Esc on the question is its first answer -->
  <div class="tf-sitebar tf-asks" role="group" aria-label="{name} asks" onkeydown={onaskkey}>
    <span class="tf-site-name" title={name}>{cut(name)}</span>
    <span class="tf-site-says" role="status">{sentence}</span>
    <span class="tf-site-acts">
      <button type="button" class="tf-btn tf-sm" bind:this={keepButton} onclick={() => answerKeep(source, actions, store)}>{answers.keep}</button>
      {#if question.kind === "address"}
        <button type="button" class="tf-btn tf-sm" onclick={() => { const url = question.url; answerKeep(source, actions, store); openInBrowser(url); /* browser on purpose: the answer *Open in browser* */ }}>Open in browser</button>
      {/if}
      <button type="button" class="tf-btn tf-sm tf-danger" onclick={() => void answerAnyway(source, actions, store)}>{answers.away}</button>
    </span>
  </div>
{:else}
  <div class="tf-sitebar">
    <span class="tf-site-hist">
      <button type="button" class="tf-iconbtn tf-sm" disabled={status?.can_back !== true} title={keyHint(`Back in ${name} (⌘[)`)} aria-label="Back in {name}" onclick={() => siteBack(source, actions, store)}>
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.5 3.5 5 8l4.5 4.5" /></svg>
      </button>
      <button type="button" class="tf-iconbtn tf-sm" disabled={status?.can_forward !== true} title={keyHint(`Forward in ${name} (⌘])`)} aria-label="Forward in {name}" onclick={() => siteForward(source, actions, store)}>
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 3.5 11 8l-4.5 4.5" /></svg>
      </button>
      {#if status?.loading && status.phase === "live"}
        <button type="button" class="tf-iconbtn tf-sm" title="Stop loading" aria-label="Stop loading" onclick={() => void reloadSite(source, actions, store)}>
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" /></svg>
        </button>
      {:else}
        <button type="button" class="tf-iconbtn tf-sm" disabled={status === null} title={keyHint("Reload (⌘R)")} aria-label="Reload" onclick={() => void reloadSite(source, actions, store)}>
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12.8 8a4.8 4.8 0 1 1-1.5-3.5" /><path d="M12.9 2.6v2.6h-2.6" /></svg>
        </button>
      {/if}
    </span>
    <span class="tf-site-name" title={name}>{cut(name)}</span>
    {#if said !== null}
      <span class="tf-site-says" role="status">
        <span class="tf-site-says-t">{said.text}</span>
        {#if said.mono}<span class="tf-mono">{said.mono}</span>{/if}
        {#if said.action}
          {@const action = said.action}
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => action.run()}>{action.label}</button>
        {/if}
      </span>
    {:else if signInNote !== null}
      <span class="tf-site-says" role="status"><span class="tf-site-says-t">{signInNote}</span></span>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions: a press selects the text to copy; it is not a control -->
      <span class="tf-site-at" tabindex="-1" class:tf-foreign={at !== null && status?.own === false} title={at?.whole ?? ""} onclick={selectAddress}>
        {#if at !== null}
          {#if at.plain}<span class="tf-site-plain">{at.plain}</span>{/if}<span class="tf-site-host">{at.host}</span><span class="tf-site-path">{at.rest}</span>
          {#if status?.own === false}<span class="tf-chip tf-warn">{status.sign_in === "provider" ? `not ${name}'s own address, so its sign-in is not filled` : `not ${name}'s own address`}</span>{/if}
        {/if}
      </span>
    {/if}
    <span class="tf-site-acts">
      {#if match !== null}
        <a class="tf-btn tf-sm tf-ghost" href={entityAddress(match.kind, match.entity_id)}>Open in knobas</a>
      {/if}
      {#if status?.fill || filling}
        <button type="button" class="tf-btn tf-sm tf-ghost" disabled={filling} title="Type {name}'s saved sign-in into this page" onclick={fill}>{filling ? "Filling…" : "Fill sign-in"}</button>
      {/if}
      <button type="button" class="tf-btn tf-sm tf-ghost" disabled={status?.url == null && !setting?.base} onclick={browse}>Open in browser</button>
      {#if back !== null}
        <a class="tf-esc" href={back.hash} onclick={goBack} title={back.name}>
          <span class="tf-esc-t">Back to {cut(back.name)}</span><kbd class:tf-kbd-down={armed}>Esc</kbd><kbd>Esc</kbd>
        </a>
        <span class="tf-sr" role="status">{armed ? `Esc again goes back to ${back.name}` : ""}</span>
      {/if}
      <button type="button" class="tf-iconbtn tf-sm" disabled={status === null} title={keyHint(`Close ${name} (⌘W)`)} aria-label="Close {name}" onclick={() => void closeSite(source, actions, store)}>
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </span>
  </div>
{/if}
