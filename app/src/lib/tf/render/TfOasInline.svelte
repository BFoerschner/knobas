<!--
  A spec drawn small (ux31, section 13): the explorer's **inline form**, for a place that is not a file view of its own:
  a Docs page's code block (`TfDocBlocks`) and a comparison's two sides (`TfDiffView`). The head (title, versions,
  counts), the filter, and the rail's rows as one full-width list: each row the method, the path and the summary, and
  each schema by its name. A row opens its reference in place under it (`TfOasOperation`, `TfOasSchemaPage`, sharing
  `OasState` with the explorer). There is no request column: which server, and whose secret, would be a guess.

  The rows are windowed inside the form's own box, which scrolls inside itself: 420 px tall on a Docs page, the stage's
  height in a comparison. A comparison hands it `marks` (`Added`, `Removed`, `Changed`, `Changed through Pet`) and
  holds the list to the rows marked; the rows it opens are the comparison's (`open`), so a changed operation opens on
  both sides at once.
-->
<script lang="ts" module>
  /** What a comparison says of a row: its mark, and for an operation changed through a schema, that schema's name. */
  export type InlineMark = { mark: "added" | "removed" | "changed"; via?: string };
</script>

<script lang="ts">
  import "../css/openapi.css";
  import { flushSync, untrack, type Snippet } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { Windowed } from "../../shell/windowed.svelte";
  import { filterKeys } from "../filter-keys";
  import Marked from "../filter/Marked.svelte";
  import { heldOrder } from "../filter/ranked.svelte";
  import TfOasOperation from "./TfOasOperation.svelte";
  import TfOasSchemaPage from "./TfOasSchemaPage.svelte";
  import { OasState, type Place } from "./oas.svelte";
  import type { PageAnswer, SpecIndex } from "../../render/openapi";
  import { shortMethod } from "../../render/openapi-view";
  import { SpecSession, specEngine, type SpecEngine } from "../../render/openapi-port";

  let {
    text,
    path = "openapi.yaml",
    engine = null,
    prose = null,
    marks = null,
    open = new SvelteSet<string>(),
    label = "API",
    fill = false,
    onindex = null,
  }: {
    text: string;
    /** Where the spec's relative refs would be read from; a Docs block has none to read. */
    path?: string;
    engine?: SpecEngine | null;
    prose?: Snippet<[string]> | null;
    /** A comparison's marks, by row key (an operation's anchor, `schemas/<name>`): the list holds to these rows. */
    marks?: ReadonlyMap<string, InlineMark> | null;
    /** The rows open, by key: a comparison hands both sides one set. */
    open?: SvelteSet<string>;
    /** The form's name for a screen reader. */
    label?: string;
    /** Takes its holder's height (a comparison's side), rather than a Docs block's fixed 420 px. */
    fill?: boolean;
    /** Hears the spec's index once read, or `null` where it is no spec. */
    onindex?: ((index: SpecIndex | null, error: string | null) => void) | null;
  } = $props();

  type Row = { k: string; kind: "op"; method: string; path: string; summary: string; deprecated: boolean } | { k: string; kind: "schema"; name: string };

  let st = $state<OasState | null>(null);
  let failed = $state<string | null>(null);
  let typed = $state("");
  $effect(() => {
    const source = text;
    const root = path;
    const session = new SpecSession(untrack(() => engine) ?? specEngine(), null);
    const state = new OasState(session, root);
    st = null;
    failed = null;
    pages = {};
    let live = true;
    void session.open(root, source).then((answer) => {
      if (!live) return;
      if (answer.ok) {
        state.take(answer.index);
        st = state;
      } else failed = answer.error;
      untrack(() => onindex)?.(answer.ok ? answer.index : null, answer.ok ? null : answer.error);
    });
    return () => {
      live = false;
      session.close();
    };
  });

  const index = $derived(st?.index ?? null);
  const all = $derived.by((): Row[] => {
    if (index === null) return [];
    const ops = [...index.ops, ...index.webhooks].map((o): Row => ({ k: o.anchor, kind: "op", method: o.method, path: o.path, summary: o.summary, deprecated: o.deprecated }));
    const schemas = index.schemas.map((name): Row => ({ k: `schemas/${name}`, kind: "schema", name }));
    return [...ops, ...schemas];
  });
  /** The rows a comparison marked, else all; the typed words rank them through the app's ranking (#2535), best first. */
  const held = $derived(marks === null ? all : all.filter((r) => marks!.has(r.k)));
  /** What the filter ranks a row by: an operation's method, path and summary; a schema's name. */
  const rowText = (r: Row) => (r.kind === "op" ? `${r.method} ${r.path} ${r.summary}` : r.name);
  const order = heldOrder<Row>("oas-inline");
  $effect(() => order.ask(typed, held, rowText));
  const rows = $derived(order.rows(typed, held, rowText));
  const marking = $derived(order.typed(typed, held));
  const opCount = $derived(index === null ? 0 : index.ops.length);

  /** Each open row's page, once read. */
  let pages = $state.raw<Record<string, PageAnswer>>({});
  $effect(() => {
    const state = st;
    if (state === null) return;
    for (const k of open) {
      if (untrack(() => pages[k]) !== undefined || !state.has(k)) continue;
      void state.load(k).then((answer) => {
        if (st === state) pages = { ...pages, [k]: answer };
      });
    }
  });

  let list = $state<HTMLElement | null>(null);
  const win = new Windowed<string>({ count: () => rows.length, key: (i) => rows[i]!.k, estimate: 30, scrolls: true, from: 60 });

  function toggle(k: string) {
    if (open.has(k)) open.delete(k);
    else open.add(k);
  }
  /** Opens a place a link inside a row names, and brings its row into view. */
  function reach(place: Place) {
    const k = place.startsWith("schemas/") ? place.split("/").slice(0, 2).join("/") : place;
    const at = rows.findIndex((r) => r.k === k);
    if (at < 0) return;
    open.add(k);
    win.reveal(at);
  }

  /** Brings row `k` into view, where this side has it: a comparison's `n` and `p`. Answers whether it does. */
  export function revealKey(k: string): boolean {
    const at = rows.findIndex((r) => r.k === k);
    if (at < 0) return false;
    win.reveal(at);
    flushSync();
    const box = list;
    const row = box?.querySelector<HTMLElement>(`[data-k="${CSS.escape(k)}"]`);
    if (box && row) box.scrollTop += row.getBoundingClientRect().top - box.getBoundingClientRect().top - 4;
    return true;
  }

  const markWords = (m: InlineMark) => (m.mark === "added" ? "Added" : m.mark === "removed" ? "Removed" : m.via !== undefined ? `Changed through ${m.via}` : "Changed");
