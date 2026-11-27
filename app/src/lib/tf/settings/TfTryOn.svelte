<!--
  **Try it on** (#2562): one panel on every renderer's, pipeline's, folder renderer's, command's and application's
  settings page, so a person tries it on an example of their own (`try-on.ts`).

  - **On**, a segmented control over the sources the page offers: *Pasted text* (a file name, whose ending picks the
    renderers as a file's does in Code, and the text), *A file* and *A folder* (the native picker, anywhere on this
    disk; nothing is read until *Try it*), *A repository* (the page's own rows, with their own *Try it*).
  - **Try it** waits with its reason beside it: the page's own (the draft is not ready), then the source's. It is held
    against a double press (the press rule) and hands the page what to try; the page saves its draft first, since only a
    saved one runs (ADR-0016).
  - `extra` draws the page's own rows above the source (a file renderer's *Drawn by*, a pipeline's *Through*); `result`
    draws what the try made, under the panel.

  The rows of a source hold their room while it is picked, and switching sources is the only thing that moves them.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { pickPath, type TryOn } from "../../ipc/app";
  import { PressHold } from "../press-hold.svelte";
  import { SOURCE_WORDS, tryDraft, tryOnOf, tryWhy, type TryDraft, type TrySource } from "./try-on";

  let {
    id,
    sources,
    why = null,
    name = "example.md",
    namePlaceholder = "example.md",
    textPlaceholder = "Paste an example here.",
    pick = pickPath,
    ontry,
    label = "Try it",
    title = null,
    extra,
    repoRows,
    result,
    intro = null,
    draft = undefined,
  }: {
    /** The ids' prefix: `<id>-name`, `<id>-text`, … */
    id: string;
    /** The sources this page offers, the first picked. */
    sources: readonly TrySource[];
    /** Why the page's draft cannot be tried yet; `null` once it can. */
    why?: string | null;
    /** The pasted text's first file name. */
    name?: string;
    namePlaceholder?: string;
    textPlaceholder?: string;
    /** The native picker (`tauri-plugin-dialog`); a test's own. */
    pick?: (kind: "file" | "folder") => Promise<string | null>;
    /** *Try it*: what to try it on. A rejection is said under the panel. */
    ontry: (on: TryOn) => unknown;
    label?: string;
    /** The button's title once it can go (what it will do). */
    title?: string | null;
    extra?: Snippet;
    /** The page's own repository rows, for *A repository*. */
    repoRows?: Snippet;
    result?: Snippet;
    /** One paragraph under the heading. */
    intro?: string | null;
    /** What the panel holds, where the page keeps it (a `$state` of its own); else the panel's own. */
    draft?: TryDraft;
  } = $props();

  // svelte-ignore state_referenced_locally
  const own = $state(tryDraft(sources[0] ?? "text", name));
  const held = $derived(draft ?? own);
  /** The source shown: the one picked, while the page still offers it (a command's sources follow what it runs on). */
  const source = $derived(sources.includes(held.source) ? held.source : (sources[0] ?? "text"));
  const shownDraft = $derived({ ...held, source });

  const waits = $derived(tryWhy(shownDraft, why));
  let picking = $state(false);
  let refused = $state<string | null>(null);
  const hold = new PressHold();

  async function choose(kind: "file" | "folder") {
    if (picking) return;
    picking = true;
    refused = null;
    try {
      const path = await pick(kind);
      if (path !== null) held[kind] = path;
    } catch (cause) {
      refused = `The picker did not open: ${ipcErrorMessage(cause)}`;
    } finally {
      picking = false;
    }
  }

  async function press() {
    const on = tryOnOf(shownDraft);
    if (waits !== null || on === null || hold.held()) return;
    hold.hold();
    refused = null;
    try {
      await ontry(on);
    } catch (cause) {
      refused = ipcErrorMessage(cause);
      hold.release();
    }
  }
</script>

<div class="tf-stg-sec tf-try" data-source={source}>
  <h2>Try it on</h2>
  {#if intro !== null}<p class="tf-stg-p">{intro}</p>{/if}
  {@render extra?.()}
  {#if sources.length > 1}
    <div class="tf-stg-row">
      <div class="tf-stg-l" id="{id}-on-l">On</div>
      <div class="tf-stg-c">
        <div class="tf-seg" role="group" aria-labelledby="{id}-on-l">
          {#each sources as one (one)}
            <button type="button" aria-pressed={source === one} onclick={() => (held.source = one)}>{SOURCE_WORDS[one]}</button>
          {/each}
        </div>
      </div>
    </div>
  {/if}
  {#if source === "text"}
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="{id}-name">File name<small>Its ending picks what takes it, as a file’s does on the Code page.</small></label>
      <div class="tf-stg-c">
        <input id="{id}-name" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder={namePlaceholder} bind:value={held.name} />
      </div>
    </div>
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="{id}-text">Text</label>
      <div class="tf-stg-c">
        <textarea id="{id}-text" class="tf-stg-in tf-mono tf-try-text" rows="6" spellcheck="false" placeholder={textPlaceholder} bind:value={held.text}></textarea>
      </div>
    </div>
  {:else if source === "file" || source === "folder"}
    {@const kind = source}
    {@const chosen = held[kind]}
    <div class="tf-stg-row">
      <div class="tf-stg-l">{kind === "file" ? "File" : "Folder"}<small>Anywhere on this computer. It is read when you press {label}, and nothing is kept.</small></div>
      <div class="tf-stg-c">
        <button type="button" class="tf-btn tf-sm" disabled={picking} onclick={() => void choose(kind)}>{kind === "file" ? "Choose a file…" : "Choose a folder…"}</button>
        <span class="tf-try-path tf-cut" class:tf-mono={chosen !== null} class:tf-dim={chosen === null} title={chosen ?? undefined}>{chosen ?? "Nothing chosen"}</span>
      </div>
    </div>
  {:else if source === "repo"}
    {@render repoRows?.()}
  {/if}
  {#if source !== "repo"}
    <div class="tf-stg-acts">
      <button type="button" class="tf-btn tf-sm" disabled={waits !== null} title={waits === null ? (title ?? undefined) : waits} onclick={() => void press()}>{label}</button>
      {#if waits !== null}<p class="tf-dwhy" role="status">{waits}</p>{/if}
    </div>
  {/if}
  {#if refused !== null}<div class="tf-banner tf-err tf-stg-gap" role="alert">{refused}</div>{/if}
  {@render result?.()}
</div>
