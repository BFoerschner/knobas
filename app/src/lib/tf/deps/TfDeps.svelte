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
  `Enter` jumps to the next, and opens a group's "N more, K found" when the
  next match waits behind it (#2212). The toolbar holds the view menu, the
  environment chips, *Problems and what they touch* and *Layers…*
  (`TfDepsLayers.svelte`).

  The reader's gestures (Find, the trace, folds opened by hand) hold for the
  page; the view's draft holds for the session (`views.svelte.ts`).

  **Adding (#876, R-11.20, R-11.21)**: *+ Add* on each layer that holds assets
  or routes, and *Add ▾* (Asset, Route, Import…, and *Link to <key>…* from a
  ticket) open *New asset* (`TfNewAsset.svelte`), the route dialog
  (`TfRouteDialog.svelte`), the estate import (`TfImport.svelte`, #1066; the
  estate is read again after it) or *Link to…*. What was added or saved is read
  again, drawn open (`Gestures.reveal`), scrolled to and flashed once; the
  toast after a new asset offers *Edit it*. A route tile opens the route
  dialog, with *Delete…*.

  **From a ticket (#874, R-11.10)**: with `ticket` set (the job's Map tab,
  `#/job/<key>/map`, or `#/deps/job/<key>`) the same read is kept to what the
  ticket reaches (`reach.ts`, over `job_reach` and `job_pipelines`), in the
  view set as *opens from a ticket*. The scope buttons, drawn while a job is
  open, switch between *This job <key>* and *Everything* (`#/deps`).
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { inTextField } from "../../shell/keys";

  import { ipcErrorMessage } from "../../ipc";
  import {
    applyEstateImport,
    assetTree,
    assetTypes,
    createAsset,
    previewEstateImport,
    produceEstateFile,
    createDepLayer,
    createRoute,
    deleteAsset,
    dependsOnThis,
    deleteRoute,
    editRoute,
    getRoute,
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
  import {
    createLink,
    getEntity,
    jobPipelines,
    jobReach,
    submitWrite,
    type JobPipeline,
    type LinkRow,
    type Reached,
  } from "../../ipc/entity";
  import { launcherHome, search } from "../../ipc/search";
  import { listSources } from "../../ipc/sources";
  import type { SessionPorts } from "../../launcher/session.svelte";
  import TfLinkTo from "../job/TfLinkTo.svelte";
  import { layers } from "../layers.svelte";
  import TfNewAsset from "./TfNewAsset.svelte";
  import TfRouteDialog from "./TfRouteDialog.svelte";
  import TfImport from "./TfImport.svelte";
  import { importedLine } from "./import";
  import { isEntityId, resolveTicket } from "../job/job";
  import { jobHash } from "../route";
  import { reachedPart } from "./reach";
  import { toastQueued } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { prefill } from "../monitors/monitors";
  import TfCreateMonitor from "../monitors/TfCreateMonitor.svelte";
  import { isAsset, nameIsCode } from "./deps";
  import { drawOf, graphOf, hitsLine, matcher, nextTile, tileEl, touches, UNNAMED_LAYER, type Gestures } from "./draw";
  import type { AddPorts, EditorPorts, ImportPorts, LayerPorts, ViewPorts } from "./ports";
  import TfAssetEditor from "./TfAssetEditor.svelte";
  import TfDepsBand from "./TfDepsBand.svelte";
  import TfDepsLayers from "./TfDepsLayers.svelte";
  import TfDepsViewMenu from "./TfDepsViewMenu.svelte";
  import { depViewState, type DepViewState } from "./views.svelte";
  import { ENVIRONMENTS } from "./views";
  import { pageVisible } from "../kept-pages.svelte";

  type Ports = {
    dependencies: () => Promise<Dependencies>;
    depViews: () => Promise<DepViews>;
    submitWrite: typeof submitWrite;
    /** A ticket key's entity id, as the job page resolves it; `null` when nothing holds it. */
    ticketId: (key: string) => Promise<string | null>;
    jobReach: (entityId: string) => Promise<Reached[]>;
    jobPipelines: (entityId: string) => Promise<JobPipeline[]>;
  } & ViewPorts &
    LayerPorts &
    EditorPorts &
    AddPorts &
    ImportPorts &
    Pick<SessionPorts, "search" | "launcherHome">;

  async function ticketIdOf(key: string): Promise<string | null> {
    const sources = isEntityId(key) ? [] : await listSources();
    return (await resolveTicket(key, sources, getEntity))?.row.entity_id ?? null;
  }

  const DEFAULT_PORTS: Ports = {
    dependencies,
    depViews,
    ticketId: ticketIdOf,
    jobReach,
    jobPipelines,
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
    dependsOnThis,
    createLink,
    submitWrite,
    createAsset: (typeId, name, parentId) => createAsset(typeId, name, parentId),
    getRoute,
    createRoute: (assetId, name, url, targetId, visibility) => createRoute(assetId, name, url, targetId, visibility),
    editRoute,
    deleteRoute,
    previewEstateImport,
    applyEstateImport,
    produceEstateFile,
    assetTree: (parentId) => assetTree(parentId),
    search: (query) => search(query),
    launcherHome: () => launcherHome(),
  };

  let {
    ready,
    ports: given = {},
    views = depViewState,
    ticket = null,
    ticketId = null,
    job = null,
    host = "page",
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The ticket key the map opens from, or `null` for the top-bar opening. */
    ticket?: string | null;
    /** That ticket's entity id when the host already knows it; else it is resolved. */
    ticketId?: string | null;
    /** The job open in the shell, for *This job* on the top-bar opening. */
    job?: string | null;
    /** Where it draws: its own page, or the job's Map tab. */
    host?: "page" | "job";
    /** Stand-ins for a test; the rest are the real commands. */
    ports?: Partial<Ports>;
    /** The session's view and draft, unless a test hands its own. */
    views?: DepViewState;
  } = $props();

  const ports = $derived({ ...DEFAULT_PORTS, ...given });

  type Reach = { id: string; key: string; reached: Reached[]; pipelines: JobPipeline[] };

  /** The top-bar read, whole. */
  let whole = $state.raw<Dependencies | null>(null);
  /** What the ticket reaches, on the opening from a ticket. */
  let reach = $state.raw<Reach | null>(null);
  /** What draws: the whole read, or the part the ticket reaches. */
  const read = $derived.by(() => {
    if (whole === null || ticket === null) return whole;
    return reach !== null && reach.key === ticket ? reachedPart(whole, reach, reach.reached, reach.pipelines) : null;
  });
  let failure = $state<string | null>(null);

  async function readReach(key: string, known: string | null): Promise<Reach | null> {
    const id = known ?? (await ports.ticketId(key));
    if (id === null) return null;
    const [reached, pipelines] = await Promise.all([
      ports.jobReach(id),
      // A job whose pipelines cannot be read still draws what it reaches.
      ports.jobPipelines(id).catch(() => [] as JobPipeline[]),
    ]);
    return { id, key, reached, pipelines };
  }

  /** The reads' generation, so an answer for an opening left behind is dropped. */
  let token = 0;

  async function load(key: string | null, known: string | null, mine: number) {
    try {
      const [estate, saved, scoped] = await Promise.all([
        ports.dependencies(),
        ports.depViews(),
        key === null ? Promise.resolve(null) : readReach(key, known),
      ]);
      if (mine !== token) return;
      views.take(saved);
      views.opening(key);
      whole = estate;
      reach = scoped;
      failure = key !== null && scoped === null ? `nothing in the mirror answers to ${key}` : null;
    } catch (error) {
      if (mine === token) failure = ipcErrorMessage(error);
    }
  }

  // Read when the database answers, and again for each opening.
  $effect(() => {
    const key = ticket;
    const known = ticketId;
    if (!ready) return;
    const mine = ++token;
    untrack(() => {
      query = "";
      trace = null;
      opened = new Set();
      shut = new Set();
      more = new Set();
      fold = new Map();
      reveal = null;
      void load(key, known, mine);
    });
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
  /** *New asset*, from a layer's *+ Add* or from *Add ▾* (`layer` `null`). */
  let adding = $state.raw<{ layer: DepLayer | null } | null>(null);
  /** The route dialog: a new route (`id` `null`) or the one being edited. */
  let routing = $state.raw<{ id: string | null } | null>(null);
  let linking = $state(false);
  /** *Import…* (#1066): the estate import dialog. */
  let importing = $state(false);
  let addMenu = $state(false);
  let addButton = $state<HTMLButtonElement | null>(null);
  let addMenuEl = $state<HTMLDivElement | null>(null);
  /** What was just added or saved: drawn open (`reveal`), scrolled to and flashed once. */
  let reveal = $state<string | null>(null);
  let flash = $state<string | null>(null);
  /** The ticket the map opens from, once its entity id is known: what *Link it to* links. */
  const scoped = $derived(ticket !== null && reach !== null && reach.key === ticket ? { key: ticket, id: reach.id } : null);

  const add = (set: ReadonlySet<string>, key: string) => new Set([...set, key]);
  const drop = (set: ReadonlySet<string>, key: string) => new Set([...set].filter((k) => k !== key));

  const gestures = $derived<Gestures>({ query, trace, open: opened, shut, more, fold, reveal });
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
    if (whole !== null) whole = { ...whole, layers };
  }

  function openItem(item: DepItem, event?: MouseEvent) {
    if (item.kind === "route") {
      event?.preventDefault();
      routing = { id: item.id };
      return;
    }
    if (!isAsset(item.kind)) return;
    event?.preventDefault();
    editing = item.id;
  }

  /** *+ Add* on a layer: a layer of routes alone opens the route dialog (prototype-6's `dpCreate`). */
  function addTo(layerId: string | null) {
    const layer = layerId === null ? null : (read?.layers.find((l) => l.id === layerId) ?? null);
    if (layer !== null && layer.kinds.length === 1 && layer.kinds[0] === "route") routing = { id: null };
    else adding = { layer };
  }

  function pickAdd(run: () => void) {
    addMenu = false;
    run();
  }

  $effect(() => {
    if (!addMenu) return;
    void tick().then(() => addMenuEl?.querySelector<HTMLElement>("button")?.focus());
    const away = (event: MouseEvent) => {
      if (event.target instanceof Node && (addMenuEl?.contains(event.target) || addButton?.contains(event.target))) return;
      addMenu = false;
    };
    window.addEventListener("mousedown", away);
    return () => window.removeEventListener("mousedown", away);
  });

  function addMenuKey(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    addMenu = false;
    addButton?.focus();
  }

  /** Created or saved: read again, then draw it open, scroll to it and flash it once. */
  function landed(id: string, text: string, edit: boolean, unlinked: string | null) {
    const action = edit ? { action: { label: "Edit it", run: () => void (editing = id) } } : {};
    if (unlinked !== null && scoped !== null) push({ text: `${text}, but it is not linked to ${scoped.key}: ${unlinked}`, tone: "err", ...action });
    else push({ text, ...action });
    trace = null;
    void reload().then(() => {
      reveal = id;
      flash = id;
    });
  }

  // The flashed tile, once drawn: scrolled to the middle and focused; the flash ends on its own.
  let flashed: string | null = null;
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  // Not the flash effect's own cleanup: that runs on every re-read and would strand the flash on.
  $effect(() => () => clearTimeout(flashTimer));
  $effect(() => {
    const id = flash;
    if (id === null || id === flashed || drawn === null || drawn.where.get(id) !== tileEl(id)) return;
    flashed = id;
    const tile = bandsEl?.querySelector<HTMLElement>(`.tf-dp-t[data-id="${CSS.escape(id)}"]`);
    tile?.scrollIntoView?.({ block: "center" });
    tile?.querySelector<HTMLElement>(".tf-dp-open")?.focus();
    // A second flash inside 1.6 s: the first one's timer goes, or the unmount clears only the last.
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      if (flash === id) flash = null;
      flashed = null;
    }, 1600);
  });

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
  let findField = $state<HTMLInputElement | null>(null);

  /** `/` focuses Find (R-2.4, R-14), in FilterBar's shape: not while typing elsewhere, not with a modifier. */
  const pageShown = pageVisible();
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      event.preventDefault();
      findField?.focus();
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });

  /** The matches drawn as tiles and the "more" lines that hold matches, in page order. */
  const findStops = () => [...(bandsEl?.querySelectorAll<HTMLElement>(".tf-dp-t.tf-dp-hit .tf-dp-open, .tf-dp-more.tf-dp-hit") ?? [])];

  /** `Enter` in *Find*: the next match; a group's "more" that holds matches opens, and its first hidden match is next (#2212). */
  async function findKey(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    let stops = findStops();
    if (stops.length === 0) return;
    jump = (jump + 1) % stops.length;
    let stop = stops[jump]!;
    if (stop.classList.contains("tf-dp-more")) {
      const tiles = stop.parentElement!;
      const drawn = new Set([...tiles.querySelectorAll<HTMLElement>(".tf-dp-t.tf-dp-hit")].map((tile) => tile.dataset.id));
      stop.click();
      await tick();
      const next = [...tiles.querySelectorAll<HTMLElement>(".tf-dp-t.tf-dp-hit")].find((tile) => !drawn.has(tile.dataset.id));
      const open = next?.querySelector<HTMLElement>(".tf-dp-open");
      if (!open) return;
      stops = findStops();
      jump = stops.indexOf(open);
      stop = open;
    }
    stop.scrollIntoView?.({ block: "center" });
    stop.focus();
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

  /** Read the estate again, and from a ticket what it reaches, since a link may have changed it. */
  async function reload() {
    try {
      const key = ticket;
      const [estate, again] = await Promise.all([
        ports.dependencies(),
        key === null ? Promise.resolve(null) : readReach(key, reach?.id ?? ticketId),
      ]);
      whole = estate;
      if (again !== null) reach = again;
    } catch (error) {
      push({ text: `The estate could not be read again: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#if host === "page"}
  <header class="tf-fhead">
    <div>
      <h1>Dependencies</h1>
      <div class="tf-sub">
        {ticket !== null ? `What ${ticket} reaches` : "Everything knobas knows"}, in layers from the work down to the hardware.
        Point at anything to see what it touches; click to open it.
      </div>
    </div>
    <div class="tf-acts">
      <a class="tf-btn tf-ghost" href="#/assets" title="The asset tree in columns">Asset tree</a>
      <a class="tf-btn tf-ghost" href="#/monitors">Monitors</a>
    </div>
  </header>
{/if}

<div class="tf-dp" class:tf-dp-job={host === "job"}>
  {#if failure !== null}
    <p class="tf-dp-fail" role="alert">The estate could not be read: {failure}. Open Dependencies again to retry.</p>
  {:else if read !== null && drawn !== null && views.draft !== null}
    {@const draft = views.draft}
    {@const scopeKey = ticket ?? job}
    <div class="tf-dp-bar" role="toolbar" aria-label="Dependencies">
      {#if scopeKey !== null}
        <div class="tf-seg" role="group" aria-label="Scope">
          <button
            type="button"
            aria-pressed={ticket !== null}
            onclick={() =>
              (location.hash = host === "job" ? `${jobHash(ticketId ?? scopeKey)}/map` : `#/deps/job/${encodeURIComponent(scopeKey)}`)}
            >This job <span class="tf-jobk">{scopeKey}</span></button
          >
          <button type="button" aria-pressed={ticket === null} onclick={() => (location.hash = "#/deps")}>Everything</button>
        </div>
      {/if}
      <TfDepsViewMenu bind:this={menu} {views} layerIds={read.layers.map((layer) => layer.id)} {ports} />
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
          data-filter
          bind:this={findField}
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
      <span class="tf-dp-vmenu tf-dp-addmenu">
        <button
          type="button"
          class="tf-btn tf-sm"
          aria-haspopup="menu"
          aria-expanded={addMenu}
          bind:this={addButton}
          onclick={() => (addMenu = !addMenu)}>Add ▾</button
        >
        {#if addMenu}
          <div class="tf-menu tf-appmenu tf-dp-menu" role="menu" tabindex="-1" bind:this={addMenuEl} onkeydown={addMenuKey}>
            <button type="button" role="menuitem" onclick={() => pickAdd(() => addTo(null))}
              ><b>Asset</b><span class="tf-dim">a name, a type and where it sits; the rest in its editor</span></button
            >
            <button type="button" role="menuitem" onclick={() => pickAdd(() => (routing = { id: null }))}
              ><b>Route</b><span class="tf-dim">an address, the asset that exposes it, and where it lands</span></button
            >
            <button type="button" role="menuitem" onclick={() => pickAdd(() => (importing = true))}
              ><b>Import…</b><span class="tf-dim">estate file, hcloud, Docker host, with a preview</span></button
            >
            {#if scoped !== null}
              <hr />
              <button type="button" role="menuitem" onclick={() => pickAdd(() => (linking = true))}
                ><b>Link to {scoped.key}…</b><span class="tf-dim">an asset, repository, pipeline or page that already exists</span></button
              >
            {/if}
          </div>
        {/if}
      </span>
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
            onfold={(layer, folded) => {
              // A fold by hand lets go of what was just added, which would hold its layer open.
              reveal = null;
              fold = new Map([...fold, [layer, folded]]);
            }}
            onopengroup={(key) => {
              opened = add(opened, key);
              shut = drop(shut, key);
            }}
            onshutgroup={(key) => {
              reveal = null;
              shut = add(shut, key);
              opened = drop(opened, key);
            }}
            onmore={(key) => (more = add(more, key))}
            onopen={openItem}
            ontrace={startTrace}
            {flash}
            onadd={addTo}
            onlink={scoped !== null ? () => (linking = true) : null}
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

{#if adding !== null && whole !== null}
  <TfNewAsset
    read={whole}
    layer={adding.layer}
    parent={traced !== null && isAsset(traced.kind) ? traced.id : null}
    ticket={scoped}
    {ports}
    onclose={() => (adding = null)}
    oncreated={(made) => {
      adding = null;
      landed(made.id, `Created ${made.name}`, true, made.unlinked);
    }}
  />
{/if}
{#if routing !== null && whole !== null}
  <TfRouteDialog
    read={whole}
    routeId={routing.id}
    ticket={scoped}
    {ports}
    onclose={() => (routing = null)}
    onsaved={(done) => {
      routing = null;
      landed(done.id, `${done.created ? "Created" : "Saved"} ${done.name}`, false, done.unlinked);
    }}
    ondeleted={(name) => {
      routing = null;
      deleted(name);
    }}
  />
{/if}
{#if importing}
  <TfImport
    {ports}
    onclose={() => (importing = false)}
    onimported={(name, outcome) => {
      importing = false;
      push({ text: importedLine(name, outcome) });
      void reload();
    }}
  />
{/if}
{#if linking && scoped !== null}
  <TfLinkTo
    jobKey={scoped.key}
    entityId={scoped.id}
    ports={{ search: ports.search, launcherHome: ports.launcherHome, createLink: (a, b, r, n) => ports.createLink(a, b, r, n) as Promise<LinkRow> }}
    {layers}
    onclose={() => (linking = false)}
    onlinked={() => {
      linking = false;
      push({ text: "Linked in knobas" });
      void reload();
    }}
  />
{/if}
{#if editing !== null && read !== null}
  <TfAssetEditor
    assetId={editing}
    read={whole ?? read}
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
    subject={{
      id: creating.asset.id,
      name: creating.asset.name,
      targets: creating.monitor_targets,
      opening: prefill(creating),
    }}
    record={(name) => ports.editAsset(creating!.asset.id, [{ field: "monitors", added: [name] }])}
    queue={ports.submitWrite}
    onclose={() => (creating = null)}
    oncreated={({ source }) => {
      toastQueued(`create monitor for ${creating?.asset.name ?? ""}`, source);
      creating = null;
    }}
  />
{/if}
