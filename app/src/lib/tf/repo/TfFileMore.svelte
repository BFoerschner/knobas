<!--
  The file header's *More* (⋯) (#2311, mockups/code-area `file.html`): the
  last icon button of the header, beside *Render as* and *Open in…*, holding
  what the mockup draws no button for: *Edit* and the file's *History* (the
  Commits tab). Its page on the source is *Open in… › Browser* beside it
  (Björn, 2026-11-13: a link here took the app's window to Gitea).

  `TfMenu` draws it. *Edit* the file cannot take is drawn, disabled, saying why.
-->
<script lang="ts">
  import TfMenu from "../TfMenu.svelte";

  let {
    edit,
    notEditable = null,
    history,
  }: {
    /** The file's edit address. */
    edit: string;
    /** Why the file cannot be edited, `null` where it can. */
    notEditable?: string | null;
    /** The repository's Commits tab. */
    history: string;
  } = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);
</script>

<span class="tf-fmore">
  <button
    type="button"
    class="tf-iconbtn tf-sm"
    title="More"
    aria-label="More"
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={button}
    onclick={() => (open = !open)}
    ><svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2" /><circle cx="8" cy="8" r="1.2" /><circle cx="12.5" cy="8" r="1.2" /></svg
    ></button
  >
  <TfMenu bind:open opener={button} label="More" className="tf-fmore-m">
      {#if notEditable === null}
        <a role="menuitem" href={edit} onclick={() => (open = false)}>Edit</a>
      {:else}
        <span role="menuitem" tabindex="-1" aria-disabled="true" title={notEditable}>Edit<small>{notEditable}</small></span>
      {/if}
      <a role="menuitem" href={history} onclick={() => (open = false)}>History</a>
  </TfMenu>
</span>
