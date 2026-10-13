<!--
  A diff block (ADR-0020; `CONTEXT.md` **Diff**), drawn the way a code review
  reads one: the shape of the change first (files, +/− counts), then one card
  per file, each a run of hunks with the old and new line numbers in two
  gutters.

  Colour only where something changed: added lines on the theme's `--ok`,
  removed ones on `--fail`, both as a faint wash so the text stays the
  loudest thing on the line. No syntax highlighting (spec #660, out of scope).

  Every string is source text and is interpolated (rule 5, gotcha 7): a line
  holding `<b>` is a line holding `<b>`.

  **Review on the diff (#709).** A file's threads -- the source's line
  comments, by side -- are drawn under the line they were written on, as a
  margin note hung at the text column: author, age, the words as Markdown
  read legibly (never markup). An outdated thread (the source's answer) is
  kept apart at the end of its file, closed, as is one the source did not
  call outdated but whose line is not drawn (past a file's cut). A thread on a
  file the diff no longer holds (`gone`, the source's) and a pending comment on
  one are kept in an outdated list at the end of the diff, never dropped. Where
  the diff takes comments (`canComment`), each line offers a `+` on hover or
  focus that opens an inline editor (Enter or `c`). The `+`s of a file are one
  tab stop: Up and Down move along its lines (a roving tabindex), so a long
  diff is not a thousand stops; what the reader writes is **pending** -- kept in
  `pending-comments`, this viewer's, until a review sends it or the reader
  discards it -- and drawn in the amber of something not yet done. A removed
  line is commented on the old side by its old number; any other line on the
  new side by its new one.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { DiffChange, DiffFile, DiffLine, FileThread, LineComment, LineThread } from "../../ipc/entity";
  import type { ReviewSide } from "../../ipc/sources";
  import { ago } from "../../shell/time";
  import MarkdownText from "./MarkdownText.svelte";
  import { lineName, pendingComments, type PendingComment } from "./pending-comments.svelte";

  let {
    files,
    truncated,
    webUrl,
    sourceName,
    onopenurl,
    entityId = "",
    canComment = false,
    gone = [],
  }: {
    files: DiffFile[];
    /** The per-view cap cut it: files after the cut carry counts only. */
    truncated: boolean;
    /** The entity's own page at its source, offered where a file was cut. */
    webUrl: string | null;
    /** The source's display name, for *Open in <source>*. */
    sourceName: string;
    onopenurl: (url: string) => void;
    /** The entity the diff is of: whose pending comments these are (#709). */
    entityId?: string;
    /** The reader may write line comments here (#709): the diff takes them and the shell can send them. */
    canComment?: boolean;
    /** Threads on files this diff no longer holds (#709): outdated, drawn at the end. */
    gone?: FileThread[];
  } = $props();

  /** Which cards the reader closed, by index. Everything opens expanded. */
  let closed = $state<Record<number, boolean>>({});
  let cards = $state<(HTMLElement | null)[]>([]);

  const additions = $derived(files.reduce((sum, file) => sum + file.additions, 0));
  const deletions = $derived(files.reduce((sum, file) => sum + file.deletions, 0));
  const allClosed = $derived(files.length > 0 && files.every((_, index) => closed[index]));

  const LETTER: Record<DiffChange, string> = {
    added: "A",
    modified: "M",
    deleted: "D",
    renamed: "R",
    binary: "B",
  };

  function toggleAll() {
    const close = !allClosed;
    closed = Object.fromEntries(files.map((_, index) => [index, close]));
  }

  /** Open the card and bring it into view. */
  function jump(index: number) {
    closed[index] = false;
    cards[index]?.scrollIntoView({ block: "start" });
  }

  // -- line comments (#709) ---------------------------------------------------

  /** Where a comment on `line` sits: a removed line on the old side, any other on the new. */
  function anchorOf(line: DiffLine): { side: ReviewSide; line: number } | null {
    if (line.kind === "del") return line.old === null ? null : { side: "old", line: line.old };
    return line.new === null ? null : { side: "new", line: line.new };
  }

  const key = (side: ReviewSide, line: number) => `${side}:${line}`;

  /** The keys `line` answers to: its own anchor, and a kept line's old number too. */
  function keysOf(line: DiffLine): string[] {
    const keys: string[] = [];
    if (line.old !== null && line.kind !== "add") keys.push(key("old", line.old));
    if (line.new !== null && line.kind !== "del") keys.push(key("new", line.new));
    return keys;
  }

  const pending = $derived(pendingComments.of(entityId));

  function commentsIn(threads: LineThread[]): number {
    return threads.reduce((sum, t) => sum + t.comments.length, 0);
  }

  /** One file's comments, sorted: what sits on each line by key, and what is kept apart. */
  interface Placed {
    on: Map<string, { threads: LineThread[]; drafts: PendingComment[] }>;
    /** Threads not drawn on a line: outdated, or on a line past the file's cut. */
    off: LineThread[];
    /** Pending comments whose line this diff does not draw. */
    offPending: PendingComment[];
    /** Comments in `off` and `offPending`. */
    offCount: number;
    count: number;
    pendingCount: number;
  }

  const placed = $derived(
    files.map((file): Placed => {
      const lineKeys = new Set(file.hunks.flatMap((h) => h.lines.flatMap(keysOf)));
      const on: Placed["on"] = new Map();
      const slot = (k: string) => {
        let entry = on.get(k);
        if (!entry) on.set(k, (entry = { threads: [], drafts: [] }));
        return entry;
      };
      const off: LineThread[] = [];
      const offPending: PendingComment[] = [];
      for (const thread of file.threads ?? []) {
        const k = key(thread.side, thread.line);
        if (!thread.outdated && lineKeys.has(k)) slot(k).threads.push(thread);
        else off.push(thread);
      }
      const mine = pending.filter((d) => d.path === file.path);
      for (const draft of mine) {
        const k = key(draft.side, draft.line);
        if (lineKeys.has(k)) slot(k).drafts.push(draft);
        else offPending.push(draft);
      }
      const said = commentsIn(file.threads ?? []);
      return {
        on,
        off,
        offPending,
        offCount: commentsIn(off) + offPending.length,
        count: said + mine.length,
        pendingCount: mine.length,
      };
    }),
  );

  /** What sits under `line` of file `index`, or nothing. */
  function under(index: number, line: DiffLine) {
    const on = placed[index]?.on;
    if (!on) return null;
    const found = keysOf(line).flatMap((k) => {
      const entry = on.get(k);
      return entry ? [entry] : [];
    });
    if (found.length === 0) return null;
    return { threads: found.flatMap((e) => e.threads), drafts: found.flatMap((e) => e.drafts) };
  }

  /** The editor that is open: on a line of a file (`-1`: none in this diff), new or over a pending comment. */
  let editing = $state<{ file: number; side: ReviewSide; line: number; draft: string | null; row?: string } | null>(
    null,
  );
  /** The open editor's words. */
  let words = $state("");

  function startOn(index: number, line: DiffLine, row: string) {
    const at = anchorOf(line);
    if (!at) return;
    editing = { file: index, ...at, draft: null, row };
    words = "";
  }

  /** The block's root, to find a line's `+` again once its editor closes. */
  let root = $state<HTMLElement | null>(null);

  /** Close the editor; one opened from a line's `+` hands the focus back to it. */
  async function close() {
    const open = editing;
    editing = null;
    if (open?.row === undefined) return;
    await tick();
    root?.querySelector<HTMLButtonElement>(`.df-add[data-file="${open.file}"][data-row="${open.row}"]`)?.focus();
  }

  /** Which line of each file holds its one tab stop, by `hunk:line`; unset, its first commentable line. */
  let rover = $state<Record<number, string>>({});

  function firstRow(file: DiffFile): string | null {
    for (const [h, hunk] of file.hunks.entries()) {
      const l = hunk.lines.findIndex((line) => anchorOf(line) !== null);
      if (l >= 0) return `${h}:${l}`;
    }
    return null;
  }

  /** Up and Down move the tab stop along the file's lines; `c` comments, as Enter does. */
  function roveKey(event: KeyboardEvent, index: number, line: DiffLine, row: string) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const here = event.currentTarget as HTMLElement;
      const all = [...(here.closest(".df-lines")?.querySelectorAll<HTMLButtonElement>(".df-add") ?? [])];
      const next = all[all.indexOf(here as HTMLButtonElement) + (event.key === "ArrowDown" ? 1 : -1)];
      event.preventDefault();
      if (!next) return;
      rover[index] = next.dataset.row!;
      next.focus();
    } else if (event.key === "c" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      startOn(index, line, row);
    }
  }

  function startEdit(index: number, draft: PendingComment) {
    editing = { file: index, side: draft.side, line: draft.line, draft: draft.id };
    words = draft.body;
  }

  function save() {
    const open = editing;
    if (!open) return;
    const file = files[open.file];
    if (open.draft !== null) pendingComments.edit(entityId, open.draft, words);
    else if (file) pendingComments.add(entityId, { path: file.path, side: open.side, line: open.line, body: words });
    void close();
  }

  function editorKey(event: KeyboardEvent) {
    if (event.key === "Escape") {
      // The editor's own: the panel behind it does not close.
      event.stopPropagation();
      event.preventDefault();
      void close();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      save();
    }
  }

  /** Whether the editor is open on this anchor of file `index` (a new comment, or one of its drafts). */
  function editingAt(index: number, line: DiffLine): boolean {
    const open = editing;
    if (!open || open.file !== index) return false;
    return keysOf(line).includes(key(open.side, open.line));
  }

  /** Focus the editor's text area as it opens, the cursor after the words. */
  function focusOnOpen(area: HTMLTextAreaElement) {
    area.focus();
    area.setSelectionRange(area.value.length, area.value.length);
  }

  /** Which files' outdated lists the reader opened (or, holding a pending comment, did not close). */
  let outdatedOpen = $state<Record<number, boolean>>({});

  /** Pending comments on a file this diff does not hold: kept with `gone`, at the end. */
  const pendingGone = $derived(pending.filter((d) => !files.some((f) => f.path === d.path)));
  const goneCount = $derived(commentsIn(gone.map((g) => g.thread)) + pendingGone.length);
  /** The end list opened (or, holding a pending comment, not closed). */
  let goneOpen = $state<boolean | null>(null);

  /** Asking before every pending comment goes. */
  let confirmingDiscard = $state(false);

  function plural(n: number, one: string, many = `${one}s`): string {
    return `${n} ${n === 1 ? one : many}`;
  }


  function linesOf(file: DiffFile): number {
    return file.hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0);
  }

  /** Wide enough for the largest line number in the file, so the gutters line up. */
  function gutter(file: DiffFile): number {
    let max = 0;
    for (const hunk of file.hunks) {
      for (const line of hunk.lines) max = Math.max(max, line.old ?? 0, line.new ?? 0);
    }
    return Math.max(2, String(max).length);
  }
</script>

{#snippet bubble(count: number, pendingHere: boolean)}
  <span class="df-ccount k" class:pending={pendingHere} title="{plural(count, 'comment')}">
    <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2.5h8v5.5H6l-2.5 2V8H2z" /></svg>
    {count}
  </span>
{/snippet}

{#snippet said(comment: LineComment)}
  <article class="df-c">
    <header class="df-c-head">
      <span class="df-c-who">{comment.author}</span>
      {#if comment.at}<time class="df-c-at" datetime={comment.at} title={comment.at}>{ago(comment.at)}</time>{/if}
    </header>
    <div class="df-c-body"><MarkdownText text={comment.body} {onopenurl} /></div>
  </article>
{/snippet}

{#snippet mine(index: number, draft: PendingComment)}
  <article class="df-c pending">
    <header class="df-c-head">
      <span class="df-c-who">You</span>
      <span class="df-c-state"><span class="df-dot" aria-hidden="true"></span>Pending</span>
      <span class="df-c-acts">
        <button class="btn ghost sm" type="button" onclick={() => startEdit(index, draft)}>Edit</button>
        <button class="btn ghost sm" type="button" onclick={() => pendingComments.discard(entityId, draft.id)}>Discard</button>
      </span>
    </header>
    <div class="df-c-body"><MarkdownText text={draft.body} {onopenurl} /></div>
  </article>
{/snippet}

{#snippet editor()}
  {#if editing}
    <div class="df-editor">
      <textarea
        class="inp"
        rows="3"
        aria-label="Comment on {lineName(editing.side, editing.line)}"
        placeholder="Leave a comment · Markdown"
        bind:value={words}
        onkeydown={editorKey}
        use:focusOnOpen
      ></textarea>
      <div class="df-ebar">
        <span class="df-hint k">⌘↵ to {editing.draft === null ? "add" : "save"} · Esc to cancel</span>
        <button class="btn ghost sm" type="button" onclick={() => void close()}>Cancel</button>
        <button class="btn pri sm" type="button" onclick={save}>{editing.draft === null ? "Add comment" : "Save"}</button>
      </div>
    </div>
  {/if}
{/snippet}

<div class="df" bind:this={root}>
  {#if truncated}
    <p class="df-cut">
      This diff stops at the line cap: files after the cut show their counts only.
      {#if webUrl}
        <button class="btn sm" onclick={() => onopenurl(webUrl)}>Open in {sourceName}</button>
      {/if}
    </p>
  {/if}

  <div class="df-sum">
    <span class="k">{files.length} {files.length === 1 ? "file" : "files"}</span>
    <span class="k add">+{additions}</span>
    <span class="k del">−{deletions}</span>
    {#if pending.length > 0}
      <span class="df-pending k" role="status"><span class="df-dot" aria-hidden="true"></span>{pending.length} pending</span>
      {#if confirmingDiscard}
        <span class="df-ask">
          <button class="btn sm danger" type="button" onclick={() => {
            pendingComments.clear(entityId);
            confirmingDiscard = false;
          }}>Discard {pending.length}</button>
          <button class="btn ghost sm" type="button" onclick={() => (confirmingDiscard = false)}>Keep</button>
        </span>
      {:else}
        <button class="btn ghost sm" type="button" onclick={() => (confirmingDiscard = true)}>Discard pending</button>
      {/if}
    {/if}
    {#if files.length > 1}
      <button class="btn ghost sm df-all" onclick={toggleAll}>
        {allClosed ? "Expand all" : "Collapse all"}
      </button>
    {/if}
  </div>

  {#if files.length > 1}
    <ol class="df-index">
      {#each files as file, index (index)}
        <li>
          <button title="Go to {file.path}" onclick={() => jump(index)}>
            <span class="df-letter {file.change}">{LETTER[file.change]}</span>
            <span class="df-ipath">{file.path}</span>
            {#if placed[index]?.count}
              {@render bubble(placed[index].count, placed[index].pendingCount > 0)}
            {/if}
            <span class="k add">+{file.additions}</span>
            <span class="k del">−{file.deletions}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}

  {#each files as file, index (index)}
    {@const open = !closed[index]}
    <section class="df-file" bind:this={cards[index]}>
      <button
        class="df-head"
        aria-expanded={open}
        onclick={() => (closed[index] = open)}
      >
        <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true">
          <path d="M6 4l4 4-4 4" />
        </svg>
        <span class="df-change {file.change}">{file.change}</span>
        <span class="df-path">
          {#if file.old_path}
            <span class="old">{file.old_path}</span>
            <span class="arrow">→</span>
          {/if}
          {file.path}
        </span>
        {#if placed[index]?.count}
          {@render bubble(placed[index].count, placed[index].pendingCount > 0)}
        {/if}
        <span class="k add">+{file.additions}</span>
        <span class="k del">−{file.deletions}</span>
      </button>

      {#if open}
        {#if file.change === "binary"}
          <p class="df-note">Binary file: no lines to show.</p>
        {:else if file.hunks.length === 0 && !file.truncated}
          <p class="df-note">
            {file.change === "renamed" ? "Renamed without changes." : "No line changes."}
          </p>
        {:else if file.hunks.length > 0}
          <div class="df-body" style:--gw="{gutter(file)}ch">
            <div class="df-lines">
              {#each file.hunks as hunk, h (h)}
                <div class="df-hunk">{hunk.header}</div>
                {#each hunk.lines as line, l (l)}
                  {@const here = under(index, line)}
                  {@const writing = editingAt(index, line)}
                  <div class="df-line {line.kind}" class:noted={here !== null} data-kind={line.kind}>
                    <span class="df-old">{line.old ?? ""}</span>
                    <span class="df-new">{line.new ?? ""}</span>
                    <span class="df-mark" aria-hidden="true">
                      {line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}
                    </span>
                    <span class="df-text">{line.text}</span>
                    {#if canComment && anchorOf(line)}
                      {@const at = anchorOf(line)!}
                      {@const row = `${h}:${l}`}
                      <button
                        class="df-add"
                        type="button"
                        data-file={index}
                        data-row={row}
                        tabindex={(rover[index] ?? firstRow(file)) === row ? 0 : -1}
                        aria-label="Comment on {lineName(at.side, at.line)}"
                        title="Comment on {lineName(at.side, at.line)} (Enter or c; Up and Down move)"
                        onfocus={() => (rover[index] = row)}
                        onkeydown={(event) => roveKey(event, index, line, row)}
                        onclick={() => startOn(index, line, row)}
                      >
                        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M5 1.5v7M1.5 5h7" /></svg>
                      </button>
                    {/if}
                  </div>
                  {#if here || writing}
                    <div class="df-thread">
                      <div class="df-tbox">
                        {#each here?.threads ?? [] as thread, t (t)}
                          {#each thread.comments as comment, c (c)}
                            {@render said(comment)}
                          {/each}
                        {/each}
                        {#each here?.drafts ?? [] as draft (draft.id)}
                          {#if editing?.draft === draft.id}
                            {@render editor()}
                          {:else}
                            {@render mine(index, draft)}
                          {/if}
                        {/each}
                        {#if writing && editing?.draft === null}
                          {@render editor()}
                        {/if}
                      </div>
                    </div>
                  {/if}
                {/each}
              {/each}
            </div>
          </div>
        {/if}
        {@const notes = placed[index]}
        {#if notes && notes.offCount > 0}
          {@const allOutdated = notes.offPending.length === 0 && notes.off.every((t) => t.outdated)}
          {@const shown = outdatedOpen[index] ?? notes.offPending.length > 0}
          <div class="df-outdated">
            <button
              class="df-otoggle"
              type="button"
              aria-expanded={shown}
              onclick={() => (outdatedOpen[index] = !shown)}
            >
              <svg class="chev" class:open={shown} viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
              {allOutdated
                ? plural(notes.offCount, "outdated comment")
                : `${plural(notes.offCount, "comment")} not on a line shown`}
            </button>
            {#if shown}
              {#each notes.off as thread, t (t)}
                <div class="df-tbox stale">
                  <span class="df-where k"
                    >{lineName(thread.side, thread.line)}{thread.outdated ? ", outdated" : ", past the cut"}</span
                  >
                  {#each thread.comments as comment, c (c)}
                    {@render said(comment)}
                  {/each}
                </div>
              {/each}
              {#each notes.offPending as draft (draft.id)}
                <div class="df-tbox stale">
                  <span class="df-where k">{lineName(draft.side, draft.line)}, not in this diff</span>
                  {#if editing?.draft === draft.id}
                    {@render editor()}
                  {:else}
                    {@render mine(index, draft)}
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        {/if}
        {#if file.truncated}
          <p class="df-note">
            {#if linesOf(file) > 0}
              knobas shows the first {linesOf(file).toLocaleString("en")} lines of this file.
            {:else}
              This file is past the diff's cut; knobas shows its counts only.
            {/if}
            {#if webUrl}
              <button class="btn sm" onclick={() => onopenurl(webUrl)}>Open in {sourceName}</button>
            {/if}
          </p>
        {/if}
      {/if}
    </section>
  {/each}

  {#if goneCount > 0}
    {@const shown = goneOpen ?? pendingGone.length > 0}
    <div class="df-outdated df-gone">
      <button class="df-otoggle" type="button" aria-expanded={shown} onclick={() => (goneOpen = !shown)}>
        <svg class="chev" class:open={shown} viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4l4 4-4 4" /></svg>
        Outdated: {plural(goneCount, "comment")} on files no longer in this diff
      </button>
      {#if shown}
        {#each gone as g, t (t)}
          <div class="df-tbox stale">
            <span class="df-where k">{g.path}, {lineName(g.thread.side, g.thread.line)}</span>
            {#each g.thread.comments as comment, c (c)}
              {@render said(comment)}
            {/each}
          </div>
        {/each}
        {#each pendingGone as draft (draft.id)}
          <div class="df-tbox stale">
            <span class="df-where k">{draft.path}, {lineName(draft.side, draft.line)}, not in this diff</span>
            {#if editing?.draft === draft.id}
              {@render editor()}
            {:else}
              {@render mine(-1, draft)}
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .df {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .add {
    color: var(--ok);
  }

  .del {
    color: var(--fail);
  }

  .df-sum {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }

  .df-sum .df-all {
    margin-left: auto;
  }

  .df-cut,
  .df-note {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 0;
    color: var(--muted);
    font-size: 12px;
  }

  .df-cut {
    color: var(--amber);
  }

  .df-note {
    padding: 6px 10px;
  }

  /* The file list: one quiet line per file, a jump target each. */
  .df-index {
    list-style: none;
    border: 1px solid var(--hair);
    border-radius: 2px;
    max-height: 168px;
    overflow: auto;
  }

  .df-index li + li {
    border-top: 1px solid var(--hair);
  }

  .df-index button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 8px;
    text-align: left;
    font: 400 12px var(--mono);
  }

  .df-index button:hover {
    background: var(--raised);
  }

  .df-ipath {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .df-letter {
    width: 12px;
    flex: none;
    font: 500 11px var(--mono);
    color: var(--faint);
  }

  .df-letter.added {
    color: var(--ok);
  }

  .df-letter.deleted {
    color: var(--fail);
  }

  /* One file. A hairline box, not a card with a shadow. */
  .df-file {
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--bg);
    scroll-margin-top: 8px;
  }

  .df-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 4px 8px;
    text-align: left;
    background: var(--raised);
    font-size: 12px;
  }

  .df-head:hover {
    background: var(--hair);
  }

  .chev {
    width: 12px;
    height: 12px;
    flex: none;
    stroke: var(--muted);
    fill: none;
    stroke-width: 1.5;
    transition: transform 0.12s;
  }

  .chev.open {
    transform: rotate(90deg);
  }

  .df-change {
    flex: none;
    font: 500 10px var(--mono);
    letter-spacing: 0.04em;
    color: var(--faint);
    border: 1px solid var(--hair2);
    border-radius: 2px;
    padding: 0 4px;
  }

  .df-change.added {
    color: var(--ok);
  }

  .df-change.deleted {
    color: var(--fail);
  }

  .df-path {
    flex: 1;
    min-width: 0;
    font: 500 12px var(--mono);
    overflow-wrap: anywhere;
  }

  .df-path .old {
    color: var(--muted);
  }

  .df-path .arrow {
    color: var(--faint);
    margin: 0 4px;
  }

  .df-body {
    overflow-x: auto;
    border-top: 1px solid var(--hair);
  }

  .df-lines {
    min-width: 100%;
    width: max-content;
    font: 400 12px/18px var(--mono);
  }

  .df-hunk {
    padding: 2px 8px 2px calc(var(--gw) * 2 + 34px);
    color: var(--faint);
    background: var(--panel);
    white-space: pre;
  }

  .df-line {
    display: grid;
    grid-template-columns: calc(var(--gw) + 10px) calc(var(--gw) + 10px) 14px auto;
  }

  .df-old,
  .df-new {
    padding-right: 6px;
    text-align: right;
    color: var(--faint);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }

  .df-new {
    border-right: 1px solid var(--hair);
  }

  .df-mark {
    text-align: center;
    color: var(--faint);
    user-select: none;
  }

  .df-text {
    padding-right: 12px;
    white-space: pre;
  }

  .df-line.add {
    background: color-mix(in srgb, var(--ok) 11%, transparent);
  }

  .df-line.add .df-mark {
    color: var(--ok);
  }

  .df-line.del {
    background: color-mix(in srgb, var(--fail) 12%, transparent);
  }

  .df-line.del .df-mark {
    color: var(--fail);
  }

  /* -- review on the diff (#709) ------------------------------------------ */

  /* Something not yet sent is something moving: amber, and nothing else is. */
  .df-pending {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--amber);
  }

  .df-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--amber);
    flex: none;
  }

  .df-ask {
    display: inline-flex;
    gap: 4px;
  }

  .df-ccount {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: none;
    font-size: 11px;
    color: var(--muted);
  }

  .df-ccount svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linejoin: round;
  }

  .df-ccount.pending {
    color: var(--amber);
  }

  /* The diff is its own query container: a note hung under a line spans the
     card's visible width and stays put while the code scrolls sideways. */
  .df-body {
    container-type: inline-size;
  }

  .df-line {
    position: relative;
  }

  /* The way in: a small filled square over the marker column, there on hover
     or focus and nowhere else. */
  .df-add {
    position: absolute;
    top: 1px;
    left: calc(var(--gw) * 2 + 20px);
    width: 14px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 2px;
    background: var(--text);
    color: var(--bg);
    opacity: 0;
  }

  .df-add svg {
    width: 9px;
    height: 9px;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
  }

  .df-line:hover .df-add,
  .df-add:focus-visible {
    opacity: 1;
  }

  /* The line the keyboard is on reads as one. */
  .df-line:has(.df-add:focus-visible) {
    box-shadow: inset 0 0 0 1px var(--hair2);
  }

  /* A line something was said on: its new gutter carries a mark. */
  .df-line.noted .df-new {
    box-shadow: inset -2px 0 0 var(--hair2);
  }

  .df-thread {
    position: sticky;
    left: 0;
    width: 100cqi;
    padding: 6px 12px 8px calc(var(--gw) * 2 + 34px);
    background: var(--panel);
    border-top: 1px solid var(--hair);
    border-bottom: 1px solid var(--hair);
    font-family: var(--sans);
    white-space: normal;
  }

  /* A margin note: a rule down the left, the words at reading width. */
  .df-tbox {
    display: grid;
    gap: 2px;
    max-width: 76ch;
  }

  .df-c {
    padding: 6px 0 6px 10px;
    border-left: 2px solid var(--hair2);
  }

  .df-c + .df-c {
    border-top: 1px solid var(--hair);
  }

  .df-c.pending {
    border-left-color: var(--amber);
  }

  .df-c-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-height: 20px;
  }

  .df-c-who {
    font: 500 12px var(--mono);
    color: var(--text);
  }

  .df-c-at {
    font: 400 11px var(--mono);
    color: var(--faint);
  }

  .df-c-state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 500 11px var(--mono);
    color: var(--amber);
  }

  .df-c-acts {
    display: inline-flex;
    gap: 2px;
    margin-left: auto;
    opacity: 0;
  }

  .df-c:hover .df-c-acts,
  .df-c-acts:focus-within {
    opacity: 1;
  }

  .df-c-body {
    margin-top: 2px;
  }

  .df-c-body :global(.md) {
    font-size: 12.5px;
  }

  .df-editor {
    display: grid;
    gap: 6px;
    padding: 6px 0 2px 10px;
    border-left: 2px solid var(--amber);
  }

  .df-editor textarea {
    font: 400 12px/1.5 var(--mono);
    min-height: 64px;
  }

  .df-ebar {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .df-hint {
    margin-right: auto;
    font-size: 11px;
    color: var(--faint);
  }

  /* Outdated: below the file's lines, closed, the words a step down. */
  .df-outdated {
    display: grid;
    gap: 6px;
    padding: 6px 10px 8px;
    border-top: 1px solid var(--hair);
  }

  /* The end list (#709): threads on files the diff no longer holds, boxed like a file. */
  .df-gone {
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .df-otoggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    justify-self: start;
    font: 400 12px var(--mono);
    color: var(--muted);
  }

  .df-otoggle:hover {
    color: var(--text);
  }

  .df-tbox.stale {
    padding-left: 18px;
  }

  .df-tbox.stale .df-c:not(.pending) .df-c-body {
    color: var(--muted);
  }

  .df-where {
    font-size: 11px;
    color: var(--faint);
  }

  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
</style>
