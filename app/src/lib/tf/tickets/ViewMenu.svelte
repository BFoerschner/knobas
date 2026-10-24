<!--
  A ticket view's ⋯ menu (#812, R-3.12), on its row in the views pane and in
  its header: *Edit view* (#814), *Duplicate* and *Delete*; on a saved view
  (#877), *View settings*, *Make an editable copy* (#880), which is what
  *Duplicate* is for a saved view, and *Hide from the list* (#878).

  Opens under its button; `Esc`, a pick or a click anywhere else closes it,
  and focus goes back to the button.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { SavedViewSource } from "../../ipc/search";

  let {
    name,
    className = "tf-vw-more",
    savedViewKind = null,
    copies = true,
    onedit,
    onduplicate,
    ondelete,
    onhide = () => {},
  }: {
    /** The view's name, for the button's accessible name. */
    name: string;
    className?: string;
    /**
     * A saved view's kind (#877): *View settings* and *Make an editable copy*
     * (`onduplicate`, #880) only; its source decides the rest.
     */
    savedViewKind?: SavedViewSource["kind"] | null;
    /** Whether a saved view's source translates its query (#1208): no *Make an editable copy* where it does not. */
    copies?: boolean;
    onedit: () => void;
    onduplicate: () => void;
    ondelete: () => void;
    /** *Hide from the list* on a saved view (#878, R-3.25). */
    onhide?: () => void;
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
      {#if savedViewKind !== null}
        <button type="button" role="menuitem" onclick={() => pick(onedit)}
          ><b>View settings</b><span>lanes, cards and list columns</span></button
        >
        {#if copies}
          <button type="button" role="menuitem" onclick={() => pick(onduplicate)}
            ><b>Make an editable copy</b><span>your own view, starting from this {savedViewKind}</span></button
          >
        {/if}
        <hr />
        <button type="button" role="menuitem" onclick={() => pick(onhide)}
          ><b>Hide from the list</b><span>it keeps syncing, so you can show it again</span></button
        >
      {:else}
        <button type="button" role="menuitem" onclick={() => pick(onedit)}><b>Edit view</b></button>
        <button type="button" role="menuitem" onclick={() => pick(onduplicate)}><b>Duplicate</b></button>
        <hr />
        <button type="button" role="menuitem" onclick={() => pick(ondelete)}><b>Delete</b></button>
      {/if}
    </div>
  {/if}
</span>
