<!--
  Dependencies (#871 to #875, R-11.10 to R-11.19a): `#/deps`, the top-bar
  opening. Everything knobas knows, in the shared layers from the work down to
  the hardware, over one read (`dependencies`), in a saved view (`dep_views`).

  Left, the rail: every layer with a checkbox, its count and its problems.
  Right, a band per shown layer (`TfDepsBand.svelte`), folded by `draw.ts`'
  rules. Pointing at or focusing a tile draws lines to everything it touches
  and fades the rest; click or `Enter` opens it (an asset opens its editor);
  `⌖` or `T` shows only what it touches; the arrow keys move between tiles.
  *Find* dims what does not match and opens the folds that hold a match;
  `Enter` jumps to the next. The toolbar holds the view menu, the environment
  chips, *Problems and what they touch* and *Layers…* (`TfDepsLayers.svelte`).

  The reader's gestures (Find, the trace, folds opened by hand) hold for the
  page; the view's draft holds for the session (`views.svelte.ts`). *Add ▾*
  and the route dialog are #876's.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    assetTypes,
    createDepLayer,
    deleteAsset,
    deleteDepLayer,
    deleteDepView,
    dependencies,
    depViews,
    editAsset,
    getAsset,
    moveAsset,
    orderDepLayers,
    placeDepKind,
    renameDepLayer,
    saveDepView,
    setDepViewDefaults,
    type AssetDetail,
    type DepItem,
    type DepLayer,
    type DepViews,
    type Dependencies,
  } from "../../ipc/assets";
  import { createLink, submitWrite } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import TfCreateMonitor from "../monitors/TfCreateMonitor.svelte";
  import { isAsset, nameIsCode } from "./deps";
  import { drawOf, graphOf, hitsLine, matcher, nextTile, touches, UNNAMED_LAYER, type Gestures } from "./draw";
  import type { EditorPorts, LayerPorts, ViewPorts } from "./ports";
  import TfAssetEditor from "./TfAssetEditor.svelte";
  import TfDepsBand from "./TfDepsBand.svelte";
  import TfDepsLayers from "./TfDepsLayers.svelte";
  import TfDepsViewMenu from "./TfDepsViewMenu.svelte";
  import { depViewState, type DepViewState } from "./views.svelte";
  import { ENVIRONMENTS } from "./views";

  type Ports = {
    dependencies: () => Promise<Dependencies>;
    depViews: () => Promise<DepViews>;
    submitWrite: typeof submitWrite;
  } & ViewPorts &
    LayerPorts &
    EditorPorts;

  const DEFAULT_PORTS: Ports = {
    dependencies,
    depViews,
    saveDepView,
    deleteDepView,
    setDepViewDefaults,
    createDepLayer,
    renameDepLayer,
    deleteDepLayer,
    orderDepLayers,
    placeDepKind,
    getAsset,
    assetTypes,
    editAsset,
    moveAsset,
    deleteAsset,
    createLink,
    submitWrite,
  };

  let {
    ready,
    ports: given = {},
    views = depViewState,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** Stand-ins for a test; the rest are the real commands. */
    ports?: Partial<Ports>;
    /** The session's view and draft, unless a test hands its own. */
    views?: DepViewState;
  } = $props();

  const ports = $derived({ ...DEFAULT_PORTS, ...given });

  let read = $state.raw<Dependencies | null>(null);
  let failure = $state<string | null>(null);

  async function load() {
    try {
      const [estate, saved] = await Promise.all([ports.dependencies(), ports.depViews()]);
      views.take(saved);
      read = estate;
      failure = null;
    } catch (error) {
      failure = ipcErrorMessage(error);
    }
  }

  // Read once, when the database answers.
  let started = false;
  $effect(() => {
    if (ready && !started) {
      started = true;
      void load();
    }
  });

  let query = $state("");
  let trace = $state<string | null>(null);
  let hover = $state<string | null>(null);
  let opened = $state.raw<ReadonlySet<string>>(new Set());
  let shut = $state.raw<ReadonlySet<string>>(new Set());
  let more = $state.raw<ReadonlySet<string>>(new Set());
  let fold = $state.raw<ReadonlyMap<string, boolean>>(new Map());
  let panel = $state(false);
  let editing = $state<string | null>(null);
  let creating = $state.raw<AssetDetail | null>(null);
  let menu = $state<TfDepsViewMenu | null>(null);

  const add = (set: ReadonlySet<string>, key: string) => new Set([...set, key]);
  const drop = (set: ReadonlySet<string>, key: string) => new Set([...set].filter((k) => k !== key));

  const gestures = $derived<Gestures>({ query, trace, open: opened, shut, more, fold });
  const graph = $derived(read === null ? null : graphOf(read));
  const drawn = $derived(read !== null && graph !== null && views.draft !== null ? drawOf(read, views.draft, gestures, graph) : null);
  const hit = $derived(read === null ? () => false : matcher(read, query));
  const traced = $derived(trace !== null && read !== null ? read.items.find((item) => item.id === trace) ?? null : null);
  /** The elements standing for what the pointed-at item touches. */
  const lit = $derived.by(() => {
    if (hover === null || graph === null || drawn === null || !drawn.where.has(hover)) return null;
    const els = new Set<string>();
    for (const id of touches(graph, hover)) {
      const el = drawn.where.get(id);
      if (el !== undefined) els.add(el);
    }
    return els;
  });

  function setLayers(layers: DepLayer[]) {
    if (read !== null) read = { ...read, layers };
  }

  function openItem(item: DepItem, event?: MouseEvent) {
    if (!isAsset(item.kind)) return;
    event?.preventDefault();
    editing = item.id;
  }

  function startTrace(item: DepItem) {
    trace = trace === item.id ? null : item.id;
    hover = null;
  }

  let bandsEl = $state<HTMLElement | null>(null);
  let wires = $state.raw<{ d: string; title: string }[]>([]);
  let size = $state({ width: 0, height: 0 });

  /** The lines, only for what is pointed at: from element to element, never to itself. */
  function drawWires() {
    const box = bandsEl;
    if (box === null || hover === null || graph === null || drawn === null || read === null || lit === null) {
      wires = [];
      return;
    }
    const at = box.getBoundingClientRect();
    size = { width: at.width, height: box.scrollHeight };
    const on = touches(graph, hover);
    const name = new Map(read.items.map((item) => [item.id, item.name]));
    const seen = new Set<string>();
    const out: { d: string; title: string }[] = [];
    const rect = (el: string) => {
      const node = box.querySelector(`[data-el="${CSS.escape(el)}"]`);
      if (!node) return null;
      const r = node.getBoundingClientRect();
      const left = r.left - at.left;
      const top = r.top - at.top;
      return { x: left + r.width / 2, t: top, b: top + r.height, l: left, r: left + r.width, cy: top + r.height / 2 };
    };
    for (const relation of read.relations) {
      if (out.length > 500) break;
      if (!on.has(relation.from) || !on.has(relation.to)) continue;
      const ea = drawn.where.get(relation.from);
      const eb = drawn.where.get(relation.to);
      if (ea === undefined || eb === undefined || ea === eb || seen.has(`${ea}>${eb}`)) continue;
      seen.add(`${ea}>${eb}`);
      const a = rect(ea);
      const b = rect(eb);
      if (a === null || b === null) continue;
      let d: string;
      if (Math.abs(a.cy - b.cy) < 6) {
        const forward = a.x < b.x;
        d = `M${forward ? a.r : a.l} ${a.cy} L${forward ? b.l : b.r} ${b.cy}`;
      } else {
        const down = a.cy < b.cy;
        const y1 = down ? a.b : a.t;
        const y2 = down ? b.t : b.b;
        const m = (y1 + y2) / 2;
        d = `M${a.x} ${y1} C${a.x} ${m},${b.x} ${m},${b.x} ${y2}`;
      }
      out.push({ d, title: `${name.get(relation.from) ?? relation.from} ${relation.relation} ${name.get(relation.to) ?? relation.to}` });
    }
    wires = out;
  }

  $effect(() => {
    void lit;
    drawWires();
  });

  // Pointing and focusing light what a tile touches; the keys move, open and trace.
  $effect(() => {
    const box = bandsEl;
    if (box === null) return;
    const idOf = (target: EventTarget | null) =>
      target instanceof Element ? (target.closest<HTMLElement>(".tf-dp-t")?.dataset["id"] ?? null) : null;
    const over = (event: Event) => {
      const id = idOf(event.target);
      if (id !== hover) hover = id;
    };
    const leave = () => (hover = null);
    const keys = (event: KeyboardEvent) => {
      const target = event.target;
      if (!(target instanceof HTMLElement) || !target.matches(".tf-dp-open")) return;
      const id = idOf(target);
      if (id === null) return;
      if (event.key === "t" || event.key === "T") {
        event.preventDefault();
        trace = id;
        hover = null;
      } else if (event.key.startsWith("Arrow")) {
        event.preventDefault();
        const all = [...box.querySelectorAll<HTMLElement>(".tf-dp-open")].filter((node) => node !== target);
        const from = target.getBoundingClientRect();
        const to = nextTile(from, all.map((node) => node.getBoundingClientRect()), event.key);
        if (to >= 0) all[to]!.focus();
      }
    };
    box.addEventListener("mouseover", over);
    box.addEventListener("mouseleave", leave);
    box.addEventListener("focusin", over);
    box.addEventListener("keydown", keys);
    const resize = () => drawWires();
    window.addEventListener("resize", resize);
    return () => {
      box.removeEventListener("mouseover", over);
      box.removeEventListener("mouseleave", leave);
      box.removeEventListener("focusin", over);
      box.removeEventListener("keydown", keys);
      window.removeEventListener("resize", resize);
    };
  });

  let jump = -1;
  function findKey(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    const hits = [...(bandsEl?.querySelectorAll<HTMLElement>(".tf-dp-t.tf-dp-hit .tf-dp-open") ?? [])];
    if (hits.length === 0) return;
    jump = (jump + 1) % hits.length;
    const tile = hits[jump]!;
    tile.scrollIntoView?.({ block: "center" });
    tile.focus();
  }

  function scrollTo(layer: string) {
    bandsEl?.querySelector(`[data-layer="${CSS.escape(layer)}"]`)?.scrollIntoView({ block: "start", behavior: "smooth" });
  }

  function setHidden(layer: string, hidden: boolean) {
    views.change((draft) => {
      draft.layers[layer] = { ...(draft.layers[layer] ?? UNNAMED_LAYER), hidden };
    });
  }

  function toggleEnv(env: string) {
    views.change((draft) => {
      draft.environments = draft.environments.includes(env) ? draft.environments.filter((e) => e !== env) : [...draft.environments, env];
    });
  }

  function saved(what: { name: string; moved: boolean }) {
    editing = null;
    push({ text: `Saved ${what.name}${what.moved ? ", and moved it" : ""}` });
    void reload();
  }

  function deleted(name: string) {
    editing = null;
    push({ text: `Deleted ${name}` });
    void reload();
  }

  async function reload() {
    try {
      read = await ports.dependencies();
    } catch (error) {
      push({ text: `The estate could not be read again: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

<header class="tf-fhead">
  <div>
    <h1>Dependencies</h1>
    <div class="tf-sub">
      Everything knobas knows, in layers from the work down to the hardware. Point at anything to see what it touches;
      click to open it.
    </div>
  </div>
  <div class="tf-acts">
    <a class="tf-btn tf-ghost" href="#/assets" title="The asset tree in columns">Asset tree</a>
    <a class="tf-btn tf-ghost" href="#/monitors">Monitors</a>
  </div>
</header>

<div class="tf-dp">
  {#if failure !== null}
    <p class="tf-dp-fail" role="alert">The estate could not be read: {failure}. Open Dependencies again to retry.</p>
  {:else if read !== null && drawn !== null && views.draft !== null}
    {@const draft = views.draft}
    <div class="tf-dp-bar" role="toolbar" aria-label="Dependencies">
      <TfDepsViewMenu bind:this={menu} {views} layerCount={read.layers.length} {ports} />
      {#if views.dirty}
        <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => void menu?.saveChanges()}>Save view</button>
      {/if}
      <label class="tf-fsearch tf-dp-q">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
          ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
        >
        <input
          placeholder="Find among {read.items.length.toLocaleString('en')} items"
          aria-label="Find"
          autocomplete="off"
          value={query}
          oninput={(event) => {
            query = event.currentTarget.value;
            jump = -1;
          }}
          onkeydown={findKey}
        />
        <span class="tf-dp-qn" aria-live="polite">{hitsLine(query, drawn.hits)}</span>
      </label>
      <div class="tf-fchips" role="group" aria-label="Environment">
        {#each ENVIRONMENTS as env (env)}
          <button type="button" aria-pressed={draft.environments.includes(env)} onclick={() => toggleEnv(env)}>{env}</button>
        {/each}
      </div>
      <button
        type="button"
        class="tf-btn tf-sm"
        class:tf-primary={draft.problems}
        aria-pressed={draft.problems}
        title="Problems, and everything that leads to them or hangs off them"
        onclick={() => views.change((d) => void (d.problems = !d.problems))}
        >Problems and what they touch{#if drawn.problems > 0}{" "}<span class="tf-dp-n">{drawn.problems}</span>{/if}</button
      >
      <span class="tf-dp-gap"></span>
      <button type="button" class="tf-btn tf-sm" class:tf-primary={panel} aria-pressed={panel} onclick={() => (panel = !panel)}>Layers…</button>
      <button type="button" class="tf-btn tf-sm" aria-disabled="true" title="Not built in the new shell yet">Add ▾</button>
    </div>
    {#if traced !== null}
      <div class="tf-dp-trail">
        <span>Only what touches <b class:tf-mono={nameIsCode(traced.kind)}>{traced.name}</b>, up and down.</span>
        <button type="button" class="tf-btn tf-sm" onclick={() => (trace = null)}>Show everything</button>
      </div>
    {/if}
    <div class="tf-dp-grid" class:tf-dp-wp={panel}>
      <nav class="tf-dp-rail" aria-label="Layers">
        <div class="tf-dp-rh">Layers</div>
        {#each drawn.rail as row (row.id)}
          <div class="tf-dp-rl" class:tf-dp-off={!row.shown}>
            <input type="checkbox" checked={row.shown} aria-label="Show {row.name}" onchange={(event) => setHidden(row.id, !event.currentTarget.checked)} />
            <button type="button" disabled={!row.shown} onclick={() => scrollTo(row.id)}>{row.name}</button>
            <span class="tf-dp-rc">{row.count.toLocaleString("en")}</span>
            {#if row.problems > 0}<b class="tf-dp-rb" aria-label="{row.problems} problems">{row.problems}</b>{/if}
          </div>
        {/each}
        <button type="button" class="tf-linkish tf-dp-redit" onclick={() => (panel = true)}>Change the layers</button>
      </nav>

      <div class="tf-dp-bands" bind:this={bandsEl}>
        <svg class="tf-dp-wires" width={size.width} height={size.height} aria-hidden="true">
          {#each wires as wire, index (index)}<path d={wire.d}><title>{wire.title}</title></path>{/each}
        </svg>
        {#each drawn.bands as band (band.id)}
          <TfDepsBand
            {band}
            kinds={read.kinds}
            {query}
            {hit}
            {lit}
            {hover}
            onfold={(layer, folded) => (fold = new Map([...fold, [layer, folded]]))}
            onopengroup={(key) => {
              opened = add(opened, key);
              shut = drop(shut, key);
            }}
            onshutgroup={(key) => {
              shut = add(shut, key);
              opened = drop(opened, key);
            }}
            onmore={(key) => (more = add(more, key))}
            onopen={openItem}
            ontrace={startTrace}
          />
        {/each}
        {#if drawn.hidden.length > 0 || drawn.unplaced.count > 0}
          <p class="tf-dp-foot">
            {#if drawn.hidden.length > 0}
              Hidden in this view:
              {#each drawn.hidden as row, index (row.id)}
                <button type="button" class="tf-linkish" onclick={() => setHidden(row.id, false)}>{row.name} ({row.count.toLocaleString("en")})</button>{index < drawn.hidden.length - 1 ? ", " : ""}
              {/each}.
            {/if}
            {#if drawn.unplaced.count > 0}
              {drawn.unplaced.count.toLocaleString("en")} items are in no layer: {drawn.unplaced.kinds.join(", ")}.
              <button type="button" class="tf-linkish" onclick={() => (panel = true)}>Place them in Layers…</button>
            {/if}
          </p>
        {/if}
      </div>
      {#if panel}
        <TfDepsLayers
          {read}
          {views}
          {ports}
          onlayers={setLayers}
          onfoldreset={(layer) => (fold = new Map([...fold].filter(([id]) => id !== layer)))}
          onclose={() => (panel = false)}
        />
      {/if}
    </div>
  {/if}
</div>

{#if editing !== null && read !== null}
  <TfAssetEditor
    assetId={editing}
    {read}
    {ports}
    onclose={() => (editing = null)}
    onsaved={saved}
    ondeleted={deleted}
    oncreatemonitor={(detail) => {
      editing = null;
      creating = detail;
    }}
  />
{/if}
{#if creating !== null}
  <TfCreateMonitor
    detail={creating}
    record={ports.editAsset}
    queue={ports.submitWrite}
    onclose={() => (creating = null)}
    oncreated={() => {
      push({ text: `Queued: create monitor for ${creating?.asset.name ?? ""}. Sends with the next sync.`, action: { label: "Show queue", run: openWriteQueue } });
      creating = null;
    }}
  />
{/if}
