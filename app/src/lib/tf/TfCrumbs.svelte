<!--
  The breadcrumb (R-1.6, spec #2227): the real path to the page you're on,
  each place above it linking to itself, and the page's own name last, as
  `Builds › Run #4812`. On the right, *Back to <name>* beside `Esc` (a link,
  `TfBackHint.svelte`) names where `Esc` goes; where `Esc` does nothing (a top page, an item's or a
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
  import type { EscTarget, EscTargetOf } from "./escape";
  import { HEAD_NAME, parentOf, parseTfHash, SECTION_NAME, type TfRoute } from "./route";

  /** One place on the path: linked when it has an address; with none, the page's own name, or a place you are already at. */
  export interface Crumb {
    name: string;
    href?: string;
    /** The name is a code value other than a ticket key, which `isKey` finds by itself: a repository's name. */
    code?: boolean;
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
  import TfBackHint from "./TfBackHint.svelte";
  import TfKey, { isKey } from "./TfKey.svelte";

  let {
    parts,
    back,
    sectionTop = false,
  }: {
    /** The path, top first; the last is the page itself. */
    parts: readonly Crumb[];
    /** Where `Esc` goes, or `null` where it does nothing. */
    back: (Pick<EscTarget, "name"> & Partial<EscTargetOf>) | null;
    /** The shell's own breadcrumb for a page with a nav entry: a path of one draws nothing. */
    sectionTop?: boolean;
  } = $props();

  // The page's own name is the last part when it links nowhere: what *Back to <name>* calls this page once it is left.
  namePage(() => {
    const own = parts.at(-1);
    return own === undefined || own.href !== undefined || own.name === "" ? null : own.name;
  });

  /**
   * A section's own page has no path to show: the nav lights the section and the page's title names it, so a bar
   * reading only that name again is drawn nowhere. One that goes somewhere else still draws, as does any longer path.
   * Only the shell asks for this: a page that draws its own breadcrumb, as an item's does, keeps it.
   */
  const repeatsItself = $derived(sectionTop && parts.length === 1 && (back === null || back.name === parts[0]?.name));
</script>

{#if !repeatsItself}
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#each parts as part, at (at)}
      {@const code = part.code === true || isKey(part.name)}
      {#if part.href !== undefined}
        {#if code}<TfKey value={part.name} href={part.href} />{:else}<a class="tf-crumb" href={part.href}>{part.name}</a>{/if}<span class="tf-sep">›</span>
      {:else if at < parts.length - 1}
        <!-- A place on the path you are already at: it names itself, and a link would lead nowhere. -->
        {#if code}<TfKey value={part.name} class="tf-crumb-here" />{:else}<span class="tf-crumb-here">{part.name}</span>{/if}<span class="tf-sep">›</span>
      {:else if code}
        <strong><TfKey value={part.name} lead /></strong>
      {:else}
        <strong>{part.name}</strong>
      {/if}
    {/each}
    {#if back !== null}
      <TfBackHint {back} />
    {/if}
  </nav>
{/if}
