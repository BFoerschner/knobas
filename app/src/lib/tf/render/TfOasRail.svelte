<!--
  The API explorer's rail (ux31, sections 5 and 6): the API's title and version, the filter, *Tags* / *Paths* /
  *Schemas*, the count holding its room, then one windowed list of 28 px rows that scrolls on its own.

  - A row is the method (the view's one loud thing, in a 34 px column) and the path in the code face, its
    `{parameters}` dim, from its group's shared start on; a path too long for the rail keeps its first and last whole
    parts and the parts that tell it from the rows near it (`fitRows`), cut by a count of the code face's characters
    measured once. The row's `title` is the whole.
  - The selected row is the rail's ink pill; the row the keys stand on has an inset `--focus` line.
  - With words typed the rail is one flat list, best first (`ranked`), and clearing them returns the folds and the
    scroll as they were.
  - Keys in the list: `↑` `↓`, `PageUp` `PageDown` `Home` `End`, `→` opens a fold, `←` shuts it or goes to its head,
    `Enter` or `Space` opens the row. In the filter: `↓` goes to the first row, `Enter` opens it, `Esc` clears.
-->
<script lang="ts">
  import { flushSync, tick, untrack } from "svelte";

  import { Windowed } from "../../shell/windowed.svelte";
  import { filterKeys } from "../filter-keys";
  import Marked from "../filter/Marked.svelte";
  import type { OpRow, PathRef } from "../../render/openapi";
  import { counted, fitPath, fitRows, pathParts, railRows, shortMethod, type RailInput, type RailMode, type RailRow } from "../../render/openapi-view";
  import { OVERVIEW, pageOf, type OasState, type Place } from "./oas.svelte";

  let {
    st,
    ranked,
    onpick,
    onpref,
  }: {
    st: OasState;
    /** The rows the filter keeps, best first; `null` with no words typed. */
    ranked: RailInput["ranked"];
    /** Shows a place: a pick replaces what the page shows. */
    onpick: (place: Place) => void;
    /** Shows a path item kept in another file. */
    onpref: (ref: PathRef) => void;
  } = $props();

  const index = $derived(st.index!);
  const rows = $derived(railRows(index, { mode: st.mode, groups: st.groups, tree: st.mode === "paths" ? st.tree : null, open: st.folds, ranked }));
  const total = $derived(st.mode === "schemas" ? index.schemas.length : index.ops.length + index.pathRefs.length);
  const kept = $derived(ranked === null ? total : st.mode === "schemas" ? ranked.schemas.length : ranked.ops.length);
  const noun = $derived(st.mode === "schemas" ? (["schema", "schemas"] as const) : (["endpoint", "endpoints"] as const));

  // Every row is 28 px, so the rail windows from 40 rows: a fold of 200 opened draws what the rail shows, not 200.
  const win = new Windowed<string>({ count: () => rows.length, key: (i) => rows[i]!.k, estimate: 28, scrolls: true, from: 40 });

  let list = $state<HTMLElement | null>(null);
  let filter = $state<HTMLInputElement | null>(null);

  // ---- The code face's width, measured once, and the rail's, read on resize only ----
  let charW = 0;
  let railW = $state(300);
  function measure(el: HTMLElement) {
    const probe = document.createElement("span");
    probe.className = "tf-oas-rpath";
    probe.textContent = "0123456789".repeat(4);
    probe.style.setProperty("position", "absolute");
    probe.style.setProperty("visibility", "hidden");
    el.appendChild(probe);
    charW = probe.getBoundingClientRect().width / 40 || 7.8;
    probe.remove();
    railW = el.clientWidth || 300;
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver((entries) => {
      const w = Math.round(entries[0]?.contentRect.width ?? 0);
      if (w > 0 && w !== railW) railW = w;
    });
    ro.observe(el);
    return () => ro.disconnect();
  }
  /** How many characters of the code face a row at depth `d` holds: the rail less its pads, the method and the gap. */
  const charsAt = (d: number) => (charW <= 0 ? 34 : Math.max(12, Math.floor((railW - 12 - 16 - 34 - 8 - d * 14) / charW)));

  // ---- Which row is current, and where the keys stand ----
  const page = $derived(pageOf(st.current));
  function isCurrent(row: RailRow): boolean {
    if (row.kind === "overview") return page === OVERVIEW;
    if (row.kind === "op") return row.op.anchor === page;
    if (row.kind === "schema") return page === `schemas/${row.name}`;
    return false;
  }
  const cursorAt = $derived.by(() => {
    const at = st.cursor === null ? -1 : rows.findIndex((r) => r.k === st.cursor);
    if (at >= 0) return at;
    const here = rows.findIndex(isCurrent);
    return here >= 0 ? here : 0;
  });

  function press(row: RailRow) {
    st.cursor = row.k;
    if (row.kind === "group") st.fold(row.group.key);
    else if (row.kind === "dir") st.fold(row.k);
    else if (row.kind === "webhooks") st.fold("w");
    else if (row.kind === "overview") onpick(OVERVIEW);
    else if (row.kind === "op") onpick(row.op.anchor);
    else if (row.kind === "schema") onpick(`schemas/${row.name}`);
    else if (row.kind === "pref") onpref(row.ref);
  }

  async function moveTo(at: number) {
    if (rows.length === 0) return;
    const i = Math.max(0, Math.min(rows.length - 1, at));
    st.cursor = rows[i]!.k;
    win.reveal(i);
    flushSync();
    await tick();
    const el = list?.querySelector<HTMLElement>(`[data-k="${CSS.escape(rows[i]!.k)}"]`);
    if (el && list) {
      const top = el.offsetTop;
      if (top < list.scrollTop) list.scrollTop = top;
      else if (top + 28 > list.scrollTop + list.clientHeight) list.scrollTop = top + 28 - list.clientHeight;
    }
  }

  // A place the rail did not pick (an address, a followed link): its row is brought into view and the keys stand on it.
  $effect(() => {
    const at = st.current;
    untrack(() => {
      const i = rows.findIndex((r) => r.kind === "op" && r.op.anchor === at);
      if (i >= 0 && rows[i]!.k !== st.cursor) void moveTo(i);
    });
  });

  const isFold = (row: RailRow) => row.kind === "group" || row.kind === "dir" || row.kind === "webhooks";
  const foldKey = (row: RailRow) => (row.kind === "group" ? row.group.key : row.kind === "webhooks" ? "w" : row.k);
  const isOpen = (row: RailRow) => (row.kind === "group" || row.kind === "dir" || row.kind === "webhooks" ? row.open : false);

  function listKey(event: KeyboardEvent) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const row = rows[cursorAt];
    const page = Math.max(1, Math.floor((list?.clientHeight ?? 280) / 28) - 1);
    let done = true;
    if (event.key === "ArrowDown") void moveTo(cursorAt + 1);
    else if (event.key === "ArrowUp") {
      if (cursorAt === 0 && filter !== null) filter.focus();
      else void moveTo(cursorAt - 1);
    } else if (event.key === "PageDown") void moveTo(cursorAt + page);
    else if (event.key === "PageUp") void moveTo(cursorAt - page);
    else if (event.key === "Home") void moveTo(0);
    else if (event.key === "End") void moveTo(rows.length - 1);
    else if (event.key === "ArrowRight") {
      if (row !== undefined && isFold(row) && !isOpen(row)) st.fold(foldKey(row), true);
    } else if (event.key === "ArrowLeft") {
      if (row !== undefined && isFold(row) && isOpen(row)) st.fold(foldKey(row), false);
      else if (row !== undefined && row.d > 0) {
        for (let i = cursorAt - 1; i >= 0; i--) {
          if (rows[i]!.d < row.d) {
            void moveTo(i);
            break;
          }
        }
      }
    } else if (event.key === "Enter" || event.key === " ") {
      if (row !== undefined) press(row);
    } else done = false;
    if (done) event.preventDefault();
  }

  function filterKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" && rows.length > 0) {
      event.preventDefault();
      st.cursor = rows[0]!.k;
      list?.focus();
      void moveTo(0);
    } else if (event.key === "Enter" && rows.length > 0) {
      event.preventDefault();
      press(rows[0]!);
    }
  }

  // ---- The filter: its first word keeps the folds and the scroll, clearing it gives them back ----
  function typedNow(value: string) {
    const was = st.typed;
    if (was === "" && value !== "") st.beforeFilter = { folds: st.folds, scroll: list?.scrollTop ?? 0 };
    st.typed = value;
    if (was !== "" && value === "" && st.beforeFilter !== null) {
      const back = st.beforeFilter;
      st.beforeFilter = null;
      st.folds = back.folds;
      flushSync();
      if (list !== null) list.scrollTop = back.scroll;
    } else if (value !== "" && list !== null) list.scrollTop = 0;
  }

  function setMode(mode: RailMode) {
    if (st.mode === mode) return;
    if (mode === "paths") st.showPaths();
    else st.mode = mode;
    st.cursor = null;
    if (list !== null) list.scrollTop = 0;
  }

  /** Each row's path cut to the rail, told apart from the rows near it (`fitRows`): once a list or a width, not a row. */
  const fits = $derived(fitRows(rows, charsAt));
  /** What a row's path says: from its group's shared start, fitted to the rail; the whole in its title. */
  function shownOf(row: RailRow & { kind: "op" }): string {
    return fits.get(row.k) ?? fitPath(row.shown, charsAt(row.d));
  }
  const titleOf = (op: OpRow) => `${op.method.toUpperCase()} ${op.path}${op.summary !== "" ? `\n${op.summary}` : ""}`;
