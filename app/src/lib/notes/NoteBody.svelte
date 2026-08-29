<!--
  A note's body, as a reader sees it: the text, with every `[[ref]]` drawn as
  the thing it points at.

  **No `{@html}`, here least of all.** The body is a document the user typed,
  which is the content roadmap §4 gotcha 7 exists for, and the rule is enforced
  app-wide (`shell/house-rules.test.ts`). The chips are built by walking tokens
  (`note-body.ts`) and drawing them with `{#each}` — the same shape
  `launcher/Row.svelte` draws a search snippet's segments with.

  Three states a chip can be in, and all three are visible:

  * **resolved** — a button carrying the target's monogram, kind and title, so
    the reference reads as the thing rather than as a key (#46 story 7), and
    clicking it is navigation (story 8);
  * **withdrawn** — the target resolved and the source has since withdrawn it.
    Still a chip, still openable, marked; a reference must never dangle
    silently (story 9, and §5a's rule that #53's panel already follows);
  * **unresolved** — nothing carries that id. Marked as unresolved rather than
    left as plain text, which is how a typo is discoverable at all (story 10).
-->
<script lang="ts">
  import type { NoteRef } from "../ipc/entity";
  import Monogram from "../shell/Monogram.svelte";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { hashFor } from "../shell/router.svelte";
  import { tokenise } from "./note-body";

  let {
    body,
    refs,
    onopen,
  }: {
    /** The markdown the user typed. Untrusted text — drawn as text. */
    body: string;
    /** What the backend resolved. Also what decides which spans are chips. */
    refs: NoteRef[];
    /** Navigate to an address (spec §2). The shell's router is the only one. */
    onopen: (hash: string) => void;
  } = $props();

  const tokens = $derived(tokenise(body, refs));

  function labelOf(kind: string): string {
    return kindSingular(kind, kindRegistry.info(kind));
  }

  function monogramOf(kind: string): string {
    return kindMonogram(kind, kindRegistry.info(kind));
  }

  /**
   * The target's own address, built through `hashFor` rather than by
   * concatenation: entity keys carry `#` and `/`, and an unencoded one
   * truncates the fragment at the browser level.
   */
  function addressOf(ref: NoteRef): string {
    const target = ref.target;
    if (!target) return "";
    return hashFor({
      view: "room",
      ctx: "all",
      detail: { kind: target.kind, entityId: target.entity_id },
    });
  }
</script>

<p class="nbody"
  >{#each tokens as token, i (i)}{#if token.kind === "text"}{token.text}{:else if token.ref.target}<button
        class="chip"
        title="Open {token.ref.target.entity_id}"
        onclick={() => onopen(addressOf(token.ref))}
      >
        <Monogram
          text={monogramOf(token.ref.target.kind)}
          label={labelOf(token.ref.target.kind)}
        />
        <span class="t">{token.ref.target.title || token.ref.target.entity_id}</span>
        {#if token.ref.target.deleted_at}<span class="wd">withdrawn</span>{/if}
      </button>{:else}<span class="chip unres" title="Nothing carries {token.ref.target_id}"
        ><span class="t">{token.ref.target_id}</span><span class="wd">unresolved</span></span
      >{/if}{/each}</p
>

<style>
  .nbody {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.55;
    color: var(--text);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    margin: 0 1px;
    padding: 1px 6px 1px 3px;
    border: 1px solid var(--hair2);
    border-radius: 10px;
    background: var(--raised);
    color: var(--link);
    font: inherit;
    vertical-align: baseline;
    cursor: pointer;
  }

  .chip:hover,
  .chip:focus-visible {
    border-color: var(--link);
  }

  .chip .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip.unres {
    padding-left: 6px;
    border-style: dashed;
    color: var(--amber);
    cursor: default;
  }

  .chip.unres:hover {
    border-color: var(--hair2);
  }

  .wd {
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--amber);
  }
</style>
