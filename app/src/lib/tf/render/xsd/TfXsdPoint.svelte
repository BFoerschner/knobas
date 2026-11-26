<!--
  *Point to a file…* (the XSD design, section 17): the repository's `.xsd` files, started on the location's name, those
  that declare the wanted namespace first. A file that declares another is listed dim with what it declares and cannot
  be picked. A pick is kept for that namespace and location (`xsd.locations`) and used by every schema and document that
  asks for them, in any repository.
-->
<script lang="ts">
  // Its own stylesheet: the XML view draws it too, before any schema file has loaded the explorer's.
  import "../../css/xsd.css";
  import TfDialog from "../../settings/TfDialog.svelte";
  import { filterKeys } from "../../filter-keys";
  import Marked from "../../filter/Marked.svelte";
  import { heldOrder } from "../../filter/ranked.svelte";
  import { pointSorted, pointText, type Candidate } from "./xsd-point";

  let {
    namespace,
    location,
    candidates,
    onpick,
    onclose,
  }: {
    /** The namespace asked for (`null` for an include's: any of the includer's or none). */
    namespace: string | null;
    location: string | null;
    /** The files to choose among; `null` while they are read. */
    candidates: readonly Candidate[] | null;
    onpick: (c: Candidate) => void;
    onclose: () => void;
  } = $props();

  const base = (p: string) => p.slice(p.lastIndexOf("/") + 1);
  // svelte-ignore state_referenced_locally
  let typed = $state(location === null ? "" : base(location.replace(/[?#].*$/, "")));
  const fits = (c: Candidate) => namespace === null || c.declares === namespace;
  /**
   * The candidates the words keep, ranked by the app (#2535): those that fit first, this repository's before
   * another's, then best first (nothing typed: the shorter path). The dialog opens with the location's name typed, so
   * the list waits for its first answer rather than drawing the old filter and moving when the answer lands.
   */
  const all = $derived(candidates ?? []);
  const order = heldOrder<Candidate>("xsd-point");
  $effect(() => order.ask(typed, all, pointText));
  const typedNow = $derived(typed.trim() !== "");
  const shown = $derived.by(() => {
    if (!order.ready(typed, all)) return null;
    const kept = order.rows(typed, all, pointText);
    // A name typed that matches nothing still lists every file: the location's own name may not be the file's.
    return kept.length === 0 && typedNow ? pointSorted(all, fits, false) : pointSorted(kept, fits, typedNow);
  });
  const marking = $derived(order.typed(typed, all));
  /** Where a file's path starts in its ranked text. */
  const pathAt = (c: Candidate) => (c.repoName ?? "").length + 1;
  /** The rows drawn: a repository's schemas, at most this many (the index's own bound). */
  const SHOWN = 300;
</script>

<TfDialog title="Point to a file" subtitle={location ?? namespace ?? ""} {onclose} className="tf-xsd-point">
  {#snippet body()}
    <p class="tf-xsd-pointwhy">
      {#if namespace !== null}A file that declares <code class="tf-xsd-code">{namespace === "" ? "no namespace" : namespace}</code>.{" "}{/if}It is used from now on wherever a
      schema or a document asks for {location !== null ? "this location" : "this namespace"}.
    </p>
    <label class="tf-minif">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
      <input aria-label="Find a schema file" placeholder="Find a schema file" bind:value={typed} {@attach filterKeys()} />
    </label>
    <div class="tf-xsd-pointlist" role="listbox" aria-label="Schema files">
      {#if candidates === null}
        <p class="tf-oas-fnote" role="status">Reading the repository's schemas…</p>
      {:else if shown === null}
        <!-- The first answer is out: nothing drawn yet, so nothing moves when it lands. -->
      {:else if shown.length === 0}
        <p class="tf-oas-fnote">No schema file here.</p>
      {:else}
        {#each shown.slice(0, SHOWN) as c (`${c.repo}\n${c.path}`)}
          {@const ok = fits(c)}
          <button type="button" role="option" aria-selected="false" class="tf-xsd-pointrow" class:tf-xsd-pointno={!ok} disabled={!ok} title={ok ? `Use ${c.path}` : `It declares ${c.declares === "" ? "no namespace" : c.declares}`} onclick={() => onpick(c)}>
            <span class="tf-xsd-pointname"><Marked text={base(c.path)} typed={marking} pairs={order.marks(c, pathAt(c) + c.path.lastIndexOf("/") + 1)} /></span>
            <span class="tf-xsd-pointpath">{c.repoName === null ? "" : `${c.repoName}${c.branch ? `, at ${c.branch}` : ""}: `}<Marked text={c.path} typed={marking} pairs={order.marks(c, pathAt(c), c.path.length)} /></span>
            {#if !ok}<span class="tf-xsd-pointns">declares {c.declares === "" ? "no namespace" : c.declares}</span>{/if}
          </button>
        {/each}
      {/if}
    </div>
  {/snippet}
</TfDialog>
