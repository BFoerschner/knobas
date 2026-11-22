<!--
  *Open web interface* (ux30): the password manager's own web page on the open entry, the folder, or its front page,
  inside knobas and signed in where the source's site opens inside, else in the browser. Shift W, a ⌘/Ctrl press or a
  middle press opens the browser. The address is asked for at the press (`password_web_link`); the words are `web.ts`'s.
  It names no source and no vendor.
-->
<script lang="ts">
  import type { PwSource } from "../../ipc/passwords";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { siteStore, type SiteStore } from "../site/site.svelte";
  import type { PasswordPorts } from "./ports";
  import { webWords, type WebOn } from "./web";

  let {
    source,
    item,
    on,
    name = null,
    hasFolder = false,
    ports,
    key = false,
    store = siteStore,
  }: {
    source: PwSource;
    /** The entry's or folder's entity id; `null` for the source itself. */
    item: string | null;
    on: WebOn;
    /** What the press names, for the chip. */
    name?: string | null;
    /** The entry sits in a folder. */
    hasFolder?: boolean;
    ports: PasswordPorts;
    /** It carries the page's W cap. */
    key?: boolean;
    store?: Pick<SiteStore, "settings">;
  } = $props();

  const words = $derived(webWords(source, on, hasFolder, store.settings.get(source.id)));

  /** Open it; `elsewhere` asks for the browser. */
  export async function press(elsewhere = false): Promise<void> {
    if (words === null) return;
    try {
      const link = await ports.webLink(source.id, item);
      if (link === null) {
        push({ text: `${source.name} has no web address to open.`, tone: "err" });
        return;
      }
      ports.openWeb(link.url, source.id, elsewhere, name);
    } catch (cause) {
      push({ text: `Could not open the web interface: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

{#if words}
  <button
    type="button"
    class="tf-btn"
    title={words.title}
    aria-keyshortcuts={key ? "W" : undefined}
    onclick={(event) => void press(event.metaKey || event.ctrlKey)}
    onauxclick={(event) => {
      if (event.button === 1) void press(true);
    }}>Open web interface{#if key}<kbd>W</kbd>{/if}</button
  >
{/if}
