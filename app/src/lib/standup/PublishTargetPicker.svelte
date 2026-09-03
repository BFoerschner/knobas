<!--
  Which Confluence, and which page under it — the publish target, picked
  (issue #289, spec #272 stories 67-68).

  ## Why it is a component and not two copies

  The target is asked for in two places: the first *Publish* asks for it, and
  settings changes it. Those are the two halves of one sentence — *"asked for
  the first time and changeable in settings"* — and a settings panel that made
  the reader type `confluence:98400` into a text box beside a dialog that
  searched for the page would be the same setting with two very different
  asks, and the worse one would be the one people use to fix a mistake.

  ## The source select opens empty when there is a choice to make

  `presumedSource` decides: **one** configured Confluence needs no question,
  **two** have no defensible default. Story 68 is the reason — knobas picking
  one would put the team's standup in the wrong instance. Zero is a third
  case with a sentence of its own, because "pick one" is not something a
  reader can act on when there is nothing to pick.

  ## The parent is searched, never typed

  A page is picked out of the mirror, which is where the reader met it. The
  search is narrowed to pages at the backend and to the chosen source here:
  a page's source is on the hit, while the launcher's own filter vocabulary is
  about what the *reader* typed.
-->
<script lang="ts">
  import type { PublishTarget } from "../ipc/entity";
  import { search as realSearch, type SearchHit } from "../ipc/search";
  import type { PublishableSource } from "./protocol";
  import { presumedSource } from "./protocol";

  let {
    sources,
    value = null,
    search = realSearch,
    onchange,
  }: {
    /** The Confluence sources whose adapters declare `create_page`. */
    sources: PublishableSource[];
    /** The target as it stands, when there already is one. */
    value?: PublishTarget | null;
    /** The launcher's read, injectable so a test needs no Tauri. */
    search?: typeof realSearch;
    /** The target as it now stands, or `null` while it is incomplete. */
    onchange: (target: PublishTarget | null) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let sourceId = $state<string | null>(value?.source_id ?? presumedSource(sources));
  // svelte-ignore state_referenced_locally
  let parent = $state<string | null>(value?.parent ?? null);
  let query = $state("");
  let hits = $state<SearchHit[]>([]);

  function announce() {
    onchange(sourceId !== null && parent !== null ? { source_id: sourceId, parent } : null);
  }

  function chooseSource(id: string) {
    sourceId = id === "" ? null : id;
    // The parent belonged to the old instance; keeping it would offer a
    // publish into a wiki that cannot see the page.
    parent = null;
    hits = [];
    announce();
  }

  async function find(text: string) {
    query = text;
    if (sourceId === null || text.trim() === "") {
      hits = [];
      return;
    }
    try {
      const found = await search({
        raw: text,
        limit: 20,
        filters: {
          sources: [],
          kinds: ["page"],
          updated_within_days: null,
          mine: false,
          authors: [],
        },
      });
      hits = found.groups
        .flatMap((group) => group.hits)
        .filter((hit) => hit.kind === "page" && hit.source_id === sourceId);
    } catch {
      // A search that failed is no results; the reader types again. It is the
      // one read here that is not a claim about anything.
      hits = [];
    }
  }
</script>

{#if sources.length === 0}
  <p class="sub">No Confluence source is configured, so there is nowhere to publish yet.</p>
{:else}
  <label class="fld">
    Confluence
    <select
      aria-label="Confluence to publish into"
      value={sourceId ?? ""}
      onchange={(event) => chooseSource(event.currentTarget.value)}
    >
      <!--
        The empty option is selected whenever there is more than one source:
        story 68 asks to be *asked*, and a preselected instance is an answer
        nobody gave.
      -->
      <option value="">Choose…</option>
      {#each sources as source (source.id)}
        <option value={source.id}>{source.name}</option>
      {/each}
    </select>
  </label>

  <label class="fld">
    Parent page
    <input
      type="search"
      aria-label="Parent page"
      placeholder="Standup protocols"
      disabled={sourceId === null}
      value={query}
      oninput={(event) => void find(event.currentTarget.value)}
    />
  </label>

  {#if parent !== null && hits.length === 0}
    <p class="sub">Publishing under {parent}.</p>
  {/if}

  <ul class="hits">
    {#each hits as hit (hit.entity_id)}
      <li>
        <button
          class="link"
          class:picked={parent === hit.entity_id}
          onclick={() => {
            parent = hit.entity_id;
            announce();
          }}
        >
          {hit.title}
          {#if hit.path}<span class="sub">{hit.path}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .fld {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }

  .hits {
    display: grid;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  button.link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    text-align: left;
    color: var(--link);
    cursor: pointer;
  }

  button.link.picked {
    text-decoration: underline;
  }

  .sub {
    color: var(--muted);
    font-size: 12px;
  }
</style>
