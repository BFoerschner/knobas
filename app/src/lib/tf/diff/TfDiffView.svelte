<!--
  The one diff component (#2266, spec #2234 step 6a): every diff in the app,
  a commit's, a compare's, the checkout's uncommitted change, a pull
  request's *Files changed* and the edit a *Commit change…* dialog shows,
  drawn from the source's own diff (`DiffFile`). The pull request's file view
  (#842) is its base; the layout is `mockups/code-area/diff.html`.

  - **Toolbar**: Unified / Split, one choice app-wide (`diff-layout`); a
    column with too little room for split presses *Unified* and disables
    *Split* with a title, and `s` says the same. A caller's own control sits
    beside them (`tools`). Where *Viewed* is offered, how many files are viewed.
  - **File tree** (`list`, #2268): files under their directories with a
    badge (added, deleted, renamed with similarity, mode, binary) and
    `+N −N`, a filter that narrows the tree and the diffs together, a click
    that scrolls to the file and opens it if it is collapsed. The diffs are
    drawn in the tree's order, so `j` walks the rail down. The pill follows the
    scroll; a pick holds it until the reader scrolls, and the scroll's end marks
    the last file. Where the diff has too little width for it, the tree folds
    behind one bar (`TfRailBar`), and a pick folds it again.
  - **Files** (`TfDiffCard`): each collapses; lock files, generated files and
    viewed ones start collapsed. Lines are windowed and a file over 1,500
    changed lines waits for *Load diff* (#2269); changed words are marked and
    context expands where the caller passes `context` (#2267).
  - **Rendered** (#2279): a file whose renderer draws diffs (`rendered.ts`)
    offers *Rendered* in its header, the file drawn before and after by its
    renderer (`TfDiffRendered`); an image also swipes. Its sides come from
    `context`, read once and kept beside expanded context's reads.
  - **File pipelines** (#2270): where the caller runs pipelines (`context.piped`)
    and a file's pattern has a default pipeline (`pipelines`), both sides
    run through it and the outputs are diffed (`piped.ts`); the header names
    the pipeline. *Run pipelines on diffs* (`piping`, on by default) decides
    unless the toolbar's *Run pipelines* (this diff) or the file's own *Show
    raw* / *Run pipeline* says otherwise. A piped diff takes no comments and
    does not expand: its lines are not the file's. A side that fails draws
    the raw diff under the step's error.
  - **Keys** (`list`, #2268): `j`/`k` files, `n`/`p` hunks, on across files
    past a collapsed one, `v` viewed, `c` comment, on the hunk `n` reached or
    in the next open file, `s` split; none while typing, none on a hidden page.
  - **Ceiling** (`cut`): a diff the parser stopped at 50,000 lines says so.
  - **Highlighting**: each side of a file is one text through the renderer
    port (`render`, #2254), so a string or comment that spans hunks is
    highlighted as one. Plain until the tokens arrive.
  - **Capabilities the caller passes in**, absent by default:
    - `viewed`: a *Viewed* box per file that collapses it.
    - `review`: a `+` per line that opens a comment box, which takes focus;
      *Add to review* keeps the comment pending in the reader's store under
      its line. `Esc` closes the box and keeps what it holds for that line.
    - `openAt`: *Open in…* in each file's header (#2274), on the file at its
      first changed line, read at the diff's head; not on a deleted file.
    Threads the source holds are drawn wherever it answers them: under their
    line, or apart at the end of the file when outdated or not drawn
    (`pr/files.ts`); `gone` lists those on files the diff no longer holds.
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import type { DiffLine, FileThread, LineComment } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { lineName } from "../../detail/view/pending-comments.svelte";
  import { flushSync, onMount, untrack, type Snippet } from "svelte";
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
  import { diffLayout, SPLIT_ROOM, type DiffLayoutChoice } from "./diff-layout.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import type { DiffTalk } from "./talk";
  import TfDiffCard from "./TfDiffCard.svelte";
  import { ipcCommands, type CommandPort } from "../../render/command";
  import { openWith, type RendererDefault } from "../../render/defaults";
  import { rendererDefaults } from "../../render/defaults.svelte";
  import { allRenderers } from "../../render/registry";
  import { hunkText, renderedPlan, type DiffMode, type Side, type SideGot } from "./rendered";
  import type { OpenAt } from "./TfDiffCard.svelte";
  import { badgeOf, collapsedAtFirst, isLock, needsLoad, treeOf } from "./tree";
  import { diffPipelines, type DiffPipelinesSetting } from "./diff-pipelines.svelte";
  import { defaultPipelines, pipedDiff, pipelineLabel, type DiffPipeline, type DiffPipelines, type PipedDiff } from "./piped";
  import TfWrittenAt from "./TfWrittenAt.svelte";

  let {
    files,
    list = true,
    viewed = null,
    review = null,
    gone = [],
    context = null,
    cut = false,
    render = renderPort(),
    defaults = rendererDefaults,
    commands = ipcCommands,
    layout = diffLayout,
    narrow = { current: false },
    openAt = null,
    repository = null,
    pipelines = defaultPipelines,
    piping = diffPipelines,
    tools = null,
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
    /** The renderer each file opens with (#2263): its rendered diff is drawn by it. */
    defaults?: { readonly list: readonly RendererDefault[] };
    /** Runs a command renderer's rendered diff: the IPC, a fake in tests. */
    commands?: CommandPort;
    layout?: DiffLayoutChoice;
    /** Says there is too little room for split, for a test: nothing is laid out there to measure. */
    narrow?: { readonly current: boolean };
    /** *Open in…* in each file's header (#2274): the repository (or branch) and the commit or branch the files are read at. */
    openAt?: OpenAt | null;
    /** The repository the diff is in (its range's): an outdated thread's *at <sha>* links that commit's diff (#2318); none, the sha is plain. */
    repository?: string | null;
    /** Each file's default pipeline (#2270): the app's lookup by default. Runs only where `context.piped` is given. */
    pipelines?: DiffPipelines;
    /** *Run pipelines on diffs*: the app's setting by default. */
    piping?: Pick<DiffPipelinesSetting, "on">;
    /** A caller's own control in the toolbar, beside the layout: *Ignore whitespace*. */
    tools?: Snippet | null;
  } = $props();

  let filter = $state("");
  const shown = $derived(files.filter((file) => matchesPath(keyOf(file), filter)));
  const tree = $derived(list ? treeOf(shown) : []);
  /** The rail's order, so a file's place in the list is its place on the page. */
  const drawn = $derived(list ? tree.flatMap((row) => (row.kind === "file" ? [row.file] : [])) : files);
  /** The width the diffs have, once measured. */
  let room = $state(Number.POSITIVE_INFINITY);
  const tight = $derived(narrow.current || room < SPLIT_ROOM);
  const split = $derived(shownLayout(layout.chosen, tight) === "split");
  /** Why *Split* can't be pressed, and what `s` says then. */
  const NO_ROOM = "There is too little room here for split. A wider window has it.";
  const uid = $props.id();
  /** Whether the file list is unfolded under its bar, where it has one. */
  let railOpen = $state(false);

  /** Reads the column's width; a hidden page has none to read, so it keeps the last. */
  function measure(node: HTMLElement) {
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(([entry]) => {
      if (entry !== undefined && entry.contentRect.width > 0) room = entry.contentRect.width;
    });
    watch.observe(node);
    return () => watch.disconnect();
  }
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

  /** Reads a file's two sides whole, once: expanded context and its rendered diff share the read. */
  async function readWhole(file: DrawnFile, reader: DiffContext): Promise<WholeTexts> {
    const key = keyOf(file);
    const had = wholes[key];
    if (had) return had;
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
      const whole = await read;
      wholes = { ...wholes, [key]: whole };
      return whole;
    } catch (cause) {
      reads.delete(key);
      throw cause;
    }
  }

  async function expand(file: DrawnFile, hunk: number, how: "down" | "up" | "all") {
    if (!context) return;
    let whole: WholeTexts;
    try {
      whole = await readWhole(file, context);
    } catch (cause) {
      push({ text: `Could not read ${file.path}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return;
    }
    const key = keyOf(file);
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

  /** Each file's pipeline (by `keyOf`), where this diff runs pipelines and its pattern has one. */
  const pipes = $derived(new Map(files.map((file) => [keyOf(file), context?.piped ? pipelines.of(file.path) : null] as const)));
  const anyPipe = $derived([...pipes.values()].some((pipe) => pipe !== null));
  /** This diff's toggle (`null`: the setting decides) and each file's own (absent: the diff's decides). */
  let pipeAll = $state<boolean | null>(null);
  let pipeOne = $state.raw<Record<string, boolean>>({});
  const pipeOn = (key: string) => pipeOne[key] ?? pipeAll ?? piping.on;
  /** Each file's piped diff, under `key\nsteps`, once both sides answered; and the runs asked. */
  let runs = $state.raw<Record<string, PipedDiff>>({});
  const running = new Set<string>();
  const runName = (key: string, pipe: DiffPipeline) => `${key}\n${JSON.stringify(pipe.steps)}`;

  function runPipe(file: DrawnFile, pipe: DiffPipeline) {
    const name = runName(keyOf(file), pipe);
    const reader = context;
    if (!reader?.piped || running.has(name)) return;
    running.add(name);
    const one = async (side: Side) => {
      if (side === "old" ? file.change === "added" : file.change === "deleted") return { side, run: null };
      try {
        return { side, run: await reader.piped!(file, side, pipe.steps) };
      } catch (cause) {
        return { side, why: `reading it: ${ipcErrorMessage(cause)}` };
      }
    };
    void Promise.all([one("old"), one("new")]).then(([old, nu]) => {
      const refused = [old, nu].find((got) => "why" in got);
      const made: PipedDiff =
        refused && "why" in refused ? { kind: "failed", side: refused.side, why: refused.why } : pipedDiff(file, "run" in old ? old.run : null, "run" in nu ? nu.run : null);
      // A refused read is asked again the next time the pipeline is turned on.
      if (refused) running.delete(name);
      runs = { ...runs, [name]: made };
    });
  }
  $effect(() => {
    for (const file of files) {
      const pipe = pipes.get(keyOf(file));
      if (pipe && pipeOn(keyOf(file))) untrack(() => runPipe(file, pipe));
    }
  });
  /** A file's pipeline as its header shows it: on or off, running, or the side that failed. */
  const pipeShown = (file: DrawnFile) => {
    const key = keyOf(file);
    const pipe = pipes.get(key);
    if (!pipe) return null;
    const on = pipeOn(key);
    const got = on ? runs[runName(key, pipe)] : undefined;
    // A piped diff draws no thread or draft (`quiet`): they sit on the raw lines. The header counts them instead.
    const hidden = file.threads.length + drafts.filter((d) => d.path === file.path).length;
    return { label: pipelineLabel(pipe), pattern: pipe.pattern, on, running: on && got === undefined, failed: got?.kind === "failed" ? got : null, hidden };
  };
  /** The piped diff a file draws in place of its own, where its pipeline is on and ran on both sides. */
  const pipedOf = (file: DrawnFile) => {
    const key = keyOf(file);
    const pipe = pipes.get(key);
    const got = pipe && pipeOn(key) ? runs[runName(key, pipe)] : undefined;
    return got?.kind === "diff" ? got : null;
  };

  /**
   * Each file's two sides (by `keyOf`), and their tokens as they arrive, under `side\nkey`. A re-read
   * that hands over the same texts (Local changes on every focus, a refreshed commit) keeps its
   * tokens and starts no job; a side whose text changed (a file read whole for its context) drops
   * its tokens and is highlighted anew.
   */
  const sides = $derived(
    new Map(
      files.map((file) => {
        const piped = pipedOf(file);
        if (piped === null) return [keyOf(file), sidesOf(file, wholes[keyOf(file)])] as const;
        // A piped diff is highlighted as its output: `events.json.b64` decoded is JSON.
        const both = sidesOf(piped.file);
        return [keyOf(file), { ...both, old: { ...both.old, path: piped.name }, new: { ...both.new, path: piped.name } }] as const;
      }),
    ),
  );
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

  /** Each file's rendered mode, where its renderer draws diffs and the sides can be read (by `keyOf`). */
  const plans = $derived(
    new Map(
      files.map((file) => {
        const opened = openWith({ path: file.path, binary: file.change === "binary" }, allRenderers(), defaults.list);
        return [keyOf(file), renderedPlan(file, opened.renderer, { whole: context !== null, bytes: typeof context?.bytes === "function" })] as const;
      }),
    ),
  );
  /** The mode the reader picked per file; a binary file with a rendered mode opens in it (it has no lines). */
  let modes = $state.raw<Record<string, DiffMode>>({});
  const modeOf = (file: DrawnFile): DiffMode => modes[keyOf(file)] ?? (plans.get(keyOf(file)) && file.change === "binary" ? "rendered" : "lines");
  /** Each side a rendered diff read, under `side\nkey`, and the reads made. */
  let got = $state.raw<Record<string, SideGot>>({});
  const asked = new Set<string>();
  const NAMES = { old: "before", new: "after" } as const;

  function loadSide(file: DrawnFile, side: Side) {
    const name = `${side}\n${keyOf(file)}`;
    const reader = context;
    if (asked.has(name)) return;
    asked.add(name);
    const read = async (): Promise<SideGot> => {
      const hunk = hunkText(file, side);
      if (hunk !== null) return { kind: "drawn", content: hunk };
      if (reader === null) return { kind: "failed", why: "nothing reads this diff's files" };
      if (file.change === "binary") {
        const bytes = await reader.bytes!(file, side);
        return bytes === null ? { kind: "absent" } : { kind: "drawn", content: bytes };
      }
      const text = (await readWhole(file, reader))[side];
      return text === null ? { kind: "failed", why: `the file ${NAMES[side]} the change is not readable whole` } : { kind: "drawn", content: text };
    };
    read()
      .catch((cause): SideGot => {
        asked.delete(name);
        return { kind: "failed", why: ipcErrorMessage(cause) };
      })
      .then((side) => (got = { ...got, [name]: side }));
  }
  const sideOf = (file: DrawnFile, side: Side): SideGot | undefined => got[`${side}\n${keyOf(file)}`];

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
  /** What was typed in a box that went without *Add to review* or *Cancel*, by its line, so its words come back. */
  const unsent = new Map<string, string>();
  const spot = (at: { path: string; side: string; line: number }) => `${at.path}\n${at.side}\n${at.line}`;

  /** Puts the open box's words by, as it goes or another line takes the box. */
  function keepDraft() {
    if (open === null) return;
    if (draft.trim() === "") unsent.delete(spot(open));
    else unsent.set(spot(open), draft);
  }

  function addToReview() {
    if (open === null || review === null) return;
    if (review.pending.add(review.entity, { ...open, body: draft }) === null) return;
    unsent.delete(spot(open));
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
      keepDraft();
      open = { path, ...targetOf(line) };
      draft = unsent.get(spot(open)) ?? "";
    },
    get draft() {
      return draft;
    },
    set draft(text: string) {
      draft = text;
    },
    close() {
      keepDraft();
      open = null;
    },
    cancel() {
      if (open !== null) unsent.delete(spot(open));
      open = null;
      draft = "";
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
      keepReply();
      replying = thread.id;
      replyDraft = (thread.id !== null && unsentReplies.get(thread.id)) || "";
    },
    get replyDraft() {
      return replyDraft;
    },
    set replyDraft(text: string) {
      replyDraft = text;
    },
    closeReply() {
      keepReply();
      replying = null;
    },
    cancelReply() {
      if (replying !== null) unsentReplies.delete(replying);
      replying = null;
      replyDraft = "";
    },
    sendReply(path, thread) {
      const send = review?.reply;
      const body = replyDraft.trim();
      if (!send || body === "") return;
      void send(path, thread, body).then((queued) => {
        if (!queued || replying !== thread.id) return;
        if (thread.id !== null) unsentReplies.delete(thread.id);
        replying = null;
        replyDraft = "";
      });
    },
    resolve(thread, on) {
      void review?.resolve?.(thread, on);
    },
    queuedOn: (thread) => review?.queued?.(thread) ?? [],
  };
  /** The talk of a piped diff: its lines are the pipeline's, so it takes no comment and draws no draft. */
  const quiet: DiffTalk = Object.create(talk, {
    review: { get: () => null },
    drafts: { get: () => [] },
    draftsAt: { value: () => [] },
    isOpen: { value: () => false },
  });

  /** The reply box open under one thread (#2277), by the thread's id, and its words. */
  let replying = $state<string | null>(null);
  let replyDraft = $state("");
  const unsentReplies = new Map<string, string>();

  function keepReply() {
    if (replying === null) return;
    if (replyDraft.trim() === "") unsentReplies.delete(replying);
    else unsentReplies.set(replying, replyDraft);
  }
  /** Resolved threads the reader opened. */
  let shownThreads = $state<Record<string, boolean>>({});
  /** Pending comments written on an earlier head (#2276): kept apart, never sent. */
  const stale = $derived(review ? review.pending.stale(review.entity) : []);

  /** Pending comments on files the diff no longer holds. */
  const homeless = $derived(drafts.filter((d) => !files.some((file) => file.path === d.path)));

  /** Each file's card, for the tree and the keys. */
  type Card = { scrollTo(): void; nextHunk(step: 1 | -1, entering?: boolean): boolean; comment(): boolean };
  const cards: Record<string, Card | undefined> = $state({});
  /** The file `j`/`k` is on. */
  let current = $state<string | null>(null);
  /** The page's scroller, and where it stood when a press chose the file: the choice stands until the reader scrolls. */
  let scroller: Element | null = null;
  let heldAt: number | null = null;
  const hold = () => (heldAt = scroller?.scrollTop ?? null);
  /** A difference this small is the layout settling, not the reader scrolling. */
  const SCROLLED = 2;
  const nameOf = (key: string) => key.slice(key.lastIndexOf("/") + 1);

  function go(key: string) {
    current = key;
    cards[key]?.scrollTo();
    hold();
  }

  /** A file picked in the list: it opens if it was collapsed, and the list folds again, before the page moves to it. */
  function pick(file: DrawnFile) {
    const key = keyOf(file);
    railOpen = false;
    if (isClosed(file)) folds = { ...folds, [key]: false };
    flushSync();
    go(key);
  }

  /**
   * The file rail follows the scroll: the file whose head has reached the top of the scroller is the one you are in,
   * and at the scroll's end the last, whose head may never reach the top.
   */
  function spy(host: HTMLElement) {
    const found = host.closest(".tf-main, .tf-bmain");
    scroller = found;
    if (found === null) return;
    const read = () => {
      if (heldAt !== null && Math.abs(found.scrollTop - heldAt) < SCROLLED) return;
      heldAt = null;
      const last = drawn.at(-1);
      if (last !== undefined && found.scrollHeight > found.clientHeight && found.scrollTop + found.clientHeight >= found.scrollHeight - SCROLLED) {
        current = keyOf(last);
        return;
      }
      const top = found.getBoundingClientRect().top + 8;
      let at: string | null = null;
      for (const file of drawn) {
        const key = keyOf(file);
        const card = host.querySelector<HTMLElement>(`[data-file="${CSS.escape(key)}"]`);
        if (card === null) continue;
        if (card.getBoundingClientRect().top > top) break;
        at = key;
      }
      const first = drawn[0];
      current = at ?? (first === undefined ? null : keyOf(first));
    };
    read();
    found.addEventListener("scroll", read, { passive: true });
    return () => found.removeEventListener("scroll", read);
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
      case "p": {
        // Past this file's last hunk, or from a file with no lines drawn, on to the next file that has one.
        const step = event.key === "n" ? 1 : -1;
        const from = Math.max(0, at);
        for (let i = from; i >= 0 && i < keys.length; i += step) {
          if (!cards[keys[i]!]?.nextHunk(step, i !== from)) continue;
          current = keys[i]!;
          hold();
          break;
        }
        break;
      }
      case "v":
        if (!viewed) return;
        current = keyOf(here);
        setViewed(here, !isViewed(here.path));
        break;
      case "c": {
        if (!review) return;
        // The file `j` or `n` reached, else the next one with lines to comment on: a collapsed file has none.
        const from = Math.max(0, at);
        const taker = [...keys.slice(from), ...keys.slice(0, from)].find((key) => cards[key]?.comment());
        if (taker === undefined) {
          push({ text: "No open file takes a comment here. Open one, or show its raw diff, then press c." });
          break;
        }
        current = taker;
        hold();
        break;
      }
      case "s":
        if (tight) push({ text: NO_ROOM });
        else layout.set(layout.chosen === "split" ? "unified" : "split");
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
  <div class="tf-thread tf-pending"><p><b>You</b> <TfStateChip text="pending" tone="warn" /> {body}</p></div>
{/snippet}

<div class="tf-dview">
<div class="tf-dtool">
  <div class="tf-seg" role="group" aria-label="Layout">
    <button type="button" aria-pressed={!split} onclick={() => layout.set("unified")}>Unified</button>
    <button
      type="button"
      aria-pressed={split}
      aria-disabled={tight ? "true" : undefined}
      title={tight ? NO_ROOM : undefined}
      onclick={() => {
        if (!tight) layout.set("split");
      }}>Split</button
    >
  </div>
  {@render tools?.()}
  {#if anyPipe}
    {@const all = pipeAll ?? piping.on}
    <div class="tf-fchips">
      <button
        type="button"
        aria-pressed={all}
        title="Run each file's default pipeline on both sides of this diff"
        onclick={() => {
          pipeAll = !all;
          pipeOne = {};
        }}>Run pipelines</button
      >
    </div>
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
<div class:tf-dgrid={list} class:tf-rail-shut={list && !railOpen}>
  {#if list}
    <TfRailBar label="File" value={current === null ? "None" : nameOf(current)} open={railOpen} controls="{uid}-files" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} />
    <div class="tf-dnav tf-rail" id="{uid}-files">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input bind:value={filter} placeholder="Filter changed files" aria-label="Filter changed files" />
      </label>
      {#if tree.length === 0}
        <div class="tf-empty tf-flist-none">
          <p>No files match.</p>
          <button type="button" class="tf-btn tf-sm" onclick={() => (filter = "")}>Clear the filter</button>
        </div>
      {:else}
      <nav class="tf-flist" aria-label="Changed files">
        {#each tree as row (row.kind === "dir" ? `dir\n${row.path}` : keyOf(row.file))}
          {#if row.kind === "dir"}
            <div class="tf-fdir">{row.path}</div>
          {:else}
            {@const file = row.file}
            {@const done = isViewed(file.path)}
            {@const badge = badgeOf(file)}
            {@const how = /, (not staged|staged)\)$/.exec(row.name)?.[1] ?? null}
            <button
              type="button"
              class:tf-viewed={done}
              class:tf-d1={row.depth === 1}
              aria-current={current === keyOf(file) ? "true" : undefined}
              title={keyOf(file)}
              onclick={() => pick(file)}
              ><span class="tf-bd tf-bd-{badge[0] === '+' || badge[0] === '−' ? 'X' : badge[0]}">{badge}</span><span class="tf-fname">{how === null ? row.name : row.name.slice(0, row.name.lastIndexOf(" ("))}</span>{#if how !== null}<span class="tf-state">{how}</span>{/if}<span class="tf-n"
                >{#if file.change !== "binary"}<span class="tf-add-n">+{file.additions}</span> <span class="tf-del-n">−{file.deletions}</span>{/if}{done ? " ✓" : ""}</span
              ></button
            >
          {/if}
        {/each}
      </nav>
      {/if}
      <p class="tf-dkeys tf-dim">
        <span><kbd>j</kbd><kbd>k</kbd> files</span><span><kbd>n</kbd><kbd>p</kbd> hunks</span>{#if viewed}<span><kbd>v</kbd> viewed</span>{/if}{#if review}<span
            ><kbd>c</kbd> comment</span
          >{/if}<span><kbd>s</kbd> split</span>
      </p>
    </div>
  {/if}
  <div class="tf-dfiles" {@attach spy} {@attach measure}>
    {#each drawn as file (keyOf(file))}
      {@const key = keyOf(file)}
      {@const piped = pipedOf(file)}
      <TfDiffCard
        bind:this={cards[key]}
        file={piped?.file ?? file}
        pipeline={pipeShown(file)}
        onpipeline={(on) => (pipeOne = { ...pipeOne, [key]: on })}
        {split}
        closed={isClosed(file)}
        ontoggle={() => (folds = { ...folds, [key]: !isClosed(file) })}
        viewed={viewed !== null}
        done={isViewed(file.path)}
        onviewed={(on) => setViewed(file, on)}
        loaded={loads[key] === true}
        onload={() => (loads = { ...loads, [key]: true })}
        current={current === key}
        rendered={piped ? null : (plans.get(key) ?? null)}
        mode={modeOf(file)}
        onmode={(mode) => (modes = { ...modes, [key]: mode })}
        sideOf={(side) => sideOf(file, side)}
        load={(side) => loadSide(file, side)}
        {commands}
        expandable={context !== null && piped === null}
        expanded={piped ? undefined : expandedOf(key)}
        onexpand={(hunk, how) => void expand(file, hunk, how)}
        tokensOf={(side, cell) => tokensOf(file, side, cell)}
        talk={piped ? quiet : talk}
        {render}
        {openAt}
        {repository}
      />
    {/each}
    {#if filter.trim() !== "" && drawn.length === 0}
      <div class="tf-empty"><p>No changed file matches the filter.</p></div>
    {:else if gone.length > 0 || homeless.length > 0}
      <details class="tf-outdated tf-gone">
        <summary>Comments on files that are no longer in this change</summary>
        {#each gone as g, n (n)}
          <div class="tf-thread"><span class="tf-mono tf-dim">{g.path}, {lineName(g.thread.side, g.thread.line)}</span><TfWrittenAt thread={g.thread} {repository} />{@render said(g.thread.comments)}</div>
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
              <b>You</b> <TfStateChip text="pending" tone="warn" />
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
</div>
