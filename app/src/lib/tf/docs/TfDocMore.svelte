<!--
  The reader's *More ▾* (#2082, R-10.10): the last button of the actions row,
  holding *Export as Markdown*; later actions go here too.

  The ticket view menu's pattern (`tickets/ViewMenu.svelte`), copied, not
  shared: it opens under its button with focus on the first item; `Esc`, a
  pick or a click anywhere else closes it, and after `Esc` or a pick focus
  goes back to *More*.
-->
<script lang="ts">
  import { tick } from "svelte";

  let { onexport }: { onexport: () => void } = $props();

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
  <button type="button" class="tf-btn tf-sm" aria-haspopup="menu" aria-expanded={open} bind:this={button} onclick={toggle}
    >More</button
  >
  {#if open}
    <div class="tf-menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown}>
      <button type="button" role="menuitem" onclick={() => pick(onexport)}
        ><svg
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.4"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"><path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10" /></svg
        >Export as Markdown</button
      >
    </div>
  {/if}
</span>
