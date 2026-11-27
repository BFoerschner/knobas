<!--
  The schema explorer's rail (the XSD design, sections 7 and 8): the file's name and version, the filter, *Tree* /
  *Elements* / *Types*, the count holding its room, then one windowed list of 28 px rows, as the API explorer's.

  - **Tree**: a document's outline from its roots, a row an element where it may stand with *how often*; a fold reads
    its children from the worker the first time. A row already on the way down says *holds itself* and does not open.
  - **Elements** and **Types**: a fold a file, the opened file's open, each declaration in the file's order with its
    type or kind at the row's end.
  - With words typed it is one flat list across the set, best first (the launcher's rules), and clearing the words
    gives back the folds and the scroll.
  - Keys: `↑` `↓` `PageUp` `PageDown` `Home` `End` move, `→` opens a fold, `←` shuts it or goes to its head, `Enter`
    or `Space` opens the row. In the filter: `↓` to the first row, `Enter` opens it, `Esc` clears.
-->
<script lang="ts">
  import { flushSync, tick, untrack } from "svelte";

  import { Windowed } from "../../../shell/windowed.svelte";
  import { filterKeys } from "../../filter-keys";
  import Marked from "../../filter/Marked.svelte";
  import type { DeclRow } from "../../../render/xsd-answer";
  import { KIND_WORD, railRows, type RailMode, type RailRow } from "../../../render/xsd-view";
  import TfXsdOcc from "./TfXsdOcc.svelte";
  import type { XsdState } from "./xsd.svelte";
  import { isAltClick } from "../../tree-alt";

  let {
    st,
    ranked,
    onpick,
  }: {
    st: XsdState;
    ranked: readonly { decl: DeclRow; marks: readonly number[] | null }[] | null;
    /** Shows a place (a declaration's key, or `""` the overview): a pick replaces what the page shows. */
    onpick: (key: string, label: string) => void;
  } = $props();

  const index = $derived(st.index!);
  const rows = $derived(railRows(index, { mode: st.mode, open: st.folds, tree: st.tree, ranked }));
  const total = $derived(st.mode === "types" ? index.decls.filter((d) => d.kind !== "el").length : index.decls.filter((d) => d.kind === "el").length);
  const noun = $derived(st.mode === "types" ? (["type", "types"] as const) : (["element", "elements"] as const));
  const title = $derived(index.entry.slice(index.entry.lastIndexOf("/") + 1).replace(/\.xsd$/i, ""));

  // Every row is 28 px, so the rail windows from 40 rows: a fold of 440 opened draws what the rail shows.
  const win = new Windowed<string>({ count: () => rows.length, key: (i) => rows[i]!.k, estimate: 28, scrolls: true, from: 40 });

  let list = $state<HTMLElement | null>(null);
  let filter = $state<HTMLInputElement | null>(null);

  const shownKey = $derived(st.page?.key ?? null);
  function isCurrent(row: RailRow): boolean {
    if (row.kind === "overview") return st.page === null;
    if (row.kind === "decl") return row.decl.key === shownKey;
    if (row.kind === "tree") return row.row.name.key !== null && row.row.name.key === shownKey && (row.k === st.cursor || st.cursor === null);
    return false;
  }
  const cursorAt = $derived.by(() => {
    const at = st.cursor === null ? -1 : rows.findIndex((r) => r.k === st.cursor);
    if (at >= 0) return at;
    const here = rows.findIndex(isCurrent);
    return here >= 0 ? here : 0;
  });

  const labelOf = (d: DeclRow) => `${d.prefix}${d.local}`;
  function press(row: RailRow) {
    st.cursor = row.k;
    if (row.kind === "overview") onpick("", "the overview");
    else if (row.kind === "file") st.foldFile(row.path, index.files.find((f) => f.path === row.path)?.depth ?? 1);
    else if (row.kind === "decl") onpick(row.decl.key, labelOf(row.decl));
    else if (row.row.name.key !== null) onpick(row.row.name.key, `${row.row.name.prefix}${row.row.name.local}`);
    else if (row.row.kids) void st.fold(row.k);
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

  // A page the rail did not pick (a followed name, an address): in a list its file's fold opens and the row is shown.
  $effect(() => {
    const key = shownKey;
    untrack(() => {
      if (key === null || st.typed !== "") return;
      const here = rows.findIndex((r) => (r.kind === "decl" && r.decl.key === key) || (r.kind === "tree" && r.k === st.cursor && r.row.name.key === key));
      if (here >= 0) {
        if (rows[here]!.k !== st.cursor) void moveTo(here);
        return;
      }
      if (st.mode === "tree") {
        void st.reveal(key).then((id) => {
          if (id === null || st.page?.key !== key) return;
          st.cursor = id;
          void tick().then(() => {
            const i = rows.findIndex((r) => r.k === id);
            if (i >= 0) void moveTo(i);
          });
        });
        return;
      }
      const d = index.decls.find((x) => x.key === key);
      if (d === undefined) return;
      const f = index.files.find((x) => x.path === d.file);
      st.foldFile(d.file, f?.depth ?? 1, true);
      void tick().then(() => {
        const i = rows.findIndex((r) => r.kind === "decl" && r.decl.key === key);
        if (i >= 0) void moveTo(i);
      });
    });
  });

  const isFold = (row: RailRow) => row.kind === "file" || (row.kind === "tree" && row.row.kids);
  const isOpen = (row: RailRow) => (row.kind === "file" || row.kind === "tree" ? row.open : false);
  function fold(row: RailRow, open: boolean) {
    if (row.kind === "file") st.foldFile(row.path, index.files.find((f) => f.path === row.path)?.depth ?? 1, open);
    else if (row.kind === "tree") void st.fold(row.k, open);
  }

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
      if (row !== undefined && isFold(row) && !isOpen(row)) fold(row, true);
    } else if (event.key === "ArrowLeft") {
      if (row !== undefined && isFold(row) && isOpen(row)) fold(row, false);
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

  function typedNow(value: string) {
    const was = st.typed;
    if (was === "" && value !== "") st.beforeFilter = { folds: [...st.folds], scroll: list?.scrollTop ?? 0 };
    st.typed = value;
    if (was !== "" && value === "" && st.beforeFilter !== null) {
      const back = st.beforeFilter;
      st.beforeFilter = null;
      st.folds.clear();
      for (const f of back.folds) st.folds.add(f);
      flushSync();
      if (list !== null) list.scrollTop = back.scroll;
    } else if (value !== "" && list !== null) list.scrollTop = 0;
  }

  function setMode(mode: RailMode) {
    if (st.mode === mode) return;
    st.mode = mode;
    st.cursor = null;
    if (list !== null) list.scrollTop = 0;
  }
</script>

<aside class="tf-oas-rail" aria-label="Declarations">
  <div class="tf-oas-railhead">
    <div class="tf-oas-api"><b title={index.entry}>{title}</b>{#if index.version !== null}<span class="tf-lab">{index.version}</span>{/if}</div>
    <label class="tf-minif">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
      <input
        bind:this={filter}
        placeholder={st.mode === "types" ? "Filter types" : "Filter elements and types"}
        aria-label="Filter elements and types"
        value={st.typed}
        oninput={(e) => typedNow((e.currentTarget as HTMLInputElement).value)}
        onkeydown={filterKey}
        {@attach filterKeys()}
      />
    </label>
    <div class="tf-oas-modes">
      <span class="tf-seg" role="group" aria-label="List by">
        <button type="button" aria-pressed={st.mode === "tree"} onclick={() => setMode("tree")}>Tree</button>
        <button type="button" aria-pressed={st.mode === "elements"} onclick={() => setMode("elements")}>Elements</button>
        <button type="button" aria-pressed={st.mode === "types"} onclick={() => setMode("types")}>Types</button>
      </span>
      <span class="tf-oas-count" aria-live="polite">{ranked === null ? `${total.toLocaleString("en")} ${total === 1 ? noun[0] : noun[1]}` : `${ranked.length.toLocaleString("en")} of ${index.decls.length.toLocaleString("en")}`}</span>
    </div>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="tf-oas-list" bind:this={list} tabindex="0" role="listbox" aria-label="Declarations" onkeydown={listKey} {@attach win.list}>
    {#if ranked !== null && rows.length === 0}<p class="tf-oas-none">Nothing matches the filter.</p>{/if}
    {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
    {#each win.slice(rows) as row, n (row.k)}
      {@const at = win.start + n}
      {@const cursor = at === cursorAt}
      {#if row.kind === "overview"}
        <button type="button" tabindex="-1" class="tf-oas-row tf-oas-plain" class:tf-oas-cursor={cursor} data-k={row.k} aria-current={isCurrent(row)} onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-oas-gname">Overview</span>
        </button>
      {:else if row.kind === "tree"}
        <button
          type="button"
          tabindex="-1"
          class="tf-oas-row tf-xsd-t"
          class:tf-oas-cursor={cursor}
          data-k={row.k}
          data-d={Math.min(row.d, 12)}
          aria-current={isCurrent(row)}
          aria-expanded={row.row.kids ? row.open : undefined}
          title={row.row.again ? "It holds itself: it does not open again" : undefined}
          onclick={(e) => {
            if (isAltClick(e) && row.row.kids) {
              e.preventDefault();
              st.cursor = row.k;
              void st.foldAll(row.k);
            } else if ((e.target as Element).closest(".tf-oas-caret") !== null && row.row.kids) {
              st.cursor = row.k;
              void st.fold(row.k);
            } else press(row);
          }}
          {@attach win.row(row.k)}
        >
          {#if row.row.kids}<svg class="tf-oas-caret" viewBox="0 0 10 10" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>{:else}<span class="tf-xsd-leaf"></span>{/if}
          <span class="tf-xsd-name" class:tf-xsd-lostname={row.row.lost}>{#if row.row.name.prefix !== ""}<span class="tf-xsd-pre">{row.row.name.prefix}</span>{/if}{row.row.name.local}</span>
          {#if row.row.root}<span class="tf-xsd-kind">root</span>{:else if row.row.again}<span class="tf-xsd-kind">holds itself</span>{:else if row.row.occ !== null}<TfXsdOcc occ={row.row.occ} />{/if}
        </button>
      {:else if row.kind === "file"}
        <button type="button" tabindex="-1" class="tf-oas-row tf-oas-g" class:tf-oas-cursor={cursor} data-k={row.k} aria-expanded={row.open} title={row.path} onclick={() => press(row)} {@attach win.row(row.k)}>
          <svg class="tf-oas-caret" viewBox="0 0 10 10" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>
          <span class="tf-xsd-file">{row.path.slice(row.path.lastIndexOf("/") + 1)}</span>
          <span class="tf-oas-n">{row.reading ? "reading…" : row.count.toLocaleString("en")}</span>
        </button>
      {:else}
        {@const d = row.decl}
        <button type="button" tabindex="-1" class="tf-oas-row tf-xsd-n" class:tf-oas-cursor={cursor} data-k={row.k} data-d={row.d} aria-current={isCurrent(row)} title="{labelOf(d)}, {KIND_WORD[d.kind]}" onclick={() => press(row)} {@attach win.row(row.k)}>
          <span class="tf-xsd-name">{#if ranked !== null}{#if row.marks !== null}<Marked text={labelOf(d)} pairs={row.marks} />{:else}<Marked text={labelOf(d)} typed={st.typed} />{/if}{:else}{#if d.prefix !== ""}<span class="tf-xsd-pre">{d.prefix}</span>{/if}{d.local}{/if}</span>
          <span class="tf-xsd-kind">{ranked !== null ? KIND_WORD[d.kind] : d.type}</span>
        </button>
      {/if}
    {/each}
    {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
  </div>
  <div class="tf-oas-keys" aria-hidden="true">
    <span><kbd>⌘F</kbd> filter</span><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>←</kbd><kbd>→</kbd> fold</span><span><kbd>↵</kbd> open</span>
  </div>
</aside>
