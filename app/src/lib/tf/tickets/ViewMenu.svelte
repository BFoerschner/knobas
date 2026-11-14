<!--
  A ticket view's ⋯ menu (#812, R-3.12), on its row in the views pane and in
  its header: *Edit view* (#814), *Duplicate* and *Delete*; on a saved view
  (#877), *View settings*, *Make an editable copy* (#880), which is what
  *Duplicate* is for a saved view, and *Hide from the list* (#878).

  `TfMenu` draws it.
-->
<script lang="ts">
  import type { SavedViewSource } from "../../ipc/search";
  import TfMenu from "../TfMenu.svelte";

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

  function pick(run: () => void) {
    open = false;
    run();
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
    onclick={() => (open = !open)}>⋯</button
  >
  <TfMenu bind:open opener={button} label="More for {name}" className="tf-vw-m">
      {#if savedViewKind !== null}
        <button type="button" role="menuitem" onclick={() => pick(onedit)}
          ><b>View settings…</b><span>lanes, cards and list columns</span></button
        >
        {#if copies}
          <button type="button" role="menuitem" onclick={() => pick(onduplicate)}
            ><b>Make an editable copy…</b><span>your own view, starting from this {savedViewKind}</span></button
          >
        {/if}
        <hr />
        <button type="button" role="menuitem" onclick={() => pick(onhide)}
          ><b>Hide from the list</b><span>it keeps syncing, so you can show it again</span></button
        >
      {:else}
        <button type="button" role="menuitem" onclick={() => pick(onedit)}><b>Edit view…</b></button>
        <button type="button" role="menuitem" onclick={() => pick(onduplicate)}><b>Duplicate…</b></button>
        <hr />
        <button type="button" role="menuitem" class="tf-menu-danger" onclick={() => pick(ondelete)}><b>Delete</b></button>
      {/if}
  </TfMenu>
</span>
