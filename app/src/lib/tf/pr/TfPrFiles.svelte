<!--
  *Files changed* (#842, R-8.17), prototype-6's `renderPR(n, 'diff')`: a
  file list on the left with a filter, each file with `+N −N` and a tick once
  viewed (a click scrolls to it); on the right one diff per file in the
  source's hunks, `⋯` between them.

  - *Viewed* per file collapses it. It is knobas-local, kept by pull request
    and head commit (`pr_viewed_files`, `set_pr_file_viewed`): a push to the
    head brings every file back.
  - Each thread the source holds sits under its line; an outdated one, or one
    whose line is not drawn, is kept apart at the end of its file, closed
    (`files.ts`). A thread on a file the diff no longer holds is listed at the
    end.
  - Where the source takes comments (`commentable`), each line offers a `+` on
    hover or focus that opens a box; *Add to review* keeps the comment
    **pending** (`detail/view/pending-comments`, this viewer's) with a warn
    edge and a *pending* chip, until *Submit review* sends it.

  The diff is the *Files* section, read once per pull request when the tab
  opens (the whole view defers it, ADR-0020 rule 3).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { DiffFile, DiffLine, SourceView } from "../../ipc/entity";
  import { lineName, type PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import { diffOf, matchesPath, placeThreads, sameTarget, targetOf } from "./files";

  let {
    prId,
    entityView,
    viewedFiles,
    setViewed,
    pending,
  }: {
    prId: string;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    viewedFiles: (prId: string) => Promise<string[]>;
    setViewed: (prId: string, path: string, viewed: boolean) => Promise<string[]>;
    /** The reader's pending line comments. */
    pending: PendingCommentStore;
  } = $props();

  /** `undefined` while the *Files* read has not answered. */
  let view = $state<SourceView | null | undefined>(undefined);
  let failed = $state<string | null>(null);
  let viewed = $state<string[]>([]);
  let filter = $state("");

  $effect(() => {
    const id = prId;
    let live = true;
    view = undefined;
    failed = null;
    viewed = [];
    entityView(id, "files").then(
      (answer) => {
        if (live) view = answer;
      },
      (error) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    viewedFiles(id).then(
      (files) => {
        if (live) viewed = files;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  const diff = $derived(view ? diffOf(view) : null);
  const files = $derived(diff?.files ?? []);
  const shown = $derived(files.filter((file) => matchesPath(file.path, filter)));
  const drafts = $derived(pending.of(prId));

  /** Each file's card, to scroll to from the list. */
  const cards: Record<string, HTMLElement | null> = $state({});

  function jump(path: string) {
    cards[path]?.scrollIntoView({ block: "start" });
  }

  async function toggle(path: string, on: boolean) {
    try {
      viewed = await setViewed(prId, path, on);
    } catch (cause) {
      push({ text: `Could not mark ${path}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** The line whose comment box is open: its file and its place. */
  let open = $state<{ path: string; side: "old" | "new"; line: number } | null>(null);
  let draft = $state("");

  function startComment(path: string, line: DiffLine) {
    open = { path, ...targetOf(line) };
    draft = "";
  }

  function addToReview() {
    if (open === null) return;
    if (pending.add(prId, { ...open, body: draft }) === null) return;
    open = null;
    draft = "";
    push({ text: "Added to your review. Nothing is sent until you submit it." });
  }

  const SIGN = { context: " ", add: "+", del: "−" } as const;
  const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const isOpen = (path: string, line: DiffLine) => open !== null && sameTarget(open, { path, ...targetOf(line) });
  const draftsAt = (path: string, line: DiffLine) => drafts.filter((d) => sameTarget(d, { path, ...targetOf(line) }));
  /** A line's name as a comment on it says it: `line 12`, `old line 9`. */
  const nameOf = (line: DiffLine) => {
    const at = targetOf(line);
    return lineName(at.side, at.line);
  };
  /** Pending comments on files the diff no longer holds. */
  const homeless = $derived(drafts.filter((d) => !files.some((file) => file.path === d.path)));
  const flat = (file: DiffFile) => file.hunks.flatMap((hunk, h) => hunk.lines.map((line, n) => ({ line, gap: h > 0 && n === 0 })));
</script>

{#snippet said(comments: { author: string; body: string; at: string | null }[])}
  {#each comments as comment, n (n)}
    <p><b>{comment.author}</b>{comment.at ? ` ${ago(comment.at)}` : ""}: {comment.body}</p>
  {/each}
{/snippet}

{#snippet mine(body: string)}
  <div class="tf-thread tf-pending"><p><b>You</b> <span class="tf-chip tf-warn">pending</span> {body}</p></div>
{/snippet}

{#if failed !== null}
  <p class="tf-cfail">Could not read the changed files: {failed}</p>
{:else if view === null}
  <div class="tf-empty">This source draws no changed files for a pull request.</div>
{:else if view !== undefined}
  <div class="tf-prfiles">
    <div class="tf-prfiles-nav">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input bind:value={filter} placeholder="Filter changed files" aria-label="Filter changed files" />
      </label>
      <nav class="tf-flist" aria-label="Changed files">
        {#each shown as file (file.path)}
          {@const done = viewed.includes(file.path)}
          <button type="button" class:tf-viewed={done} title={file.path} onclick={() => jump(file.path)}
            >{baseName(file.path)}<span class="tf-n"
              ><span class="tf-add-n">+{file.additions}</span> <span class="tf-del-n">−{file.deletions}</span>{done ? " ✓" : ""}</span
            ></button
          >
        {:else}
          <span class="tf-dim tf-flist-none">No file matches.</span>
        {/each}
      </nav>
    </div>
    <div>
      {#if files.length === 0}
        <div class="tf-empty">This pull request changes no files.</div>
      {/if}
      {#each shown as file (file.path)}
        {@const done = viewed.includes(file.path)}
        {@const placed = placeThreads(file)}
        {@const lines = flat(file)}
        {@const strays = drafts.filter((d) => d.path === file.path && !lines.some(({ line }) => draftsAt(file.path, line).includes(d)))}
        <div class="tf-diff tf-prdiff" class:tf-viewed={done} bind:this={cards[file.path]}>
          <div class="tf-fh">
            <span class="tf-mono">{file.old_path && file.old_path !== file.path ? `${file.old_path} → ${file.path}` : file.path}</span>
            <span class="tf-add-n">+{file.additions}</span>
            <span class="tf-del-n">−{file.deletions}</span>
            {#if file.change === "added"}<span class="tf-chip">new file</span>{/if}
            <label class="tf-fh-viewed"
              ><input type="checkbox" checked={done} onchange={(event) => void toggle(file.path, event.currentTarget.checked)} /> Viewed</label
            >
          </div>
          {#if !done}
            {#each lines as { line, gap }, n (n)}
              {@const number = line.kind === "del" ? line.old : line.new}
              {#if gap}<div class="tf-ln tf-hunk"><span></span><span>⋯</span><span></span></div>{/if}
              <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
                <span>{number}</span><span>{SIGN[line.kind]} {line.text}</span>
                {#if diff?.commentable}
                  {@const name = nameOf(line)}
                  <button type="button" class="tf-addc" title="Comment on {name}" aria-label="Comment on {name} of {file.path}" onclick={() => startComment(file.path, line)}
                    >+</button
                  >
                {:else}<span></span>{/if}
              </div>
              {#each placed.under[n] ?? [] as thread, t (t)}
                <div class="tf-thread">{@render said(thread.comments)}</div>
              {/each}
              {#each draftsAt(file.path, line) as d (d.id)}{@render mine(d.body)}{/each}
              {#if isOpen(file.path, line)}
                {@const name = nameOf(line)}
                <div class="tf-newc">
                  <textarea bind:value={draft} placeholder="Comment on {name}" aria-label="Comment on {name} of {file.path}"></textarea>
                  <div class="tf-acts">
                    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (open = null)}>Cancel</button>
                    <button type="button" class="tf-btn tf-sm tf-primary" disabled={draft.trim() === ""} onclick={addToReview}>Add to review</button>
                  </div>
                </div>
              {/if}
            {/each}
            {#if file.truncated}
              <div class="tf-ln tf-hunk"><span></span><span>Cut here: the rest of this file's change is at the source.</span><span></span></div>
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
      {/each}
      {#if (diff?.gone.length ?? 0) > 0 || homeless.length > 0}
        <details class="tf-outdated tf-gone">
          <summary>On files this change no longer holds</summary>
          {#each diff?.gone ?? [] as gone, g (g)}
            <div class="tf-thread"><span class="tf-mono tf-dim">{gone.path}, {lineName(gone.thread.side, gone.thread.line)}</span>{@render said(gone.thread.comments)}</div>
          {/each}
          {#each homeless as d (d.id)}
            {@render mine(`${d.path}, ${lineName(d.side, d.line)}: ${d.body}`)}
          {/each}
        </details>
      {/if}
      {#if diff?.truncated}
        <p class="tf-dim tf-prfiles-cut">The source cut this diff; the rest is on its own page.</p>
      {/if}
    </div>
  </div>
{/if}