</script>

<aside class="tf-oas-rail" aria-label="Endpoints">
  <div class="tf-oas-railhead">
    <div class="tf-oas-api"><b title={index.title}>{index.title}</b>{#if index.apiVersion !== ""}<span class="tf-lab">{index.apiVersion}</span>{/if}</div>
    <label class="tf-minif">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
      <input
        bind:this={filter}
        placeholder={st.mode === "schemas" ? "Filter schemas" : "Filter endpoints and schemas"}
        aria-label="Filter endpoints and schemas"
        value={st.typed}
        oninput={(e) => typedNow((e.currentTarget as HTMLInputElement).value)}
        onkeydown={filterKey}
        {@attach filterKeys()}
      />
    </label>
    <div class="tf-oas-modes">
      <span class="tf-seg" role="group" aria-label="List by">
        <button type="button" aria-pressed={st.mode === "tags"} onclick={() => setMode("tags")}>Tags</button>
        <button type="button" aria-pressed={st.mode === "paths"} onclick={() => setMode("paths")}>Paths</button>
        <button type="button" aria-pressed={st.mode === "schemas"} onclick={() => setMode("schemas")}>Schemas</button>
      </span>
      <span class="tf-oas-count" aria-live="polite">{ranked === null ? counted(total, noun[0], noun[1]) : `${kept.toLocaleString("en")} of ${total.toLocaleString("en")}`}</span>
    </div>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="tf-oas-list" bind:this={list} tabindex="0" role="listbox" aria-label="Endpoints" onkeydown={listKey} {@attach measure} {@attach win.list}>
    {#if rows.length === 0}
      <p class="tf-oas-none">{ranked !== null ? "Nothing matches the filter." : st.mode === "schemas" ? "The spec names no schemas." : "The spec lists no endpoints."}</p>
    {/if}
    {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
    {#each win.slice(rows) as row, n (row.k)}
      {@const at = win.start + n}
      {@const cursor = at === cursorAt}
      {#if row.kind === "overview"}
        <button type="button" tabindex="-1" class="tf-oas-row tf-oas-plain" class:tf-oas-cursor={cursor} data-k={row.k} aria-current={isCurrent(row)} onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-oas-gname">Overview</span>
        </button>
      {:else if row.kind === "group" || row.kind === "dir" || row.kind === "webhooks"}
        <button
          type="button"
          tabindex="-1"
          class="tf-oas-row tf-oas-g"
          class:tf-oas-cursor={cursor}
          data-k={row.k}
          data-d={row.d}
          aria-expanded={row.open}
          title={row.kind === "group" ? row.group.description : undefined}
          onclick={() => press(row)}
          {@attach win.row(row.k)}
        >
          <svg class="tf-oas-caret" viewBox="0 0 10 10" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>
          {#if row.kind === "group"}
            <span class="tf-oas-gname" class:tf-oas-code={row.group.code}>{row.group.name}{#if row.group.shared !== ""}<span class="tf-oas-gpre">{row.group.shared}</span>{/if}</span>
            <span class="tf-oas-n">{row.group.ops.length + row.group.refs.length}</span>
          {:else if row.kind === "dir"}
            <span class="tf-oas-gname tf-oas-code">/{row.dir.name}</span>
            <span class="tf-oas-n">{row.dir.count}</span>
          {:else}
            <span class="tf-oas-gname">Webhooks</span>
            <span class="tf-oas-n">{row.n}</span>
          {/if}
        </button>
      {:else if row.kind === "op"}
        {@const shown = shownOf(row)}
        <button type="button" tabindex="-1" class="tf-oas-row" class:tf-oas-cursor={cursor} data-k={row.k} data-d={row.d} aria-current={isCurrent(row)} title={titleOf(row.op)} onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-oas-m">{shortMethod(row.op.method)}</span>
          <span class="tf-oas-rpath" class:tf-oas-dep={row.op.deprecated}
            >{#if ranked !== null && shown === row.op.path}{#if row.marks !== null}<Marked text={shown} pairs={row.marks} />{:else}<Marked text={shown} typed={st.typed} />{/if}{:else}{#each pathParts(shown) as part, m (m)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}{/if}</span
          >
        </button>
      {:else if row.kind === "pref"}
        <button type="button" tabindex="-1" class="tf-oas-row" class:tf-oas-cursor={cursor} data-k={row.k} data-d={row.d} title="{row.ref.path} is kept in {row.ref.ref}" onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-oas-m">…</span>
          <span class="tf-oas-rpath">{fitPath(row.ref.path, charsAt(row.d))}</span>
        </button>
      {:else}
        <button type="button" tabindex="-1" class="tf-oas-row tf-oas-plain" class:tf-oas-cursor={cursor} data-k={row.k} aria-current={isCurrent(row)} onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-oas-sname">{#if ranked !== null}{#if row.marks !== null}<Marked text={row.name} pairs={row.marks} />{:else}<Marked text={row.name} typed={st.typed} />{/if}{:else}{row.name}{/if}</span>
        </button>
      {/if}
    {/each}
    {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
  </div>
  <div class="tf-oas-keys" aria-hidden="true">
    <span><kbd>⌘F</kbd> filter</span><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>←</kbd><kbd>→</kbd> fold</span><span><kbd>↵</kbd> open</span>
  </div>
</aside>
