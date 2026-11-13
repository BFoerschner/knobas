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
  import { filterKeys } from "../filter-keys";
  import TfFindBar from "../TfFindBar.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import type { DiffLine, FileThread, LineComment } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { lineName } from "../../detail/view/pending-comments.svelte";
  import { flushSync, onMount, untrack, type Snippet } from "svelte";
  import { renderPort, type HighlightJob, type LineTokens, type RenderPort } from "../../render/port";
  import { inTextField } from "../../shell/keys";
  import { ago, agoStamp } from "../../shell/time";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { reviewSubject } from "../../shell/queued-toast";
  import { pageVisible } from "../kept-pages.svelte";
  import { matchesPath, sameTarget, targetOf } from "../pr/files";
  import {
    DIFF_CEILING,
    EXPAND_STEP,
    hiddenBefore,
    keyOf,
    waitingRows,
    shownLayout,
    sidesOf,
    type DiffContext,
    type DiffFill,
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
  import { Windowed } from "../../shell/windowed.svelte";

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
    me = null,
    fill = null,
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
    /** The reader's name at the source: a thread's comments by that name read *You*, as Conversation's do. None, each keeps its name. */
    me?: string | null;
    /**
     * Reads a file's lines a page at a time (the compare pass, 2026-11-13): `files` then carry none (`hunks` empty, the
     * list `entity_range_files` answers), and a file's lines are read as its card comes near the view, the next page as
     * its end does. None: `files` carry their lines.
     */
    fill?: DiffFill | null;
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
  /** As a set: a pull request of thousands of files asks it of every file, many times. */
  const viewedSet = $derived(new Set(viewed?.paths ?? []));
  const isViewed = (path: string) => viewedSet.has(path);

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
    // The files drawn: a change of thousands runs the pipelines of those near the view.
    for (const file of fileWin.slice(drawn)) {
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
  const sides = $derived.by(() =>
    new Map(
      // The files drawn, with their lines: a change of thousands of files highlights what is near the view.
      fileWin.slice(drawn).map((listed) => {
        const file = lineOf(listed);
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
      // A file that left the view keeps its tokens for its return; one whose text changed is highlighted anew.
      if ((want.get(name)?.text ?? made.text) === made.text && made.port === port) {
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
  const plans = $derived.by(() => {
    // Worked out per file as it is drawn, not for every file of a change of thousands as it opens.
    const can = { whole: context !== null, bytes: typeof context?.bytes === "function" };
    const list = defaults.list;
    // By the file as drawn: one whose lines came is planned again with them.
    const made = new WeakMap<DrawnFile, ReturnType<typeof renderedPlan>>();
    return {
      get(key: string) {
        const listed = byKey.get(key);
        if (listed === undefined) return null;
        const file = lineOf(listed);
        if (!made.has(file)) made.set(file, renderedPlan(file, openWith({ path: file.path, binary: file.change === "binary" }, allRenderers(), list).renderer, can));
        return made.get(file) ?? null;
      },
    };
  });
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
      // A file whose lines are still to be read has no hunk text yet: its side is read whole.
      const hunk = waits(file) ? null : hunkText(lineOf(file), side);
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
  /** Each file by its key. */
  const byKey = $derived(new Map(files.map((file) => [keyOf(file), file] as const)));

  /**
   * The lines `fill` read (the compare pass, 2026-11-13), by file: the file with the pages read so far, where the next
   * page starts, and why a read failed. A file of a change `fill` reads has none until its card comes near the view.
   */
  let lined = $state.raw<Record<string, { file: DrawnFile; next: number | null; failed: string | null }>>({});
  /** The reads in flight, by file, each under the name `cancelRead` calls it off by; and the files waiting for a turn. */
  const filling = new Map<string, string>();
  const queued: { key: string; from: number }[] = [];
  /** At most this many files' lines are read at once, the ones nearest the view first. */
  const AT_ONCE = 4;
  let fills = 0;
  const readName = () => `diff-${uid}-${++fills}`;
  /** The file with the lines read so far; `file` itself where nothing is read for it. */
  const lineOf = (file: DrawnFile): DrawnFile => lined[keyOf(file)]?.file ?? file;
  /** Its lines are still to be read: the card holds their room. */
  const waits = (file: DrawnFile) => fill !== null && lined[keyOf(file)] === undefined && file.additions + file.deletions > 0 && file.change !== "binary";
  /** Lines the card would draw now, were they here: open, as lines, not held behind *Load diff*. */
  const wantsLines = (file: DrawnFile) =>
    !isClosed(file) && modeOf(file) === "lines" && file.change !== "binary" && (loads[keyOf(file)] === true || !needsLoad(file));

  function ask(key: string, from: number) {
    if (fill === null || filling.has(key) || queued.some((q) => q.key === key)) return;
    queued.push({ key, from });
    pump();
  }

  function pump() {
    const port = fill;
    while (port !== null && filling.size < AT_ONCE && queued.length > 0) {
      const { key, from } = queued.shift()!;
      const file = byKey.get(key);
      if (file === undefined) continue;
      const name = readName();
      filling.set(key, name);
      void port.page(file, from, name).then(
        (page) => {
          if (filling.get(key) !== name) return;
          const had = lined[key];
          const hunks = from === 0 || had === undefined ? page.hunks : [...had.file.hunks];
          if (from > 0 && had !== undefined) {
            const more = [...page.hunks];
            if (page.continues && more.length > 0 && hunks.length > 0) {
              const last = hunks.at(-1)!;
              hunks[hunks.length - 1] = { ...last, lines: [...last.lines, ...more.shift()!.lines] };
            }
            hunks.push(...more);
          }
          lined = { ...lined, [key]: { file: { ...file, hunks, truncated: page.truncated }, next: page.next, failed: null } };
        },
        (cause) => {
          if (filling.get(key) !== name) return;
          const had = lined[key];
          lined = { ...lined, [key]: { file: had?.file ?? file, next: had?.next ?? null, failed: ipcErrorMessage(cause) } };
        },
      ).finally(() => {
        if (filling.get(key) === name) filling.delete(key);
        pump();
      });
    }
  }

  // The files near the view that would draw lines and have none read: theirs are asked, nearest first. A file whose
  // card leaves before its turn comes is passed over.
  $effect(() => {
    if (fill === null) return;
    const near = fileWin.slice(drawn).filter((file) => waits(file) && wantsLines(file)).map(keyOf);
    untrack(() => {
      const want = new Set(near);
      for (let i = queued.length - 1; i >= 0; i--) if (!want.has(queued[i]!.key) && queued[i]!.from === 0) queued.splice(i, 1);
      for (const key of near) ask(key, 0);
    });
  });
  // A diff that goes (another branch picked, the page left) calls off the reads it started.
  $effect(() => () => {
    queued.length = 0;
    const port = untrack(() => fill);
    for (const name of filling.values()) port?.stop?.(name);
    filling.clear();
  });

  /**
   * A change of thousands of files draws only the files near the viewport, and the rail only the rows near its own
   * scroll (the extremes pass, 2026-11-13: a pull request of 3,000 files built every card and rail row as the tab
   * opened). A file's room before it is drawn is its lines', or its header's while it is collapsed or waits for *Load
   * diff*; each card's own lines are windowed as before.
   */
  const fileWin = new Windowed({ count: () => drawn.length, key: (i) => keyOf(drawn[i]!), estimate: 120, gap: 12, estimateOf: (key) => fileRoom.get(key) ?? 120 });
  const fileRoom = $derived(
    new Map(
      drawn.map((file) => {
        const key = keyOf(file);
        const lines = waits(file) ? waitingRows(file) : lineOf(file).hunks.reduce((sum, hunk) => sum + hunk.lines.length + 1, 0);
        const shut = isClosed(file) || (needsLoad(file) && loads[key] !== true);
        return [key, shut ? 44 : 42 + Math.min(lines, 400_000) * 21];
      }),
    ),
  );
  const treeKey = (row: (typeof tree)[number]) => (row.kind === "dir" ? `dir\n${row.path}` : keyOf(row.file));
  const railWin = new Windowed({ count: () => tree.length, key: (i) => treeKey(tree[i]!), estimate: 31, scrolls: true });

  /** File `key`'s card, drawn first where the list holds it back. */
  function cardFor(key: string): Card | undefined {
    if (cards[key] === undefined) {
      const at = drawn.findIndex((file) => keyOf(file) === key);
      if (at < 0) return undefined;
      fileWin.reveal(at);
      flushSync();
    }
    return cards[key];
  }

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
    push({ text: "Added to your review. Nothing is sent until you submit it.", subject: reviewSubject(review.entity) });
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
    isMe: (name) => me !== null && name === me,
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
    cardFor(key)?.scrollTo();
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
    filesHost = host;
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
      // The files drawn are the ones near the view: the one at its top is among them.
      for (const file of fileWin.slice(drawn)) {
        const key = keyOf(file);
        const card = host.querySelector<HTMLElement>(`[data-file="${CSS.escape(key)}"]`);
        if (card === null) continue;
        if (card.getBoundingClientRect().top > top) break;
        at = key;
      }
      const first = fileWin.slice(drawn)[0];
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
          if (!cardFor(keys[i]!)?.nextHunk(step, i !== from)) continue;
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
        const taker = [...keys.slice(from), ...keys.slice(0, from)].find((key) => cardFor(key)?.comment());
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

  let filesHost = $state<HTMLElement | null>(null);
  /** A windowed file is holding rows back: it pads for them (`Windowed`), and the find bar finds only what is drawn. */
  const heldBack = () =>
    fileWin.before + fileWin.after > 0 ||
    [...(filesHost?.querySelectorAll<HTMLElement>(".tf-dbody") ?? [])].some((lines) => lines.style.paddingTop !== "" || lines.style.paddingBottom !== "");
  const cardOf = (key: string) => filesHost?.querySelector<HTMLElement>(`[data-file="${CSS.escape(key)}"]`) ?? null;
  /** The file a mark is changing, and where its head stands on screen. */
  let marking: { key: string; path: string; on: boolean; top: number } | null = null;

  /** Marks a file viewed or not; a collapse the reader set by hand gives way to it. */
  function setViewed(file: DrawnFile, on: boolean) {
    const key = keyOf(file);
    const { [key]: _, ...rest } = folds;
    folds = rest;
    const top = scroller?.getBoundingClientRect().top ?? 0;
    marking = { key, path: file.path, on, top: Math.max(cardOf(key)?.getBoundingClientRect().top ?? top, top) };
    current = key;
    viewed?.set(file.path, on);
  }

  // The file keeps its place through the collapse, so it stays current, a second press finds its box under the pointer, and the next
  // `j` goes on from it. The page moved because of the mark, not the reader, so the spy leaves the choice alone (F-250).
  $effect(() => {
    void folds;
    if (marking === null || isViewed(marking.path) !== marking.on) return;
    const { key, top } = marking;
    marking = null;
    const card = cardOf(key);
    if (scroller !== null && card !== null) scroller.scrollTop += card.getBoundingClientRect().top - top;
    hold();
  });
</script>

{#snippet said(comments: readonly LineComment[])}
  {#each comments as comment, n (n)}
    <!-- A line comment's body is the source's Markdown (`LineComment`), drawn by the Markdown component (#2261). -->
    <div class="tf-said">
      <p title={agoStamp(comment.at)}><b>{talk.isMe(comment.author) ? "You" : (comment.author_name ?? comment.author)}</b>{comment.at ? ` ${ago(comment.at)}` : ""}</p>
      <TfMarkdown text={comment.body} {render} />
    </div>
  {/each}
{/snippet}

{#snippet mine(body: string)}
  <div class="tf-thread tf-pending"><p><b>You</b> <TfStateChip text="pending" tone="warn" /> {body}</p></div>
{/snippet}

<div class="tf-dview">
<TfFindBar root={() => filesHost} within="in open files" more={heldBack} />
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
        <input bind:value={filter} placeholder="Filter changed files" aria-label="Filter changed files" {@attach filterKeys({ aside: true })} />
      </label>
      {#if tree.length === 0}
        <div class="tf-empty tf-flist-none">
          <p>No files match.</p>
          <button type="button" class="tf-btn tf-sm" onclick={() => (filter = "")}>Clear the filter</button>
        </div>
      {:else}
      <nav class="tf-flist" aria-label="Changed files" {@attach railWin.list}>
        {#if railWin.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach railWin.room("before")}></div>{/if}
        {#each railWin.slice(tree) as row (treeKey(row))}
          {#if row.kind === "dir"}
            <div class="tf-fdir" {@attach railWin.row(treeKey(row))}>{row.path}</div>
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
              {@attach railWin.row(treeKey(row))}
              ><span class="tf-bd tf-bd-{badge[0] === '+' || badge[0] === '−' ? 'X' : badge[0]}">{badge}</span><span class="tf-fname">{how === null ? row.name : row.name.slice(0, row.name.lastIndexOf(" ("))}</span>{#if how !== null}<span class="tf-state">{how}</span>{/if}<span class="tf-n"
                >{#if file.change !== "binary"}<span class="tf-add-n">+{file.additions}</span> <span class="tf-del-n">−{file.deletions}</span>{/if}{done ? " ✓" : ""}</span
              ></button
            >
          {/if}
        {/each}
        {#if railWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach railWin.room("after")}></div>{/if}
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
    <div class="tf-dcards" {@attach fileWin.list}>
    {#each fileWin.slice(drawn) as file (keyOf(file))}
      {@const key = keyOf(file)}
      {@const piped = pipedOf(file)}
      {@const got = lined[key]}
      <TfDiffCard
        bind:this={cards[key]}
        file={piped?.file ?? lineOf(file)}
        waiting={waits(file)}
        more={got?.next ?? null}
        onmore={() => {
          if (got?.next != null) ask(key, got.next);
        }}
        failedLines={got?.failed ?? null}
        onretry={() => {
          const { [key]: _, ...rest } = lined;
          lined = rest;
        }}
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
        onexpand={(hunk, how) => void expand(lineOf(file), hunk, how)}
        tokensOf={(side, cell) => tokensOf(file, side, cell)}
        talk={piped ? quiet : talk}
        {render}
        {openAt}
        {repository}
        place={fileWin.row(key)}
      />
    {/each}
    </div>
    <!-- A filter that matches nothing says so once, in the file list beside its box, with its way off. -->
    {#if (filter.trim() === "" || drawn.length > 0) && (gone.length > 0 || homeless.length > 0)}
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
