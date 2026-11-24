<!--
  An OpenAPI or Swagger spec on the Code page: the API explorer (ux31, `docs/decisions/2026-11-22-an-api-spec-is-
  explored-sent-from-and-opened-in-a-client.md`), replacing ux18's single list. The file's renderer by default,
  *Source* one pick away in *Render as*.

  **It fits the pane**, as a PDF does: a box as tall as the window leaves under the file's header, set once at mount
  and on `resize` (never on scroll), with columns that each scroll on their own: the rail (`TfOasRail`) and the page
  (`TfOasPage`), and from 1,480 px of width the request beside them. So the list and the thing read are on screen
  together, and nothing moves when a column's content changes.

  **Addresses** name a place (`at`): an operation (`get /pets/{petId}`), a schema (`schemas/Pet`) or a tag's fold
  (`tags/pets`). A pick in the rail replaces the place (`onplace(…, "replace")`); a followed link (a schema's
  name, a *Used by* row, a response's *Then*) pushes one, so the app's Back is the explorer's Back. A place this
  version of the file lacks opens the overview and says so.

  The parse runs in the spec worker (`render/openapi-port.ts`); this component, its parts and the worker load with
  the first spec opened, nothing of them at start-up. A version key the sniff found past its head is a guess the
  parse judges: a file with none at its top is drawn as Source with nothing said (section 12).
-->
<script lang="ts" module>
  /** Each explorer's held list for the filter has an id of its own. */
  let heldIds = 0;
</script>

<script lang="ts">
  import "../css/openapi.css";

  import { flushSync, tick, untrack, type Snippet } from "svelte";

  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import TfOasAnswer from "./TfOasAnswer.svelte";
  import TfOasPage from "./TfOasPage.svelte";
  import TfOasRail from "./TfOasRail.svelte";
  import TfOasRequest from "./TfOasRequest.svelte";
  import { apiSent, apiServers, type SavedServer, type SentLine } from "../../ipc/api";
  import { OasState, OVERVIEW, labelOf, pageOf, type Place } from "./oas.svelte";
  import type { Opened, Operation, OpRow, PathRef } from "../../render/openapi";
  import type { RailInput } from "../../render/openapi-view";
  import { FuzzyOrder } from "../filter/ranked.svelte";
  import { SpecSession, specEngine, type ReadFile, type SpecEngine } from "../../render/openapi-port";
  import { sniffSpec } from "../../render/openapi-sniff";
  import { columnsSeen, paneHeight } from "./explorer/layout";
  import { zen } from "../zen.svelte";
  import { clientsFound, specOpener, type SpecOpener } from "./oas-open.svelte";

  /** What the parse answers for a file with no version key at its top (`indexSpec`). */
  const NOT_AT_TOP = "The file has no openapi or swagger version at its top.";

  let {
    text,
    path,
    read = null,
    at = null,
    line = null,
    repo = "",
    opener = $bindable(null),
    fileHref = () => null,
    oncopylink = null,
    onsource = null,
    onplace = null,
    onback = null,
    prose = null,
    engine = null,
  }: {
    text: string;
    /** The spec's path from the repository's top: where its relative refs are read from. */
    path: string;
    /** Another file of the repository at the spec's commit, for a ref into it; `null` where none can be read. */
    read?: ReadFile | null;
    /** The place an address names: an operation (`get /pets/{petId}`), a schema (`schemas/Pet`), a fold (`tags/pets`). */
    at?: string | null;
    /**
     * A line of the file an address names (a search hit's): where no place is named, the explorer opens at the
     * operation or schema that holds it, and the address says that place.
     */
    line?: number | null;
    /** The repository the spec is read from: with its path, what *Sent since knobas started* is kept by. */
    repo?: string;
    /** What the file view's *Open in…* opens in an API client, once the spec is read. */
    opener?: SpecOpener | null;
    /** A file on the Code page, at a place in it. */
    fileHref?: (file: string, place: string | null) => string | null;
    oncopylink?: ((place: string, label: string) => void) | null;
    /** Shows the file as Source. */
    onsource?: (() => void) | null;
    /** Puts the page shown in the address: `replace` for a pick, `push` for a followed link. */
    onplace?: ((place: Place, how: "replace" | "push", label: string) => void) | null;
    /** The app's Back, for *Back to*: where it is not given, the explorer steps back itself. */
    onback?: (() => void) | null;
    /** Draws Markdown. */
    prose?: Snippet<[string]> | null;
    /** Where the spec is parsed: the worker when left out. */
    engine?: SpecEngine | null;
  } = $props();

  // Raw: the answer is replaced whole, never changed in place. A deep proxy made each of a 5,000-operation spec's rows a
  // proxy of its own the first time the filter's list read them, on the open's frame (the ux30 end gate).
  let opened = $state.raw<Opened | null>(null);
  let st = $state<OasState | null>(null);
  /** Shown once a read has taken long enough to be noticed. */
  let slow = $state(false);

  $effect(() => {
    const source = text;
    const root = path;
    const session = new SpecSession(untrack(() => engine) ?? specEngine(), untrack(() => read));
    const state = new OasState(session, root);
    state.fileHref = (file, place) => untrack(() => fileHref)(file, place);
    st = state;
    opened = null;
    pref = null;
    prefOps = null;
    shownAt = undefined;
    let live = true;
    const timer = setTimeout(() => {
      if (live) slow = true;
    }, 200);
    session.open(root, source).then((answer) => {
      if (!live) return;
      clearTimeout(timer);
      slow = false;
      if (answer.ok) state.take(answer.index);
      else if (answer.error === NOT_AT_TOP && sniffSpec(source)?.deep === true) {
        // A version key found past the sniff's head was a guess, and the parse found none at the top: the file is
        // no spec, so it is drawn as Source and nothing is said (section 12).
        untrack(() => onsource)?.();
        return;
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

  // ---- The filter: the launcher's rules over a list the picker holds for the spec (section 6) ----
  /** One thing the filter finds: an operation, or a schema by its name. */
  type Found = { op: OpRow; schema?: undefined } | { op?: undefined; schema: string };
  /** What the filter reads of each: an operation's method, path, summary, tags and operationId; a schema's name. */
  const textOf = (f: Found) => (f.op !== undefined ? `${f.op.method} ${f.op.path} ${f.op.summary} ${f.op.tags.join(" ")} ${f.op.operationId ?? ""}` : f.schema);
  /** Built once the filter has words, not on the open: the rail draws no flat list before then. */
  const found = $derived(index === null ? [] : [...index.ops.map((op): Found => ({ op })), ...index.schemas.map((schema): Found => ({ schema }))]);
  const NONE: Found[] = [];
  let order = $state.raw(FuzzyOrder.held<Found>(`oas:${++heldIds}`));
  $effect(() => {
    // A new spec, a new held list; the old one is dropped.
    void index;
    const mine = FuzzyOrder.held<Found>(`oas:${++heldIds}`);
    order = mine;
    return () => mine.drop();
  });
  $effect(() => {
    const typed = st?.typed ?? "";
    // No words: the ask only drops an answer still coming, so it needs no list.
    order.ask(typed, typed.trim() === "" ? NONE : found, textOf);
  });
  const ranked = $derived.by((): RailInput["ranked"] => {
    const typed = st?.typed ?? "";
    if (typed.trim() === "") return null;
    const kept = order.rows(typed, found, textOf);
    const ops: { op: OpRow; marks: readonly number[] | null }[] = [];
    const schemas: { name: string; marks: readonly number[] | null }[] = [];
    for (const f of kept) {
      const marks = order.marks(f);
      if (f.op !== undefined) {
        // The row draws the path: the marks are moved back past the method and its space.
        const from = f.op.method.length + 1;
        const end = from + f.op.path.length;
        const onPath: number[] = [];
        for (let i = 0; i + 1 < marks.length; i += 2) {
          const a = Math.max(marks[i]!, from);
          const b = Math.min(marks[i + 1]!, end);
          if (b > a) onPath.push(a - from, b - from);
        }
        ops.push({ op: f.op, marks: onPath });
      } else schemas.push({ name: f.schema, marks: marks.length > 0 ? marks : null });
    }
    return { ops, schemas };
  });

  // ---- The pane: its height set once at mount and on resize, its columns by the file view's width ----
  let box = $state<HTMLElement | null>(null);
  let cols = $state<2 | 3 | 4>(2);
  function pane(el: HTMLElement) {
    const fit = () => {
      const scroller = scrollerOf(el);
      const status = document.querySelector<HTMLElement>(".tf-status")?.offsetHeight ?? 0;
      // The box's top with the page's scroller at 0: where it stands now, plus what the page has scrolled.
      const top = el.getBoundingClientRect().top + (scroller?.scrollTop ?? 0);
      const tall = paneHeight(window.innerHeight, top, status);
      el.style.setProperty("--oas-h", `${tall}px`);
      const state = untrack(() => st);
      if (state !== null) state.paneTall = tall;
    };
    fit();
    window.addEventListener("resize", fit);
    // Zen mode moves the pane's top: measured again once the page has laid out without the repository's header.
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

  // ---- Pages: picks, followed links, the way back, and the place the address names ----
  let pageCol = $state<HTMLElement | null>(null);
  /** The address's place the explorer last answered, so its own writes to the address change nothing. */
  let shownAt: string | null | undefined = undefined;
  let pref = $state<PathRef | null>(null);
  let prefOps = $state<{ ok: true; ops: Operation[] } | { ok: false; error: string } | null>(null);

  async function land(place: Place, scroll: number) {
    const state = st;
    if (state === null) return;
    pref = null;
    // A page gone back to at a scroll draws whole, so the scroll it had is there to go back to.
    state.drawWhole = scroll > 0;
    const done = await state.show(place);
    if (done === null || st !== state) {
      state.drawWhole = false;
      return;
    }
    flushSync();
    state.drawWhole = false;
    if (pageCol !== null) pageCol.scrollTop = scroll;
  }

  /** A pick in the rail: it replaces what the page shows and clears the way back. */
  function pick(place: Place) {
    const state = st;
    if (state === null) return;
    state.backs = [];
    shownAt = place === OVERVIEW ? null : place;
    void land(place, 0);
    onplace?.(place, "replace", labelOf(place));
  }

  /** A followed link: the page left is a step back. */
  function go(place: Place, label: string) {
    const state = st;
    if (state === null) return;
    if (pref === null && pageOf(state.current) === pageOf(place)) return;
    state.backs = [...state.backs, { place: state.current, label: labelOf(state.current), scroll: pageCol?.scrollTop ?? 0 }];
    shownAt = place === OVERVIEW ? null : place;
    void land(place, 0);
    onplace?.(place, "push", label);
  }

  const follow = (name: string) => go(`schemas/${name}`, name);

  function back() {
    const state = st;
    if (state === null || state.backs.length === 0) return;
    if (onback !== null) {
      onback();
      return;
    }
    stepBack();
  }

  /** Back to the last page a link left, at the scroll it had. */
  function stepBack() {
    const state = st;
    const to = state?.backs.at(-1);
    if (state === null || to === undefined) return;
    state.backs = state.backs.slice(0, -1);
    shownAt = to.place === OVERVIEW ? null : to.place;
    void land(to.place, to.scroll);
  }

  // The address's place: shown when it differs from what the explorer drew, and a step back when it is the last left.
  $effect(() => {
    const place = at;
    const hit = line;
    const state = st;
    if (state === null || index === null) return;
    untrack(() => {
      // A line and no place (an Everything hit): the place that holds the line, once, put in the address.
      if (place === null && hit !== null && shownAt === undefined) {
        shownAt = null;
        void state.session.placeAt(hit).then((found) => {
          if (st !== state || found === null || shownAt !== null) return;
          shownAt = found;
          void land(found, 0);
          onplace?.(found, "replace", labelOf(found));
        });
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

  // ---- Open in an API client (sections 23 to 27) ----
  $effect(() => {
    const state = st;
    const idx = index;
    if (state === null || idx === null) {
      opener = null;
      return;
    }
    clientsFound.look();
    opener = specOpener({
      title: idx.title,
      text: untrack(() => text),
      path,
      key: `${repo}\n${path}`,
      requests: idx.ops.length,
      folders: state.groups.length,
      collection: (specFile) => state.session.collection(specFile),
    });
    return () => (opener = null);
  });

  // ---- The request and the answer (sections 14 to 21) ----
  const fileKey = $derived(`${repo}\n${path}`);
  let saved = $state<SavedServer[]>([]);
  let sent = $state<SentLine[]>([]);
  /** Under 1,480 px an operation's page shows its reference or its request, picked at its head. */
  let view = $state<"reference" | "request">("reference");
  let sendKey = $state<(() => void) | null>(null);
  $effect(() => {
    void apiServers().then((list) => (saved = list.servers), () => (saved = []));
  });
  function readSent() {
    const at = fileKey;
    void apiSent(at).then((lines) => {
      if (at === fileKey) sent = lines;
    }, () => {});
  }
  $effect(() => {
    void fileKey;
    untrack(readSent);
  });
  /** The operation shown, where it can be sent: not a webhook, not a path item kept elsewhere. */
  const shownOp = $derived(st !== null && pref === null && st.shown.answer?.kind === "op" && !st.shown.answer.op.webhook ? st.shown.answer.op : null);
  /** Each media type's starting body: the spec's example, else the one made from the schema. */
  const bodies = $derived.by((): Record<string, unknown> => {
    const op = shownOp;
    const state = st;
    if (op === null || state === null) return {};
    const out: Record<string, unknown> = {};
    for (const m of op.body?.content ?? []) out[m.type] = m.examples[0]?.value ?? state.samples.get(`${state.shown.place}/b:${m.type}`);
    return out;
  });
  function useExample(type: string, value: unknown) {
    const state = st;
    const op = shownOp;
    if (state === null || op === null) return;
    const place = state.shown.place;
    const was = state.drafts.get(place);
    if (was !== undefined) state.drafts.set(place, { ...was, media: type, body: typeof value === "string" ? value : JSON.stringify(value, null, 2) });
    else state.drafts.delete(place);
    if (cols === 2) view = "request";
  }
  function openSent(line: SentLine) {
    const state = st;
    if (state === null) return;
    state.answers.set(line.send.operation, { state: "done", id: line.id, line: `${line.send.method} ${line.send.url}`, answer: line.answer });
    pick(line.send.operation);
    if (cols === 2) view = "request";
  }
  function keys(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && sendKey !== null && shownOp !== null) {
      event.preventDefault();
      sendKey();
    }
  }

  async function showPref(ref: PathRef) {
    const state = st;
    if (state === null) return;
    pref = ref;
    prefOps = null;
    const answer = await state.session.pathItem(ref);
    if (st !== state || pref !== ref) return;
    prefOps = answer;
  }

  /** A group on the overview: its fold opens in the rail and the keys go there. */
  async function openGroup(key: string) {
    const state = st;
    if (state === null) return;
    if (state.mode !== "tags") state.mode = "tags";
    state.typed = "";
    state.fold(key, true);
    state.cursor = key;
    await tick();
    const list = box?.querySelector<HTMLElement>(".tf-oas-list");
    list?.focus();
    list?.querySelector<HTMLElement>(`[data-k="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "start" });
  }
</script>

{#if opened === null}
  <div class="tf-oas-wait">{#if slow}<p class="tf-oas-reading" role="status">Reading the spec…</p>{/if}</div>
{:else if !opened.ok}
  <div class="tf-oas-bad" role="alert">
    <p><b>This spec could not be read.</b></p>
    <pre>{opened.error}</pre>
    {#if opened.line !== null}<p class="tf-dim">Line {opened.line}.</p>{/if}
    {#if onsource !== null}<p><button type="button" class="tf-btn tf-sm" onclick={() => onsource?.()}>Show source</button></p>{/if}
  </div>
{:else if index !== null && st !== null}
  {@const state = st}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="tf-oas tf-oas-{cols}" data-cols={cols} bind:this={box} onkeydown={keys} {@attach pane} {@attach sideWidth("oas-rail")}>
    <TfOasRail st={state} {ranked} onpick={pick} onpref={(ref) => void showPref(ref)} />
    <TfSideEdge side="oas-rail" />
    <TfOasPage
      st={state}
      {pref}
      {prefOps}
      {prose}
      onfollow={follow}
      ongo={go}
      onback={back}
      onopengroup={(key) => void openGroup(key)}
      {oncopylink}
      onuse={shownOp === null ? null : useExample}
      head={cols === 2 && shownOp !== null ? pageSeg : null}
      reference={!(cols === 2 && shownOp !== null && view === "request")}
      bind:scroller={pageCol}
    />
    {#if cols >= 3}
      {#if shownOp !== null}
        {#key state.shown.place}
          <div class="tf-oas-col tf-oas-reqcol">
            <TfOasRequest st={state} op={shownOp} place={state.shown.place} {index} file={fileKey} {saved} {bodies} onsent={readSent} bind:sendKey />
            {#if cols === 3}<TfOasAnswer sending={state.answers.get(state.shown.place)} {sent} onopen={openSent} />{/if}
          </div>
        {/key}
      {:else}
        <div class="tf-oas-col tf-oas-req"><p class="tf-oas-fnote">Pick an endpoint, and its request stands here.</p>{#if cols === 3 && sent.length > 0}<TfOasAnswer sending={undefined} {sent} onopen={openSent} />{/if}</div>
      {/if}
      {#if cols === 4}
        <div class="tf-oas-col tf-oas-anscol"><TfOasAnswer sending={shownOp === null ? undefined : state.answers.get(state.shown.place)} {sent} onopen={openSent} /></div>
      {/if}
    {/if}
  </div>
{/if}

{#snippet pageSeg()}
  <div class="tf-oas-pageseg">
    <span class="tf-seg" role="group" aria-label="Show">
      <button type="button" aria-pressed={view === "reference"} onclick={() => (view = "reference")}>Reference</button>
      <button type="button" aria-pressed={view === "request"} onclick={() => (view = "request")}>Request</button>
    </span>
  </div>
  {#if view === "request" && shownOp !== null && st !== null}
    {#key st.shown.place}
      <TfOasRequest st={st} op={shownOp} place={st.shown.place} index={index!} file={fileKey} {saved} {bodies} onsent={readSent} bind:sendKey />
      <TfOasAnswer sending={st.answers.get(st.shown.place)} {sent} onopen={openSent} />
    {/key}
  {/if}
{/snippet}
