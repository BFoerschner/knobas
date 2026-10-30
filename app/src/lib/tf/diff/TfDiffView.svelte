<!--
  The one diff component (#2266, spec #2234 step 6a): every diff in the app,
  a commit's, a compare's, the checkout's uncommitted change, a pull
  request's *Files changed* and the edit a *Commit change…* dialog shows,
  drawn from the source's own diff (`DiffFile`). The pull request's file view
  (#842) is its base; the layout is `mockups/code-area/diff.html`.

  - **Toolbar**: Unified / Split, one choice app-wide (`diff-layout`); a
    window too narrow for split shows unified and says so. Where *Viewed* is
    offered, how many files are viewed.
  - **File tree** (`list`, #2268): files under their directories with a
    badge (added, deleted, renamed with similarity, mode, binary) and
    `+N −N`, a filter that narrows the tree and the diffs together, a click
    that scrolls to the file.
  - **Files** (`TfDiffCard`): each collapses; lock files, generated files and
    viewed ones start collapsed. Lines are windowed and a file over 1,500
    changed lines waits for *Load diff* (#2269); changed words are marked and
    context expands where the caller passes `context` (#2267).
  - **Keys** (`list`, #2268): `j`/`k` files, `n`/`p` hunks, `v` viewed, `c`
    comment, `s` split; none while typing, none on a hidden page.
  - **Ceiling** (`cut`): a diff the parser stopped at 50,000 lines says so.
  - **Highlighting**: each side of a file is one text through the renderer
    port (`render`, #2254), so a string or comment that spans hunks is
    highlighted as one. Plain until the tokens arrive.
  - **Capabilities the caller passes in**, absent by default:
    - `viewed`: a *Viewed* box per file that collapses it.
    - `review`: a `+` per line that opens a comment box; *Add to review*
      keeps the comment pending in the reader's store under its line.
    Threads the source holds are drawn wherever it answers them: under their
    line, or apart at the end of the file when outdated or not drawn
    (`pr/files.ts`); `gone` lists those on files the diff no longer holds.
-->
<script lang="ts">
  import type { DiffLine, FileThread, LineComment } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { lineName } from "../../detail/view/pending-comments.svelte";
  import { onMount, untrack } from "svelte";
  import { renderPort, type HighlightJob, type LineTokens, type RenderPort } from "../../render/port";
  import { inTextField } from "../../shell/keys";
  import { ago } from "../../shell/time";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { matchesPath, sameTarget, targetOf } from "../pr/files";
  import {
    DIFF_CEILING,
    EXPAND_STEP,
    hiddenBefore,
    keyOf,
    shownLayout,
    sidesOf,
    type DiffContext,
    type DiffReview,
    type DiffViewed,
    type DrawnFile,
    type DrawnLine,
    type Expanded,
    type Shown,
    type SideText,
    type WholeTexts,
  } from "./diff";
  import { diffLayout, windowNarrow, type DiffLayoutChoice } from "./diff-layout.svelte";
  import type { DiffTalk } from "./talk";
  import TfDiffCard from "./TfDiffCard.svelte";
  import { badgeOf, collapsedAtFirst, isLock, needsLoad, treeOf } from "./tree";

  let {
    files,
    list = true,
    viewed = null,
    review = null,
    gone = [],
    context = null,
    cut = false,
    render = renderPort(),
    layout = diffLayout,
    narrow = windowNarrow(),
  }: {
    /** A file's `label`, where the caller gives one, is drawn in place of its path (`DrawnFile`). */
    files: readonly DrawnFile[];
    /** The file tree and its filter beside the diffs, and the keys. */
    list?: boolean;
    viewed?: DiffViewed | null;
    review?: DiffReview | null;
    /** Threads on files the diff no longer holds. */
    gone?: readonly FileThread[];
    /** Reads a file whole at either side's commit, for expanded context; none, none offered. */
    context?: DiffContext | null;
    /** The parser's ceiling cut this diff. */
    cut?: boolean;
    render?: RenderPort;
    layout?: DiffLayoutChoice;
    narrow?: { readonly current: boolean };
  } = $props();

  let filter = $state("");
  const shown = $derived(files.filter((file) => matchesPath(keyOf(file), filter)));
  const drawn = $derived(list ? shown : files);
  const split = $derived(shownLayout(layout.chosen, narrow.current) === "split");
  const isViewed = (path: string) => viewed?.paths.includes(path) ?? false;

  /** Each file's whole texts, once expanded context read them (by `keyOf`), and the read in flight. */
  let wholes = $state.raw<Record<string, WholeTexts>>({});
  const reads = new Map<string, Promise<WholeTexts>>();
  /** What the reader expanded before each hunk, by file and hunk. */
  let opened = $state.raw<Record<string, Record<number, Shown>>>({});
  const newLines = $derived(new Map(Object.entries(wholes).map(([key, whole]) => [key, whole.new?.split("\n") ?? null])));
  const expandedOf = (key: string): Expanded | undefined => {
    if (!context) return undefined;
    return { lines: newLines.get(key) ?? [], shown: opened[key] ?? {} };
  };

  async function expand(file: DrawnFile, hunk: number, how: "down" | "up" | "all") {
    const key = keyOf(file);
    if (!context) return;
    let whole = wholes[key];
    if (!whole) {
      const reader = context;
      let read = reads.get(key);
      if (!read) {
        read = Promise.all([
          // The old side only highlights: a failed read of it costs that, not the expansion.
          file.change === "added" ? null : reader.text(file, "old").catch(() => null),
          file.change === "deleted" ? null : reader.text(file, "new"),
        ]).then(([old, nu]) => ({ old, new: nu }));
        reads.set(key, read);
      }
      try {
        whole = await read;
      } catch (cause) {
        reads.delete(key);
        push({ text: `Could not read ${file.path}: ${ipcErrorMessage(cause)}`, tone: "err" });
        return;
      }
      wholes = { ...wholes, [key]: whole };
    }
    if (whole.new === null) {
      push({ text: `${file.path} could not be read whole, so its context does not expand.`, tone: "err" });
      return;
    }
    const gap = hiddenBefore(file)[hunk];
    if (!gap) return;
    const was = opened[key]?.[hunk] ?? { top: 0, bottom: 0 };
    const left = Math.max(0, gap.count - was.top - was.bottom);
    const take = Math.min(EXPAND_STEP, left);
    const now = how === "down" ? { ...was, top: was.top + take } : how === "up" ? { ...was, bottom: was.bottom + take } : { ...was, top: was.top + left };
    opened = { ...opened, [key]: { ...opened[key], [hunk]: now } };
  }

  /**
   * Each file's two sides (by `keyOf`), and their tokens as they arrive, under `side\nkey`. A re-read
   * that hands over the same texts (Local changes on every focus, a refreshed commit) keeps its
   * tokens and starts no job; a side whose text changed (a file read whole for its context) drops
   * its tokens and is highlighted anew.
   */
  const sides = $derived(new Map(files.map((file) => [keyOf(file), sidesOf(file, wholes[keyOf(file)])])));
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
  const tokensOf = (file: DrawnFile, side: "old" | "new", cell: DrawnLine) => {
    const key = keyOf(file);
    const both = sides.get(key);
    if (!both) return undefined;
    const number = side === "old" ? cell.line.old : cell.line.new;
    const row = both.whole[side] ? (number === null ? null : number - 1) : cell.at >= 0 ? both.index[cell.at]?.[side] : null;
    if (row === null || row === undefined) return undefined;
    const toks = tokens[`${side}\n${key}`]?.[row];
    return toks && toks.map(([part]) => part).join("") === cell.line.text ? toks : undefined;
  };

  /**
   * Collapsed by hand (`true`) or opened by hand (`false`); otherwise viewed, lock and generated files start
   * collapsed. A generated file over the *Load diff* limit starts open on its *Load diff* line, its collapsed
   * form (ruling on #2252: `src/generated/schema.rs`).
   */
  let folds = $state<Record<string, boolean>>({});
  const isClosed = (file: DrawnFile) =>
    folds[keyOf(file)] ?? (isViewed(file.path) || (collapsedAtFirst(file.path) && (isLock(file.path) || !needsLoad(file))));
  /** Files over the *Load diff* limit the reader loaded. */
  let loads = $state<Record<string, true>>({});

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

  const talk: DiffTalk = {
    get review() {
      return review;
    },
    get drafts() {
      return drafts;
    },
    draftsAt: (path: string, line: DiffLine) => drafts.filter((d) => sameTarget(d, { path, ...targetOf(line) })),
    isOpen: (path: string, line: DiffLine) => open !== null && sameTarget(open, { path, ...targetOf(line) }),
    openAt(path: string, line: DiffLine) {
      open = { path, ...targetOf(line) };
      draft = "";
    },
    get draft() {
      return draft;
    },
    set draft(text: string) {
      draft = text;
    },
    cancel() {
      open = null;
    },
    add: addToReview,
    canReply: (thread) => !!review?.reply && thread.replies && thread.id !== null,
    canResolve: (thread) => !!review?.resolve && thread.id !== null,
    isResolved: (thread) => review?.resolved?.(thread) ?? thread.resolved_by !== null,
    isShown: (thread) => thread.id !== null && shownThreads[thread.id] === true,
    show(thread, on) {
      if (thread.id !== null) shownThreads = { ...shownThreads, [thread.id]: on };
    },
    isReplying: (thread) => thread.id !== null && replying === thread.id,
    openReply(thread) {
      replying = thread.id;
      replyDraft = "";
    },
    get replyDraft() {
      return replyDraft;
    },
    set replyDraft(text: string) {
      replyDraft = text;
    },
    cancelReply() {
      replying = null;
    },
    sendReply(path, thread) {
      const send = review?.reply;
      const body = replyDraft.trim();
      if (!send || body === "") return;
      void send(path, thread, body).then((queued) => {
        if (!queued || replying !== thread.id) return;
        replying = null;
        replyDraft = "";
      });
    },
    resolve(thread, on) {
      void review?.resolve?.(thread, on);
    },
    queuedOn: (thread) => review?.queued?.(thread) ?? [],
  };

  /** The reply box open under one thread (#2277), by the thread's id, and its words. */
  let replying = $state<string | null>(null);
  let replyDraft = $state("");
  /** Resolved threads the reader opened. */
  let shownThreads = $state<Record<string, boolean>>({});
  /** Pending comments written on an earlier head (#2276): kept apart, never sent. */
  const stale = $derived(review ? review.pending.stale(review.entity) : []);

  /** Pending comments on files the diff no longer holds. */
  const homeless = $derived(drafts.filter((d) => !files.some((file) => file.path === d.path)));

  /** Each file's card, for the tree and the keys. */
  type Card = { scrollTo(): void; nextHunk(step: 1 | -1): boolean; comment(): boolean };
  const cards: Record<string, Card | undefined> = $state({});
  /** The file `j`/`k` is on. */
  let current = $state<string | null>(null);
  const tree = $derived(list ? treeOf(shown) : []);

  function go(key: string) {
    current = key;
    cards[key]?.scrollTo();
  }

  function onkey(event: KeyboardEvent) {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (inTextField(event.target) || inTextField(document.activeElement)) return;
    const keys = drawn.map(keyOf);
    if (keys.length === 0) return;
    const at = current === null ? -1 : keys.indexOf(current);
    const here = drawn[Math.max(0, at)]!;
    switch (event.key) {
      case "j":
      case "k": {
        const to = event.key === "j" ? Math.min(keys.length - 1, at + 1) : Math.max(0, at - 1);
        go(keys[to]!);
        break;
      }
      case "n":
      case "p":
        current = keyOf(here);
        cards[current]?.nextHunk(event.key === "n" ? 1 : -1);
        break;
      case "v":
        if (!viewed) return;
        current = keyOf(here);
        setViewed(here, !isViewed(here.path));
        break;
      case "c":
        current = keyOf(here);
        if (!cards[current]?.comment()) return;
        break;
      case "s":
        layout.set(layout.chosen === "split" ? "unified" : "split");
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  const pageShown = pageVisible();
  onMount(() => {
    if (!list) return;
    const keys = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (pageShown()) onkey(event);
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  });

  /** Marks a file viewed or not; a collapse the reader set by hand gives way to it. */
  function setViewed(file: DrawnFile, on: boolean) {
    const { [keyOf(file)]: _, ...rest } = folds;
    folds = rest;
    viewed?.set(file.path, on);
  }
</script>

{#snippet said(comments: readonly LineComment[])}
  {#each comments as comment, n (n)}
    <!-- A line comment's body is the source's Markdown (`LineComment`), drawn by the Markdown component (#2261). -->
    <div class="tf-said">
      <p><b>{comment.author}</b>{comment.at ? ` ${ago(comment.at)}` : ""}</p>
      <TfMarkdown text={comment.body} {render} />
    </div>
  {/each}
{/snippet}

{#snippet mine(body: string)}
  <div class="tf-thread tf-pending"><p><b>You</b> <span class="tf-chip tf-warn">pending</span> {body}</p></div>
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
{#if cut}
  <p class="tf-dcut">
    This diff is over knobas's ceiling of {DIFF_CEILING.toLocaleString("en-US")} lines: it is drawn up to there, and the source's own page has the rest.
  </p>
{/if}
<div class:tf-dgrid={list}>
  {#if list}
    <div class="tf-dnav">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input bind:value={filter} placeholder="Filter changed files" aria-label="Filter changed files" />
      </label>
      <nav class="tf-flist" aria-label="Changed files">
        {#each tree as row (row.kind === "dir" ? `dir\n${row.path}` : keyOf(row.file))}
          {#if row.kind === "dir"}
            <div class="tf-fdir">{row.path}</div>
          {:else}
            {@const file = row.file}
            {@const done = isViewed(file.path)}
            {@const badge = badgeOf(file)}
            <button
              type="button"
              class:tf-viewed={done}
              class:tf-d1={row.depth === 1}
              aria-current={current === keyOf(file) ? "true" : undefined}
              title={keyOf(file)}
              onclick={() => go(keyOf(file))}
              ><span class="tf-bd tf-bd-{badge[0] === '+' || badge[0] === '−' ? 'X' : badge[0]}">{badge}</span><span class="tf-fname">{row.name}</span><span class="tf-n"
                >{#if file.change !== "binary"}<span class="tf-add-n">+{file.additions}</span> <span class="tf-del-n">−{file.deletions}</span>{/if}{done ? " ✓" : ""}</span
              ></button
            >
          {/if}
        {:else}
          <span class="tf-dim tf-flist-none">No file matches.</span>
        {/each}
      </nav>
      <p class="tf-dkeys tf-dim">
        <span><kbd>j</kbd><kbd>k</kbd> files</span><span><kbd>n</kbd><kbd>p</kbd> hunks</span>{#if viewed}<span><kbd>v</kbd> viewed</span>{/if}{#if review}<span
            ><kbd>c</kbd> comment</span
          >{/if}<span><kbd>s</kbd> split</span>
      </p>
    </div>
  {/if}
  <div class="tf-dfiles">
    {#each drawn as file (keyOf(file))}
      {@const key = keyOf(file)}
      <TfDiffCard
        bind:this={cards[key]}
        {file}
        {split}
        closed={isClosed(file)}
        ontoggle={() => (folds = { ...folds, [key]: !isClosed(file) })}
        viewed={viewed !== null}
        done={isViewed(file.path)}
        onviewed={(on) => setViewed(file, on)}
        loaded={loads[key] === true}
        onload={() => (loads = { ...loads, [key]: true })}
        current={current === key}
        expandable={context !== null}
        expanded={expandedOf(key)}
        onexpand={(hunk, how) => void expand(file, hunk, how)}
        tokensOf={(side, cell) => tokensOf(file, side, cell)}
        {talk}
        {render}
      />
    {/each}
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
    {#if review && stale.length > 0}
      <!-- #2276: drafts written on an earlier head count their lines on a head that is gone, so no review sends them. -->
      <details class="tf-outdated tf-stale" open>
        <summary>{stale.length} of your pending {stale.length === 1 ? "comment was" : "comments were"} written on an earlier head</summary>
        {#each stale as d (d.id)}
          <div class="tf-thread tf-pending">
            <p>
              <b>You</b> <span class="tf-chip tf-warn">pending</span>
              <span class="tf-mono tf-dim">{d.path}, {lineName(d.side, d.line)}{d.head ? ` at ${d.head.slice(0, 7)}` : ""}</span>
              {d.body}
            </p>
            <div class="tf-acts">
              <button type="button" class="tf-btn tf-sm tf-ghost tf-danger" onclick={() => review.pending.discard(review.entity, d.id)}>Discard</button>
            </div>
          </div>
        {/each}
      </details>
    {/if}
  </div>
</div>
