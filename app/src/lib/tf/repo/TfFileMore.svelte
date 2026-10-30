<!--
  The file header's *More* (⋯) (#2311, mockups/code-area `file.html`): the
  last icon button of the header, beside *Render as* and *Open in…*, holding
  what the mockup draws no button for: *Edit*, the file's *History* (the
  Commits tab) and its page on the source.

  The Docs reader's *More* pattern (`docs/TfDocMore.svelte`): it opens under
  its button with focus on the first item; `↑`/`↓` move between the items,
  `Esc`, a pick or a click anywhere else closes it, and after `Esc` focus goes
  back to *More*. *Edit* the file cannot take is drawn, disabled, saying why.
-->
<script lang="ts">
  import { tick } from "svelte";

  let {
    edit,
    notEditable = null,
    history,
    page = null,
    sourceName,
  }: {
    /** The file's edit address. */
    edit: string;
    /** Why the file cannot be edited, `null` where it can. */
    notEditable?: string | null;
    /** The repository's Commits tab. */
    history: string;
    /** The file's page on the source, `null` where it named none. */
    page?: string | null;
    /** The source, as *Open on <source>* names it. */
    sourceName: string;
  } = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const items = () => [...(menu?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      items()[0]?.focus();
    }
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
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open = false;
      button?.focus();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const next = event.key === "ArrowDown" ? Math.min(all.length - 1, at + 1) : Math.max(0, at - 1);
    all[next]?.focus();
  }
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
    onclick={toggle}
    ><svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2" /><circle cx="8" cy="8" r="1.2" /><circle cx="12.5" cy="8" r="1.2" /></svg
    ></button
  >
  {#if open}
    <div class="tf-menu" role="menu" tabindex="-1" aria-label="More" bind:this={menu} {onkeydown}>
      {#if notEditable === null}
        <a role="menuitem" href={edit} onclick={() => (open = false)}>Edit</a>
      {:else}
        <span role="menuitem" tabindex="-1" aria-disabled="true" title={notEditable}>Edit<small>{notEditable}</small></span>
      {/if}
      <a role="menuitem" href={history} onclick={() => (open = false)}>History</a>
      {#if page !== null}
        <a role="menuitem" href={page} onclick={() => (open = false)}>Open on {sourceName}</a>
      {/if}
    </div>
  {/if}
</span>
