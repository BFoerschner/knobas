<!--
  Settings › Docs (PR #2479, 2026-11-17, `docs/decisions/2026-11-17-where-docs-opens-and-the-spaces-it-hides.md`):
  where Docs opens, and which spaces it shows. The same choices a space's and a page's right-click menu make
  (`docs-choices.svelte.ts`), each drawn at once and written behind.

  *Docs opens on* lists Docs' own page, then every space, hidden ones too, then the page chosen as Docs' home when it
  is one: a page is chosen from its own menu, since a list of every page would be thousands long. *Spaces shown in Docs*
  is a box per space; a box cleared hides the space from the rail, the home and the Space picker, and search still
  finds its pages. The spaces are the tree Docs already read, else `docs_outline`, which the backend keeps warm.
-->
<script lang="ts">
  import { docsOutline as realDocsOutline, type DocSpace, type DocsTree } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { counted } from "../counted";
  import { docsChoices as sharedChoices, type Choices } from "../docs/docs-choices.svelte";
  import { pageCount, spaceId, spaceTitle } from "../docs/docs";
  import { heldTree } from "../docs/tree-store";

  let {
    outline,
    choices = sharedChoices,
  }: {
    /** The spaces' read; the IPC's when omitted. */
    outline?: () => Promise<DocsTree>;
    /** The choices; the window's when omitted. */
    choices?: Choices;
  } = $props();

  let spaces = $state.raw<DocSpace[] | null>(heldTree.tree?.spaces ?? null);
  let failed = $state<string | null>(null);
  $effect(() => {
    // The IPC read named only when asked, so a suite that mocks the bridge without it can still draw the section.
    void (outline ?? realDocsOutline)().then(
      (tree) => {
        spaces = tree.spaces;
        failed = null;
      },
      (cause: unknown) => {
        if (spaces === null) failed = ipcErrorMessage(cause);
      },
    );
  });

  /** The select's value for Docs' own page. */
  const OWN = "";
  const value = $derived(choices.home === null ? OWN : `${choices.home.kind}:${choices.home.id}`);
  /** A home the spaces do not list (a page, or a space the mirror lost) still names itself in the list. */
  const homeListed = $derived(choices.home?.kind === "space" && (spaces ?? []).some((space) => spaceId(space) === choices.home?.id));
  const shownCount = $derived((spaces ?? []).filter((space) => !choices.hidden.has(spaceId(space))).length);

  function pickHome(event: Event): void {
    const picked = (event.currentTarget as HTMLSelectElement).value;
    if (picked === value) return;
    if (picked === OWN) return void choices.setHome(null);
    const space = (spaces ?? []).find((each) => `space:${spaceId(each)}` === picked);
    if (space !== undefined) void choices.setHome({ kind: "space", id: spaceId(space), title: spaceTitle(space, spaces ?? []) });
  }
</script>

<header class="tf-stg-h"><h1>Docs</h1><p>Where Docs opens, and which spaces it shows.</p></header>
<div class="tf-stg-row">
  <label class="tf-stg-l" for="tf-docs-opens"
    >Docs opens on<small>Where the Docs entry and the launcher's Docs row go. A page you were reading comes back first. A page's or a space's right-click menu sets it too.</small></label
  >
  <div class="tf-stg-c">
    <select id="tf-docs-opens" class="tf-stg-in" {value} onchange={pickHome} disabled={spaces === null && choices.home === null}>
      <option value={OWN}>Docs' home, every space</option>
      {#if choices.home !== null && !homeListed}
        <option value={value}>{choices.home.title}{choices.home.kind === "page" ? " (a page)" : ""}</option>
      {/if}
      {#each spaces ?? [] as space (spaceId(space))}
        <option value="space:{spaceId(space)}">{spaceTitle(space, spaces ?? [])}{choices.hidden.has(spaceId(space)) ? " (hidden)" : ""}</option>
      {/each}
    </select>
  </div>
</div>
<section class="tf-stg-sec" aria-label="Spaces shown in Docs">
  <h2>Spaces shown in Docs</h2>
  <p class="tf-stg-p">
    A space you clear leaves the rail, the Docs home and the Space picker for a fold at their end. Search still finds its pages, links into it still open, and the sync keeps it.
  </p>
  {#if failed !== null}
    <p class="tf-stg-p tf-stg-errt" role="status">Could not read the spaces: {failed}</p>
  {:else if spaces === null}
    <p class="tf-stg-p" role="status">Reading the spaces…</p>
  {:else if spaces.length === 0}
    <p class="tf-stg-p">No space has synced yet.</p>
  {:else}
    <p class="tf-stg-p">{shownCount} of {counted(spaces.length, "space", "spaces")} shown.</p>
    <div class="tf-stg-spaces">
      {#each spaces as space (spaceId(space))}
        {@const id = spaceId(space)}
        {@const name = spaceTitle(space, spaces)}
        <label class="tf-ck"
          ><input type="checkbox" checked={!choices.hidden.has(id)} onchange={(event) => void choices.setHidden(id, name, !event.currentTarget.checked)} />
          <span class="tf-stg-spn">{name}</span> <span class="tf-dim">{pageCount(space)}</span></label
        >
      {/each}
    </div>
  {/if}
</section>
