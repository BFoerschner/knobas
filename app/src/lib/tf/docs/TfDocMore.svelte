<!--
  The reader's *More ▾* (#2082, R-10.10): in the header's actions before "Open in" (#2286),
  holding what a reader reaches for less: *Link to…*, *Copy link* and *Export as Markdown*. Without them the
  header's actions run to a second line in the reading column.

  The ticket view menu's pattern (`tickets/ViewMenu.svelte`), copied, not
  shared: it opens under its button with focus on the first item; `Esc`, a
  pick or a click anywhere else closes it, and after `Esc` or a pick focus
  goes back to *More*.
-->
<script lang="ts">
  import { tick } from "svelte";

  let {
    onlink,
    oncopy,
    onexport,
  }: {
    /** *Link to…*, where this reader can link. */
    onlink?: (() => void) | undefined;
    oncopy: () => void;
    /** *Export as Markdown*, for a page that can be read here. */
    onexport?: (() => void) | undefined;
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

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    close();
  }
</script>

<span class="tf-rd-more">
  <button type="button" class="tf-btn" aria-haspopup="menu" aria-expanded={open} bind:this={button} onclick={toggle}
    >More</button
  >
  {#if open}
    <div class="tf-menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown}>
      {#if onlink}<button type="button" role="menuitem" onclick={() => pick(onlink)}>Link to…</button>{/if}
      <button type="button" role="menuitem" onclick={() => pick(oncopy)}>Copy link</button>
      {#if onexport}<button type="button" role="menuitem" onclick={() => pick(onexport)}>Export as Markdown</button>{/if}
    </div>
  {/if}
</span>
