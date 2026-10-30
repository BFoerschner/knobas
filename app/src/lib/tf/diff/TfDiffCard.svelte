<!--
  One file of the one diff component (`TfDiffView`, #2266): its header and
  its lines, in the layout the view shows. The layout is
  `mockups/code-area/diff.html`.

  - **Header**: a caret that collapses the file, its badge (`tree.ts`), the
    path (`old → new` on a rename), `+N −N`, a *lock file* or *generated*
    chip, *Viewed* where the caller offers it.
  - **Lines, windowed** (#2269): the file's rows are one flat list (lines,
    split pairs, gaps, threads, pending comments, the comment box), each its
    own element, and only the rows near the screen are drawn
    (`shell/windowed.svelte.ts`); nothing is cut. A file over 1,500 changed
    lines draws *Load diff* in their place until asked.
  - **Word-level changes** (#2267): a removed line and the added line beside
    it mark their changed words (`words.ts`).
  - **Expand context** (#2267): where the caller can read the file
    (`expandable`), each gap offers the hidden lines down from the hunk
    above, up from the hunk below, or all of them; expanded fully, the two
    hunks join. The view reads the file once and keeps it (`onexpand`).
  - **Keys** come from the view: `nextHunk`, `comment` and `scrollTo` are
    this card's part of `n`/`p`, `c` and `j`/`k` (#2268).
-->
<script lang="ts">
  import { flushSync } from "svelte";
  import type { DiffLine, LineComment } from "../../ipc/entity";
  import { lineName } from "../../detail/view/pending-comments.svelte";
  import type { LineTokens } from "../../render/port";
  import { Windowed } from "../../shell/windowed.svelte";
  import { ago } from "../../shell/time";
  import { placeThreads, sameTarget, targetOf } from "../pr/files";
  import { changedWords, DIFF_CEILING, EXPAND_STEP as STEP, keyOf, splitRows, unifiedRows, type DrawnFile, type DrawnLine, type Expanded, type Gap } from "./diff";
  import type { DiffTalk } from "./talk";
  import { badgeOf, isGenerated, isLock, LOAD_OVER, needsLoad } from "./tree";
  import { marked, type Range } from "./words";

  let {
    file,
    split,
    closed,
    ontoggle,
    viewed,
    done,
    onviewed,
    loaded,
    onload,
    current,
    expandable,
    expanded,
    onexpand,
    tokensOf,
    talk,
  }: {
    file: DrawnFile;
    split: boolean;
    closed: boolean;
    ontoggle: () => void;
    /** Whether the caller offers *Viewed*, and whether this file is. */
    viewed: boolean;
    done: boolean;
    onviewed: (on: boolean) => void;
    /** A file over {@link LOAD_OVER} changed lines draws its lines only once loaded. */
    loaded: boolean;
    onload: () => void;
    /** The file `j`/`k` is on. */
    current: boolean;
    expandable: boolean;
    expanded: Expanded | undefined;
    onexpand: (hunk: number, how: Expand) => void;
    tokensOf: (side: "old" | "new", drawn: DrawnLine) => LineTokens | undefined;
    talk: DiffTalk;
  } = $props();

  type Expand = "down" | "up" | "all";

  type Item =
    | { kind: "gap"; key: string; gap: Gap }
    | { kind: "line"; key: string; cell: DrawnLine }
    | { kind: "pair"; key: string; old: DrawnLine | null; new: DrawnLine | null }
    | { kind: "thread"; key: string; comments: readonly LineComment[]; onNew: boolean }
    | { kind: "draft"; key: string; body: string }
    | { kind: "box"; key: string; line: DiffLine; onNew: boolean };

  const lineKey = (cell: DrawnLine | null) => (cell === null ? "-" : cell.extra ? `x${cell.line.new}` : `l${cell.at}`);
  const placed = $derived(placeThreads(file));
  const drawsLines = $derived(!closed && file.change !== "binary" && (loaded || !needsLoad(file)));
  const marks = $derived(drawsLines ? changedWords(file) : new Map<number, Range[]>());

  /** Where each hunk starts among the items: its gap, else its first line. */
  let starts: number[] = [];
  const items = $derived.by((): Item[] => {
    if (!drawsLines) return [];
    const out: Item[] = [];
    const firstAt: number[] = [];
    let at = 0;
    for (const hunk of file.hunks) firstAt.push(at), (at += hunk.lines.length);
    const found: number[] = [];
    const note = (hunk: number) => found[hunk] ?? (found[hunk] = out.length);
    const hunkOf = (cell: DrawnLine | null) => (cell && !cell.extra ? firstAt.indexOf(cell.at) : -1);
    /** What follows a line: its threads, the reader's pending comments on it, the open comment box. */
    const after = (cell: DrawnLine, onNew: boolean) => {
      (placed.under[cell.at] ?? []).forEach((thread, n) => {
        if (!cell.extra) out.push({ kind: "thread", key: `t${cell.at}:${n}`, comments: thread.comments, onNew });
      });
      if (cell.extra) return;
      for (const d of talk.draftsAt(file.path, cell.line)) out.push({ kind: "draft", key: `d${d.id}`, body: d.body });
      if (talk.isOpen(file.path, cell.line)) out.push({ kind: "box", key: "box", line: cell.line, onNew });
    };
    if (split) {
      for (const row of splitRows(file, expanded)) {
        if (row.kind === "gap") {
          note(row.hunk);
          out.push({ kind: "gap", key: `g${row.hunk}`, gap: row });
          continue;
        }
        for (const cell of [row.old, row.new]) if (hunkOf(cell) >= 0) note(hunkOf(cell));
        out.push({ kind: "pair", key: `${lineKey(row.old)}|${lineKey(row.new)}`, old: row.old, new: row.new });
        for (const [n, cell] of [row.old, row.new].entries())
          if (cell !== null && (n === 0 || cell !== row.old)) after(cell, targetOf(cell.line).side === "new");
      }
    } else {
      for (const row of unifiedRows(file, expanded)) {
        if (row.kind === "gap") {
          note(row.hunk);
          out.push({ kind: "gap", key: `g${row.hunk}`, gap: row });
          continue;
        }
        if (hunkOf(row) >= 0) note(hunkOf(row));
        out.push({ kind: "line", key: lineKey(row), cell: row });
        after(row, false);
      }
    }
    starts = found;
    return out;
  });

  const win = new Windowed<string>({ count: () => items.length, key: (i) => items[i]!.key, estimate: 21 });
  const nodes = new Map<string, HTMLElement>();
  const keep = (key: string) => (node: HTMLElement) => {
    nodes.set(key, node);
    return () => {
      if (nodes.get(key) === node) nodes.delete(key);
    };
  };

  let root = $state<HTMLElement | null>(null);
  /** The hunk `n`/`p` is on. */
  let hunk = $state(-1);

  /** Draws item `index` and scrolls it into view. */
  function show(index: number) {
    win.reveal(index);
    flushSync();
    nodes.get(items[index]?.key ?? "")?.scrollIntoView?.({ block: "center" });
  }

  export function scrollTo() {
    root?.scrollIntoView?.({ block: "start" });
  }

  /** `n` (`step` 1) and `p` (-1): the next or previous hunk of this file; false when there is none that way. */
  export function nextHunk(step: 1 | -1): boolean {
    const to = hunk + step;
    if (!drawsLines || to < 0 || to >= starts.length) return false;
    hunk = to;
    show(starts[to]!);
    return true;
  }

  /** `c`: a comment on the first changed line of the hunk `n`/`p` is on (the first hunk before either). */
  export function comment(): boolean {
    if (!drawsLines || !talk.review) return false;
    const from = starts[Math.max(0, hunk)] ?? 0;
    const lines = items.slice(from).flatMap((item) => (item.kind === "line" ? [item.cell] : item.kind === "pair" ? [item.new ?? item.old] : []));
    const cell = lines.find((c) => c && !c.extra && c.line.kind !== "context") ?? lines.find((c) => c && !c.extra);
    if (!cell) return false;
    talk.openAt(file.path, cell.line);
    flushSync();
    const box = items.findIndex((item) => item.kind === "box");
    if (box >= 0) show(box);
    root?.querySelector<HTMLTextAreaElement>(".tf-newc textarea")?.focus();
    return true;
  }

  const SIGN = { context: " ", add: "+", del: "−" } as const;
  const old = $derived(file.old_path && file.old_path !== file.path ? file.old_path : null);
  const badge = $derived(badgeOf(file));
  const chip = $derived(isLock(file.path) ? "lock file" : isGenerated(file.path) ? "generated" : null);
  const changed = $derived(file.additions + file.deletions);
  const n = (count: number) => count.toLocaleString("en-US");
  const strays = $derived(
    talk.drafts.filter((d) => d.path === file.path && !file.hunks.some((h) => h.lines.some((line) => talk.draftsAt(file.path, line).includes(d)))),
  );
  /** The buttons of a gap: one when the rest fits in a step, else down, up and all (up only above the first hunk). */
  const expands = (gap: Gap): { how: Expand; label: string; title: string }[] => {
    const left = gap.hidden ?? 0;
    if (!expandable || left <= 0) return [];
    if (left <= STEP) return [{ how: "all", label: `Expand ${left} ${left === 1 ? "line" : "lines"}`, title: `Show the ${left} hidden ${left === 1 ? "line" : "lines"}` }];
    const up = { how: "up" as const, label: `↑ ${STEP} lines`, title: `Show ${STEP} more lines above this hunk` };
    if (gap.hunk === 0) return [up];
    return [{ how: "down", label: `↓ ${STEP} lines`, title: `Show ${STEP} more lines below the hunk above` }, up, { how: "all", label: `Expand all ${n(left)}`, title: `Show all ${n(left)} hidden lines` }];
  };
</script>

{#snippet piece(text: string, kind: string)}{#if kind !== ""}<span class="tf-hl-{kind}">{text}</span>{:else}{text}{/if}{/snippet}

<!-- A line's code: plain until its tokens arrive, its changed words marked. -->
{#snippet code(cell: DrawnLine, side: "old" | "new")}
  {@const toks = tokensOf(side, cell)}
  {@const words = cell.extra ? undefined : marks.get(cell.at)}
  {#if words}{#each marked(cell.line.text, toks, words) as seg, k (k)}{#if seg.mark}<span class="tf-wd">{@render piece(seg.text, seg.kind)}</span
        >{:else}{@render piece(seg.text, seg.kind)}{/if}{/each}{:else if toks}{#each toks as [part, kind], k (k)}{@render piece(part, kind)}{/each}{:else}{cell.line.text}{/if}
{/snippet}

{#snippet said(comments: readonly LineComment[])}
  {#each comments as c, k (k)}
    <p><b>{c.author}</b>{c.at ? ` ${ago(c.at)}` : ""}: {c.body}</p>
  {/each}
{/snippet}

<!-- The `+` on a line, where the source takes comments; an empty cell otherwise. -->
{#snippet plus(cell: DrawnLine)}
  {#if talk.review && !cell.extra}
    {@const at = targetOf(cell.line)}
    {@const name = lineName(at.side, at.line)}
    <button type="button" class="tf-addc" title="Comment on {name}" aria-label="Comment on {name} of {file.path}" onclick={() => talk.openAt(file.path, cell.line)}>+</button>
  {:else}<span></span>{/if}
{/snippet}

<!-- One side of a split row: its number, sign and code, or padding where the side has no line. -->
{#snippet half(cell: DrawnLine | null, side: "old" | "new")}
  {#if cell === null}
    <div class="tf-ln tf-pad"><span></span><span></span><span></span></div>
  {:else}
    {@const line = cell.line}
    <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
      <span class="tf-no">{side === "old" ? line.old : line.new}</span><span>{SIGN[line.kind]} {@render code(cell, side)}</span>
      {#if targetOf(line).side === side}{@render plus(cell)}{:else}<span></span>{/if}
    </div>
  {/if}
{/snippet}

{#snippet item(it: Item)}
  {#if it.kind === "gap"}
    <div class="tf-hunk" {@attach win.row(it.key)} {@attach keep(it.key)}>
      {#each expands(it.gap) as button (button.how)}
        <button type="button" class="tf-exp" title={button.title} onclick={() => onexpand(it.gap.hunk, button.how)}>{button.label}</button>
      {/each}<span>{it.gap.header}</span>
    </div>
  {:else if it.kind === "line"}
    {@const line = it.cell.line}
    <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"} class:tf-extra={it.cell.extra} {@attach win.row(it.key)} {@attach keep(it.key)}>
      <span class="tf-no">{line.old ?? ""}</span><span class="tf-no">{line.new ?? ""}</span><span>{SIGN[line.kind]} {@render code(it.cell, line.kind === "del" ? "old" : "new")}</span>
      {@render plus(it.cell)}
    </div>
  {:else if it.kind === "pair"}
    <div class="tf-srow" {@attach win.row(it.key)} {@attach keep(it.key)}>{@render half(it.old, "old")}{@render half(it.new, "new")}</div>
  {:else if it.kind === "thread"}
    <div class="tf-thread" class:tf-on-new={split && it.onNew} {@attach win.row(it.key)}>{@render said(it.comments)}</div>
  {:else if it.kind === "draft"}
    <div class="tf-thread tf-pending" {@attach win.row(it.key)}><p><b>You</b> <span class="tf-chip tf-warn">pending</span> {it.body}</p></div>
  {:else}
    {@const at = targetOf(it.line)}
    {@const name = lineName(at.side, at.line)}
    <div class="tf-newc" class:tf-on-new={split && it.onNew} {@attach win.row(it.key)} {@attach keep(it.key)}>
      <textarea value={talk.draft} oninput={(event) => (talk.draft = event.currentTarget.value)} placeholder="Comment on {name}" aria-label="Comment on {name} of {file.path}"></textarea>
      <div class="tf-acts">
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => talk.cancel()}>Cancel</button>
        <button type="button" class="tf-btn tf-sm tf-primary" disabled={talk.draft.trim() === ""} onclick={() => talk.add()}>Add to review</button>
      </div>
    </div>
  {/if}
{/snippet}

<div class="tf-diff" class:tf-viewed={done} class:tf-split={split} class:tf-closed={closed} class:tf-current={current} bind:this={root}>
  <div class="tf-fh">
    <button type="button" class="tf-fh-caret" aria-expanded={!closed} aria-label="{closed ? 'Expand' : 'Collapse'} {keyOf(file)}" onclick={ontoggle}
      ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d={closed ? "m6 4 4 4-4 4" : "m4 6 4 4 4-4"} /></svg></button
    >
    <span class="tf-bd tf-bd-{badge[0] === '+' || badge[0] === '−' ? 'X' : badge[0]}" title={file.change}>{badge}</span>
    <span class="tf-mono">{old ? `${old} → ${keyOf(file)}` : keyOf(file)}</span>
    {#if file.change !== "binary"}
      <span class="tf-add-n">+{file.additions}</span>
      <span class="tf-del-n">−{file.deletions}</span>
    {/if}
    {#if chip}<span class="tf-chip">{chip}</span>{/if}
    {#if viewed}
      <label class="tf-fh-viewed"><input type="checkbox" checked={done} onchange={(event) => onviewed(event.currentTarget.checked)} /> Viewed</label>
    {/if}
  </div>
  {#if !closed}
    {#if file.change === "binary"}
      <div class="tf-dwhy">A binary file: no lines to draw.</div>
    {:else if !drawsLines}
      <div class="tf-dwhy">
        <span>{chip === "generated" ? "Generated, and over" : "Over"} {n(LOAD_OVER)} lines ({n(changed)} changed): not loaded until you ask; nothing is cut.</span>
        <button type="button" class="tf-btn tf-sm" onclick={onload}>Load diff</button>
      </div>
    {:else if file.hunks.length === 0 && file.old_mode && file.new_mode && file.old_mode !== file.new_mode}
      <div class="tf-dwhy">File mode changed from <span class="tf-mono">{file.old_mode}</span> to <span class="tf-mono">{file.new_mode}</span>. No content change.</div>
    {:else if file.hunks.length === 0 && file.change === "renamed"}
      <div class="tf-dwhy">Renamed{file.similarity != null ? ` with ${file.similarity}% similarity` : ""}. No content change.</div>
    {:else}
      <div class="tf-dbody" {@attach win.list}>
        {#each win.slice(items) as it (it.key)}{@render item(it)}{/each}
      </div>
    {/if}
    {#if drawsLines && file.truncated}
      <div class="tf-hunk">Cut here at the {n(DIFF_CEILING)}-line ceiling: the rest of this file's change is at the source.</div>
    {/if}
    {#if placed.apart.length > 0 || strays.length > 0}
      <details class="tf-outdated">
        <summary>{placed.apart.length + strays.length} outdated</summary>
        {#each placed.apart as thread, t (t)}
          <div class="tf-thread"><span class="tf-dim">{lineName(thread.side, thread.line)}</span>{@render said(thread.comments)}</div>
        {/each}
        {#each strays as d (d.id)}
          <div class="tf-thread tf-pending"><p><b>You</b> <span class="tf-chip tf-warn">pending</span> {`${lineName(d.side, d.line)}: ${d.body}`}</p></div>
        {/each}
      </details>
    {/if}
  {/if}
</div>
