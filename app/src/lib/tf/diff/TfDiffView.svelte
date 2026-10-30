<!--
  The one diff component (#2266, spec #2234 step 6a): every diff in the app,
  a commit's, a compare's, the checkout's uncommitted change, a pull
  request's *Files changed* and the edit a *Commit change…* dialog shows,
  drawn from the source's own diff (`DiffFile`). The pull request's file view
  (#842) is its base; the layout is `mockups/code-area/diff.html`.

  - **Toolbar**: Unified / Split, one choice app-wide (`diff-layout`); a
    window too narrow for split shows unified and says so. Where *Viewed* is
    offered, how many files are viewed.
  - **File list** (`list`): each file with `+N −N`, a filter that narrows the
    list and the diffs together, a click that scrolls to the file.
  - **Files**: a header with the path (`old → new` on a rename), the counts,
    *new file*; the hunks with both line numbers (unified) or each side's
    (split), the next hunk's `@@` line between hunks.
  - **Highlighting**: each side of a file is one text through the renderer
    port (`render`, #2254), so a string or comment that spans hunks is
    highlighted as one. Plain until the tokens arrive.
  - **Capabilities the caller passes in**, absent by default:
    - `viewed`: a *Viewed* box per file that collapses it.
    - `review`: a `+` per line that opens a comment box; *Add to review*
      keeps the comment pending in the reader's store under its line.
    - `openAt`: *Open in…* in each file's header (#2274), on the file at
      its first changed line, read at the diff's head; not on a deleted file.
    Threads the source holds are drawn wherever it answers them: under their
    line, or apart at the end of the file when outdated or not drawn
    (`pr/files.ts`); `gone` lists those on files the diff no longer holds.
-->
<script lang="ts">
  import type { DiffLine, FileThread, LineComment } from "../../ipc/entity";
  import { lineName } from "../../detail/view/pending-comments.svelte";
  import { untrack } from "svelte";
  import { renderPort, type HighlightJob, type LineTokens, type RenderPort } from "../../render/port";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import { matchesPath, placeThreads, sameTarget, targetOf } from "../pr/files";
  import { keyOf, shownLayout, sidesOf, splitRows, unifiedRows, type DiffReview, type DiffViewed, type DrawnFile, type DrawnLine, type SideText } from "./diff";
  import { diffLayout, windowNarrow, type DiffLayoutChoice } from "./diff-layout.svelte";
  import { firstLine } from "../openin/openin";
  import TfOpenObject from "../openin/TfOpenObject.svelte";

  let {
    files,
    list = true,
    viewed = null,
    review = null,
    gone = [],
    render = renderPort(),
    layout = diffLayout,
    narrow = windowNarrow(),
    openAt = null,
  }: {
    /** A file's `label`, where the caller gives one, is drawn in place of its path (`DrawnFile`). */
    files: readonly DrawnFile[];
    /** The file list and its filter beside the diffs. */
    list?: boolean;
    viewed?: DiffViewed | null;
    review?: DiffReview | null;
    /** Threads on files the diff no longer holds. */
    gone?: readonly FileThread[];
    render?: RenderPort;
    layout?: DiffLayoutChoice;
    narrow?: { readonly current: boolean };
    /** The repository (or branch) the files are in, and the commit or branch they are read at: *Open in…* per file. */
    openAt?: { entityId: string; at: string | null } | null;
  } = $props();

  let filter = $state("");
  const shown = $derived(files.filter((file) => matchesPath(keyOf(file), filter)));
  const split = $derived(shownLayout(layout.chosen, narrow.current) === "split");
  const isViewed = (path: string) => viewed?.paths.includes(path) ?? false;

  /**
   * Each file's two sides (by `keyOf`), and their tokens as they arrive, under `side\nkey`. A re-read
   * that hands over the same texts (Local changes on every focus, a refreshed commit) keeps its
   * tokens and starts no job; a side whose text changed drops its tokens and is highlighted anew.
   */
  const sides = $derived(new Map(files.map((file) => [keyOf(file), sidesOf(file)])));
  let tokens = $state.raw<Record<string, (LineTokens | undefined)[]>>({});
  const jobs = new Map<string, { text: string; port: RenderPort; job: HighlightJob }>();
  $effect(() => {
    const port = render;
    const want = new Map<string, SideText>();
    for (const [key, both] of sides) for (const side of ["old", "new"] as const) want.set(`${side}\n${key}`, both[side]);
    const had = untrack(() => tokens);
    const kept: Record<string, (LineTokens | undefined)[]> = {};
    for (const [name, made] of jobs) {
      if (want.get(name)?.text === made.text && made.port === port) {
        if (had[name]) kept[name] = had[name];
      } else {
        made.job.cancel();
        jobs.delete(name);
      }
    }
    tokens = kept;
    for (const [name, side] of want) {
      if (jobs.has(name)) continue;
      const got: (LineTokens | undefined)[] = [];
      const job = port.highlight(side, {
        onLines(from, lines) {
          lines.forEach((line, n) => (got[from + n] = line));
          tokens = { ...tokens, [name]: [...got] };
        },
      });
      jobs.set(name, { text: side.text, port, job });
    }
  });
  $effect(() => () => {
    jobs.forEach((made) => made.job.cancel());
    jobs.clear();
  });
  /** A line's tokens, only while they are of the text drawn: a re-read draws plain until its own arrive. */
  const tokensOf = (file: DrawnFile, side: "old" | "new", drawn: DrawnLine) => {
    const key = keyOf(file);
    const row = sides.get(key)?.index[drawn.at]?.[side];
    if (row === null || row === undefined) return undefined;
    const toks = tokens[`${side}\n${key}`]?.[row];
    return toks && toks.map(([part]) => part).join("") === drawn.line.text ? toks : undefined;
  };

  /** Each file's card, to scroll to from the list. */
  const cards: Record<string, HTMLElement | null> = $state({});

  /** The line whose comment box is open: its file and its place. */
  let open = $state<{ path: string; side: "old" | "new"; line: number } | null>(null);
  let draft = $state("");
  const drafts = $derived(review ? review.pending.of(review.entity) : []);

  function addToReview() {
    if (open === null || review === null) return;
    if (review.pending.add(review.entity, { ...open, body: draft }) === null) return;
    open = null;
    draft = "";
    push({ text: "Added to your review. Nothing is sent until you submit it." });
  }

  const SIGN = { context: " ", add: "+", del: "−" } as const;
  const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const isOpen = (path: string, line: DiffLine) => open !== null && sameTarget(open, { path, ...targetOf(line) });
  const draftsAt = (path: string, line: DiffLine) => drafts.filter((d) => sameTarget(d, { path, ...targetOf(line) }));
  const nameOf = (line: DiffLine) => {
    const at = targetOf(line);
    return lineName(at.side, at.line);
  };
  /** Pending comments on files the diff no longer holds. */
  const homeless = $derived(drafts.filter((d) => !files.some((file) => file.path === d.path)));
  /** The lines a split row's threads and comments follow: one for a context line, both of a pair. */
  const under = (row: { old: DrawnLine | null; new: DrawnLine | null }) =>
    [row.old, row.new].filter((cell, n, both): cell is DrawnLine => cell !== null && (n === 0 || cell !== both[0]));
</script>

{#snippet code(text: string, toks: LineTokens | undefined)}{#if toks}{#each toks as [part, kind], k (k)}{#if kind !== ""}<span class="tf-hl-{kind}">{part}</span>{:else}{part}{/if}{/each}{:else}{text}{/if}{/snippet}

{#snippet said(comments: readonly LineComment[])}
  {#each comments as comment, n (n)}
    <p><b>{comment.author}</b>{comment.at ? ` ${ago(comment.at)}` : ""}: {comment.body}</p>
  {/each}
{/snippet}

{#snippet mine(body: string)}
  <div class="tf-thread tf-pending"><p><b>You</b> <span class="tf-chip tf-warn">pending</span> {body}</p></div>
{/snippet}

<!-- The `+` on a line, where the source takes comments; an empty cell otherwise. -->
{#snippet plus(path: string, line: DiffLine)}
  {#if review}
    {@const name = nameOf(line)}
    <button type="button" class="tf-addc" title="Comment on {name}" aria-label="Comment on {name} of {path}" onclick={() => ((open = { path, ...targetOf(line) }), (draft = ""))}
      >+</button
    >
  {:else}<span></span>{/if}
{/snippet}

<!-- What follows a line: its threads, the reader's pending comments on it, the open comment box. -->
{#snippet after(path: string, line: DiffLine, threads: readonly { comments: readonly LineComment[] }[], side: "old" | "new" | null)}
  {#each threads as thread, t (t)}
    <div class="tf-thread" class:tf-on-new={side === "new"}>{@render said(thread.comments)}</div>
  {/each}
  {#each draftsAt(path, line) as d (d.id)}{@render mine(d.body)}{/each}
  {#if isOpen(path, line)}
    {@const name = nameOf(line)}
    <div class="tf-newc" class:tf-on-new={side === "new"}>
      <textarea bind:value={draft} placeholder="Comment on {name}" aria-label="Comment on {name} of {path}"></textarea>
      <div class="tf-acts">
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (open = null)}>Cancel</button>
        <button type="button" class="tf-btn tf-sm tf-primary" disabled={draft.trim() === ""} onclick={addToReview}>Add to review</button>
      </div>
    </div>
  {/if}
{/snippet}

<!-- One side of a split row: its number, sign and code, or padding where the side has no line. -->
{#snippet half(file: DrawnFile, cell: DrawnLine | null, side: "old" | "new")}
  {#if cell === null}
    <div class="tf-ln tf-pad"><span></span><span></span><span></span></div>
  {:else}
    {@const line = cell.line}
    <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
      <span class="tf-no">{side === "old" ? line.old : line.new}</span><span>{SIGN[line.kind]} {@render code(line.text, tokensOf(file, side, cell))}</span>
      {#if targetOf(line).side === side}{@render plus(file.path, line)}{:else}<span></span>{/if}
    </div>
  {/if}
{/snippet}

{#snippet card(file: DrawnFile)}
  {@const done = isViewed(file.path)}
  {@const placed = placeThreads(file)}
  {@const lines = file.hunks.flatMap((hunk) => hunk.lines)}
  {@const strays = drafts.filter((d) => d.path === file.path && !lines.some((line) => draftsAt(file.path, line).includes(d)))}
  <div class="tf-diff" class:tf-viewed={done} class:tf-split={split} bind:this={cards[keyOf(file)]}>
    <div class="tf-fh">
      <span class="tf-mono">{file.old_path && file.old_path !== file.path ? `${file.old_path} → ${keyOf(file)}` : keyOf(file)}</span>
      <span class="tf-add-n">+{file.additions}</span>
      <span class="tf-del-n">−{file.deletions}</span>
      {#if file.change === "added"}<span class="tf-chip">new file</span>{/if}
      {#if viewed}
        <label class="tf-fh-viewed"
          ><input type="checkbox" checked={done} onchange={(event) => viewed.set(file.path, event.currentTarget.checked)} /> Viewed</label
        >
      {/if}
      {#if openAt !== null && file.change !== "deleted"}
        <span class="tf-fh-open"
          ><TfOpenObject
            entityId={openAt.entityId}
            object={{ kind: "file", path: file.path, line: firstLine(file), at: openAt.at }}
          /></span
        >
      {/if}
    </div>
    {#if !done}
      {#if split}
        {#each splitRows(file) as row, r (r)}
          {#if row.kind === "gap"}
            <div class="tf-hunk">{row.header}</div>
          {:else}
            <div class="tf-srow">{@render half(file, row.old, "old")}{@render half(file, row.new, "new")}</div>
            {#each under(row) as cell (cell.at)}
              {@render after(file.path, cell.line, placed.under[cell.at] ?? [], targetOf(cell.line).side)}
            {/each}
          {/if}
        {/each}
      {:else}
        {#each unifiedRows(file) as row, r (r)}
          {#if row.kind === "gap"}
            <div class="tf-hunk">{row.header}</div>
          {:else}
            {@const line = row.line}
            <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
              <span class="tf-no">{line.old ?? ""}</span><span class="tf-no">{line.new ?? ""}</span><span
                >{SIGN[line.kind]} {@render code(line.text, tokensOf(file, line.kind === "del" ? "old" : "new", row))}</span
              >
              {@render plus(file.path, line)}
            </div>
            {@render after(file.path, line, placed.under[row.at] ?? [], null)}
          {/if}
        {/each}
      {/if}
      {#if file.truncated}
        <div class="tf-hunk">Cut here: the rest of this file's change is at the source.</div>
      {/if}
      {#if placed.apart.length > 0 || strays.length > 0}
        <details class="tf-outdated">
          <summary>{placed.apart.length + strays.length} outdated</summary>
          {#each placed.apart as thread, t (t)}
            <div class="tf-thread"><span class="tf-dim">{lineName(thread.side, thread.line)}</span>{@render said(thread.comments)}</div>
          {/each}
          {#each strays as d (d.id)}{@render mine(`${lineName(d.side, d.line)}: ${d.body}`)}{/each}
        </details>
      {/if}
    {/if}
  </div>
{/snippet}

<div class="tf-dtool">
  <div class="tf-seg" role="group" aria-label="Layout">
    <button type="button" aria-pressed={layout.chosen === "unified"} onclick={() => layout.set("unified")}>Unified</button>
    <button type="button" aria-pressed={layout.chosen === "split"} onclick={() => layout.set("split")}>Split</button>
  </div>
  {#if layout.chosen === "split" && narrow.current}
    <span class="tf-dim tf-dtool-why">Unified while the window is too narrow for split.</span>
  {/if}
  {#if viewed && files.length > 0}
    {@const count = files.filter((file) => isViewed(file.path)).length}
    <span class="tf-dtool-prog">{count} of {files.length} viewed</span>
  {/if}
</div>
<div class:tf-dgrid={list}>
  {#if list}
    <div class="tf-dnav">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input bind:value={filter} placeholder="Filter changed files" aria-label="Filter changed files" />
      </label>
      <nav class="tf-flist" aria-label="Changed files">
        {#each shown as file (keyOf(file))}
          {@const done = isViewed(file.path)}
          <button type="button" class:tf-viewed={done} title={keyOf(file)} onclick={() => cards[keyOf(file)]?.scrollIntoView({ block: "start" })}
            >{baseName(keyOf(file))}<span class="tf-n"
              ><span class="tf-add-n">+{file.additions}</span> <span class="tf-del-n">−{file.deletions}</span>{done ? " ✓" : ""}</span
            ></button
          >
        {:else}
          <span class="tf-dim tf-flist-none">No file matches.</span>
        {/each}
      </nav>
    </div>
  {/if}
  <div class="tf-dfiles">
    {#each list ? shown : files as file (keyOf(file))}{@render card(file)}{/each}
    {#if gone.length > 0 || homeless.length > 0}
      <details class="tf-outdated tf-gone">
        <summary>On files this change no longer holds</summary>
        {#each gone as g, n (n)}
          <div class="tf-thread"><span class="tf-mono tf-dim">{g.path}, {lineName(g.thread.side, g.thread.line)}</span>{@render said(g.thread.comments)}</div>
        {/each}
        {#each homeless as d (d.id)}
          {@render mine(`${d.path}, ${lineName(d.side, d.line)}: ${d.body}`)}
        {/each}
      </details>
    {/if}
  </div>
</div>
