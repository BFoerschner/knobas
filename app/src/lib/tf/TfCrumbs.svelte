<!--
  The breadcrumb (R-1.6, spec #2227): the real path to the page you're on,
  each place above it linking to itself, and the page's own name last, as
  `Builds › Run #4812`. On the right, *Back to <name>* beside `Esc` names
  where `Esc` goes; where `Esc` does nothing (a top page, an item's or a
  note's page) there is no hint. `back` is the shell's `escTarget`, the same
  answer the key acts on, so the two cannot disagree. No job's chip: the
  breadcrumb describes this page, not the job you were last in (#2228).

  `TfPage.svelte` draws it above a page that sets `crumbs` and above a full
  view not built yet, with {@link shellCrumbs}' path; a page that knows more
  of its own path (Builds, Settings, the item, note and ticket pages) draws it
  itself with its own `parts`. Its last part, when it links nowhere, is the
  name the page gives itself (`page-name.svelte.ts`): what *Back to <name>*
  calls this page once the reader has left it.
-->
<script module lang="ts">
  import type { EscTarget } from "./escape";
  import { HEAD_NAME, parentOf, parseTfHash, SECTION_NAME, type TfRoute } from "./route";

  /** One place on the path: linked when it has an address, the page's own name when it has none. */
  export interface Crumb {
    name: string;
    href?: string;
  }

  /**
   * The path the shell's own breadcrumb draws for `route`: the section's
   * name on its own pages, and on a deeper address the parent's name linking
   * to it, then what the address names.
   */
  export function shellCrumbs(route: Extract<TfRoute, { view: "full" }>): Crumb[] {
    const own = SECTION_NAME[route.section];
    const name = HEAD_NAME[route.head];
    const parent = parentOf(route);
    if (name === undefined || parent === null) return [{ name: own }];
    const up = parseTfHash(parent);
    return [{ name: up.view === "full" ? SECTION_NAME[up.section] : own, href: parent }, { name }];
  }
</script>

<script lang="ts">
  import { namePage } from "./page-name.svelte";

  let {
    parts,
    back,
  }: {
    /** The path, top first; the last is the page itself. */
    parts: readonly Crumb[];
    /** Where `Esc` goes, or `null` where it does nothing. */
    back: Pick<EscTarget, "name"> | null;
  } = $props();

  // The page's own name is the last part when it links nowhere: what *Back to <name>* calls this page once it is left.
  namePage(() => {
    const own = parts.at(-1);
    return own === undefined || own.href !== undefined || own.name === "" ? null : own.name;
  });
</script>

<nav class="tf-crumbs" aria-label="Breadcrumb">
  {#each parts as part, at (at)}
    {#if part.href !== undefined}
      <a class="tf-crumb" href={part.href}>{part.name}</a><span class="tf-sep">›</span>
    {:else}
      <strong>{part.name}</strong>
    {/if}
  {/each}
  {#if back !== null}
    <span class="tf-esc">Back to {back.name} <kbd>Esc</kbd></span>
  {/if}
</nav>