</script>

<div class="tf-oas-inline tf-dview" class:tf-oas-ifill={fill} role="region" aria-label={label}>
  {#if failed !== null}
    <p class="tf-oas-fnote">{failed}</p>
  {:else if st === null || index === null}
    <p class="tf-oas-wait" role="status">Reading the spec…</p>
  {:else}
    {@const state = st}
    <div class="tf-oas-ihead">
      <div class="tf-oas-api"><b title={index.title}>{index.title}</b>{#if index.apiVersion !== ""}<span class="tf-lab">{index.apiVersion}</span>{/if}<span class="tf-lab">{index.key === "swagger" ? "Swagger" : "OpenAPI"} {index.version}</span></div>
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <!-- `Esc` clears it as it clears every filter; it narrows this block's own list, so ⌘F keeps the page's meaning. -->
        <input placeholder="Filter endpoints and schemas" aria-label="Filter endpoints and schemas" bind:value={typed} {@attach filterKeys({ rail: true })} />
      </label>
      <span class="tf-oas-count">{marks === null ? `${opCount.toLocaleString("en")} ${opCount === 1 ? "endpoint" : "endpoints"} · ${index.schemas.length.toLocaleString("en")} ${index.schemas.length === 1 ? "schema" : "schemas"}` : `${rows.length.toLocaleString("en")} changed`}</span>
    </div>
    {#if rows.length === 0}
      <p class="tf-oas-fnote">{typed.trim() !== "" ? "Nothing here holds those words." : marks !== null ? "No endpoint or schema differs." : "The spec lists no endpoint."}</p>
    {:else}
      <div class="tf-oas-ilist" bind:this={list} {@attach win.list}>
        {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
        {#each win.slice(rows) as row (row.k)}
          {@const mark = marks?.get(row.k)}
          {@const page = open.has(row.k) ? pages[row.k] : undefined}
          <div class="tf-oas-irow" data-k={row.k} data-mark={mark?.mark} {@attach win.row(row.k)}>
            <button type="button" class="tf-oas-ibtn" aria-expanded={open.has(row.k)} onclick={() => toggle(row.k)}>
              {#if row.kind === "op"}
                <span class="tf-oas-m">{shortMethod(row.method)}</span>
                <span class="tf-oas-rpath" class:tf-oas-dep={row.deprecated}><Marked text={row.path} typed={marking} pairs={order.marks(row, row.method.length + 1, row.path.length)} /></span>
                <span class="tf-oas-rsum"><Marked text={row.summary} typed={marking} pairs={order.marks(row, row.method.length + row.path.length + 2, row.summary.length)} /></span>
              {:else}
                <span class="tf-oas-m tf-oas-mschema">Schema</span>
                <span class="tf-oas-rpath"><Marked text={row.name} typed={marking} pairs={order.marks(row)} /></span>
                <span class="tf-oas-rsum"></span>
              {/if}
              {#if mark !== undefined}<span class="tf-oas-imark">{markWords(mark)}</span>{/if}
            </button>
            {#if open.has(row.k)}
              <div class="tf-oas-iopen">
                {#if page === undefined}
                  <p class="tf-oas-wait" role="status">Reading…</p>
                {:else if page.kind === "op"}
                  <TfOasOperation st={state} op={page.op} k={row.k} {prose} onfollow={(name) => reach(`schemas/${name}`)} ongo={(place) => reach(place)} />
                {:else if page.kind === "schema"}
                  <TfOasSchemaPage st={state} name={page.name} node={page.node} shape={page.shape} uses={page.uses} sample={page.sample} given={page.given} {prose} onfollow={(name) => reach(`schemas/${name}`)} ongo={(place) => reach(place)} />
                {:else}
                  <p class="tf-oas-fnote">This side has no such place.</p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
        {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
      </div>
    {/if}
  {/if}
</div>
