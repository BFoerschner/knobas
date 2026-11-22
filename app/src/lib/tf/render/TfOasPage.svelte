<!--
  The API explorer's page column (ux31, sections 7 to 10): the overview, an operation or a schema, whichever the
  worker last answered (`OasState.shown`), swapped whole in one frame with the column scrolled to its top. Past
  200 ms the old page says *Reading…* in its place; nothing is drawn half. A followed link puts *Back to ⟨where you
  were⟩* at its top. A path item kept in another file is read when picked and drawn as its operations.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Operation, PathRef } from "../../render/openapi";
  import TfOasOperation from "./TfOasOperation.svelte";
  import TfOasOverview from "./TfOasOverview.svelte";
  import TfOasSchemaPage from "./TfOasSchemaPage.svelte";
  import { labelOf, type OasState, type Place } from "./oas.svelte";

  let {
    st,
    pref = null,
    prefOps = null,
    prose = null,
    onfollow,
    ongo,
    onback,
    onopengroup,
    oncopylink = null,
    onuse = null,
    head = null,
    reference = true,
    scroller = $bindable(null),
  }: {
    st: OasState;
    /** A path item kept in another file, picked in the rail, and its operations once read. */
    pref?: PathRef | null;
    prefOps?: { ok: true; ops: Operation[] } | { ok: false; error: string } | null;
    prose?: Snippet<[string]> | null;
    onfollow: (name: string) => void;
    ongo: (place: Place, label: string) => void;
    onback: () => void;
    onopengroup: (key: string) => void;
    oncopylink?: ((place: Place, label: string) => void) | null;
    onuse?: ((type: string, value: unknown) => void) | null;
    /** What stands at the page's head: the *Reference / Request* switch under 1,480 px. */
    head?: Snippet | null;
    /** Whether the page draws its reference; under 1,480 px the head's *Request* stands in its place. */
    reference?: boolean;
    scroller?: HTMLElement | null;
  } = $props();

  const shown = $derived(st.shown);
  const copy = (place: Place, label: string) => (oncopylink === null ? null : () => oncopylink?.(place, label));
</script>

<div class="tf-oas-col tf-oas-page" bind:this={scroller} tabindex="-1" aria-label="Reference">
  {#if st.backs.length > 0 && pref === null}
    <div class="tf-oas-back"><button type="button" class="tf-btn tf-sm" onclick={onback}>Back to {st.backs.at(-1)?.label}</button></div>
  {/if}
  {#if head !== null}{@render head()}{/if}
  {#if !reference}
    <!-- The request stands in the reference's place. -->
  {:else if st.reading}
    <p class="tf-oas-reading" role="status">Reading…</p>
  {:else if pref !== null}
    {@const href = st.hrefOf(st.root, pref.ref)}
    <h2 class="tf-oas-h"><span class="tf-oas-path">{pref.path}</span></h2>
    <div class="tf-oas-meta"><span>Kept in <code>{pref.ref}</code></span>{#if href !== null}<a class="tf-oas-link" href={href}>Open the file</a>{/if}</div>
    {#if prefOps === null}
      <p class="tf-oas-reading" role="status">Reading {pref.ref}…</p>
    {:else if !prefOps.ok}
      <p class="tf-oas-gone">{prefOps.error}</p>
    {:else}
      {#each prefOps.ops as op (op.method)}
        <section class="tf-oas-sec"><TfOasOperation {st} {op} k="{op.method} {op.path}" {prose} {onfollow} {ongo} /></section>
      {/each}
    {/if}
  {:else if shown.answer === null}
    {#if st.index !== null}<TfOasOverview {st} index={st.index} {prose} {onopengroup} oncopylink={copy("", "the overview")} />{/if}
  {:else if shown.answer.kind === "op"}
    {@const op = shown.answer.op}
    <TfOasOperation {st} {op} k={shown.place} {prose} {onfollow} {ongo} oncopylink={copy(shown.place, labelOf(shown.place))} {onuse} />
  {:else if shown.answer.kind === "schema"}
    {@const s = shown.answer}
    <TfOasSchemaPage {st} name={s.name} node={s.node} shape={s.shape} uses={s.uses} sample={s.sample} given={s.given} {prose} {onfollow} {ongo} oncopylink={copy(shown.place, s.name)} />
  {/if}
</div>
