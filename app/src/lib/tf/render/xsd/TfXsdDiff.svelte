<!--
  A schema in a comparison (the XSD design, 2026-11-23, section 27; step S7): what changed between two versions of a
  schema, by declaration, not by line. One list as wide as the stage, after a line of counts:

  - **A row** is a mark in words (*Changed*, *Added*, *Removed*, *Changed through*), the declaration's name, what
    changed in one line, and at its end which way: *stricter* (a document that passed may fail now), *looser* (one that
    passes now may have failed before) or *different*. Its left edge is the stage's own: `--ok`, `--err`, `--warn`.
  - **Opened**, it says each fact that differs, in words, and under them the declaration on both sides as the explorer
    draws it.
  - `n` and `p` step through the rows (`nextChange`), opening each.

  Each side is read as a set at its own commit, in the schema worker, on one engine, and compared there
  (`render/xsd-changes.ts`). Formatting is no change: a reformatted schema says *The same schema on both sides*.
-->
<script lang="ts">
  import "../../css/openapi.css";
  import "../../css/xsd.css";

  import { tick, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import { XsdPort, compareSets, xsdEngine, type XsdEngine } from "../../../render/xsd-port";
  import type { ChangeRow, ChangesAnswer } from "../../../render/xsd-answer";
  import { isXsd } from "../../../render/xsd-sniff";
  import TfXsdPage from "./TfXsdPage.svelte";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import { XsdState } from "./xsd.svelte";

  type Side = "old" | "new";

  let {
    before,
    after,
    path,
    beforePath = null,
    read,
    engine = null,
    onsay = () => {},
  }: {
    /** The side before as text; `null` where the file is not on that side. */
    before: string | null;
    after: string | null;
    /** The file's path after the change, and before it where it moved. */
    path: string;
    beforePath?: string | null;
    /** Another file of the set at a side's commit: the diff's own reader (`TfDiffView`). */
    read: (side: Side, path: string) => Promise<string | null>;
    /** Where the sets are kept: the worker when left out. */
    engine?: XsdEngine | null;
    /** What the stage's foot says. */
    onsay?: (words: string) => void;
  } = $props();

  let answer = $state.raw<ChangesAnswer | null>(null);
  let ports: Record<Side, XsdPort | null> = { old: null, new: null };
  /** Each side that is there but holds no XML Schema. */
  const notSchema = $derived({ old: before !== null && !isXsd(before), new: after !== null && !isXsd(after) });
  const neither = $derived((before === null || notSchema.old) && (after === null || notSchema.new));

  $effect(() => {
    const a = before;
    const b = after;
    const at = { old: beforePath ?? path, new: path };
    const eng = untrack(() => engine) ?? xsdEngine();
    let live = true;
    answer = null;
    open.clear();
    cur = -1;
    const opened: XsdPort[] = [];
    const side = async (s: Side, text: string | null): Promise<XsdPort | null> => {
      if (text === null || !isXsd(text)) return null;
      const port = new XsdPort(eng);
      opened.push(port);
      const reads = {
        paths: () => Promise.resolve([] as string[]),
        read: (p: string) => read(s, p),
        index: () => Promise.resolve({ declares: [], catalogs: [], total: 0, read: 0 }),
        pointed: [],
      };
      const done = await port.open(at[s], text, reads);
      if (!done.ok) return null;
      await port.whole;
      return port;
    };
    void Promise.all([side("old", a), side("new", b)]).then(async ([po, pn]) => {
      if (!live) return;
      ports = { old: po, new: pn };
      if (po === null && pn === null) return;
      const got = await compareSets(po, pn, at.old, at.new);
      if (live) answer = got;
    });
    return () => {
      live = false;
      for (const port of opened) port.close();
      ports = { old: null, new: null };
    };
  });

  const plural = (n: number, one: string, many: string) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  /** The counts' line, and what the stage's foot says. */
  const said = $derived.by(() => {
    const c = answer;
    if (neither) return "Neither side is an XML Schema: Lines has the change.";
    if (c === null) return "Comparing the two sides…";
    if (c.rows.length === 0) return "The same schema on both sides.";
    if (!c.sides.old) return `${plural(c.added, "declaration", "declarations")}, all added`;
    if (!c.sides.new) return `${plural(c.removed, "declaration", "declarations")}, all removed`;
    return `${plural(c.changed, "declaration changed", "declarations changed")}, ${c.added.toLocaleString("en")} added, ${c.removed.toLocaleString("en")} removed`;
  });
  $effect(() => onsay(said));

  // ---- Rows: opened ones draw their facts and both sides' declarations ----
  const open = new SvelteSet<string>();
  let cur = $state(-1);
  let list = $state<HTMLElement | null>(null);
  /** Each opened row's declaration on each side: its page, `null` where it is not declared there. */
  let pages = $state.raw<Record<string, Record<Side, XsdState | null | "reading">>>({});

  async function pageOf(row: ChangeRow, s: Side): Promise<XsdState | null> {
    const port = ports[s];
    if (port === null || (s === "old" && row.mark === "added") || (s === "new" && row.mark === "removed")) return null;
    const got = await port.page(row.place);
    if ("missing" in got) return null;
    const st = new XsdState(port);
    st.page = got;
    return st;
  }
  function toggle(row: ChangeRow) {
    if (open.has(row.key)) {
      open.delete(row.key);
      return;
    }
    open.add(row.key);
    if (pages[row.key] !== undefined) return;
    pages = { ...pages, [row.key]: { old: "reading", new: "reading" } };
    for (const s of ["old", "new"] as const)
      void pageOf(row, s).then((st) => {
        const had = pages[row.key];
        if (had !== undefined) pages = { ...pages, [row.key]: { ...had, [s]: st } };
      });
  }

  /** `n` and `p`: the next or previous row, opened and brought into view; false where there is none that way. */
  export function nextChange(step: 1 | -1, entering = false): boolean {
    const rows = answer?.rows ?? [];
    const to = entering ? (step === 1 ? 0 : rows.length - 1) : cur + step;
    if (to < 0 || to >= rows.length) return false;
    cur = to;
    const row = rows[to]!;
    if (!open.has(row.key)) toggle(row);
    void tick().then(() => list?.querySelector<HTMLElement>(`[data-crow="${to}"]`)?.scrollIntoView?.({ block: "nearest" }));
    return true;
  }

  /** The rows drawn: a file of thousands of declarations, all added, says how many more there are. */
  const ROWS_SHOWN = 600;
  const WORD = { changed: "Changed", added: "Added", removed: "Removed", through: "Changed through" } as const;
  const DIRS = ["stricter", "looser", "different"] as const;
</script>

{#snippet sideOf(row: ChangeRow, s: Side)}
  {@const got = pages[row.key]?.[s]}
  <div class="tf-xsd-cside">
    <span class="tf-xsd-cwhich">{s === "old" ? "Before" : "After"}</span>
    {#if got === "reading" || got === undefined}
      <p class="tf-xsd-gone">Reading…</p>
    {:else if got === null}
      <p class="tf-xsd-gone">{s === "old" ? "Not declared before." : "No longer declared."}</p>
    {:else}
      <TfXsdPage st={got} ongo={() => {}} onback={() => {}} onfile={() => {}} />
    {/if}
  </div>
{/snippet}

<div class="tf-xsd-cmp">
  {#if neither}
    <p class="tf-jar-dnote tf-roas-no">Neither side is an XML Schema: Lines has the change.</p>
  {:else if answer === null}
    <p class="tf-oas-wait" role="status">Comparing the two sides…</p>
  {:else}
    {@const c = answer}
    <div class="tf-xsd-csum">
      {#if c.rows.length === 0}
        <b>The same schema on both sides.</b>
        <span class="tf-dim">{plural(c.same, "declaration reads", "declarations read")} the same, however {c.same === 1 ? "it is" : "they are"} written.</span>
      {:else}
        <b>{said}.</b>
        {#if c.sides.old && c.sides.new}
          <span>{c.counts.stricter.toLocaleString("en")} stricter, {c.counts.looser.toLocaleString("en")} looser, {c.counts.different.toLocaleString("en")} different.</span>
          <span class="tf-dim">{plural(c.through, "more changed", "more changed")} through a type {c.through === 1 ? "it names" : "they name"}. {plural(c.same, "reads", "read")} the same, however {c.same === 1 ? "it is" : "they are"} written.</span>
        {/if}
      {/if}
      {#if notSchema.old}<span class="tf-xsd-cnot">Not an XML Schema on the side before.</span>{/if}
      {#if notSchema.new}<span class="tf-xsd-cnot">Not an XML Schema on the side after.</span>{/if}
    </div>
    <div class="tf-xsd-clist" bind:this={list}>
      {#each c.rows.slice(0, ROWS_SHOWN) as row, i (row.key)}
        {@const opened = open.has(row.key)}
        <div class="tf-xsd-crow" class:tf-xsd-cur={cur === i} data-mark={row.mark} data-crow={i}>
          <button type="button" class="tf-xsd-cbtn" aria-expanded={opened} onclick={() => ((cur = i), toggle(row))}>
            <svg class="tf-oas-caret" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
            <span class="tf-xsd-cmark">{WORD[row.mark]}</span>
            <span class="tf-xsd-cname">{row.label}{#if row.via !== null}{" "}<span class="tf-xsd-pre">{row.via}</span>{/if}</span>
            <span class="tf-xsd-csay">{row.say}</span>
            <span class="tf-xsd-dirs">
              {#each DIRS as d (d)}{@const n = row.dirs[d] ?? 0}{#if n > 0}<span class="tf-xsd-dir" data-dir={d}>{n > 1 ? `${n} ` : ""}{d}</span>{/if}{/each}
            </span>
          </button>
          {#if opened}
            {#if row.facts.length > 0}
              <ul class="tf-xsd-cfacts">
                {#each row.facts as f, n (n)}
                  <li><span class="tf-xsd-cplace">{f.place === "" ? row.label : f.place}</span>{" "}<span><TfXsdRuns runs={f.text} /></span>{#if f.dir !== null}{" "}<span class="tf-xsd-dir" data-dir={f.dir}>{f.dir}</span>{/if}</li>
                {/each}
              </ul>
            {:else if row.mark === "through"}
              <ul class="tf-xsd-cfacts"><li><span>Nothing in its own declaration changed. It names <span class="tf-xsd-cplace">{row.via}</span>, which did.</span></li></ul>
            {/if}
            <div class="tf-xsd-cpair">
              {@render sideOf(row, "old")}
              {@render sideOf(row, "new")}
            </div>
          {/if}
        </div>
      {/each}
      {#if c.rows.length > ROWS_SHOWN}<p class="tf-xsd-cmore">And {(c.rows.length - ROWS_SHOWN).toLocaleString("en")} more declarations: Lines has the whole change.</p>{/if}
    </div>
  {/if}
</div>
