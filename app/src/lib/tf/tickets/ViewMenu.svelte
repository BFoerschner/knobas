<!--
  A ticket view's ⋯ menu (#812, R-3.12), on its row in the views pane and in
  its header: *Duplicate* and *Delete*. *Edit view* joins it with the editor
  (#814).

  Opens under its button; `Esc`, a pick or a click anywhere else closes it,
  and focus goes back to the button.
-->
<script lang="ts">
  import { tick } from "svelte";

  let {
    name,
    className = "tf-vw-more",
    onduplicate,
    ondelete,
  }: {
    /** The view's name, for the button's accessible name. */
    name: string;
    className?: string;
    onduplicate: () => void;
    ondelete: () => void;
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
    open = false;
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

<span class="tf-vw-menu">
  <button
    type="button"
    class={className}
    aria-label="More for {name}"
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={button}
    onclick={toggle}>⋯</button
  >
  {#if open}
    <div class="tf-menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown}>
      <button type="button" role="menuitem" onclick={() => pick(onduplicate)}><b>Duplicate</b></button>
      <hr />
      <button type="button" role="menuitem" onclick={() => pick(ondelete)}><b>Delete</b></button>
    </div>
  {/if}
</span>
