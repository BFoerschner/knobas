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
  import { columnsFor, paneHeight } from "../explorer/layout";
  import { zen } from "../../zen.svelte";
  import type { DeclRow, Opened } from "../../../render/xsd-answer";
  import type { Elsewhere, Pointed } from "../../../render/xsd-locate";
  import { buildIndex } from "../../../render/xsd-index";
  import { XsdPort, xsdEngine, type XsdEngine } from "../../../render/xsd-port";
  import { filterText, inMode } from "../../../render/xsd-view";
  import TfXsdPage from "./TfXsdPage.svelte";
  import TfXsdRail from "./TfXsdRail.svelte";
  import { OVERVIEW, XsdState, labelOfPlace } from "./xsd.svelte";

  let {
    text,
    path,
    repo = "",
    read,
    paths,
    readAway = null,
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
    /** A file read from elsewhere (`@repo/…`, `@download/…`). */
    readAway?: ((path: string) => Promise<string | null>) | null;
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
    /** *Point to a file…* on a reference not found here. */
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
      index: (from: string, list: readonly string[]) => buildIndex(repo, list, from, read),
      ...(readAway !== null ? { readAway } : {}),
      ...(elsewhere !== null ? { elsewhere } : {}),
    }));
    port.open(root, source, reads).then((answer) => {
      if (!live) return;
      clearTimeout(timer);
      slow = false;
      if (answer.ok) state.take(answer.index);
      opened = answer;
    });
    return () => {
      live = false;
      clearTimeout(timer);
      port.close();
    };
  });

  const index = $derived(opened !== null && opened.ok ? (st?.index ?? opened.index) : null);

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
    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver((entries) => {
        const next = columnsFor(entries[0]?.contentRect.width ?? 0);
        if (next !== cols) cols = next;
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
    const place = at;
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
    });
  });
</script>

{#if opened === null}
  <div class="tf-oas-wait">{#if slow}<p class="tf-oas-reading" role="status">Reading the schema…</p>{/if}</div>
{:else if !opened.ok}
  <div class="tf-oas-bad" role="alert">
    <p><b>This schema could not be read.</b></p>
    <pre>{opened.error}</pre>
    {#if opened.line !== null}<p class="tf-dim">Line {opened.line}.</p>{/if}
  </div>
{:else if index !== null && st !== null}
  {@const state = st}
  <div class="tf-oas tf-oas-2 tf-xsd" data-cols={cols} {@attach pane} {@attach sideWidth("xsd-rail")}>
    <TfXsdRail st={state} {ranked} onpick={pick} />
    <TfSideEdge side="xsd-rail" />
    <TfXsdPage
      st={state}
      ongo={go}
      onback={back}
      onfile={(p) => onfile?.(p)}
      {onsource}
      {oncopylink}
      {onpoint}
      bind:scroller={pageCol}
    />
  </div>
{/if}
