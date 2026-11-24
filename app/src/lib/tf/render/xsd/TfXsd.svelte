<!--
  An XML Schema on the Code page: the schema explorer (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-
  its-files-and-an-example-is-made-from-it.md`). *Render as* says *Schema*; *Source* is one pick away.

  **It is the API explorer's pane with other contents**: a box as tall as the window leaves under the file's header, set
  at mount and on `resize`, with columns that scroll on their own, the rail (`TfXsdRail`) and the page (`TfXsdPage`).

  **A schema is read as a set**, in the schema worker (`render/xsd-port.ts`): the opened file at once, the files it
  reaches as their reads land. The shell draws from the first answer; each file's row holds its room and says
  *reading…* until its declarations arrive, in folds that stay shut, so nothing drawn moves.

  **Addresses** name a place (`el/ORDER`, `type/bmecat:dtSTRING`). A pick in the rail replaces the place; a followed
  name pushes one, so the app's Back is the explorer's Back. A place this version of the file lacks opens the overview
  and says so.

  **An example opened in the XML view** (step E1; Fable, 2026-11-24) is a place of its own, `example/ORDER`: the
  explorer lands ORDER underneath, hidden and inert, and draws the full XML view over it in its pane's box, checked
  against the set it holds. Back and `Esc` show the explorer again as it was left; nothing is read again.
-->
<script lang="ts" module>
  let heldIds = 0;
</script>

<script lang="ts">
  import "../../css/openapi.css";
  import "../../css/xsd.css";

  import { flushSync, untrack } from "svelte";

  import TfSideEdge from "../../TfSideEdge.svelte";
  import { sideWidth } from "../../side-width.svelte";
  import { FuzzyOrder } from "../../filter/ranked.svelte";
  import { columnsSeen, paneHeight } from "../explorer/layout";
  import { zen } from "../../zen.svelte";
  import type { DeclRow, Opened } from "../../../render/xsd-answer";
  import type { Elsewhere, Pointed } from "../../../render/xsd-locate";
  import { heldIndex } from "../../../render/xsd-index";
  import { XsdPort, xsdEngine, type XsdEngine } from "../../../render/xsd-port";
  import { filterText, inMode } from "../../../render/xsd-view";
  import TfXsdPage from "./TfXsdPage.svelte";
  import TfXsdExample from "./TfXsdExample.svelte";
  import type { RenderPort } from "../../../render/port";
  import TfXsdPoint from "./TfXsdPoint.svelte";
  import { asPointed, gatherCandidates, keepDownload, keepPick, readDownloaded, splitAway, type Candidate } from "./xsd-point";
  import { xsdLocations } from "../../../ipc/api";
  import { ipcErrorMessage } from "../../../ipc";
  import { push } from "../../../shell/toasts.svelte";
  import TfXsdRail from "./TfXsdRail.svelte";
  import { OVERVIEW, XsdState, labelOfPlace } from "./xsd.svelte";
  import TfDocView from "../doc/TfDocView.svelte";
  import { textPiped, textPipedSave } from "../../../ipc/api";
  import { contentOf } from "../../../render/content-key";
  import { layers } from "../../layers.svelte";
  import type { Example, ExampleOptions } from "../../../render/xsd-example";
  import { EXAMPLE_PLACE, exampleMeta, exampleOfPlace, heldOptions, holdOptions, shown } from "./example-held.svelte";

  let {
    text,
    path,
    repo = "",
    read,
    paths,
    readAway = null,
    render = null,
    readOther = null,
    elsewhere = null,
    pointed = [],
    at = null,
    line = null,
    onplace = null,
    onback = null,
    oncopylink = null,
    onsource = null,
    onfile = null,
    onpoint = null,
    engine = null,
  }: {
    text: string;
    /** The schema's path from the repository's top: where its references are read from. */
    path: string;
    /** The repository: what its index is kept by. */
    repo?: string;
    /** Another file of the repository at the schema's commit; `null` where it cannot be read. */
    read: (path: string) => Promise<string | null>;
    /** Every path of the repository at the commit: read only when a reference is not found beside its file. */
    paths: () => Promise<readonly string[]>;
    /** The highlighter an example's lines are coloured by; no example is drawn without one. */
    render?: RenderPort | null;
    /** A file read from elsewhere (`@repo/…`, `@download/…`). */
    readAway?: ((path: string) => Promise<string | null>) | null;
    /** Another repository's file at its default branch's head (a pointed file, step 6). */
    readOther?: ((repo: string, path: string) => Promise<string | null>) | null;
    /** Other synced repositories' schemas that declare a namespace. */
    elsewhere?: ((namespace: string) => Promise<Elsewhere[]>) | null;
    /** What the person pointed to (`xsd.locations`). */
    pointed?: readonly Pointed[];
    /** The place an address names. */
    at?: string | null;
    /** A line of the file an address names (an Everything hit): the declaration that holds it opens. */
    line?: number | null;
    onplace?: ((place: string, how: "replace" | "push", label: string) => void) | null;
    onback?: (() => void) | null;
    oncopylink?: ((place: string, label: string) => void) | null;
    /** Shows a file of the set as Source at a line. */
    onsource?: ((file: string, line: number) => void) | null;
    /** Opens a file of the set as a file of its own. */
    onfile?: ((path: string) => void) | null;
    /** *Point to a file…* on a reference not found here: the explorer's own picker when left out. */
    onpoint?: ((namespace: string | null, location: string | null) => void) | null;
    /** Where the set is read: the worker when left out. */
    engine?: XsdEngine | null;
  } = $props();

  let opened = $state.raw<Opened | null>(null);
  let st = $state<XsdState | null>(null);
  let slow = $state(false);
  /** Bumped as each round's index lands: the page is asked again so names now found are links. */
  let round = $state(0);

  $effect(() => {
    const source = text;
    const root = path;
    const port = new XsdPort(untrack(() => engine) ?? xsdEngine());
    const state = new XsdState(port);
    // The example's settings, as they were left for this schema (held until knobas quits).
    exKey = `${untrack(() => repo)}\n${root}\n${contentOf(source, null)}`;
    const kept = heldOptions(exKey);
    if (kept !== undefined) state.ex = kept;
    st = state;
    opened = null;
    shownAt = undefined;
    let live = true;
    const timer = setTimeout(() => {
      if (live) slow = true;
    }, 200);
    port.onindex = (index) => {
      if (!live) return;
      state.take(index);
      round += 1;
    };
    const reads = untrack(() => ({
      paths,
      read,
      pointed,
      index: (from: string, list: readonly string[]) => heldIndex(repo, list, from, read),
      readAway: (p: string) => {
        // A pointed file of this repository is read as one of its own; another's at its own head.
        const away = splitAway(p);
        if (away !== null) return away.repo === repo ? read(away.path) : (readOther?.(away.repo, away.path) ?? Promise.resolve(null));
        return readDownloaded(p) ?? readAway?.(p) ?? Promise.resolve(null);
      },
      ...(elsewhere !== null ? { elsewhere } : {}),
    }));
    port.open(root, source, reads).then((answer) => {
      if (!live) return;
      clearTimeout(timer);
      slow = false;
      if (answer.ok) state.take(answer.index);
      opened = answer;
      // What a person pointed to answers before every guess: read once the shell stands, and the set again with it.
      if (answer.ok && untrack(() => pointed).length === 0)
        void xsdLocations().then(
          (kept) => {
            if (live && kept.pointed.length > 0) void port.repoint(asPointed(kept.pointed));
          },
          () => {},
        );
    });
    return () => {
      live = false;
      clearTimeout(timer);
      port.close();
    };
  });

  const index = $derived(opened !== null && opened.ok ? (st?.index ?? opened.index) : null);

  // ---- The example opened in the XML view (E1) ----
  /** What the example's settings are held by: the repository, the path and the content. */
  let exKey = "";
  $effect(() => {
    const options = st?.ex;
    if (options !== undefined) untrack(() => holdOptions(exKey, options));
  });
  /** The element an `example/` address names; `null` on every other place. */
  const exampleAt = $derived(render === null ? null : exampleOfPlace(at));
  /** The place the explorer shows: under an opened example, its element's page. */
  const under = $derived(exampleAt !== null ? `el/${exampleAt}` : at);
  /** The example drawn as the XML view: the explorer's own, taken when the view opens and still while it shows. */
  let snap = $state.raw<{ key: string; options: ExampleOptions; example: Example } | null>(null);
  /** Whether the view was opened by a press here (Back leaves it), or reached by an address (`Esc` replaces it). */
  let pushed = false;
  $effect(() => {
    const state = st;
    const name = exampleAt;
    if (state === null || name === null) {
      snap = null;
      return;
    }
    const page = state.page;
    if (page === null || !page.key.startsWith("el ") || state.current !== `el/${name}`) return;
    const made = state.made;
    if (made !== null && made.key === page.key) {
      if (untrack(() => snap)?.key !== page.key) snap = made;
      return;
    }
    untrack(() => void state.makeExample(page.key));
  });
  // One verdict: the view's check is the column's, so going back shows it, and nothing is checked twice.
  $effect(() => {
    const state = st;
    const taken = snap;
    if (state === null || taken === null || state.made !== taken || state.verdict !== null) return;
    untrack(() => void state.checkExample());
  });
  // The header says the example's name and how it was made.
  $effect(() => {
    const taken = snap;
    const place = at;
    if (taken === null || place === null) return;
    const head = { place, name: taken.example.fileName, meta: exampleMeta(taken.options, taken.example.lines.length), xml: taken.example.xml };
    shown.head = head;
    return () => {
      if (shown.head === head) shown.head = null;
    };
  });
  // While it shows, `Esc` leaves it before it leaves the page: an inline panel's rung.
  $effect(() => {
    const name = exampleAt;
    if (snap === null || name === null) return;
    return untrack(() => layers.add({ kind: "panel", close: () => leaveExample(name) }));
  });
  function leaveExample(name: string) {
    if (pushed && onback !== null) onback();
    else onplace?.(`el/${name}`, "replace", name);
    pushed = false;
  }
  /** *Open in XML view*: the example shown, as a place of its own. */
  function openExample() {
    const state = st;
    const made = state?.made ?? null;
    if (state === null || made === null) return;
    const place = placeOfKey(made.key);
    if (!place.startsWith("el/")) return;
    pushed = true;
    onplace?.(`${EXAMPLE_PLACE}${place.slice(3)}`, "push", made.example.fileName);
  }
  /** The set the opened example is checked against: the explorer's own port, named as the overview names its entry. */
  const against = $derived(st === null || index === null ? null : { port: st.port, name: path.slice(path.lastIndexOf("/") + 1), path, round });

  // ---- The filter: the launcher's rules over a list the picker holds for the set (section 8) ----
  const decls = $derived(index === null ? [] : index.decls);
  const NONE: DeclRow[] = [];
  let order = $state.raw(FuzzyOrder.held<DeclRow>(`xsd:${++heldIds}`));
  $effect(() => {
    void index?.entry;
    const mine = FuzzyOrder.held<DeclRow>(`xsd:${++heldIds}`);
    order = mine;
    return () => mine.drop();
  });
  $effect(() => {
    const typed = st?.typed ?? "";
    order.ask(typed, typed.trim() === "" ? NONE : decls, filterText);
  });
  const ranked = $derived.by(() => {
    const state = st;
    const typed = state?.typed ?? "";
    if (state === null || typed.trim() === "") return null;
    // In *Types* only types; in the other modes everything the set declares.
    const pool = state.mode === "types" ? decls.filter((d) => inMode("types", d)) : decls;
    return order.rows(typed, decls, filterText)
      .filter((d) => state.mode !== "types" || pool.includes(d))
      .map((decl) => {
        const marks = order.marks(decl);
        const name = decl.prefix.length + decl.local.length;
        const on: number[] = [];
        for (let i = 0; i + 1 < marks.length; i += 2) if (marks[i]! < name) on.push(marks[i]!, Math.min(marks[i + 1]!, name));
        return { decl, marks: on.length > 0 ? on : null };
      });
  });

  // ---- Point to a file… (section 17) ----
  let pointing = $state<{ namespace: string | null; location: string | null } | null>(null);
  let candidates = $state.raw<Candidate[] | null>(null);
  function point(namespace: string | null, location: string | null) {
    if (onpoint !== null) {
      onpoint(namespace, location);
      return;
    }
    pointing = { namespace, location };
    candidates = null;
    void gatherCandidates(repo, path, namespace, { paths, read, elsewhere }).then((list) => {
      if (pointing?.namespace === namespace && pointing.location === location) candidates = list;
    });
  }
  async function pickFile(c: Candidate) {
    const at = pointing;
    if (at === null) return;
    pointing = null;
    await pickFor(at.namespace, at.location, c);
  }
  /** *Download it*: the schema at the address downloaded on the press, kept for the reference, the set read again. */
  async function downloadFor(namespace: string | null, location: string | null, address: string) {
    const state = st;
    if (state === null) return;
    try {
      await state.port.repoint(await keepDownload(namespace, location, address));
      push({ text: `Downloaded ${address}. It answers ${location ?? namespace ?? "it"} from now on.` });
    } catch (cause) {
      push({ text: `Could not download it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  /** Keeps `c` for a reference and reads the set again through it: the picker's pick, or one of two repositories'. */
  async function pickFor(namespace: string | null, location: string | null, c: Candidate) {
    const state = st;
    if (state === null) return;
    try {
      await state.port.repoint(await keepPick(namespace, location, c));
      push({ text: `${c.repoName === null ? c.path : `${c.repoName}: ${c.path}`} answers ${location ?? namespace ?? "it"} from now on.` });
    } catch (cause) {
      push({ text: `Could not keep it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  // ---- The pane ----
  let cols = $state<2 | 3 | 4>(2);
  function pane(el: HTMLElement) {
    const fit = () => {
      const scroller = scrollerOf(el);
      const status = document.querySelector<HTMLElement>(".tf-status")?.offsetHeight ?? 0;
      const top = el.getBoundingClientRect().top + (scroller?.scrollTop ?? 0);
      el.style.setProperty("--oas-h", `${paneHeight(window.innerHeight, top, status)}px`);
    };
    fit();
    window.addEventListener("resize", fit);
    $effect(() => {
      void zen.shown;
      const frame = requestAnimationFrame(fit);
      return () => cancelAnimationFrame(frame);
    });
    const view = el.closest<HTMLElement>(".tf-fileview") ?? el;
    const first = columnsSeen(view.getBoundingClientRect().width);
    if (first !== null && first !== cols) cols = first;
    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver((entries) => {
        const next = columnsSeen(entries[0]?.contentRect.width ?? 0);
        if (next !== null && next !== cols) cols = next;
      });
      ro.observe(view);
    }
    return () => {
      window.removeEventListener("resize", fit);
      ro?.disconnect();
    };
  }
  function scrollerOf(el: HTMLElement): HTMLElement | null {
    for (let up = el.parentElement; up !== null; up = up.parentElement) {
      const y = getComputedStyle(up).overflowY;
      if (y === "auto" || y === "scroll") return up;
    }
    return null;
  }

  // ---- The example (sections 19 to 23) ----
  /** An example stands for a global element's page, where lines can be coloured. */
  const exampleShown = $derived(render !== null && st !== null && st.page !== null && st.page.key.startsWith("el "));
  /** `A`: another example, from anywhere in the explorer outside a field, while one is in view. */
  function keys(event: KeyboardEvent) {
    if (event.key !== "a" && event.key !== "A") return;
    if (event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
    const t = event.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement || (t instanceof HTMLElement && t.isContentEditable)) return;
    const state = st;
    if (state === null || !exampleShown || snap !== null || (cols < 3 && state.tab !== "example")) return;
    event.preventDefault();
    state.another();
  }

  // ---- Pages: picks, followed names, the way back, and the address's place ----
  let pageCol = $state<HTMLElement | null>(null);
  let shownAt: string | null | undefined = undefined;

  /** A key (`el {ns}ORDER`) or a place (`el/ORDER`) as the address writes it. */
  const placeOfKey = (key: string): string => {
    if (key === OVERVIEW) return OVERVIEW;
    if (!/^(el|ty|gr|ag|at) \{/.test(key)) return key;
    const d = index?.decls.find((x) => x.key === key);
    const space = key.slice(0, 2);
    const word = space === "el" ? "el" : space === "ty" ? "type" : space === "gr" ? "group" : space === "ag" ? "attrs" : "attr";
    return d === undefined ? key : `${word}/${d.prefix}${d.local}`;
  };

  async function land(place: string, scroll: number) {
    const state = st;
    if (state === null) return;
    state.drawWhole = scroll > 0;
    const done = await state.show(place);
    if (!done || st !== state) {
      state.drawWhole = false;
      return;
    }
    flushSync();
    state.drawWhole = false;
    if (pageCol !== null) pageCol.scrollTop = scroll;
  }

  /** A pick in the rail: it replaces what the page shows and clears the way back. */
  function pick(key: string, label: string) {
    const state = st;
    if (state === null) return;
    const place = placeOfKey(key);
    state.backs = [];
    shownAt = place === OVERVIEW ? null : place;
    void land(place, 0);
    onplace?.(place, "replace", label);
  }

  /** A followed name: the page left is a step back. */
  function go(key: string, label: string) {
    const state = st;
    if (state === null) return;
    const place = placeOfKey(key);
    if (place === state.current) return;
    state.backs = [...state.backs, { place: state.current, label: labelOfPlace(state.current), scroll: pageCol?.scrollTop ?? 0 }];
    shownAt = place === OVERVIEW ? null : place;
    void land(place, 0);
    onplace?.(place, "push", label);
  }

  function back() {
    const state = st;
    if (state === null || state.backs.length === 0) return;
    if (onback !== null) {
      onback();
      return;
    }
    stepBack();
  }
  function stepBack() {
    const state = st;
    const to = state?.backs.at(-1);
    if (state === null || to === undefined) return;
    state.backs = state.backs.slice(0, -1);
    shownAt = to.place === OVERVIEW ? null : to.place;
    void land(to.place, to.scroll);
  }

  $effect(() => {
    // An opened example's element is the place underneath it: an `example/` place never reaches the set.
    const place = under;
    const hit = line;
    const state = st;
    if (state === null || index === null) return;
    untrack(() => {
      if (place === null && hit !== null && shownAt === undefined) {
        // An Everything hit: the declaration whose lines hold it.
        const own = index.decls.filter((d) => d.file === index.entry && d.line <= hit).sort((a, b) => b.line - a.line)[0];
        shownAt = null;
        if (own !== undefined) {
          const found = placeOfKey(own.key);
          shownAt = found;
          void land(found, 0);
          onplace?.(found, "replace", `${own.prefix}${own.local}`);
        }
        return;
      }
      if (place === shownAt) return;
      if (shownAt !== undefined && state.backs.length > 0 && (place ?? OVERVIEW) === state.backs.at(-1)!.place) {
        stepBack();
        return;
      }
      if (shownAt !== undefined) state.backs = [];
      shownAt = place;
      void land(place ?? OVERVIEW, 0);
    });
  });

  // A round of files landed: the page shown is asked again, its rows now holding what the new files declare.
  $effect(() => {
    if (round === 0) return;
    const state = untrack(() => st);
    if (state === null) return;
    if (state.waiting !== null || state.current === OVERVIEW) return;
    untrack(() => {
      state.kids.clear();
      state.expand.clear();
      const scroll = pageCol?.scrollTop ?? 0;
      void land(state.current, scroll);
      // The opened example is checked again against the set as it now stands; its text stays.
      if (snap !== null && state.made === snap) void state.checkExample();
    });
  });
</script>

{#if opened !== null && !opened.ok}
  <div class="tf-oas-bad" role="alert">
    <p><b>This schema could not be read.</b></p>
    <pre>{opened.error}</pre>
    {#if opened.line !== null}<p class="tf-dim">Line {opened.line}.</p>{/if}
  </div>
{:else}
  <!-- The pane stands from the first frame at its height, the wait inside it, so the page's height and its scrollbar
       do not change when the worker answers (`moves`, 2026-11-24: the header's buttons shifted 16 px). -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="tf-xsd-stage">
  <div class="tf-oas tf-oas-{cols >= 3 && render !== null ? 3 : 2} tf-xsd" class:tf-xsd-under={snap !== null} inert={snap !== null} data-cols={cols} onkeydown={keys} {@attach pane} {@attach sideWidth("xsd-rail")}>
    {#if index === null || st === null}
      <div class="tf-oas-wait tf-xsd-wait">{#if slow}<p class="tf-oas-reading" role="status">Reading the schema…</p>{/if}</div>
    {:else}
    {@const state = st}
    <TfXsdRail st={state} {ranked} onpick={pick} />
    <TfSideEdge side="xsd-rail" />
    <TfXsdPage
      st={state}
      ongo={go}
      onback={back}
      onfile={(p) => onfile?.(p)}
      {onsource}
      {oncopylink}
      onpoint={point}
      onpick={(namespace, location, c) => void pickFor(namespace, location, c)}
      ondownload={downloadFor}
      head={cols < 3 && exampleShown ? tabs : null}
      body={cols < 3 && exampleShown && state.tab === "example" && render !== null ? examplePage : null}
      bind:scroller={pageCol}
    />
    {#if cols >= 3 && render !== null}
      <div class="tf-oas-col tf-xsd-excol" aria-label="Example"><TfXsdExample st={state} {render} ongo={go} onopen={openExample} /></div>
    {/if}
    {/if}
  </div>
  {#if snap !== null && st !== null && render !== null}
    {@const state = st}
    {@const doc = snap.example}
    <!-- The example as the XML view, in the explorer's own box: the explorer stays drawn under it, so Back moves nothing. -->
    <div class="tf-xsd-opened">
      <TfDocView
        text={doc.xml}
        path={doc.fileName}
        {repo}
        {render}
        verdict={state.made === snap ? state.verdict : undefined}
        against={against}
        values={{ run: (steps) => textPiped(doc.xml, doc.fileName, steps), save: (steps, name) => textPipedSave(doc.xml, doc.fileName, steps, name) }}
      />
    </div>
  {/if}
  </div>
{/if}

{#snippet tabs()}
  {@const state = st!}
  <span class="tf-seg tf-oas-pageseg" role="group" aria-label="Show">
    <button type="button" aria-pressed={state.tab === "ref"} onclick={() => (state.tab = "ref")}>Reference</button>
    <button type="button" aria-pressed={state.tab === "example"} onclick={() => (state.tab = "example")}>Example</button>
  </span>
{/snippet}
{#snippet examplePage()}
  <TfXsdExample st={st!} render={render!} ongo={go} onopen={openExample} />
{/snippet}

{#if pointing !== null}
  {@const at = pointing}
  <TfXsdPoint namespace={at.namespace} location={at.location} {candidates} onpick={(c) => void pickFile(c)} onclose={() => (pointing = null)} />
{/if}
