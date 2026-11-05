<!--
  The reader's *More ▾* (#2082, R-10.10): in the header's actions before "Open in" (#2286),
  holding what a reader reaches for less: *Link to…*, *Copy link* and *Export as Markdown*. Without them the
  header's actions run to a second line in the reading column. The view's *Sync now* joins them where the header has no
  room for it (`readerFit`): the same button, which is why it comes as the frame's snippet.

  The ticket view menu's pattern (`tickets/ViewMenu.svelte`), copied, not
  shared: it opens under its button with focus on the first item; `Esc`, a
  pick or a click anywhere else closes it, and after `Esc` or a pick focus
  goes back to *More*.
-->
<script lang="ts">
  import { menuKeys } from "../menu-keys";
  import { tick, type Snippet } from "svelte";

  let {
    onlink,
    oncopy,
    onexport,
    sync,
  }: {
    /** *Link to…*, where this reader can link. */
    onlink?: (() => void) | undefined;
    oncopy: () => void;
    /** *Export as Markdown*, for a page that can be read here. */
    onexport?: (() => void) | undefined;
    /** The view's *Sync now*, where the header has no room for it. */
    sync?: Snippet | undefined;
  } = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      menu?.querySelector<HTMLElement>("button")?.focus();
    }
  }

  function close() {
    open = false;
    button?.focus();
  }

  function pick(run: () => void) {
    close();
    run();
  }

  $effect(() => {
    if (!open) return;
    const away = (event: MouseEvent) => {
      if (event.target instanceof Node && (menu?.contains(event.target) || button?.contains(event.target))) return;
      open = false;
    };
    window.addEventListener("mousedown", away);
    return () => window.removeEventListener("mousedown", away);
  });

  /** The frame's button is an item here: the arrow keys reach it, as they do the others. */
  function asItem(slot: HTMLElement) {
    slot.querySelector("button")?.setAttribute("role", "menuitem");
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    close();
  }
</script>

<span class="tf-rd-more">
  <button type="button" class="tf-btn" aria-haspopup="menu" aria-expanded={open} bind:this={button} onclick={toggle}
    >More<svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
  >
  {#if open}
    <div class="tf-menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown} {@attach menuKeys(() => (open = false))}>
      {#if onlink}<button type="button" role="menuitem" onclick={() => pick(onlink)}>Link to…</button>{/if}
      <button type="button" role="menuitem" onclick={() => pick(oncopy)}>Copy link</button>
      {#if onexport}<button type="button" role="menuitem" onclick={() => pick(onexport)}>Export as Markdown</button>{/if}
      {#if sync}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <span class="tf-rd-moresync" role="none" {@attach asItem} onclick={close}>{@render sync()}</span>
      {/if}
    </div>
  {/if}
</span>
