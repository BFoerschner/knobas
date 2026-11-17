<!--
  An OpenAPI or Swagger spec on the Code page (ux18, `docs/decisions/2026-11-17-an-api-spec-is-drawn-as-its-endpoints.md`):
  the file's renderer by default, *Source* one pick away in *Render as*. knobas's own look, not Swagger UI's.

  - **The head**: the API's title, its version and the spec's, its description (Markdown), its servers as text
    (nothing here calls them), and what the reading passed over.
  - **The band**, stuck to the top of the scroller: *Filter endpoints* (method, path, summary, tag, operationId;
    every word must match), the count, *Schemas*, and *Back to ⟨where you were⟩* after a ref was followed.
  - **The endpoints**, grouped by tag (operations no tag holds by their path's first segment), one row each: the
    method in the narrow heavy face, the path in the code face with its `{parameters}` dim, the summary. A row
    opens in place to its parameters, body and responses (`TfOasOperation`). The list is windowed: a spec of
    5,000 operations draws what the screen shows.
  - **Schemas**, the named ones, each opening in place. A ref's name follows to its schema here.
  - A spec that does not parse or is not one knobas draws says why, on which line, and offers *Show source*.

  The parse runs in the spec worker (`render/openapi-port.ts`); this component and the worker load with the first
  spec opened, nothing of them at start-up. An address can name an operation or a schema (`at/<place>/<path>`,
  `repo.ts`): the view opens there.
-->
<script lang="ts">
  import "../css/openapi.css";

  import { flushSync, untrack, type Snippet } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { Windowed } from "../../shell/windowed.svelte";
  import { filterKeys } from "../filter-keys";
  import TfOasOperation from "./TfOasOperation.svelte";
  import TfOasSchema from "./TfOasSchema.svelte";
  import { OasState, innerRef } from "./oas.svelte";
  import { refTarget, type Opened, type Operation, type OpRow, type PathRef } from "../../render/openapi";
  import { groupsOf, haystack, matches, rowsOf, terms, type Row } from "../../render/openapi-view";
  import { SpecSession, specEngine, type ReadFile, type SpecEngine } from "../../render/openapi-port";

  let {
    text,
    path,
    read = null,
    at = null,
    fileHref = () => null,
    oncopylink = null,
    onsource = null,
    prose = null,
    engine = null,
  }: {
    text: string;
    /** The spec's path from the repository's top: where its relative refs are read from. */
    path: string;
    /** Another file of the repository at the spec's commit, for a ref into it; `null` where none can be read. */
    read?: ReadFile | null;
    /** The place an address names: an operation (`get /pets/{petId}`) or a schema (`schemas/Pet`). */
    at?: string | null;
    /** A file on the Code page, at a place in it. */
    fileHref?: (file: string, place: string | null) => string | null;
    oncopylink?: ((place: string, label: string) => void) | null;
    /** Shows the file as Source. */
    onsource?: (() => void) | null;
    /** Draws Markdown. */
    prose?: Snippet<[string]> | null;
    /** Where the spec is parsed: the worker when left out. */
    engine?: SpecEngine | null;
  } = $props();

  let opened = $state<Opened | null>(null);
  let st = $state<OasState | null>(null);
  /** Shown once a read has taken long enough to be noticed. */
  let slow = $state(false);

  $effect(() => {
    const source = text;
    const root = path;
    const session = new SpecSession(untrack(() => engine) ?? specEngine(), untrack(() => read));
    const state = new OasState(session, root);
    state.fileHref = (file, place) => fileHref(file, place);
    state.follow = (name) => void follow(name);
    st = state;
    opened = null;
    openOps.clear();
    details.clear();
    openRefs.clear();
    refOps.clear();
    openSchemas.clear();
    backs.length = 0;
    typed = "";
    let live = true;
    const timer = setTimeout(() => {
      if (live) slow = true;
    }, 200);
    session.open(root, source).then((answer) => {
      if (!live) return;
      clearTimeout(timer);
      slow = false;
      if (answer.ok) {
        state.schemaBase = answer.index.schemaBase;
        state.schemas = new Set(answer.index.schemas);
      }
      opened = answer;
    });
    return () => {
      live = false;
      clearTimeout(timer);
      session.close();
    };
  });

  const index = $derived(opened !== null && opened.ok ? opened.index : null);
  const groups = $derived(index === null ? [] : groupsOf(index));
  const hays = $derived(new Map<OpRow, string>(index === null ? [] : index.ops.map((op) => [op, haystack(op)])));

  let typed = $state("");
  const words = $derived(terms(typed));

  /** Open operations by index, their detail once read; open external path items; open schemas. */
  const openOps = new SvelteSet<number>();
  const details = new SvelteMap<number, Operation>();
  const openRefs = new SvelteSet<string>();
  const refOps = new SvelteMap<string, { ok: true; ops: Operation[] } | { ok: false; error: string }>();
  const openSchemas = new SvelteSet<string>();

  const rows = $derived(index === null ? [] : rowsOf(groups, hays, index.schemas, words, { ops: openOps, refs: openRefs, schemas: openSchemas }));
  const total = $derived(index === null ? 0 : index.ops.length + index.pathRefs.length);
  const kept = $derived(
    index === null || words.length === 0
      ? total
      : index.ops.filter((op) => matches(hays.get(op) ?? "", words)).length + index.pathRefs.filter((r) => matches(r.path.toLowerCase(), words)).length,
  );

  // A row is 33 px; a group's head 44; an operation opened is about a screen.
  const win = new Windowed<string>({
    count: () => rows.length,
    key: (i) => rows[i]!.k,
    estimate: 33,
    estimateOf: (k) => (k.startsWith("d") ? 420 : k.startsWith("g") ? 44 : 33),
  });

  let list = $state<HTMLElement | null>(null);

  /** The refs an operation's schemas open onto at once, in its own file: resolved before it opens, so it opens whole. */
  function topRefs(op: Operation): string[] {
    const nodes: unknown[] = [];
    for (const p of op.parameters) nodes.push(p.schema);
    for (const m of op.body?.content ?? []) nodes.push(m.schema);
    for (const r of op.responses) {
      for (const m of r.content) nodes.push(m.schema);
      for (const h of r.headers) nodes.push(h.schema);
    }
    const refs = new Set<string>();
    for (const n of nodes) {
      const ref = innerRef(n);
      if (ref === null) continue;
      const t = refTarget(op.base, ref);
      if (t.ok && t.file === op.base) refs.add(ref);
    }
    return [...refs];
  }

  async function openOp(op: OpRow): Promise<void> {
    const state = st;
    if (state === null) return;
    let detail = details.get(op.i);
    if (detail === undefined) {
      const answer = await state.session.operation(op.i);
      if (answer === null || st !== state) return;
      await Promise.all(topRefs(answer).map((ref) => state.ensure(answer.base, ref)));
      if (st !== state) return;
      details.set(op.i, answer);
      detail = answer;
    }
    openOps.add(op.i);
  }

  function toggleOp(op: OpRow) {
    if (openOps.has(op.i)) openOps.delete(op.i);
    else void openOp(op);
  }

  async function toggleRef(ref: PathRef) {
    const state = st;
    if (state === null) return;
    if (openRefs.has(ref.path)) {
      openRefs.delete(ref.path);
      return;
    }
    // Another file: open at once on *Reading*, filled in below it.
    openRefs.add(ref.path);
    if (!refOps.has(ref.path)) {
      const answer = await state.session.pathItem(ref);
      if (st !== state) return;
      if (answer.ok) for (const op of answer.ops) await Promise.all(topRefs(op).map((r) => state.ensure(op.base, r)));
      refOps.set(ref.path, answer);
    }
  }

  async function openSchema(name: string): Promise<void> {
    const state = st;
    if (state === null || index === null) return;
    await state.ensure(state.root, index.schemaBase + name);
    if (st === state) openSchemas.add(name);
  }

  function toggleSchema(name: string) {
    if (openSchemas.has(name)) openSchemas.delete(name);
    else void openSchema(name);
  }

  // ---- Following a ref, and coming back ----

  /** Where a ref was followed from: the row and where it stood on screen. */
  const backs = $state<{ k: string; label: string; y: number }[]>([]);
  /** The row the last press landed in, and where it stood: what *Back to* returns to. */
  let pressed: { k: string; y: number } | null = null;

  function notePress(event: MouseEvent) {
    const row = (event.target as Element | null)?.closest<HTMLElement>("[data-k]");
    pressed = row === null || row === undefined ? null : { k: row.dataset.k ?? "", y: row.getBoundingClientRect().top };
  }

  /** The label *Back to* names for the row `k`: its operation or its schema. */
  function labelOf(k: string): string {
    const row = rows.find((r) => r.k === k);
    if (row === undefined) return "where you were";
    if (row.kind === "op" || row.kind === "detail") return `${row.op.method.toUpperCase()} ${row.op.path}`;
    if (row.kind === "schema" || row.kind === "sbody") return row.name;
    if (row.kind === "pref" || row.kind === "prefbody") return row.ref.path;
    return "where you were";
  }

  function scroller(el: HTMLElement): HTMLElement | null {
    for (let at = el.parentElement; at !== null; at = at.parentElement) {
      const y = getComputedStyle(at).overflowY;
      if (y === "auto" || y === "scroll") return at;
    }
    return document.scrollingElement as HTMLElement | null;
  }

  /** Brings row `k` on screen: its top at `y` where given, else under the band. Returns its element. */
  function bring(k: string, y: number | null): HTMLElement | null {
    const i = rows.findIndex((r) => r.k === k);
    if (i < 0 || list === null) return null;
    win.reveal(i);
    flushSync();
    const el = list.querySelector<HTMLElement>(`[data-k="${CSS.escape(k)}"]`);
    if (el === null) return null;
    if (y === null) el.scrollIntoView({ block: "start" });
    else scroller(el)?.scrollBy(0, el.getBoundingClientRect().top - y);
    return el;
  }

  async function follow(name: string, remember = true) {
    const from = pressed;
    if (remember && from !== null && from.k !== `s:${name}` && from.k !== `ds:${name}`) {
      backs.push({ k: from.k, label: labelOf(from.k), y: from.y });
    }
    // A filter that hides the schema would leave nothing to land on.
    if (words.length > 0 && !matches(name.toLowerCase(), words)) typed = "";
    await openSchema(name);
    flushSync();
    bring(`s:${name}`, null)?.querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
  }

  function back() {
    const to = backs.pop();
    if (to === undefined) return;
    const el = bring(to.k, to.y);
    el?.querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
  }

  function toSchemas() {
    bring("s", null);
  }

  // ---- The place an address names ----

  let placed = $state<string | null>(null);
  $effect(() => {
    const place = at;
    const idx = index;
    if (place === null || idx === null || untrack(() => placed) === place) return;
    placed = place;
    void (async () => {
      if (place.startsWith("schemas/")) {
        const name = place.slice("schemas/".length);
        if (idx.schemas.includes(name)) await follow(name, false);
        return;
      }
      const op = idx.ops.find((o) => o.anchor === place);
      if (op === undefined) return;
      await openOp(op);
      flushSync();
      const row = rows.find((r) => r.kind === "op" && r.op === op);
      if (row !== undefined) bring(row.k, null);
    })();
  });

  /** A path's parts, its `{parameters}` marked to draw dim. */
  function pathParts(p: string): { text: string; param: boolean }[] {
    return p
      .split(/(\{[^}]*\})/)
      .filter((s) => s !== "")
      .map((s) => ({ text: s, param: s.startsWith("{") }));
  }
