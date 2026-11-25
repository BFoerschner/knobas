<!--
  A schema on a Docs page (the XSD design, 2026-11-23, section 28; step D1): the block's own declarations in a box
  420 px tall that scrolls inside itself. A row is a name and its kind, and opens what it holds in place, as the
  explorer's page draws it. A name from another file is drawn as written: a Docs page reads no other file. There is no
  example and no check: both need the set.
-->
<script lang="ts">
  import "../../css/openapi.css";
  import "../../css/xsd.css";

  import { untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { XsdPort, xsdEngine, type XsdEngine } from "../../../render/xsd-port";
  import type { DeclRow, SetIndex } from "../../../render/xsd-answer";
  import TfXsdPage from "./TfXsdPage.svelte";
  import { XsdState } from "./xsd.svelte";

  let { text, engine = null }: { text: string; engine?: XsdEngine | null } = $props();

  let index = $state.raw<SetIndex | null>(null);
  let failed = $state<string | null>(null);
  let port: XsdPort | null = null;
  const ENTRY = "block.xsd";
  $effect(() => {
    const source = text;
    const p = new XsdPort(untrack(() => engine) ?? xsdEngine());
    port = p;
    index = null;
    failed = null;
    open.clear();
    pages.clear();
    let live = true;
    // No reads: a Docs page reads no other file, so every reference beyond the block stays as written.
    const reads = { paths: () => Promise.resolve([] as string[]), read: () => Promise.resolve(null), index: () => Promise.resolve({ declares: [], catalogs: [], total: 0, read: 0 }), pointed: [] };
    void p.open(ENTRY, source, reads).then((answer) => {
      if (!live) return;
      if (answer.ok) index = answer.index;
      else failed = answer.error;
    });
    return () => {
      live = false;
      p.close();
    };
  });

  /** The declarations drawn: a block holding a whole schema of thousands says how many more. */
  const DECLS_SHOWN = 400;
  const own = $derived((index?.decls ?? []).filter((d) => d.file === ENTRY));
  const KIND: Record<DeclRow["kind"], string> = { el: "element", ct: "complex type", st: "simple type", gr: "group", ag: "attribute group", at: "attribute" };
  const PLACE: Record<DeclRow["kind"], string> = { el: "el", ct: "type", st: "type", gr: "group", ag: "attrs", at: "attr" };

  const open = new SvelteSet<string>();
  const pages = new SvelteMap<string, XsdState | null>();
  function toggle(d: DeclRow) {
    if (open.has(d.key)) {
      open.delete(d.key);
      return;
    }
    open.add(d.key);
    if (pages.has(d.key) || port === null) return;
    const p = port;
    void p.page(`${PLACE[d.kind]}/${d.prefix}${d.local}`).then((got) => {
      if (p !== port) return;
      if ("missing" in got) pages.set(d.key, null);
      else {
        const st = new XsdState(p);
        st.page = got;
        pages.set(d.key, st);
      }
    });
  }
</script>

<div class="tf-xsd-inline tf-dview" aria-label="Schema">
  {#if failed !== null}
    <p class="tf-stepfail" role="alert">This block is no schema knobas can read: {failed}</p>
  {:else if index === null}
    <p class="tf-oas-wait" role="status">Reading the schema…</p>
  {:else}
    {#each own.slice(0, DECLS_SHOWN) as d (d.key)}
      {@const opened = open.has(d.key)}
      <div class="tf-xsd-irow">
        <button type="button" class="tf-xsd-ibtn" aria-expanded={opened} onclick={() => toggle(d)}>
          <svg class="tf-oas-caret" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
          <span class="tf-xsd-cname">{d.prefix}{d.local}</span>
          <span class="tf-xsd-ikind">{KIND[d.kind]}</span>
        </button>
        {#if opened}
          {@const st = pages.get(d.key)}
          <div class="tf-xsd-iopen">
            {#if st === undefined}<p class="tf-xsd-gone">Reading…</p>
            {:else if st === null}<p class="tf-xsd-gone">It names what this block does not hold.</p>
            {:else}<TfXsdPage {st} ongo={() => {}} onback={() => {}} onfile={() => {}} />{/if}
          </div>
        {/if}
      </div>
    {/each}
    {#if own.length > DECLS_SHOWN}<p class="tf-xsd-cmore">And {(own.length - DECLS_SHOWN).toLocaleString("en")} more declarations: Source has them all.</p>{/if}
  {/if}
</div>
