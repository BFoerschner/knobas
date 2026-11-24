<!--
  The rail's *Results* (the XML view's design, 2026-11-23, sections 22 and 23): the matches, 28 px a row, windowed: the
  line, the node's name, its value cut. A press goes to its line and picks it. An expression that answers a value shows
  it large. With nothing asked, queries to press: what was asked since knobas started, then queries made from the
  document's own names.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { Windowed } from "../../../shell/windowed.svelte";
  import { askedBefore, type DocState } from "./doc.svelte";

  let {
    st,
    ongo,
    onask,
    list = $bindable(null),
  }: {
    st: DocState;
    /** Goes to result `i`'s line and picks it. */
    ongo: (i: number) => void;
    /** Types a query into the field. */
    onask: (source: string) => void;
    list?: HTMLElement | null;
  } = $props();

  const q = $derived(st.q);
  const rows = $derived(st.rows);
  const win = new Windowed<number>({ count: () => rows.length, key: (i) => i, estimate: 28, from: 60, scrolls: true });
  const start = $derived(win.on ? win.start : 0);
  /** How many queries asked before the list offers. */
  const ASKED_SHOWN = 6;
  // Near the last row drawn, the next rows are asked for.
  $effect(() => {
    const end = win.end;
    if (win.on && end >= rows.length - 40) untrack(() => st.more());
  });

  let examples = $state<string[]>([]);
  $effect(() => {
    const state = st;
    if (state.opened === null) return;
    void state.session.examples().then((list) => (examples = list));
  });
</script>

{#if q.state === "nodes"}
  <div class="tf-xv-rlist" bind:this={list} {@attach win.list} role="presentation">
    {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
    {#each win.slice(rows) as r, k (start + k)}
      {@const i = start + k}
      <button type="button" class="tf-oas-row tf-xv-rrow" role="option" aria-selected={st.cur === i} data-xv-res={i} {@attach win.row(i)} onclick={() => ongo(i)}>
        <span class="tf-xv-fline">{r.line.toLocaleString("en")}</span>
        <span class="tf-xv-fname">{r.name}</span>
        {#if r.value === null}<span class="tf-xv-rkind">{r.kids.toLocaleString("en")} {r.kids === 1 ? "element" : "elements"}</span>{:else}<span class="tf-xv-rval">{r.value}</span>{/if}
      </button>
    {/each}
    {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
  </div>
{:else if q.state === "value"}
  <div class="tf-xv-one"><span class="tf-xv-occ">{q.text}</span><span class="tf-dim">{q.type === "number" ? "a number" : q.type === "boolean" ? "true or false" : "a string"}</span></div>
{:else if q.state === "empty" || q.state === "error"}
  {@const before = askedBefore()}
  <div class="tf-xv-try">
    {#if before.length > 0}
      <h4>Asked before, since knobas started</h4>
      {#each before.slice(0, ASKED_SHOWN) as s (s)}<button type="button" class="tf-xv-ex" onclick={() => onask(s)}>{s}</button>{/each}
    {/if}
    {#if examples.length > 0}
      <h4>Ask this document</h4>
      {#each examples as s (s)}<button type="button" class="tf-xv-ex" onclick={() => onask(s)}>{s}</button>{/each}
    {/if}
  </div>
{/if}