</script>

{#snippet text_(words: string)}
  {#if prose !== null}{@render prose(words)}{:else}<p class="tf-oas-desc">{words}</p>{/if}
{/snippet}

{#snippet methodPath(method: string, p: string, dep: boolean)}
  <span class="tf-oas-m">{method.toUpperCase()}</span>
  <span class="tf-oas-path" class:tf-oas-dep={dep}>{#each pathParts(p) as part, n (n)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
{/snippet}

<div class="tf-oas">
  {#if opened === null}
    <div class="tf-oas-wait">{#if slow}<p class="tf-dim" role="status">Reading the spec…</p>{/if}</div>
  {:else if !opened.ok}
    <div class="tf-oas-bad" role="alert">
      <p><b>This spec could not be read.</b></p>
      <pre>{opened.error}</pre>
      {#if opened.line !== null}<p class="tf-dim">Line {opened.line}.</p>{/if}
      {#if onsource !== null}<p><button type="button" class="tf-btn tf-sm" onclick={() => onsource?.()}>Show source</button></p>{/if}
    </div>
  {:else if index !== null && st !== null}
    {@const state = st}
    <header class="tf-oas-head">
      <h2 class="tf-oas-title">{index.title}</h2>
      <p class="tf-oas-vers">
        {#if index.apiVersion !== ""}<span class="tf-lab">{index.apiVersion}</span>{/if}
        <span class="tf-dim">{index.key === "swagger" ? "Swagger" : "OpenAPI"} {index.version}</span>
      </p>
      {#if index.description !== ""}<div class="tf-oas-hdesc">{@render text_(index.description)}</div>{/if}
      {#if index.servers.length > 0}
        <dl class="tf-oas-servers">
          <dt class="tf-oas-glabel">{index.servers.length === 1 ? "Server" : "Servers"}</dt>
          {#each index.servers as server, n (n)}
            <dd><code>{server.url}</code>{#if server.description !== ""}<span class="tf-dim">{server.description}</span>{/if}</dd>
          {/each}
        </dl>
      {/if}
      {#if index.problems.length > 0}
        <details class="tf-oas-problems">
          <summary>{index.problems.length === 1 ? "One entry was left out" : `${index.problems.length} entries were left out`}</summary>
          <ul>{#each index.problems.slice(0, 200) as problem, n (n)}<li>{problem}</li>{/each}</ul>
        </details>
      {/if}
    </header>

    <div class="tf-oas-band">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input placeholder="Filter endpoints" aria-label="Filter endpoints" bind:value={typed} {@attach filterKeys()} />
      </label>
      <span class="tf-oas-count" aria-live="polite">{words.length === 0 ? `${total} ${total === 1 ? "endpoint" : "endpoints"}` : `${kept} of ${total}`}</span>
      <span class="tf-trail">
        {#if backs.length > 0}
          <button type="button" class="tf-btn tf-sm" onclick={back}>Back to {backs.at(-1)?.label}</button>
        {/if}
        {#if index.schemas.length > 0}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={toSchemas}>Schemas</button>{/if}
      </span>
    </div>

    {#if rows.length === 0}
      <p class="tf-oas-none">{words.length > 0 ? "No endpoint or schema matches the filter." : "The spec lists no endpoints."}</p>
    {/if}
    <div class="tf-oas-list" bind:this={list} onclickcapture={notePress} {@attach win.list}>
      {#each win.slice(rows) as row (row.k)}
        {@render drawRow(row, state)}
      {/each}
    </div>
  {/if}
</div>

{#snippet drawRow(row: Row, state: OasState)}
  {#if row.kind === "group"}
    <div class="tf-oas-group" data-k={row.k} {@attach win.row(row.k)}>
      <h3 class:tf-oas-gcode={row.group.code}>{row.group.name}</h3>
      {#if row.group.description !== ""}<span class="tf-oas-gdesc" title={row.group.description}>{row.group.description}</span>{/if}
      <span class="tf-oas-gcount">{row.shown}</span>
    </div>
  {:else if row.kind === "op"}
    <div class="tf-oas-oprow" data-k={row.k} {@attach win.row(row.k)}>
      <button type="button" class="tf-oas-opbtn" aria-expanded={openOps.has(row.op.i)} onclick={() => toggleOp(row.op)}>
        {@render methodPath(row.op.method, row.op.path, row.op.deprecated)}
        <span class="tf-oas-sum">{row.op.summary}</span>
        {#if row.op.deprecated}<span class="tf-oas-deplab">Deprecated</span>{/if}
      </button>
    </div>
  {:else if row.kind === "detail"}
    {@const detail = details.get(row.op.i)}
    <div class="tf-oas-detail" data-k={row.k} {@attach win.row(row.k)}>
      {#if detail !== undefined}<TfOasOperation op={detail} st={state} k={row.k} {prose} {oncopylink} />{/if}
    </div>
  {:else if row.kind === "pref"}
    <div class="tf-oas-oprow" data-k={row.k} {@attach win.row(row.k)}>
      <button type="button" class="tf-oas-opbtn" aria-expanded={openRefs.has(row.ref.path)} onclick={() => void toggleRef(row.ref)}>
        <span class="tf-oas-m tf-dim">…</span>
        {@render methodPath("", row.ref.path, false)}
        <span class="tf-oas-sum">In {row.ref.ref}</span>
      </button>
    </div>
  {:else if row.kind === "prefbody"}
    {@const got = refOps.get(row.ref.path)}
    {@const href = state.hrefOf(state.root, row.ref.ref)}
    <div class="tf-oas-detail" data-k={row.k} {@attach win.row(row.k)}>
      {#if got === undefined}
        <p class="tf-oas-note" role="status">Reading {row.ref.ref}…</p>
      {:else if !got.ok}
        <p class="tf-oas-note">{got.error}{#if href !== null} <a href={href}>Open the file</a>{/if}</p>
      {:else}
        {#each got.ops as op (op.method)}
          <div class="tf-oas-subop">
            <p class="tf-oas-subhead">{@render methodPath(op.method, op.path, op.deprecated)} <span class="tf-oas-sum">{op.summary}</span></p>
            <TfOasOperation {op} st={state} k="{row.k}:{op.method}" {prose} oncopylink={null} />
          </div>
        {/each}
      {/if}
    </div>
  {:else if row.kind === "shead"}
    <div class="tf-oas-group tf-oas-shead" data-k={row.k} {@attach win.row(row.k)}>
      <h3>Schemas</h3>
      <span class="tf-oas-gcount">{row.shown}</span>
    </div>
  {:else if row.kind === "schema"}
    <div class="tf-oas-oprow" data-k={row.k} {@attach win.row(row.k)}>
      <button type="button" class="tf-oas-opbtn tf-oas-sbtn" aria-expanded={openSchemas.has(row.name)} onclick={() => toggleSchema(row.name)}>
        <span class="tf-oas-sname">{row.name}</span>
      </button>
    </div>
  {:else if row.kind === "sbody"}
    {@const ref = state.schemaBase + row.name}
    {@const got = state.ref(state.root, ref)}
    <div class="tf-oas-detail" data-k={row.k} {@attach win.row(row.k)}>
      {#if got !== undefined && "ok" in got && got.ok}
        <TfOasSchema node={got.node} base={got.base} st={state} k={row.k} trail={[`${state.root}#${ref.slice(1)}`]} from={{ k: row.k, label: row.name }} top />
      {:else if got !== undefined && "error" in got}
        <p class="tf-oas-note">{got.error}</p>
      {/if}
    </div>
  {/if}
{/snippet}
