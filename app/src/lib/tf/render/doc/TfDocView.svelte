<!--
  **The XML view** (`docs/decisions/2026-11-23-an-xml-file-is-read-as-a-document.md`): an XML file drawn as a document.
  Its own lines, as Source draws them, in the explorers' pane, with what a document has that text has not.

  - **The pane** fits the window, as the API explorer's does: the rail (the head, *Findings* and *Results*, the keys),
    the document (the way down and *Levels*, the query, the lines), and from 1,480 px the element picked.
  - **The first frame is the file's lines**, drawn from the text at hand with every column held: the caret's column
    beside the number, the line's end, the rail's head. The outline is read in the document worker
    (`render/doc-port.ts`) and lands as paint: carets, the way down, the fold counts. Nothing moves when it does.
  - **Folding** by the caret (Alt for everything under), `←` `→` (`Shift` for everything under), the digit keys and the
    *Levels* stepper (`-` `+`). A fold is one line: its first line, what it hides counted, its closing tag dim.
  - **Find in file** is Source's, through `TfLines`: no box until ⌘F, then a bar at the top of the lines that takes no
    room (Fable, 2026-11-23).

  The view, its CSS and the worker load with the first document opened, never at start-up (`doc-module.ts`).
-->
<script lang="ts">
  import "../../css/openapi.css";
  import "../../css/xml.css";

  import { tick, untrack } from "svelte";

  import TfLines from "../../code/TfLines.svelte";
  import TfSideEdge from "../../TfSideEdge.svelte";
  import { sideWidth } from "../../side-width.svelte";
  import { zen } from "../../zen.svelte";
  import { columnsFor, paneHeight } from "../oas-layout";
  import TfDocRail from "./TfDocRail.svelte";
  import TfDocPicked from "./TfDocPicked.svelte";
  import TfDocQuery from "./TfDocQuery.svelte";
  import TfDocResults from "./TfDocResults.svelte";
  import TfMenu from "../../TfMenu.svelte";
  import { DocState, contentOf, remember } from "./doc.svelte";
  import { UNITS, countInside } from "../../../render/doc-folds";
  import type { CopyWhat, ValueAt } from "../../../render/doc-model";
  import TfValueViewer, { type LookedAt } from "../TfValueViewer.svelte";
  import { guess as guessOf, type End, type Guess } from "../../../render/value-guess";
  import { keptFor } from "../../../render/value-kept";
  import { namedPipelines, commandSteps } from "../../../render/pipelines.svelte";
  import type { NamedPipeline } from "../../../ipc/app";
  import type { PipeRun, PipeStep } from "../../../ipc/entity";
  import { menuThing } from "../../context/thing";
  import TfFileViewer, { type ViewerContent } from "../TfFileViewer.svelte";
  import { linksIn, type Link } from "../../../render/doc-links";
  import { openWeb, pressElsewhere } from "../../../shell/open-web";
  import { imageFetch, imageHosts, setImageHosts } from "../../../ipc/api";
  import { ipcErrorMessage } from "../../../ipc";
  import { KEPT_HOSTS_LOAD_BY_ITSELF } from "./doc-keys";
  import { wantsLayout } from "../../../render/doc-line";
  import { DocSession, docEngine, type DocEngine } from "../../../render/doc-port";
  import { languageOf } from "../../../render/engines";
  import type { RenderPort } from "../../../render/port";
  import { push } from "../../../shell/toasts.svelte";
  import { DOC_KEYS, isKey } from "./doc-keys";

  let {
    text,
    path,
    repo = "",
    sha = null,
    render,
    line = null,
    engine = null,
    values = null,
    pipelines = null,
    files = null,
    images = null,
  }: {
    text: string;
    /** The file's path from the repository's top. */
    path: string;
    /** The repository: with the path, what folds are kept by. */
    repo?: string;
    /** The file's blob, where the source said: what tells the same content from another. */
    sha?: string | null;
    render: RenderPort;
    /** The line a link names. */
    line?: number | null;
    /** Where the outline is read: the worker when left out. */
    engine?: DocEngine | null;
    /** How a value pipeline runs and saves over this file; `null` where no pipeline can run (no value is looked at). */
    values?: { run: (steps: PipeStep[]) => Promise<PipeRun>; save: (steps: PipeStep[], name: string) => Promise<string> } | null;
    /**
     * The repository's files: whether one is there (`null` until its paths are read), a read of its paths, opening one,
     * and its bytes. `null` where none is known: no path is a link.
     */
    files?: { has: (path: string) => boolean | null; ask: () => void; open: (path: string) => void; bytes: (path: string) => Promise<Uint8Array> } | null;
    /** How an image at an address is loaded and which hosts are kept: the app's when left out. */
    images?: { fetch: typeof imageFetch; hosts: typeof imageHosts; setHosts: typeof setImageHosts } | null;
    /** The named pipelines, kept chains among them: the app's when left out. */
    pipelines?: { list: readonly NamedPipeline[]; save: (list: never) => Promise<unknown> } | null;
  } = $props();

  const kept = $derived(pipelines ?? namedPipelines);

  /** The lines of a document a row is: fixed, so a windowed list of 800,000 rows holds one height. */
  const ROW = 22;
  /** A line longer than this is drawn as its first {@link KEEP} characters and a count (section 6). */
  const CLIP = { over: 400, keep: 320 };

  let st = $state<DocState | null>(null);
  /** *As written*, pressed on a document knobas laid out. */
  let asWritten = $state(false);
  /** A file written on one line is laid out (section 6): decided from the text at hand, before the first frame. */
  const layout = $derived(!asWritten && wantsLayout(text));
  /** The layout, once the worker made it. */
  let laid = $state<string | null>(null);
  /** The text the lines draw: the file's, or its layout. */
  const shownText = $derived(layout ? laid : text);
  $effect(() => {
    const source = text;
    const at = path;
    const laying = layout;
    laid = null;
    const lines = laying ? 1 : countLines(source);
    const session = new DocSession(untrack(() => engine) ?? docEngine());
    const state = new DocState(session, `${untrack(() => repo)}\n${at}${laying ? "\nlaid" : ""}`, contentOf(source, untrack(() => sha)), lines);
    st = state;
    let live = true;
    void session.open(source).then(async (answer) => {
      // A worker that failed answers no table: the lines stay as drawn, unfolding nothing.
      if (!live || answer === null || typeof answer !== "object" || !("folds" in answer)) return;
      if (!laying) {
        state.take(answer);
        return;
      }
      const made = await session.layout();
      if (!live || made === null || typeof made !== "object" || !("text" in made)) return;
      laid = made.text;
      state.take(made.opened);
    });
    return () => {
      live = false;
      state.close();
    };
  });

  function countLines(s: string): number {
    let n = 1;
    for (let i = s.indexOf("\n"); i !== -1; i = s.indexOf("\n", i + 1)) n += 1;
    return n > 1 && s.endsWith("\n") ? n - 1 : n;
  }

  /** The path the highlighter reads: the file's own where it names XML, else an XML file's. */
  const colourAs = $derived(languageOf(path) === "xml" || languageOf(path) === "xsl" ? path : "document.xml");

  /** The longest line as drawn, in characters: every row is as wide, so a line's end holds to the pane's edge. */
  const longest = $derived.by(() => {
    const drawn = shownText ?? "";
    let most = 0;
    let from = 0;
    for (let i = drawn.indexOf("\n"); ; i = drawn.indexOf("\n", i + 1)) {
      const len = (i === -1 ? drawn.length : i) - from;
      if (len > most) most = len;
      if (i === -1) break;
      from = i + 1;
    }
    return Math.min(most, CLIP.keep + 16);
  });

  // ---- The pane: its height set once at mount and on resize, its columns by the file view's width ----
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
  function wide(el: HTMLElement) {
    $effect(() => {
      el.style.setProperty("--xv-longest", `${longest}ch`);
    });
  }

  // ---- The lines ----
  let lines = $state<ReturnType<typeof TfLines> | null>(null);
  let code = $state<HTMLElement | null>(null);
  let mode = $state<"findings" | "results">("results");

  /** Goes to a line, opening what hides it, picks it and scrolls it into view. */
  async function go(n: number, centre = true) {
    const state = st;
    if (state === null) return;
    state.pick(n);
    await tick();
    lines?.reveal(n, centre);
  }

  // A line link picks its line.
  $effect(() => {
    const at = line;
    const state = st;
    if (at === null || state === null) return;
    untrack(() => {
      state.cursor = at;
      state.ask(at);
    });
  });

  function rowClass(n: number): string {
    if (st === null) return "tf-xv-l";
    return `tf-xv-l${n === st.cursor ? " tf-xv-cur" : ""}${st.hits.has(n) ? " tf-xv-hit" : ""}`;
  }

  // ---- Values (sections 15 to 18) ----
  /** What a line's value is taken for: the first of its values with a chain kept for its field, or a guess. */
  function chipOf(n: number): { value: ValueAt; guess: Guess | null; kept: { pipeline: NamedPipeline; end: string } | null } | null {
    const list = st?.values.get(n);
    if (list === undefined) return null;
    for (const v of list) {
      const k = keptFor(v.field, path, kept.list);
      const g = guessOf(v.head, v.said, { length: v.length, bare: v.bare });
      if (k !== null || g !== null) return { value: v, guess: g, kept: k === null ? null : { pipeline: k.pipeline, end: k.kept.end } };
    }
    return null;
  }

  /** The lines drawn moved: their values are read. */
  function drawnLines(from: number, to: number) {
    const state = st;
    if (state === null) return;
    const vis = state.visible;
    const out: number[] = [];
    let lo = 0;
    let hi = vis.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (vis[mid]! < from) lo = mid + 1;
      else hi = mid;
    }
    for (let i = lo; i < vis.length && vis[i]! <= to; i += 1) out.push(vis[i]!);
    state.readValues(out);
  }

  let looking = $state<{ value: LookedAt; guess: Guess | null; kept: { pipeline: NamedPipeline; end: string } | null; first: { steps: PipeStep[]; end: End | null } | null } | null>(null);

  const lookedAt = (v: ValueAt): LookedAt => ({ from: v.from, to: v.to, read: v.read, name: v.name, field: v.field, length: v.length, fileName: v.fileName, line: v.line });

  /** Opens the value viewer on line `n`'s value: knobas's guess or the kept chain, or `step` first, or none. */
  async function lookAt(n: number, through: PipeStep | "none" | null = null) {
    const state = st;
    if (state === null || values === null || n < 1) return;
    const shown = chipOf(n);
    let v: ValueAt | undefined = shown?.value;
    if (v === undefined || through !== null) {
      const got = await state.session.values([n], true);
      v = shown?.value ?? got[n]?.[0];
    }
    if (v === undefined) {
      push({ text: "This line holds no value to look at." });
      return;
    }
    const g = guessOf(v.head, v.said, { length: v.length, bare: v.bare });
    const k = keptFor(v.field, path, kept.list);
    looking = {
      value: lookedAt(v),
      guess: through === null ? g : null,
      kept: through === null && k !== null ? { pipeline: k.pipeline, end: k.kept.end } : null,
      first: through === null ? null : { steps: through === "none" ? [] : [through], end: through === "none" ? "text" : null },
    };
  }

  /** A row's right-click: the line's own rows and its value's. */
  function rowThing(n: number) {
    return menuThing(() =>
      st === null
        ? null
        : {
            kind: "value",
            repoId: repo,
            path,
            line: n,
            link: !layout,
            does: {
              look: () => void lookAt(n),
              through: (step) => void lookAt(n, step === null ? "none" : ({ step } as PipeStep)),
              copyXPath: () => {
                const state = st;
                if (state === null) return;
                state.ask(n);
                void state.session.at(n).then((a) => (a.picked === null ? undefined : copied(a.picked.path, "the XPath")));
              },
              like: () => {
                const state = st;
                if (state === null) return;
                void state.session.like(n).then((q) => {
                  if (q !== null) ask(q);
                });
              },
            },
          },
    );
  }

  // ---- Links and images (sections 13 and 14) ----
  const imagePort = $derived(images ?? { fetch: imageFetch, hosts: imageHosts, setHosts: setImageHosts });
  /** Each drawn line's links, by line: read once a line is drawn, again when the repository's paths come in. */
  let linkCache = new Map<number, readonly Link[]>();
  let pathsKnown = $state(false);
  $effect(() => {
    void text;
    void pathsKnown;
    linkCache = new Map();
  });
  function linksOf(n: number, shown: string): readonly Link[] {
    const held = linkCache.get(n);
    if (held !== undefined) return held;
    const has = files === null ? null : (p: string) => files.has(p) === true;
    // The first line that holds something path-like asks for the repository's paths.
    if (files !== null && files.has("") === null && /[\w)]\.[A-Za-z][A-Za-z0-9]{0,7}\b/.test(shown)) {
      files.ask();
      waitForPaths();
    }
    const links = linksIn(shown, path, files === null || files.has("") === null ? null : has);
    linkCache.set(n, links);
    return links;
  }
  let pathsTimer: ReturnType<typeof setInterval> | null = null;
  /** The repository's paths are read once; when they land, the lines drawn are read for paths again. */
  function waitForPaths() {
    if (pathsTimer !== null || files === null) return;
    pathsTimer = setInterval(() => {
      if (files?.has("") !== null) {
        if (pathsTimer !== null) clearInterval(pathsTimer);
        pathsTimer = null;
        pathsKnown = true;
      }
    }, 100);
  }
  $effect(() => () => {
    if (pathsTimer !== null) clearInterval(pathsTimer);
  });

  /** What the image dialog shows: an image of the repository, or one at an address. */
  let imaging = $state<{ name: string; line: string; content: ViewerContent; bytes: Uint8Array | null } | null>(null);
  let imageUrl: string | null = null;
  function showImage(name: string, line: string, bytes: Uint8Array, mime: string) {
    if (imageUrl !== null) URL.revokeObjectURL(imageUrl);
    imageUrl = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
    imaging = { name, line, content: { kind: "image", src: imageUrl }, bytes };
  }
  function closeImage() {
    if (imageUrl !== null) URL.revokeObjectURL(imageUrl);
    imageUrl = null;
    imaging = null;
  }
  $effect(() => () => {
    if (imageUrl !== null) URL.revokeObjectURL(imageUrl);
  });

  /** An image of the repository: in the file viewer, as a ticket's attachment opens. */
  function openRepoImage(to: string) {
    if (files === null) return;
    const name = to.slice(to.lastIndexOf("/") + 1);
    imaging = { name, line: to, content: { kind: "reading" }, bytes: null };
    files.bytes(to).then(
      (bytes) => {
        if (imaging?.line !== to) return;
        const ext = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
        showImage(name, to, bytes, ext === "svg" ? "image/svg+xml" : `image/${ext === "jpg" ? "jpeg" : ext}`);
      },
      (cause) => {
        if (imaging?.line === to) imaging = { name, line: to, content: { kind: "missing", why: ipcErrorMessage(cause), retry: () => openRepoImage(to) }, bytes: null };
      },
    );
  }

  /** An image at an address: offered, or loaded at once where its host is kept. Never without the press that opened it. */
  async function openWebImage(url: string) {
    let host = "";
    try {
      host = new URL(url).host.toLowerCase();
    } catch {
      return;
    }
    const name = url.slice(url.lastIndexOf("/") + 1).replace(/[?#].*$/, "") || host;
    const load = (always: boolean) => {
      imaging = { name, line: url, content: { kind: "offer", url, host, load, browser: () => openWeb(url, { elsewhere: true }), loading: true, why: null }, bytes: null };
      if (always) void imagePort.hosts().then((kept) => imagePort.setHosts({ hosts: [...kept.hosts, host] })).catch(() => {});
      imagePort.fetch(url).then(
        (got) => {
          if (imaging?.line !== url) return;
          const raw = atob(got.base64);
          const bytes = new Uint8Array(raw.length);
          for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
          showImage(name, url, bytes, got.mime);
        },
        (cause) => {
          if (imaging?.line === url && imaging.content.kind === "offer") imaging = { ...imaging, content: { ...imaging.content, loading: false, why: ipcErrorMessage(cause) } };
        },
      );
    };
    imaging = { name, line: url, content: { kind: "offer", url, host, load, browser: () => openWeb(url, { elsewhere: true }), loading: false, why: null }, bytes: null };
    const kept = await imagePort.hosts().catch(() => ({ hosts: [] as string[] }));
    if (KEPT_HOSTS_LOAD_BY_ITSELF && kept.hosts.includes(host) && imaging?.line === url) load(false);
  }

  /** A link pressed, or a line's link by key: where it goes. `elsewhere` asks for the browser. */
  function follow(link: Link, elsewhere: boolean) {
    if (link.kind === "web") {
      if (link.image && !elsewhere) void openWebImage(link.to);
      else openWeb(link.to, { elsewhere });
    } else if (link.kind === "path") {
      if (link.image) openRepoImage(link.to);
      else files?.open(link.to);
    } else if (link.kind === "schema") {
      const there = files === null ? null : linksIn(`<a>${link.to}</a>`, path, (p) => files.has(p) === true).find((l) => l.kind === "path");
      if (there !== undefined && there !== null) files?.open(there.to);
      else if (/^https?:/.test(link.to)) openWeb(link.to, { elsewhere });
      else push({ text: `No file is at ${link.to} in this repository.` });
    } else {
      // A namespace is a name, not a page: the schema that declares it, where one is read, comes with the check.
      push({ text: `${link.to} is a namespace's name, not a page. Right-click to open it in the browser all the same.` });
    }
  }

  function linkPressed(target: Element, event: MouseEvent): boolean {
    const a = target.closest<HTMLElement>("[data-xv-link]");
    if (a === null) return false;
    const [n, i] = (a.dataset["xvLink"] ?? "").split(":").map(Number);
    const link = linkCache.get(n!)?.[i!];
    if (link === undefined) return false;
    event.preventDefault();
    follow(link, pressElsewhere(event));
    return true;
  }

  /** `W`: the picked line's address; several open a menu of them. `Shift W` asks for the browser. */
  let webMenu = $state<{ links: Link[]; elsewhere: boolean; opener: HTMLElement } | null>(null);
  function web(elsewhere: boolean) {
    const state = st;
    if (state === null || state.cursor === 0) return;
    const row = code?.querySelector<HTMLElement>(`.tf-l[data-n="${state.cursor}"]`);
    const links = (linkCache.get(state.cursor) ?? []).filter((l) => l.kind === "web" || l.kind === "path" || l.kind === "schema");
    if (links.length === 0) {
      push({ text: "This line holds no address." });
      return;
    }
    if (links.length === 1 || row === null || row === undefined) follow(links[0]!, elsewhere);
    else {
      webMenu = { links, elsewhere, opener: row };
      webMenuOpen = true;
    }
  }
  let webMenuOpen = $state(false);
  $effect(() => {
    if (!webMenuOpen) webMenu = null;
  });

  // ---- The query (section 22) ----
  let field = $state<HTMLInputElement | null>(null);
  let resultList = $state<HTMLElement | null>(null);
  let copyOpen = $state(false);
  let copyButton = $state<HTMLButtonElement | null>(null);

  /** The lines of what the rail lists, in its order: the break of a broken file, or the matches. */
  function listed(state: DocState): number[] {
    if (mode === "findings") return state.opened?.error ? [state.opened.error.line] : [];
    return state.rows.map((r) => r.line);
  }

  /** `N` and `P`, `Enter` in the field: the next or the one before of what the rail lists. */
  function step(by: 1 | -1) {
    const state = st;
    if (state === null) return;
    const lines = listed(state);
    if (lines.length === 0) return;
    const at = state.cur < 0 ? (by > 0 ? 0 : lines.length - 1) : (state.cur + by + lines.length) % lines.length;
    state.cur = at;
    if (mode === "results") {
      remember(state.source);
      if (at >= lines.length - 40) state.more();
    }
    void go(lines[at]!);
    void tick().then(() => resultList?.querySelector(`[data-xv-res="${at}"]`)?.scrollIntoView?.({ block: "nearest" }));
  }

  /** A press on a result. */
  function goResult(i: number) {
    const state = st;
    const row = state?.rows[i];
    if (state === null || row === undefined) return;
    state.cur = i;
    remember(state.source);
    void go(row.line);
  }

  /** Types a query into the field, as a press on an offered one does. */
  function ask(source: string) {
    const state = st;
    if (state === null) return;
    mode = "results";
    state.queryTyped(source, text.length);
    if (field !== null) field.value = source;
    field?.focus();
  }

  async function copyResults(what: CopyWhat | "xpath") {
    const state = st;
    copyOpen = false;
    if (state === null) return;
    remember(state.source);
    const words = { written: "the nodes as written", values: "the values", lines: "the lines", xpath: "the XPath" }[what];
    if (what === "xpath") return copied(state.source, words);
    copied(await state.session.copy(what), words);
  }

  /** A press on a line picks it; a press on a caret, a count or a link is its own, and a drag selects text. */
  function onpress(event: MouseEvent) {
    const target = event.target as Element | null;
    if (target === null || st === null) return;
    const caret = target.closest<HTMLElement>("[data-xv-caret]");
    if (caret !== null) {
      st.toggle(Number(caret.dataset["xvCaret"]), event.altKey);
      return;
    }
    const fold = target.closest<HTMLElement>("[data-xv-fold]");
    if (fold !== null) {
      st.toggle(Number(fold.dataset["xvFold"]), false, false);
      return;
    }
    if (linkPressed(target, event)) return;
    const chip = target.closest<HTMLElement>("[data-xv-value]");
    if (chip !== null) {
      const n = Number(chip.dataset["xvValue"]);
      st.pick(n);
      void lookAt(n);
      return;
    }
    const more = target.closest<HTMLElement>(".tf-xv-more");
    if (more !== null) {
      const n = Number(more.closest<HTMLElement>(".tf-l[data-n]")?.dataset["n"] ?? 0);
      st.pick(n);
      void lookAt(n);
      return;
    }
    if (target.closest("button, a") !== null) return;
    if (!(getSelection()?.isCollapsed ?? true)) return;
    const row = target.closest<HTMLElement>(".tf-l[data-n]");
    if (row !== null) st.pick(Number(row.dataset["n"]));
  }

  /** The keys (section 7, 9): on the picked line, from anywhere in the view outside a field. */
  function keys(event: KeyboardEvent) {
    const state = st;
    if (state === null || event.defaultPrevented || event.metaKey || event.ctrlKey) return;
    const target = event.target as Element | null;
    if (target?.closest("input, textarea, select, [contenteditable]") != null) return;
    const done = () => {
      event.preventDefault();
      event.stopPropagation();
    };
    const key = event.key;
    if (key === "ArrowDown" || key === "ArrowUp") {
      if (event.altKey) return;
      done();
      state.move(key === "ArrowDown" ? 1 : -1, firstInView());
      void tick().then(() => lines?.reveal(state.cursor, false));
      return;
    }
    if (key === "ArrowLeft" || key === "ArrowRight") {
      if (event.altKey) return;
      done();
      if (key === "ArrowLeft") state.left(event.shiftKey);
      else state.right(event.shiftKey);
      void tick().then(() => lines?.reveal(state.cursor, false));
      return;
    }
    if (event.altKey) return;
    if (isKey(DOC_KEYS.web, key)) {
      done();
      web(event.shiftKey);
      return;
    }
    if (isKey(DOC_KEYS.look, key) && state.cursor > 0) {
      done();
      void lookAt(state.cursor);
      return;
    }
    if (isKey(DOC_KEYS.query, key) && !event.shiftKey) {
      done();
      field?.focus();
      field?.select();
      return;
    }
    if (isKey(DOC_KEYS.next, key) || isKey(DOC_KEYS.previous, key)) {
      done();
      step(isKey(DOC_KEYS.next, key) ? 1 : -1);
      return;
    }
    if (isKey(DOC_KEYS.fewer, key)) {
      done();
      state.stepLevel(-1);
      void tick().then(() => (state.cursor > 0 ? lines?.reveal(state.cursor, false) : undefined));
      return;
    }
    if (isKey(DOC_KEYS.more, key)) {
      done();
      state.stepLevel(1);
      void tick().then(() => (state.cursor > 0 ? lines?.reveal(state.cursor, false) : undefined));
      return;
    }
    if (/^[0-9]$/.test(key) && !event.shiftKey) {
      done();
      state.toLevel(Number(key));
      void tick().then(() => (state.cursor > 0 ? lines?.reveal(state.cursor, false) : undefined));
      return;
    }
    if (key === "Enter" && state.cursor > 0) {
      // What the line holds: its address or path, else its value, else its fold.
      const links = (linkCache.get(state.cursor) ?? []).filter((l) => l.kind !== "namespace");
      done();
      if (links.length > 0) follow(links[0]!, false);
      else if (chipOf(state.cursor) !== null) void lookAt(state.cursor);
      else if ((state.folds.byLine[state.cursor] ?? -1) !== -1) state.toggle(state.cursor);
    }
  }

  /** The first line in the lines' view: where `↓` starts with nothing picked. */
  function firstInView(): number {
    const box = code;
    if (box === null) return 1;
    const top = box.getBoundingClientRect().top;
    for (const row of box.querySelectorAll<HTMLElement>(".tf-l[data-n]")) if (row.getBoundingClientRect().bottom > top + 1) return Number(row.dataset["n"]);
    return 1;
  }

  function copied(value: string, what: string) {
    void navigator.clipboard.writeText(value).then(
      () => push({ text: `Copied ${what}.` }),
      () => push({ text: `Could not copy ${what}.`, tone: "err" }),
    );
  }

  /** The count of what a shut fold hides: its number loud, its word quiet. */
  function countOf(state: DocState, n: number): { count: string; word: string } | null {
    const f = state.folds.byLine[n] ?? -1;
    if (f === -1) return null;
    const c = state.folds.count[f]!;
    const unit = UNITS[state.folds.unit[f]!]!;
    return { count: c.toLocaleString("en"), word: c === 1 ? unit[0] : unit[1] };
  }
</script>

{#snippet caret(n: number)}{#if st !== null && (st.folds.byLine[n] ?? -1) !== -1}{@const shut = st.folded.has(n)}<button type="button" class="tf-xv-caret" data-xv-caret={n} aria-expanded={!shut} aria-label={shut ? "Unfold" : "Fold"} tabindex="-1"><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3.5 1.5 7 5 3.5 8.5" /></svg></button>{:else}<span class="tf-xv-cc"></span>{/if}{/snippet}

{#snippet end(n: number)}{@const c = chipOf(n)}{#if c !== null}<span class="tf-xv-end">{#if c.kept !== null}<button type="button" class="tf-xv-chip tf-xv-kept" data-xv-value={n} title="Kept as {c.kept.pipeline.name}. Space looks at it.">{c.kept.pipeline.name}</button>{:else if c.guess !== null}<button type="button" class="tf-xv-chip" data-xv-value={n} title="knobas takes this for {c.guess.why}. Space looks at it.">{#if c.guess.chip.lead !== ""}<span>{c.guess.chip.lead}</span>{/if}<b>{c.guess.chip.bold}</b>{#if c.guess.chip.tail !== ""}<span>{c.guess.chip.tail}</span>{/if}</button>{/if}</span>{/if}{/snippet}

{#snippet tail(n: number, len: number)}{#if len > 0}<button type="button" class="tf-xv-more" title="The line is {len.toLocaleString('en')} characters long.">{(len - CLIP.keep).toLocaleString("en")} more</button>{/if}{#if st !== null && st.shut(n)}{@const said = countOf(st, n)}{@const f = st.folds.byLine[n]!}<button type="button" class="tf-xv-fold" data-xv-fold={n} title="{(st.folds.end[f]! - n).toLocaleString('en')} lines folded. Press to unfold.">{#if said !== null}<b class="tf-xv-occ">{said.count}</b>{" "}<span>{said.word}</span>{/if}{#if st.hitLines.length > 0}{@const inside = countInside(st.folds, f, st.hitLines)}{#if inside > 0}<span class="tf-xv-in tf-xv-inhit">{inside.toLocaleString("en")} {inside === 1 ? "match" : "matches"}</span>{/if}{/if}</button><span class="tf-xv-shut">{st.folds.closers[st.folds.closer[f]!]}</span>{/if}{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tf-oas tf-xv tf-oas-{cols >= 3 ? 3 : 2}" data-cols={cols} onkeydown={keys} {@attach pane} {@attach sideWidth("doc-rail")}>
  {#if st !== null}
    {@const state = st}
    <TfDocRail st={state} bind:mode ongo={(n) => void go(n)}>
      {#snippet results()}<TfDocResults st={state} ongo={goResult} onask={ask} bind:list={resultList} />{/snippet}
      {#snippet copy()}
        {#if state.q.state === "nodes" && state.q.count > 0}
          <button type="button" class="tf-btn tf-sm tf-ghost" bind:this={copyButton} aria-haspopup="menu" aria-expanded={copyOpen} onclick={() => (copyOpen = !copyOpen)}>Copy ▾</button>
        {/if}
      {/snippet}
    </TfDocRail>
    <TfSideEdge side="doc-rail" />
    <div class="tf-oas-col tf-xv-doc">
      <div class="tf-xv-bar">
        <span class="tf-xv-crumbs">
          {#if state.crumbs.length === 0}
            <span class="tf-dim">Pick a line: the way down to it stands here.</span>
          {:else}
            {@const all = state.crumbs}
            {@const shown = all.length > 6 ? [all[0]!, null, ...all.slice(-4)] : all}
            {#each shown as crumb, i (i)}
              {#if i > 0}<span class="tf-sep" aria-hidden="true">›</span>{/if}
              {#if crumb === null}<span class="tf-dim">…</span>{:else}<button type="button" title="Line {crumb.line.toLocaleString('en')}" onclick={() => void go(crumb.line)}>{crumb.name}</button>{/if}
            {/each}
          {/if}
        </span>
        {#if layout}
          <span class="tf-xv-laid">Laid out by knobas</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" title="Draw the file as it is written, on its own lines" onclick={() => (asWritten = true)}>As written</button>
        {:else if asWritten}
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (asWritten = false)}>Lay it out</button>
        {/if}
        {#if state.before !== null}
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => state.foldAsBefore()}>Fold as before ({state.before.length})</button>
        {/if}
        <span class="tf-xv-lv">
          <span>Levels</span>
          <span class="tf-seg tf-xv-step" role="group" aria-label="Levels shown">
            <button type="button" aria-label="Show one level fewer" title="Fold the deepest level that is open (-)" disabled={state.depth === 0 || state.levels <= 1} onclick={() => state.stepLevel(-1)}>−</button>
            <button
              type="button"
              class="tf-xv-stepn"
              aria-label="{state.levels} of {state.depth} levels shown"
              title={state.levels >= state.depth ? `All ${state.depth} levels are shown` : `${state.levels} of ${state.depth} levels are shown. Press to unfold everything (0)`}
              onclick={() => state.toLevel(0)}>{state.depth === 0 ? "–" : state.levels}</button
            >
            <button type="button" aria-label="Show one level more" title="Unfold one more level (+)" disabled={state.levels >= state.depth} onclick={() => state.stepLevel(1)}>+</button>
          </span>
        </span>
      </div>
      <TfDocQuery
        st={state}
        size={text.length}
        bind:field
        onstep={step}
        onlist={() => {
          mode = "results";
          void tick().then(() => resultList?.querySelector<HTMLElement>(".tf-oas-row")?.focus());
        }}
        onleave={() => code?.focus()}
      />
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      {#if shownText === null}
        <div class="tf-xv-lines"><p class="tf-xv-laying" role="status">Laying out…</p></div>
      {:else}
      <div class="tf-xv-lines" onclick={onpress} onauxclick={(event) => event.button === 1 && event.target instanceof Element && linkPressed(event.target, event)} {@attach wide}>
        <TfLines
          bind:this={lines}
          bind:code
          text={shownText}
          path={colourAs}
          {render}
          line={layout ? null : line}
          find="overlay"
          rows={state.visible}
          scrolls
          clip={CLIP}
          rowHeight={ROW}
          label={path}
          {rowClass}
          {caret}
          {tail}
          {end}
          reach={(n) => state.reach(n)}
          onwindow={drawnLines}
          {linksOf}
          rowAttach={rowThing}
        />
      </div>
      {/if}
    </div>
    {#if cols >= 3}
      <TfDocPicked picked={state.picked} oncopy={copied} />
    {/if}
  {/if}
</div>

{#if looking !== null && values !== null}
  {@const ports = values}
  <TfValueViewer
    value={looking.value}
    {path}
    guess={looking.guess}
    kept={looking.kept}
    first={looking.first}
    pipelines={kept.list}
    commands={commandSteps.list}
    run={ports.run}
    save={ports.save}
    keep={(list) => kept.save(list as never)}
    {render}
    onclose={() => {
      looking = null;
      void tick().then(() => code?.focus());
    }}
  />
{/if}

{#if imaging !== null}
  {@const shown = imaging}
  <TfFileViewer
    name={shown.name}
    line={shown.line}
    kind="Image"
    content={shown.content}
    download={() => (shown.bytes !== null && /^https?:/.test(shown.line) ? openWeb(shown.line, { elsewhere: true }) : files?.open(shown.line))}
    downloadWaits={shown.bytes === null ? "Nothing is loaded yet" : null}
    onclose={closeImage}
  />
{/if}

{#if webMenu !== null}
  {@const menu = webMenu}
  <TfMenu bind:open={webMenuOpen} opener={menu.opener} label="Open an address" className="tf-pmenu-m">
    {#each menu.links as l (l.at)}
      <button type="button" role="menuitem" onclick={() => ((webMenuOpen = false), follow(l, menu.elsewhere))}>{l.to}</button>
    {/each}
  </TfMenu>
{/if}

{#if copyOpen && copyButton !== null}
  <TfMenu bind:open={copyOpen} opener={copyButton} label="Copy the results" className="tf-pmenu-m">
    <button type="button" role="menuitem" onclick={() => void copyResults("written")}>Copy as written</button>
    <button type="button" role="menuitem" onclick={() => void copyResults("values")}>Copy the values</button>
    <button type="button" role="menuitem" onclick={() => void copyResults("lines")}>Copy the lines</button>
    <button type="button" role="menuitem" onclick={() => void copyResults("xpath")}>Copy the XPath</button>
  </TfMenu>
{/if}
