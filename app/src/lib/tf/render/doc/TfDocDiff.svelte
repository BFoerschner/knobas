<!--
  XML and JSON in a comparison (the XML view's design, 2026-11-23, section 28; step C1): what changed between two
  versions of a document, by element, not by line, on S7's list (`TfXsdDiff`'s rows and classes). A line of counts,
  then a row a part that differs: *Changed* with both values, *Added*, *Removed*, *Moved*, each at its path
  (`/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[2]/QUANTITY`, `.orders[3].total`). Formatting is no change, so a re-indented POM
  says *The same document on both sides* where its lines say 140 changed. Opened, a row says each value before and
  after. `n` and `p` step through the rows, opening each. Compared in the document worker (`render/doc-compare.ts`).
-->
<script lang="ts">
  import "../../css/openapi.css";
  import "../../css/xsd.css";

  import { tick, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import { compareInWorker, docEngine, type DocEngine } from "../../../render/doc-port";
  import type { DocChanges, DocRow } from "../../../render/doc-compare";

  let {
    before,
    after,
    kind,
    engine = null,
    onsay = () => {},
  }: {
    /** The side before as text; `null` where the file is not on that side. */
    before: string | null;
    after: string | null;
    kind: "xml" | "json";
    /** Where the documents are compared: the worker when left out. */
    engine?: DocEngine | null;
    onsay?: (words: string) => void;
  } = $props();

  let answer = $state.raw<DocChanges | null>(null);
  $effect(() => {
    const a = before;
    const b = after;
    const as = kind;
    let live = true;
    answer = null;
    open.clear();
    cur = -1;
    void compareInWorker(a, b, as, untrack(() => engine) ?? docEngine()).then((got) => {
      if (live) answer = got;
    });
    return () => {
      live = false;
    };
  });

  const plural = (n: number, one: string, many: string) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  const NOUN = $derived(kind === "xml" ? ["element", "elements"] : ["value", "values"]);
  const said = $derived.by(() => {
    const c = answer;
    if (c === null) return "Comparing the two sides…";
    if (c.bad.old !== null || c.bad.new !== null) return "Lines has the change.";
    if (c.rows.length === 0) return "The same document on both sides.";
    return `${plural(c.changed, `${NOUN[0]} changed`, `${NOUN[1]} changed`)}, ${c.added.toLocaleString("en")} added, ${c.removed.toLocaleString("en")} removed, ${c.moved.toLocaleString("en")} moved`;
  });
  $effect(() => onsay(said));

  const open = new SvelteSet<number>();
  let cur = $state(-1);
  let list = $state<HTMLElement | null>(null);

  /** `n` and `p`: the next or previous row, opened and brought into view; false where there is none that way. */
  export function nextChange(step: 1 | -1, entering = false): boolean {
    const rows = (answer?.rows ?? []).slice(0, ROWS_SHOWN);
    const to = entering ? (step === 1 ? 0 : rows.length - 1) : cur + step;
    if (to < 0 || to >= rows.length) return false;
    cur = to;
    open.add(to);
    void tick().then(() => list?.querySelector<HTMLElement>(`[data-crow="${to}"]`)?.scrollIntoView?.({ block: "nearest" }));
    return true;
  }
  function toggle(i: number) {
    cur = i;
    if (open.has(i)) open.delete(i);
    else open.add(i);
  }

  /** The rows drawn: a change of thousands of parts says how many more there are. */
  const ROWS_SHOWN = 600;
  const WORD: Record<DocRow["mark"], string> = { changed: "Changed", added: "Added", removed: "Removed", moved: "Moved" };
</script>

<div class="tf-xsd-cmp">
  {#if answer === null}
    <p class="tf-oas-wait" role="status">Comparing the two sides…</p>
  {:else}
    {@const c = answer}
    <div class="tf-xsd-csum">
      {#if c.bad.old !== null || c.bad.new !== null}
        {#if c.bad.old !== null}<span class="tf-xsd-cnot">The side before is {c.bad.old}.</span>{/if}
        {#if c.bad.new !== null}<span class="tf-xsd-cnot">The side after is {c.bad.new}.</span>{/if}
        <span>Lines has the change.</span>
      {:else if c.rows.length === 0}
        <b>The same document on both sides.</b>
        <span class="tf-dim">{plural(c.same, `${NOUN[0]} reads`, `${NOUN[1]} read`)} the same, however {c.same === 1 ? "it is" : "they are"} written.</span>
      {:else}
        <b>{said}.</b>
        <span class="tf-dim">{plural(c.same, "reads", "read")} the same, however {c.same === 1 ? "it is" : "they are"} written.</span>
      {/if}
    </div>
    <div class="tf-xsd-clist" bind:this={list}>
      {#each c.rows.slice(0, ROWS_SHOWN) as row, i (i)}
        {@const opened = open.has(i)}
        <div class="tf-xsd-crow" class:tf-xsd-cur={cur === i} data-mark={row.mark} data-crow={i}>
          <button type="button" class="tf-xsd-cbtn" aria-expanded={opened} onclick={() => toggle(i)}>
            <svg class="tf-oas-caret" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
            <span class="tf-xsd-cmark">{WORD[row.mark]}</span>
            <span class="tf-xsd-cname" title={row.path}>{row.path}</span>
            <span class="tf-xsd-csay">{row.say}</span>
            <span></span>
          </button>
          {#if opened}
            <ul class="tf-xsd-cfacts">
              {#each row.facts as f, n (n)}
                <li>
                  <span class="tf-xsd-cplace">{f.place}</span>{" "}
                  {#if f.before === null}<span>added, <code class="tf-xsd-code">{f.after}</code></span>
                  {:else if f.after === null}<span>removed, it was <code class="tf-xsd-code">{f.before}</code></span>
                  {:else}<span><code class="tf-xsd-code">{f.before}</code> → <code class="tf-xsd-code">{f.after}</code></span>{/if}
                </li>
              {/each}
              {#if row.mark === "added"}<li><span>On line {row.line.toLocaleString("en")} after.</span></li>
              {:else if row.mark === "removed"}<li><span>It stood on line {row.line.toLocaleString("en")} before.</span></li>
              {:else if row.mark === "moved"}<li><span>{row.say}: line {(row.otherLine ?? 0).toLocaleString("en")} before, line {row.line.toLocaleString("en")} after.</span></li>
              {:else}<li><span class="tf-dim">Line {(row.otherLine ?? 0).toLocaleString("en")} before, line {row.line.toLocaleString("en")} after.</span></li>{/if}
            </ul>
          {/if}
        </div>
      {/each}
      {#if c.rows.length > ROWS_SHOWN}<p class="tf-xsd-cmore">And {(c.rows.length - ROWS_SHOWN).toLocaleString("en")} more: Lines has the whole change.</p>{/if}
    </div>
  {/if}
</div>
