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
  - **Rendered** (#2279; ux26's stage, `docs/decisions/2026-11-20-a-comparison-
    is-drawn-rendered-and-may-end-at-the-working-tree.md`): a file one of whose
    renderers draws both sides (`rendered.ts` `renderedChoices`) offers *Lines |
    Rendered* in its header, the file drawn before and after in a stage whose
    height the renderer's kind sets (`TfDiffRendered`); an image also swipes
    and onion-skins. Its sides are `context`'s bytes, read at the clone first,
    once, and kept; a card's room while rendered is the stage's constants, so
    the windowed list places it right in its first frame.
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
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
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
    FILL_PAGE,
    EXPAND_STEP,
    hiddenBefore,
    keyOf,
    waitingRows,
    shownLayout,
    sidesOf,
    type DiffContext,
    type DiffFill,
    type DiffReview,
    type FilePage,
    type DiffViewed,
    type DrawnFile,
    type DrawnLine,
    type Expanded,
    type Shown,
    type SideText,
    type WholeTexts,
  } from "./diff";
  import { diffLayout, SPLIT_ROOM, type DiffLayoutChoice } from "./diff-layout.svelte";
  import { diffRendered, type DiffRenderedChoice } from "./diff-rendered.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import type { DiffTalk } from "./talk";
  import TfDiffCard from "./TfDiffCard.svelte";
  import { ipcCommands, type CommandPort } from "../../render/command";
  import { MADE_DEFAULTS, makeDefaultPattern, openWith, withDefault, type Opened, type RendererDefault } from "../../render/defaults";
  import { rendererDefaults } from "../../render/defaults.svelte";
  import { allRenderers, type Renderer } from "../../render/registry";
  import { hunkText, renderedChoices, renderedPlan, renderedRoom, type DiffMode, type RenderedPlan, type Side, type SideGot } from "./rendered";
  import type { OpenAt } from "./TfDiffCard.svelte";
  import { badgeOf, collapsedAtFirst, isLock, needsLoad, treeOf } from "./tree";
  import { diffPipelines, type DiffPipelinesSetting } from "./diff-pipelines.svelte";
  import { defaultPipelines, pipedDiff, pipelineLabel, type DiffPipeline, type DiffPipelines, type PipedDiff } from "./piped";
  import TfWrittenAt from "./TfWrittenAt.svelte";
  import { Windowed, landOn, listScrolled, placeWindowsNow } from "../../shell/windowed.svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { menuThing } from "../context/thing";
  import { thousands } from "../counted";
  import TfPath from "../TfPath.svelte";
  import TfDiffFolder from "./TfDiffFolder.svelte";
  import TfFolderPickMenu from "../render/TfFolderPickMenu.svelte";
  import { glyphFolder, groupFolders, hasGlob, takenFolders, type FolderGroup } from "./folders";
  import type { TreeGroup } from "./tree";
  import { FolderCompare, ipcFolderComparePorts, type Compared, type FolderComparePorts } from "../../render/folder-compare.svelte";
  import { folderRenderers as appFolderRenderers } from "../../render/folders.svelte";
  import { folderPicks as appFolderPicks, type FolderPicks } from "../../render/folder-picks.svelte";
  import { folderRenderFileBytes, folderRenderMarks, type FolderMarks, type FolderRenderer } from "../../ipc/app";
  import { heldWrite } from "../../shell/held-write";
  import { renderersFor } from "../../render/folder";
  import { lineDiff } from "./line-diff";
  import { WORKING_TREE } from "./address";

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
    counted = true,
    calm = true,
    sideNames = null,
    frame = undefined,
    rendering = diffRendered,
    epoch = null,
    folders = null,
    folderPorts = ipcFolderComparePorts,
    folderSaved = appFolderRenderers,
    picks = appFolderPicks,
    folderMarks = folderRenderMarks,
    madeBytes = folderRenderFileBytes,
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
    /** The renderer each file opens with (#2263): its rendered diff is drawn by it. *Render as*'s *Use … by default* saves here. */
    defaults?: { readonly list: readonly RendererDefault[]; save?: (list: RendererDefault[]) => Promise<unknown> };
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
    /**
     * The files carry their counts. False for a list of names alone (Compare's first draw, the stored-compare pass,
     * 2026-11-14): each file's `+N −N` waits empty in its row, and its lines are read as its card nears the view all the
     * same, until a counted list of the same files replaces it and keeps the lines read.
     */
    counted?: boolean;
    /**
     * Nothing else is being read for the page: the view reads ahead only then (the big-compare pass, 2026-11-16).
     * Compare clears it while another pick's list is read, so the diff it dims sends nothing beside that read.
     */
    calm?: boolean;
    /**
     * Where each side of a file is read, for a rendered file's caption (`main at 4f2a91c`, *working tree*, *the index*).
     * None: the caption says *before* and *after* alone.
     */
    sideNames?: ((file: DrawnFile) => { old: string; new: string }) | null;
    /** Keeps a page's sanitized document for its frame (a rendered `.html`): the IPC by default, a fake in tests. */
    frame?: ((document: string) => Promise<string>) | undefined;
    /** The band's *Rendered*: the app's one choice, kept across a restart; a fake in tests. */
    rendering?: DiffRenderedChoice;
    /**
     * Where the working tree stood when `files` were read (ux26: the checkout's stamp), for a diff with an end there.
     * The same epoch lays nothing down again; a new one reads again what was read of the old: each rendered side and
     * pipeline run in place (what was drawn stays until its answer lands), the first page of the files near the view,
     * and nothing of the files away from it until they come near. None: the files never move under the view.
     */
    epoch?: string | null;
    /**
     * Where a folder renderer can run on each side (ux26, rulings 7 and 8): the repository and the two ends, `before` a
     * whole sha or `null` (a root commit), `after` a whole sha or the working tree's token. Given only at a clone. None:
     * no folder is offered.
     */
    folders?: { entityId: string; before: string | null; after: string } | null;
    /** A folder's two runs and their compare: the IPC, a fake in tests. */
    folderPorts?: FolderComparePorts;
    /** The saved folder renderers: the app's copy, a fake in tests. */
    folderSaved?: { readonly list: readonly FolderRenderer[]; ensure(): Promise<void> };
    /** The renderer picked per folder (ruling 10): the app's copy, a fake in tests. */
    picks?: Pick<FolderPicks, "of" | "set" | "ensure">;
    /** Which folders the renderers holding a file glob take, at one end: the IPC, a fake in tests. */
    folderMarks?: (entityId: string, commit: string | null, paths: string | null) => Promise<FolderMarks[]>;
    /** A generated file's bytes by its folder's token: the IPC, a fake in tests. */
    madeBytes?: (token: string, path: string) => Promise<Uint8Array>;
  } = $props();

  // ---- A folder renderer in a comparison (ux26, rulings 7 and 8; the decision list's sections 8 to 11) ----

  /** Folders are offered: the diff has its file list and the caller said where the two runs go. */
  const offered = $derived(list && folders !== null);
  let savedRead = $state(false);
  $effect(() => {
    if (!offered) return;
    untrack(() => {
      void folderSaved.ensure().then(
        () => (savedRead = true),
        () => (savedRead = true),
      );
      void picks.ensure().catch(() => {});
    });
  });
  const savedFolders = $derived(offered ? folderSaved.list : []);
  const needsMarks = $derived(savedFolders.some((r) => r.holds.length > 0));
  /** Both ends' marks, merged (a folder taken on either side is taken), and the ends they were read for. */
  let marks = $state.raw<{ at: string; sets: ReadonlyMap<string, ReadonlySet<string>> } | null>(null);
  const NO_MARKS: ReadonlyMap<string, ReadonlySet<string>> = new Map();
  $effect(() => {
    const at = folders;
    if (!offered || at === null || !needsMarks) return;
    const name = `${at.entityId}\n${at.before}\n${at.after}`;
    if (untrack(() => marks?.at) === name) return;
    let live = true;
    const ends = [at.before, at.after].filter((end): end is string => end !== null);
    void Promise.all(ends.map((end) => folderMarks(at.entityId, end, null).catch((): FolderMarks[] => []))).then((answers) => {
      if (!live) return;
      const merged = new Map<string, Set<string>>();
      for (const found of answers)
        for (const one of found) {
          const into = merged.get(one.renderer) ?? new Set<string>();
          for (const folder of one.folders) into.add(folder);
          merged.set(one.renderer, into);
        }
      marks = { at: name, sets: merged };
    });
    return () => {
      live = false;
    };
  });
  const markSetsNow = $derived(marks?.sets ?? NO_MARKS);
  /** What groups the folders is known: the saved renderers read, and both ends' marks where a renderer needs them. */
  const foldersKnown = $derived(offered && savedRead && (!needsMarks || marks !== null));
  /**
   * The grouping came later than {@link GROUPS_HOLD} ms: the list drew ungrouped, and stays so until the person presses
   * the band's *Rendered* or a folder's glyph (section 20: the list never regroups under the reader).
   */
  let lateGroups = $state(false);
  const GROUPS_HOLD = 300;
  $effect(() => {
    if (!offered || foldersKnown || !rendering.on) return;
    const late = setTimeout(() => (lateGroups = true), GROUPS_HOLD);
    return () => clearTimeout(late);
  });
  /** The band's toggle groups every folder a renderer with a glob takes. */
  const autoGroups = $derived(rendering.on && foldersKnown && !lateGroups);
  /** The list is held unseen in its room while what groups it is read, at most {@link GROUPS_HOLD} ms. */
  const holding = $derived(offered && rendering.on && !foldersKnown && !lateGroups && (!savedRead || savedFolders.some(hasGlob)));
  const pickOf = (folder: string) => (folders === null ? null : picks.of(folders.entityId, folder));
  /** Folders grouped at their glyph (by renderer id), and folders whose real files are back, for the visit. */
  let chosen = $state.raw<ReadonlyMap<string, string>>(new Map());
  let realFolders = $state.raw<ReadonlySet<string>>(new Set());
  const NO_TAKEN: ReadonlyMap<string, FolderRenderer> = new Map();
  /** The folders the toggle groups, each with its renderer; each distinct folder of the list asked once. */
  const taken = $derived(foldersKnown ? takenFolders(files.map((file) => file.path), savedFolders, markSetsNow, pickOf) : NO_TAKEN);
  const chosenBy = $derived(
    new Map([...chosen].flatMap(([folder, id]) => {
      const by = savedFolders.find((r) => r.id === (pickOf(folder) ?? id)) ?? savedFolders.find((r) => r.id === id);
      return by === undefined ? [] : [[folder, by] as const];
    })),
  );
  const listed = $derived(offered && (autoGroups || chosenBy.size > 0) ? groupFolders(files, { taken, chosen: chosenBy, real: realFolders, on: autoGroups }) : null);
  const groups = $derived(listed === null ? [] : listed.flatMap((one) => (one.kind === "group" ? [one.group] : [])));
  /** The files listed as themselves: every file but those under a grouped folder. */
  const realFiles = $derived(listed === null ? files : listed.flatMap((one) => (one.kind === "file" ? [one.file] : [])));

  const compareKey = (group: FolderGroup) => `${group.folder}\u0000${group.renderer.id}`;
  /** Each grouped folder's two runs and their compare, by `compareKey`, made in an effect root of its own. */
  const compares = new SvelteMap<string, FolderCompare>();
  const unroot = new Map<string, () => void>();
  $effect(() => {
    const want = new Map(groups.map((group) => [compareKey(group), group] as const));
    untrack(() => {
      for (const [key, stop] of unroot)
        if (!want.has(key)) {
          stop();
          unroot.delete(key);
          compares.delete(key);
        }
      for (const [key, group] of want) {
        if (unroot.has(key)) continue;
        let made: FolderCompare | null = null;
        const id = group.renderer.id;
        const stop = $effect.root(() => {
          made = new FolderCompare(() => {
            const at = folders;
            const by = folderSaved.list.find((r) => r.id === id);
            return at === null || by === undefined ? null : { entityId: at.entityId, renderer: by, folder: group.folder, before: at.before, after: at.after };
          }, folderPorts);
        });
        unroot.set(key, stop);
        if (made !== null) compares.set(key, made);
      }
    });
  });
  $effect(() => () => {
    for (const stop of unroot.values()) stop();
    unroot.clear();
  });

  /**
   * What each grouped folder's two runs made, as the cards column draws it (by `compareKey`): a new answer lands in one
   * frame, the place held on the first row in view outside the folder (`heldWrite`, the Files tab's graft's way); what
   * was made stays drawn while the folder runs again.
   */
  let shownMade = $state.raw<ReadonlyMap<string, Compared>>(new Map());
  $effect(() => {
    const next = new Map<string, Compared>();
    for (const [key, compare] of compares) {
      const made = compare.made ?? (compare.anyKept ? untrack(() => shownMade.get(key)) : undefined);
      if (made !== undefined) next.set(key, made);
    }
    untrack(() => {
      const moved = [...new Set([...next.keys(), ...shownMade.keys()])].filter((key) => next.get(key) !== shownMade.get(key));
      if (moved.length === 0) return;
      landMade(() => (shownMade = next), moved.map((key) => key.slice(0, key.indexOf("\u0000"))));
    });
  });

  /** A grouped folder's answer drawn in one write, the view held where it was. */
  function landMade(write: () => void, changed: readonly string[]) {
    const host = filesHost;
    const box = scroller;
    if (host === null || box === null || !host.isConnected || host.checkVisibility?.() === false) {
      write();
      return;
    }
    const inside = (row: HTMLElement) => {
      const file = row.dataset["file"] ?? row.dataset["end"];
      return file !== undefined && changed.some((folder) => file.startsWith(`${folder}\u0000`));
    };
    const rowsNow = () => [...host.querySelectorAll<HTMLElement>("[data-file], [data-folder], [data-end]")];
    const idOf = (row: HTMLElement) => row.dataset["file"] ?? (row.dataset["folder"] !== undefined ? `\u0001${row.dataset["folder"]}` : `\u0002${row.dataset["end"]}`);
    heldWrite(write, {
      rows: rowsNow,
      top: () => topOfFiles(box),
      choose: (drawnRows) => drawnRows.find((row) => !inside(row)) ?? drawnRows.find((row) => row.dataset["folder"] !== undefined && changed.includes(row.dataset["folder"])),
      idOf,
      same: (id) => rowsNow().find((row) => idOf(row) === id),
      scrollBy: (row, px) => landOn(row, box, px),
    });
  }

  /** A generated file's two sides are read by bytes, never by a whole text or the forge. */
  const MADE_CAN = { whole: false, bytes: true } as const;
  /** A generated file over this many bytes on a side is not diffed as lines in the window (section 9). */
  const MADE_LINES_CAP = 2 * 1024 * 1024;
  const madeMemo = new WeakMap<Compared, DrawnFile[]>();
  /** The extension a single output's kind is named by, so its renderer is picked as a file's is. */
  const outputExt = (compare: FolderCompare | undefined) => {
    const kind = compare?.after.view?.drawn?.kind ?? compare?.before.view?.drawn?.kind;
    if (kind === undefined) return "txt";
    if (kind.kind === "image") return ({ "svg+xml": "svg", jpeg: "jpg" } as Record<string, string>)[kind.mime.split("/")[1] ?? ""] ?? kind.mime.split("/")[1] ?? "png";
    return ({ markdown: "md", html: "html", text: "txt" } as Record<string, string>)[kind.kind] ?? "bin";
  };
  /** The generated files of what a folder's runs made that differ, as the view draws files. */
  function madeFilesOf(group: FolderGroup, made: Compared, compare: FolderCompare | undefined): DrawnFile[] {
    const had = madeMemo.get(made);
    if (had !== undefined) return had;
    const base = { old_path: null, additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] };
    let out: DrawnFile[];
    if (made.kind === "single") {
      const ext = outputExt(compare);
      const image = ext !== "txt" && ext !== "md" && ext !== "html" && ext !== "bin";
      const change: DrawnFile["change"] = image || ext === "bin" ? "binary" : made.old === null ? "added" : made.new === null ? "deleted" : "modified";
      out = made.same
        ? []
        : [{ ...base, path: `output.${ext}`, key: `${group.folder}\u0000\u0001output`, change, made: { folder: group.folder, old: made.old, new: made.new, single: true, sizes: { old: null, new: null } } }];
    } else {
      out = made.compare.files.map((one) => {
        const big = (one.before_size ?? 0) > MADE_LINES_CAP || (one.after_size ?? 0) > MADE_LINES_CAP;
        return {
          ...base,
          path: one.path,
          key: `${group.folder}\u0000${one.path}`,
          change: one.binary || big ? "binary" : one.change,
          made: {
            folder: group.folder,
            old: one.change === "added" ? null : made.old,
            new: one.change === "deleted" ? null : made.new,
            single: false,
            sizes: { old: one.before_size, new: one.after_size },
          },
        } satisfies DrawnFile;
      });
    }
    madeMemo.set(made, out);
    return out;
  }
  /** A grouped folder as the rail and the cards column draw it. */
  type ShownGroup = TreeGroup & { group: FolderGroup; key: string; compare: FolderCompare | undefined; made: Compared | null; all: DrawnFile[]; lists: boolean; same: number };
  const groupsShown = $derived.by((): ShownGroup[] => {
    const words = filter;
    const narrowed = words.trim() !== "";
    return groups.flatMap((group) => {
      const key = compareKey(group);
      const compare = compares.get(key);
      const made = shownMade.get(key) ?? null;
      const all = made === null ? [] : madeFilesOf(group, made, compare);
      const shownFiles = narrowed ? all.filter((file) => matchesPath(file.path, words)) : all;
      if (narrowed && shownFiles.length === 0 && !matchesPath(group.folder, words)) return [];
      const same = made?.kind === "folder" && all.length > 0 ? made.compare.same : 0;
      return [{ folder: group.folder, files: shownFiles, group, key, compare, made, all, lists: all.length > 0, same }];
    });
  });
  /** Every generated file the groups list, by key, for the reads a card makes. */
  const madeAll = $derived(groupsShown.flatMap((group) => group.all));
  /** A generated file's side, by its folder's token. */
  const madeRead = (file: DrawnFile, token: string) => (file.made!.single ? folderPorts.output(token) : madeBytes(token, file.path));
  /** A generated file's lines, worked out here from its two sides as its card nears the view (as a pipeline's are). */
  const madeAsked = new Set<string>();
  function readMadeLines(file: DrawnFile) {
    const key = keyOf(file);
    if (madeAsked.has(key)) return;
    madeAsked.add(key);
    const decode = new TextDecoder();
    const side = async (s: Side) => {
      const token = file.made![s];
      return token === null ? "" : decode.decode(await madeRead(file, token));
    };
    Promise.all([side("old"), side("new")]).then(
      ([before, after]) => {
        const made = lineDiff(file.path, before, after);
        lined = {
          ...lined,
          [key]: { file: { ...file, hunks: made.hunks, additions: made.additions, deletions: made.deletions, ...(made.coarse ? { coarse: true as const } : {}) }, next: null, failed: null },
        };
      },
      (cause: unknown) => {
        madeAsked.delete(key);
        lined = { ...lined, [key]: { file, next: null, failed: ipcErrorMessage(cause) } };
      },
    );
  }
  $effect(() => {
    for (const file of nearFiles()) {
      if (file.made === undefined || file.change === "binary" || lined[keyOf(file)] !== undefined || !(wantsLines(file) || wantsMarks(file))) continue;
      untrack(() => readMadeLines(file));
    }
  });

  /** Each end in a grouped folder's words: the caption's, the working tree as *your working tree*. */
  function endsOf(group: FolderGroup): { old: string; new: string } {
    const said = sideNames?.(group.real[0]!) ?? null;
    const at = folders;
    const word = (end: string | null, caption: string | undefined) =>
      end === WORKING_TREE ? "your working tree" : (caption ?? (end === null ? "nothing" : end.slice(0, 7)));
    return { old: word(at?.before ?? null, said?.old), new: word(at?.after ?? null, said?.new) };
  }
  /** The renderers that take a grouped folder, for its bar's *Render with*. */
  const takingOf = (folder: string) => renderersFor(folder, savedFolders, markSetsNow);
  /** *Render with* in a folder's bar: the folder's pick (ruling 10), and the group's renderer with it. */
  function pickFolder(folder: string, id: string) {
    if (folders === null) return;
    if (chosen.has(folder)) chosen = new Map([...chosen, [folder, id]]);
    void picks.set(folders.entityId, folders.entityId, folder, id).catch(() => {});
  }
  /**
   * *Real files*: the folder's own files back, for the visit. Focus was on the band or the bar, which go: it lands on the
   * glyph of the folder's directory, else on the list's filter.
   */
  function putBack(folder: string) {
    const had = railHas(document.activeElement);
    realFolders = new Set([...realFolders, folder]);
    if (chosen.has(folder)) chosen = new Map([...chosen].filter(([at]) => at !== folder));
    if (had || document.activeElement?.closest(".tf-dfolder") != null) refocus(`.tf-dfmark[data-glyph="${CSS.escape(folder)}"]`);
  }
  /** Whether `el` is in this view's file list. */
  const railHas = (el: Element | null) => el !== null && filesRail !== null && filesRail.contains(el);
  let filesRail = $state<HTMLElement | null>(null);
  /** Focus on `selector` in the file list once the list is drawn again, else on its filter: never on the page's body. */
  function refocus(selector: string) {
    flushSync();
    const rail = filesRail;
    if (rail === null) return;
    (rail.querySelector<HTMLElement>(selector) ?? rail.querySelector<HTMLElement>("input"))?.focus({ preventScroll: true });
  }
  /** A directory row's glyph groups its folder; several renderers and no pick, it asks which first. */
  const glyphOf = (dir: string) => (offered && foldersKnown ? glyphFolder(dir, taken, savedFolders, markSetsNow) : null);
  let glyphMenu = $state<{ folder: string; opener: HTMLElement; taking: FolderRenderer[] } | null>(null);
  let glyphOpen = $state(false);
  /** A folder grouped at its glyph: the glyph's row goes, and focus lands on the folder's band, which puts it back. */
  function groupAt(folder: string, by: FolderRenderer, fromMenu = false) {
    const had = fromMenu || railHas(document.activeElement);
    chosen = new Map([...chosen, [folder, by.id]]);
    if (realFolders.has(folder)) realFolders = new Set([...realFolders].filter((at) => at !== folder));
    if (had) refocus(`.tf-dgband[data-folder="${CSS.escape(folder)}"]`);
  }
  function pressGlyph(event: MouseEvent, glyph: { folder: string; taking: FolderRenderer[] }) {
    if (glyph.taking.length > 1 && pickOf(glyph.folder) === null) {
      glyphMenu = { folder: glyph.folder, opener: event.currentTarget as HTMLElement, taking: glyph.taking };
      glyphOpen = true;
      return;
    }
    const by = glyph.taking.find((r) => r.id === pickOf(glyph.folder)) ?? glyph.taking[0];
    if (by !== undefined) groupAt(glyph.folder, by);
  }
  /** The band's words for a grouped folder in the rail. */
  function bandWords(group: ShownGroup): string {
    const name = group.group.renderer.name;
    switch (group.compare?.state) {
      case "running":
        return `${name} is running…`;
      case "failed":
        return `${name} made nothing`;
      case "made":
      case "same":
      case "stale":
      case "comparing":
        return `Made by ${name}`;
      default:
        return "Not rendered yet";
    }
  }
  /** *Run N folder renders* (section 10): at most this many folders a press. */
  const RUN_AT_ONCE = 20;
  const folderRuns = $derived.by(() => {
    const all = groups.map((group) => compares.get(compareKey(group))).filter((c): c is FolderCompare => c !== undefined);
    const states = all.map((c) => c.state);
    return {
      all,
      done: states.filter((state) => state === "made" || state === "same").length,
      going: states.includes("running"),
      runnable: all.filter((c) => c.runnable),
    };
  });
  function runFolders() {
    if (folderRuns.going) {
      for (const compare of folderRuns.all) compare.stop();
      return;
    }
    // A failed folder is left to its own bar: a batch does not run a failing tool again.
    for (const compare of folderRuns.runnable.slice(0, RUN_AT_ONCE)) compare.press();
  }
  /**
   * The band's button keeps the widest room its words have taken (section 10): *Stop all* and *Run 1 folder render*
   * after *Run 2 folder renders* move nothing beside it.
   */
  let runWidest = 0;
  const keepWidth = (_words: string) => (node: HTMLElement) => {
    runWidest = Math.max(runWidest, node.getBoundingClientRect().width);
    if (runWidest > 0) node.style.minWidth = `${runWidest}px`;
  };
  const runLabel = $derived.by(() => {
    if (folderRuns.going) return "Stop all";
    const n = folderRuns.runnable.length;
    if (n === 0) return "Run folder renders";
    if (n > RUN_AT_ONCE) return `Run ${RUN_AT_ONCE} of ${thousands(n)} folder renders`;
    return `Run ${n} folder ${n === 1 ? "render" : "renders"}`;
  });

  let filter = $state("");
  const shown = $derived.by(() => {
    const words = filter;
    // No filter, every file: a list of thousands asked the filter of each file (the stored-compare pass, 2026-11-14).
    return words.trim() === "" ? realFiles : realFiles.filter((file) => matchesPath(keyOf(file), words));
  });
  const tree = $derived(list ? treeOf(shown, groupsShown) : []);
  /** A row of the cards column: a file, a grouped folder's bar, or the line that ends its generated files (ux26). */
  type CardRow = { kind: "file"; file: DrawnFile } | { kind: "folder"; group: ShownGroup } | { kind: "end"; group: ShownGroup };
  const rowKey = (row: CardRow) => (row.kind === "file" ? keyOf(row.file) : row.kind === "folder" ? `\u0001${row.group.folder}` : `\u0002${row.group.folder}`);
  /** The cards column's rows in the rail's order: a grouped folder's bar, then its generated files and its end line. */
  const cardRows = $derived.by((): CardRow[] => {
    if (!list) return files.map((file) => ({ kind: "file", file }));
    const out: CardRow[] = [];
    for (const row of tree) {
      if (row.kind === "file" && row.made !== true) out.push({ kind: "file", file: row.file });
      else if (row.kind === "gdir") {
        const group = row.group as ShownGroup;
        out.push({ kind: "folder", group });
        for (const file of group.files) out.push({ kind: "file", file });
        if (group.same > 0) out.push({ kind: "end", group });
      }
    }
    return out;
  });
  /** The rail's order, so a file's place in the list is its place on the page. */
  const drawn = $derived(cardRows.flatMap((row) => (row.kind === "file" ? [row.file] : [])));
  /** The rows near the view as the column draws them: a grouped folder's generated files and end line in one inset box. */
  type Segment = { kind: "file"; key: string; file: DrawnFile } | { kind: "folder"; key: string; group: ShownGroup } | { kind: "under"; key: string; rows: CardRow[] };
  const cardSegments = $derived.by((): Segment[] => {
    const out: Segment[] = [];
    for (const row of fileWin.slice(cardRows)) {
      if (row.kind === "folder") {
        out.push({ kind: "folder", key: rowKey(row), group: row.group });
        continue;
      }
      const folder = row.kind === "end" ? row.group.folder : row.file.made?.folder;
      if (folder === undefined) {
        out.push({ kind: "file", key: rowKey(row), file: (row as { file: DrawnFile }).file });
        continue;
      }
      const last = out.at(-1);
      if (last?.kind === "under" && last.key === `under\u0000${folder}`) last.rows.push(row);
      else out.push({ kind: "under", key: `under\u0000${folder}`, rows: [row] });
    }
    return out;
  });
  /** The files of the rows near the view. */
  const nearFiles = () => fileWin.slice(cardRows).flatMap((row) => (row.kind === "file" ? [row.file] : []));
  /** The width the diffs have, once measured. */
  let room = $state(Number.POSITIVE_INFINITY);
  const tight = $derived(narrow.current || room < SPLIT_ROOM);
  const split = $derived(shownLayout(layout.chosen, tight) === "split");
  /** Why *Split* can't be pressed, and what `s` says then. */
  const NO_ROOM = "There is too little room here for split. A wider window has it.";
  const uid = $props.id();
  /** Whether the file list is unfolded under its bar, where it has one. */
  let railOpen = $state(false);

  /** The window's height, for a rendered card's stage (`clamp(360px, 56vh, 640px)`): read where the width is. */
  let viewHeight = $state(typeof window === "undefined" ? 868 : window.innerHeight);
  /** Reads the column's width, and the window's height with it; a hidden page has none to read, so it keeps the last. */
  function measure(node: HTMLElement) {
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(([entry]) => {
      if (entry !== undefined && entry.contentRect.width > 0) room = entry.contentRect.width;
      if (window.innerHeight > 0 && window.innerHeight !== viewHeight) viewHeight = window.innerHeight;
    });
    watch.observe(node);
    // The window growing taller leaves the column's width as it was: the stage's height follows the window's.
    const resized = () => {
      if (window.innerHeight > 0 && window.innerHeight !== viewHeight) viewHeight = window.innerHeight;
    };
    window.addEventListener("resize", resized, { passive: true });
    return () => {
      watch.disconnect();
      window.removeEventListener("resize", resized);
    };
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
        refused && "why" in refused ? { kind: "failed", side: refused.side, why: refused.why } : pipedDiff(file, "run" in old ? old.run : null, "run" in nu ? nu.run : null, drawsBytes);
      // A refused read is asked again the next time the pipeline is turned on.
      if (refused) running.delete(name);
      runs = { ...runs, [name]: made };
    });
  }
  $effect(() => {
    void sideEpoch;
    // The files drawn: a change of thousands runs the pipelines of those near the view.
    for (const file of nearFiles()) {
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
      nearFiles().map((listed) => {
        const file = lineOf(listed);
        const piped = pipedOf(file);
        if (piped === null) return [keyOf(file), sidesOf(file, wholes[keyOf(file)])] as const;
        // A piped diff is highlighted as its output: `events.json.b64` decoded is JSON.
        const both = sidesOf(piped.file);
        return [keyOf(file), { ...both, old: { ...both.old, path: piped.name }, new: { ...both.new, path: piped.name } }] as const;
      }),
    ),
  );
  // Each side's tokens under its own key (the inner-6x pass, 2026-11-15): an answer redraws the lines of the card it is
  // for. One record for all of them redrew every card drawn on each of a diff's answers, two a file, as each landed.
  const tokens = new SvelteMap<string, (LineTokens | undefined)[]>();
  const jobs = new Map<string, { text: string; port: RenderPort; job: HighlightJob }>();
  $effect(() => {
    const port = render;
    const want = new Map<string, SideText>();
    for (const [key, both] of sides) for (const side of ["old", "new"] as const) want.set(`${side}\n${key}`, both[side]);
    untrack(() => {
      for (const [name, made] of jobs) {
        // A file that left the view keeps its tokens for its return; one whose text changed is highlighted anew.
        if ((want.get(name)?.text ?? made.text) === made.text && made.port === port) continue;
        made.job.cancel();
        jobs.delete(name);
        tokens.delete(name);
      }
    });
    for (const [name, side] of want) {
      if (jobs.has(name)) continue;
      const got: (LineTokens | undefined)[] = [];
      // Its entry is there before any answer: a read of a key the map lacks wakes on every key added.
      untrack(() => tokens.set(name, got));
      const job = port.highlight(side, {
        onLines(from, lines) {
          lines.forEach((line, n) => (got[from + n] = line));
          tokens.set(name, [...got]);
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
    const toks = tokens.get(`${side}\n${key}`)?.[row];
    return toks && toks.map(([part]) => part).join("") === cell.line.text ? toks : undefined;
  };

  /** The renderer *Render as* picked per card, by `keyOf`, for the visit. */
  let picked = $state.raw<Record<string, string>>({});
  /** A file's rendered mode: the renderers that can draw both of its sides, and the plan of the one it draws with. */
  /** `path`: the name it is opened by, where it is not the file's own (a pipeline's output, ux26). */
  type Planned = { plan: RenderedPlan; choices: Renderer[]; opened: Opened; path?: string };
  /** Whether a renderer draws a pipeline's output of bytes by its name (a gunzipped picture): it is drawn rendered. */
  const drawsBytes = (name: string) =>
    renderedChoices({ path: name, old_path: null, change: "binary", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] }, openWith({ path: name, binary: true }, allRenderers(), defaults.list), PIPED_CAN).length > 0;
  /** A pipeline's two outputs are in hand: read as bytes, never again. */
  const PIPED_CAN = { whole: false, bytes: true } as const;
  /** A piped file's rendered mode, by its output's name (a decoded `notes.md` is Markdown), kept per run. */
  const pipedPlans = new WeakMap<object, Planned | null>();
  function pipedPlan(file: DrawnFile, piped: { file: DrawnFile; name: string }): Planned | null {
    const chosen = picked[keyOf(file)];
    const had = pipedPlans.get(piped);
    if (had !== undefined && (had === null || chosen === undefined || had.plan.renderer.id === chosen)) return had;
    const shown = { ...piped.file, path: piped.name };
    const opened = openWith({ path: piped.name, binary: shown.change === "binary" }, allRenderers(), defaults.list);
    const choices = renderedChoices(shown, opened, PIPED_CAN);
    const plan = renderedPlan(shown, choices.find((r) => r.id === chosen) ?? choices[0] ?? null, PIPED_CAN);
    const made = plan === null ? null : { plan, choices, opened, path: piped.name };
    pipedPlans.set(piped, made);
    return made;
  }
  /** A file's rendered mode: its pipeline output's where it is drawn piped, else its own. */
  const planOf = (file: DrawnFile): Planned | null => {
    const piped = pipedOf(file);
    return piped !== null ? pipedPlan(file, piped) : plans.get(keyOf(file));
  };
  /** Each file's rendered mode, where one of its renderers draws both sides and they can be read (by `keyOf`). */
  const plans = $derived.by(() => {
    // Worked out per file as it is drawn, not for every file of a change of thousands as it opens.
    const can = { whole: context !== null, bytes: typeof context?.bytes === "function" };
    const list = defaults.list;
    const chosen = picked;
    const all = allRenderers();
    // The choices by the file's name where nothing takes a file by its folder and every side is read as bytes: the
    // band's *Rendered* plans every file of a list for its room, and 10,000 files asked the registry's globs each.
    const byName = can.bytes && !all.some((r) => r.patterns.some((p) => p.includes("/"))) && !list.some((d) => d.pattern.includes("/")) ? new Map<string, { opened: Opened; choices: Renderer[] }>() : null;
    const madeList = [...list, ...MADE_DEFAULTS];
    const choicesOf = (file: DrawnFile) => {
      const binary = file.change === "binary";
      // A generated file opens as the Files tab opens it in a generated folder (a page as a page), read as bytes.
      const gen = file.made !== undefined;
      const name = byName === null ? null : `${gen ? "m" : ""}${binary ? "b" : "t"}${file.path.slice(file.path.lastIndexOf("/") + 1)}`;
      const had = name === null ? undefined : byName!.get(name);
      if (had !== undefined) return had;
      const opened = openWith({ path: file.path, binary }, all, gen ? madeList : list);
      const made = { opened, choices: renderedChoices(file, opened, gen ? MADE_CAN : can) };
      if (name !== null) byName!.set(name, made);
      return made;
    };
    // By the file as drawn: one whose lines came is planned again with them.
    const made = new WeakMap<DrawnFile, Planned | null>();
    return {
      get(key: string): Planned | null {
        const listed = byKey.get(key);
        if (listed === undefined) return null;
        const file = lineOf(listed);
        if (!made.has(file)) {
          const { opened, choices } = choicesOf(file);
          // The card's pick, else the renderer it opens with where it draws both sides, else the first that does.
          const renderer = choices.find((r) => r.id === chosen[key]) ?? choices[0] ?? null;
          const plan = renderedPlan(file, renderer, file.made !== undefined ? MADE_CAN : can);
          made.set(file, plan === null ? null : { plan, choices, opened });
        }
        return made.get(file) ?? null;
      },
    };
  });
  /** *Render as*'s *Use ⟨name⟩ for `*.ext` by default*: the same `render.defaults` row the file viewer writes. */
  async function makeDefault(file: DrawnFile, opened: Opened, chosen: Renderer, path = file.path) {
    if (defaults.save === undefined) return;
    try {
      await defaults.save(withDefault(defaults.list, makeDefaultPattern(opened, path), chosen.id));
      const { [keyOf(file)]: _, ...rest } = picked;
      picked = rest;
    } catch (cause) {
      push({ text: `Could not make ${chosen.name} the default: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  /** The mode the reader picked per file; a binary file with a rendered mode opens in it (it has no lines). */
  let modes = $state.raw<Record<string, DiffMode>>({});
  /**
   * A card's own press, else *Rendered* where the band's toggle is on or the file is binary (it has no lines) and a
   * renderer can draw both of its sides, else its lines.
   */
  const modeOf = (file: DrawnFile): DiffMode => modes[keyOf(file)] ?? ((rendering.on || file.change === "binary" || pipedOf(file)?.file.change === "binary") && planOf(file) ? "rendered" : "lines");
  /** The band's *Rendered*, and `d`: every card's own press goes, so the toggle says how every file shows. */
  function toggleRendered() {
    rendering.set(!rendering.on);
    modes = {};
    // A folder's *Real files* is an override as a card's press is; and a press groups again what came late (section 20).
    realFolders = new Set();
    lateGroups = false;
  }
  /** Each side a rendered diff read, under `side\nkey`, and the reads made. */
  let got = $state.raw<Record<string, SideGot>>({});
  const asked = new Set<string>();
  const NAMES = { old: "before", new: "after" } as const;

  function loadSide(file: DrawnFile, side: Side) {
    const name = `${side}\n${keyOf(file)}`;
    const reader = context;
    if (asked.has(name)) return;
    asked.add(name);
    // A pipeline's outputs are in hand (ux26): drawn from the run, with no read.
    if (pipedOf(file) !== null) {
      asked.delete(name);
      return;
    }
    const read = async (): Promise<SideGot> => {
      // A generated file (ux26): its side is the generated folder's, by its token; none on a side it is not on.
      if (file.made !== undefined) {
        const token = file.made[side];
        if (token === null) return file.change === "binary" ? { kind: "absent" } : { kind: "failed", why: `the file ${NAMES[side]} the change is not there` };
        return { kind: "drawn", content: await madeRead(file, token) };
      }
      // A file whose lines are still to be read has no hunk text yet: its side is read whole.
      const hunk = waits(file) ? null : hunkText(lineOf(file), side);
      if (hunk !== null) return { kind: "drawn", content: hunk };
      if (reader === null) return { kind: "failed", why: "nothing reads this diff's files" };
      // Every side as bytes, read at the clone first (`entity_file_bytes`): a text side read as a `file@` view asked the
      // forge twice for one Markdown file at a clone (the stage pass, 2026-11-20). A text renderer decodes them.
      if (reader.bytes !== undefined) {
        const bytes = await reader.bytes(file, side);
        if (bytes !== null) return { kind: "drawn", content: bytes };
        return file.change === "binary" ? { kind: "absent" } : { kind: "failed", why: `the file ${NAMES[side]} the change is not there` };
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
  const sideOf = (file: DrawnFile, side: Side): SideGot | undefined => {
    const piped = pipedOf(file);
    if (piped !== null) {
      const content = piped.sides[side];
      return content === null ? { kind: "absent" } : { kind: "drawn", content };
    }
    return got[`${side}\n${keyOf(file)}`];
  };

  /** Counts the epochs seen: a rendered stage reads its sides again at each (`TfDiffRendered`'s `epoch`). */
  let sideEpoch = $state(0);
  let seenEpoch: string | null | undefined = undefined;
  $effect.pre(() => {
    const now = epoch;
    untrack(() => {
      const moved = seenEpoch !== undefined && now !== seenEpoch;
      seenEpoch = now;
      if (!moved) return;
      // What was read of the old tree is asked again; what it drew stays until each answer lands.
      asked.clear();
      reads.clear();
      running.clear();
      wholes = {};
      sideEpoch += 1;
      if (fill === null) return;
      const near = new Set(nearFiles().map(keyOf));
      const kept: typeof lined = {};
      // A generated file's lines are of what the runs made, not of the working tree: they stay.
      for (const [key, had] of Object.entries(lined)) if (near.has(key) || had.file.made !== undefined) kept[key] = had;
      lined = kept;
      for (const [key, had] of Object.entries(kept)) if (had.file.made === undefined) ask(key, 0, false);
      pump();
    });
  });

  /**
   * Collapsed by hand (`true`) or opened by hand (`false`); otherwise viewed, lock and generated files start
   * collapsed. A generated file over the *Load diff* limit starts open on its *Load diff* line, its collapsed
   * form (ruling on #2252: `src/generated/schema.rs`).
   */
  // Raw, each change a new record: a proxy made a signal for every file it was asked of, 10,000 of them as a list opened (the
  // stored-compare pass, 2026-11-14).
  let folds = $state.raw<Record<string, boolean>>({});
  const isClosed = (file: DrawnFile) =>
    folds[keyOf(file)] ?? (isViewed(file.path) || (collapsedAtFirst(file.path) && (isLock(file.path) || !needsLoad(file))));
  /** Files over the *Load diff* limit the reader loaded. */
  let loads = $state.raw<Record<string, true>>({});
  /** Each file by its key. */
  const byKey = $derived(new Map([...files, ...madeAll].map((file) => [keyOf(file), file] as const)));

  /**
   * The lines `fill` read (the compare pass, 2026-11-13), by file: the file with the pages read so far, where the next
   * page starts, and why a read failed. A file of a change `fill` reads has none until its card comes near the view.
   */
  let lined = $state.raw<Record<string, { file: DrawnFile; next: number | null; failed: string | null }>>({});
  // A list that replaced the one the lines were read for (Compare's names by the counted list, the stored-compare
  // pass): each file read keeps its lines and takes the new listing's counts, change and modes.
  $effect.pre(() => {
    const now = byKey;
    untrack(() => {
      let next: typeof lined | null = null;
      for (const [key, got] of Object.entries(lined)) {
        const listed = now.get(key);
        if (listed === undefined || got.file.made !== undefined || (listed.additions === got.file.additions && listed.deletions === got.file.deletions && listed.change === got.file.change && listed.old_mode === got.file.old_mode && listed.new_mode === got.file.new_mode)) continue;
        next ??= { ...lined };
        next[key] = { ...got, file: { ...listed, hunks: got.file.hunks, truncated: got.file.truncated } };
      }
      if (next !== null) lined = next;
    });
  });
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
  const waits = (file: DrawnFile) =>
    file.made !== undefined
      ? lined[keyOf(file)] === undefined && file.change !== "binary"
      : fill !== null && lined[keyOf(file)] === undefined && (!counted || file.additions + file.deletions > 0) && file.change !== "binary";
  /**
   * A rendered Markdown file marks its changed blocks from its lines (ux26, ruling 3): they are read as its card nears the
   * view, as an open file's lines are, though the card draws none.
   */
  const wantsMarks = (file: DrawnFile) => !isClosed(file) && modeOf(file) === "rendered" && plans.get(keyOf(file))?.plan.renderer.id === "markdown";
  /** Lines the card would draw now, were they here: open, as lines, not held behind *Load diff*. */
  const wantsLines = (file: DrawnFile) =>
    !isClosed(file) && modeOf(file) === "lines" && file.change !== "binary" && (loads[keyOf(file)] === true || !needsLoad(file));

  /** Lays page `from` of file `key` into what the view drew of it. */
  function apply(key: string, file: DrawnFile, from: number, page: FilePage) {
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
  }

  /** Asks page `from` of file `key`; `now` false queues it for a `pump` the caller makes once it has queued the rest. */
  function ask(key: string, from: number, now = true) {
    if (fill === null || filling.has(key) || queued.some((q) => q.key === key)) return;
    // A page read before (this view's own, or another's of the same two commits: *Open as diff*, a jump back) is drawn at
    // once, with no read (the big-compare pass, 2026-11-16).
    const file = byKey.get(key);
    const kept = file === undefined ? undefined : fill.cached?.(file, from);
    if (file !== undefined && kept !== undefined) {
      apply(key, file, from, kept);
      return;
    }
    queued.push({ key, from });
    if (now) pump();
  }

  /** At most this many files' first pages in one read, where the fill reads many at once. */
  const NEAR_AT_ONCE = 40;

  function pump() {
    const port = fill;
    while (port !== null && filling.size < AT_ONCE && queued.length > 0) {
      // The first pages of the files near the view in one read (one start of git for all of them): a Windows laptop
      // spends most of a file's wait starting git (the big-compare pass, 2026-11-16).
      if (port.pages !== undefined && queued[0]!.from === 0) {
        const batch: { key: string; file: DrawnFile }[] = [];
        for (let i = 0; i < queued.length && batch.length < NEAR_AT_ONCE; ) {
          const { key, from } = queued[i]!;
          const file = byKey.get(key);
          if (from !== 0 || file === undefined) {
            i++;
            continue;
          }
          queued.splice(i, 1);
          batch.push({ key, file });
        }
        if (batch.length > 1) {
          const name = readName();
          for (const { key } of batch) filling.set(key, name);
          void port.pages(batch.map((one) => one.file), name).then(
            (pages) => {
              batch.forEach(({ key, file }, n) => {
                if (filling.get(key) !== name) return;
                filling.delete(key);
                const page = pages[n];
                // A file the read could not answer whole is read alone.
                if (page === null || page === undefined) queued.push({ key, from: 0 });
                else apply(key, file, 0, page);
              });
            },
            () => {
              // The read as a whole failed: each file is read alone, and says so if it fails too.
              for (const { key } of batch) if (filling.get(key) === name) {
                filling.delete(key);
                queued.push({ key, from: 0 });
              }
            },
          ).finally(pump);
          continue;
        }
        if (batch.length === 1) queued.unshift({ key: batch[0]!.key, from: 0 });
      }
      const { key, from } = queued.shift()!;
      const file = byKey.get(key);
      if (file === undefined) continue;
      const name = readName();
      filling.set(key, name);
      void port.page(file, from, name).then(
        (page) => {
          if (filling.get(key) !== name) return;
          apply(key, file, from, page);
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
    const near = nearFiles().filter((file) => file.made === undefined && waits(file) && (wantsLines(file) || wantsMarks(file))).map(keyOf);
    // The file the reader is on first, then outward from it: a pick's own file is read before the overscan's.
    const on = current;
    if (on !== null && near.includes(on)) {
      const place = new Map(near.map((key, n) => [key, n] as const));
      const at = place.get(on)!;
      near.sort((a, b) => Math.abs(place.get(a)! - at) - Math.abs(place.get(b)! - at));
    }
    untrack(() => {
      const want = new Set(near);
      for (let i = queued.length - 1; i >= 0; i--) if (!want.has(queued[i]!.key) && queued[i]!.from === 0) queued.splice(i, 1);
      // All of them queued before any read goes, so they go in one read where the fill reads many.
      for (const key of near) ask(key, 0, false);
      pump();
    });
  });
  /**
   * **Reading ahead** (the big-compare pass, 2026-11-16): where the fill reads many files at once at a checkout
   * (`fill.ahead`), the first pages of the files not drawn yet are read while the page is idle, a batch at a time, out
   * from the file the reader is on, up to {@link AHEAD_LINES} changed lines. They are kept (`fill.cached`), so a pick in the
   * file list, a `j` far down or *Open as diff* draws the file's lines in the frame it lands in, with no read.
   */
  const AHEAD_LINES = 100_000;
  const AHEAD_AT_ONCE = 100;
  $effect(() => {
    const port = fill;
    const all = drawn.filter((file) => file.made === undefined);
    if (port === null || port.ahead !== true || port.pages === undefined || port.cached === undefined || !counted || !calm || all.length === 0) return;
    let live = true;
    let reading: string | null = null;
    let wait: (() => void) | null = null;
    let budget = AHEAD_LINES;
    const later = (then: () => void) => {
      if (typeof requestIdleCallback === "function") {
        const id = requestIdleCallback(then, { timeout: 1000 });
        wait = () => cancelIdleCallback(id);
      } else {
        const id = setTimeout(then, 50);
        wait = () => clearTimeout(id);
      }
    };
    // The order is made once, out from the reader's file (below it first, then above), and walked with a cursor: each
    // file is looked at once, not every file on every turn (a turn that scanned 4,200 read files was 50 ms at 6x).
    const here = untrack(() => Math.max(0, current === null ? 0 : all.findIndex((file) => keyOf(file) === current)));
    const order: number[] = [here];
    for (let d = 1; here + d < all.length || here - d >= 0; d++) {
      if (here + d < all.length) order.push(here + d);
      if (here - d >= 0) order.push(here - d);
    }
    let cursor = 0;
    const next = () => {
      wait = null;
      if (!live || budget <= 0) return;
      const batch: DrawnFile[] = [];
      // At most a thousand files looked at a turn, so a list read already is walked in short turns.
      for (let seen = 0; cursor < order.length && batch.length < AHEAD_AT_ONCE && budget > 0 && seen < 1000; seen++) {
        const file = all[order[cursor++]!]!;
        if (!waits(file) || filling.has(keyOf(file)) || port.cached!(file, 0) !== undefined) continue;
        batch.push(file);
        budget -= Math.min(file.additions + file.deletions, FILL_PAGE);
      }
      if (batch.length === 0) {
        if (cursor < order.length) later(next);
        return;
      }
      const name = readName();
      reading = name;
      void port.pages!(batch, name).then(
        () => {
          if (reading === name) reading = null;
          if (live) later(next);
        },
        () => {
          if (reading === name) reading = null;
        },
      );
    };
    later(next);
    return () => {
      live = false;
      wait?.();
      if (reading !== null) port.stop?.(reading);
    };
  });

  // A diff that goes (another branch picked, the page left) calls off the reads it started.
  $effect(() => () => {
    queued.length = 0;
    const port = untrack(() => fill);
    for (const name of new Set(filling.values())) port?.stop?.(name);
    filling.clear();
  });

  /**
   * A change of thousands of files draws only the files near the viewport, and the rail only the rows near its own
   * scroll (the extremes pass, 2026-11-13: a pull request of 3,000 files built every card and rail row as the tab
   * opened). A file's room before it is drawn is its lines', or its header's while it is collapsed or waits for *Load
   * diff*; each card's own lines are windowed as before.
   */
  // From three files (the big-compare pass, 2026-11-16): a pull request of seven drew every card and every line of each
  // in its first frame, 530 ms of a sixth-speed main thread on Windows; now the cards on screen, the rest in pieces after.
  const fileWin = new Windowed({ count: () => cardRows.length, key: (i) => rowKey(cardRows[i]!), estimate: 120, gap: 12, estimateOf: (key) => fileRoom.get(key) ?? 120, from: 3 });
  /** A grouped folder's bar: the made bar, and under it the 96 px note box (and its 12 px inset) until cards stand there. */
  const FOLDER_BAR = 42;
  const FOLDER_NOTE = 12 + 96;
  /** The line that ends a grouped folder's generated files. */
  const FOLDER_END = 24;
  const fileRoom = $derived.by(() => {
    // What each file's room reads, read once for all of them: a list of 10,000 files asked each of every file (the
    // stored-compare pass, 2026-11-14).
    const [filling, read, shutBy, loaded, all, chosen, high, pill] = [fill !== null, lined, folds, loads, counted, modes, viewHeight, rendering.on];
    const viewedNow = viewedSet;
    return new Map(
      cardRows.map((row) => {
        if (row.kind === "folder") return [rowKey(row), FOLDER_BAR + (row.group.lists ? 0 : FOLDER_NOTE)] as const;
        if (row.kind === "end") return [rowKey(row), FOLDER_END] as const;
        const file = row.file;
        const key = keyOf(file);
        // A card open rendered is its header, the caption, the stage and the foot: constants known before a side is read.
        // Planned only where it may be rendered (picked so, or binary): a list of thousands plans none as it opens.
        const planned = (chosen[key] ?? (pill || file.change === "binary" ? "rendered" : "lines")) === "rendered" ? planOf(file) : null;
        const got = read[key];
        const waiting =
          file.made !== undefined
            ? got === undefined && file.change !== "binary"
            : filling && got === undefined && (!all || file.additions + file.deletions > 0) && file.change !== "binary";
        const lines = waiting ? waitingRows(file) : (got?.file ?? file).hunks.reduce((sum, hunk) => sum + hunk.lines.length + 1, 0);
        const big = needsLoad(file);
        const closed = shutBy[key] ?? (viewedNow.has(file.path) || (collapsedAtFirst(file.path) && (isLock(file.path) || !big)));
        if (planned) return [key, closed ? 44 : renderedRoom(planned.plan.renderer, high)] as const;
        const shut = closed || (big && loaded[key] !== true);
        return [key, shut ? 44 : 42 + Math.min(lines, 400_000) * 21] as const;
      }),
    );
  });
  const treeKey = (row: (typeof tree)[number]) =>
    row.kind === "dir" ? `dir\n${row.path}` : row.kind === "gdir" ? `gdir\n${row.group.folder}` : row.kind === "gband" ? `gband\n${row.group.folder}` : keyOf(row.file);
  const railWin = new Windowed({ count: () => tree.length, key: (i) => treeKey(tree[i]!), estimate: 31, scrolls: true });

  /**
   * File `key`'s card, drawn first where the list holds it back. A card drawn once and taken out by the window since
   * leaves its binding null, not undefined (the compare-jump pass, 2026-11-17: a pick of a file drawn earlier near the
   * view, then scrolled away from, did nothing at all), so it is the card's box that says whether it is drawn.
   */
  function cardFor(key: string, landing = false): Card | undefined {
    if (cards[key] == null || cardOf(key) === null) {
      const at = cardRows.findIndex((row) => row.kind === "file" && keyOf(row.file) === key);
      if (at < 0) return undefined;
      // A file chosen (`go`) lands at the band's foot at once, with the files on screen alone in its frame and the rest
      // after it (`land`, the compare-jump-speed pass, 2026-11-18); `n`, `p` and `c` bring the card near as before.
      if (!landing || scroller === null || !fileWin.land(at, bandCover())) fileWin.reveal(at);
      flushSync();
      // The lists the cards brought (their lines) hold their room before the card is measured, in one layout with the
      // read of where they sit: unplaced, a big file's card was its first screen of lines tall, and a card measured
      // under it stood a file's height from where it was put (the ux21 overs pass).
      placeWindowsNow();
    }
    return cards[key] ?? undefined;
  }

  /** How much of the scroller's top the band covers: none where the band does not stick (no file list). */
  const bandCover = () => (list && band !== null ? band.getBoundingClientRect().height : 0);

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

  /**
   * A file chosen (picked in the list, `j`/`k`): its head lands at the scroller's top, under the band where there is
   * one, and is held there through what draws after it until the reader scrolls (the compare-jump pass, 2026-11-17).
   * Scrolled into view at the scroller's top, a file sat under the band: a short one (a line or two, a rename, a binary
   * or collapsed file) could not push its sticky head out from under it, so the reader saw the next file's head and
   * the rail marked the one before. And the topmost row on screen then held the page, the file before the picked one,
   * whose lines arriving pushed it down.
   */
  function go(key: string) {
    current = key;
    const card = cardFor(key, true);
    const el = cardOf(key);
    landed = null;
    if (card !== undefined && el !== null && scroller !== null) {
      landOn(el, scroller, reach(scroller, el.getBoundingClientRect().top - topOfFiles(scroller)));
      landed = key;
    } else card?.scrollTo();
    hold();
  }

  /**
   * The file a pick landed, until the reader touches the page (a wheel, a press, a touch, a key): a scroll before that
   * is the page's, not the reader's, and the file goes back to the band's foot in the frame it would have moved. The
   * page near the list's end, shortened for a layout while a file's lines came in, was cut back to its new end and
   * stayed there once the lines filled it out again (the compare-jump pass, 2026-11-17: the last files, picked, landed
   * a screen or more from where they were put).
   */
  let landed: string | null = null;
  const letGo = () => (landed = null);

  /**
   * Puts the landed file back where it was put, if the page moved it. The lists have read the view it was moved to by
   * then, and may have taken the file's card out: it is drawn again first.
   */
  function reland(box: Element): boolean {
    if (landed === null) return false;
    cardFor(landed);
    const el = cardOf(landed);
    if (el === null) return false;
    const off = el.getBoundingClientRect().top - topOfFiles(box);
    if (Math.abs(off) < 1) return false;
    // A file near the list's end, where the scroll's end stops it below the band: it is where it lands, and asking for
    // the scroll the end refuses had every list read the view again on each of the page's scrolls.
    const go = reach(box, off);
    if (Math.abs(go) >= 1) landOn(el, box, go);
    return true;
  }

  /** `px` of scroll, as far as `box` can go that way: not past its start or its end. */
  function reach(box: Element, px: number): number {
    const top = box.scrollTop;
    return Math.max(-top, Math.min(px, box.scrollHeight - box.clientHeight - top));
  }

  /** Where the files' view starts on screen: the scroller's top, or the band's foot where the band sticks over it. */
  function topOfFiles(box: Element): number {
    return box.getBoundingClientRect().top + box.clientTop + bandCover();
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
    const read = (event?: Event) => {
      // A page kept hidden has no boxes to measure, every card at the top: it keeps the file it was in until shown. The
      // shell's scroller stays shown around a hidden kept page, so it is the files' own box that says.
      if (host.checkVisibility?.() === false) return;
      if (heldAt !== null && Math.abs(found.scrollTop - heldAt) < SCROLLED) return;
      // The lists holding the chosen file where it landed (lines read in above it) moved the page, not the reader.
      if (heldAt !== null && event !== undefined && listScrolled(event)) {
        heldAt = found.scrollTop;
        return;
      }
      // The page moved under a pick the reader has not touched since: the file goes back.
      if (heldAt !== null && event !== undefined && landed !== null && reland(found)) {
        heldAt = found.scrollTop;
        return;
      }
      heldAt = null;
      const last = drawn.at(-1);
      if (last !== undefined && found.scrollHeight > found.clientHeight && found.scrollTop + found.clientHeight >= found.scrollHeight - SCROLLED) {
        current = keyOf(last);
        return;
      }
      // Under the band: a file whose head stands at its foot is the one you are in.
      const top = topOfFiles(found) + 8;
      let at: string | null = null;
      // The files drawn are the ones near the view: the one at its top is among them.
      for (const file of nearFiles()) {
        const key = keyOf(file);
        const card = host.querySelector<HTMLElement>(`[data-file="${CSS.escape(key)}"]`);
        if (card === null) continue;
        if (card.getBoundingClientRect().top > top) break;
        at = key;
      }
      const first = nearFiles()[0];
      current = at ?? (first === undefined ? null : keyOf(first));
    };
    found.addEventListener("scroll", read, { passive: true });
    const touched = ["wheel", "pointerdown", "touchstart"] as const;
    for (const name of touched) found.addEventListener(name, letGo, { passive: true, capture: true });
    // Before the view's own keys, so a `j` lets go of the last pick and lands its own.
    window.addEventListener("keydown", letGo, { capture: true });
    // The first read waits for the files' first layout (a resize observer's first call, before the frame paints): read as
    // they are attached, its visibility and boxes laid out the half-built page in the task that builds it (the inner-6x
    // pass, 2026-11-15: a style pass of 1,200 elements in a return to *Files changed* at 6x).
    const off = () => {
      found.removeEventListener("scroll", read);
      for (const name of touched) found.removeEventListener(name, letGo, { capture: true });
      window.removeEventListener("keydown", letGo, { capture: true });
    };
    if (typeof ResizeObserver === "undefined") {
      read();
      return off;
    }
    const first = new ResizeObserver(() => {
      first.disconnect();
      read();
    });
    first.observe(host);
    // The files growing or shrinking above a landed pick (their lines coming in) move it with no scroll to hear: it is
    // put back after the layout that moved it, before that frame is painted.
    const grew = new ResizeObserver(() => {
      if (landed !== null && heldAt !== null && host.checkVisibility?.() !== false && reland(found)) heldAt = found.scrollTop;
    });
    grew.observe(host);
    return () => {
      first.disconnect();
      grew.disconnect();
      off();
    };
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
        // A generated file has no *Viewed*: it is not a file of the change.
        if (!viewed || here.made !== undefined) return;
        current = keyOf(here);
        setViewed(here, !isViewed(here.path));
        break;
      case "c": {
        if (!review) return;
        // The file `j` or `n` reached, else the next one with lines to comment on: a collapsed file has none.
        const from = Math.max(0, at);
        const taker = [...keys.slice(from), ...keys.slice(0, from)].find((key) => cardFor(key)?.comment());
        if (taker === undefined) {
          // A rendered card has no lines to comment on (ux26, ruling 1): it says how to get them.
          const rendered = modeOf(here) === "rendered" && !isClosed(here);
          push({ text: rendered ? "No open file takes a comment here. Open one as lines, then press c." : "No open file takes a comment here. Open one, or show its raw diff, then press c." });
          break;
        }
        current = taker;
        hold();
        break;
      }
      case "d":
        toggleRendered();
        break;
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

  /**
   * **The band** (the big-compare pass, 2026-11-16): the toolbar sticks to the top of the page's scroller through a long
   * diff, the file list sticking under it, and once it has stuck it ends in *Back to top*, which takes the page back to
   * its start: Compare's branches and *Create pull request*, a pull request's tabs. Whether it has stuck is read off a
   * mark of no height just above it, which the scroller's top has passed.
   */
  let stuck = $state(false);
  let band = $state<HTMLElement | null>(null);
  function stickMark(mark: HTMLElement) {
    const root = mark.closest(".tf-main, .tf-bmain");
    if (root === null || typeof IntersectionObserver === "undefined") return;
    const watch = new IntersectionObserver(
      ([entry]) => {
        if (entry !== undefined) stuck = !entry.isIntersecting && entry.boundingClientRect.top < (entry.rootBounds?.top ?? 0);
      },
      { root },
    );
    watch.observe(mark);
    return () => watch.disconnect();
  }
  /** The band's height, for the file heads and the file list that stick under it. */
  function bandRoom(node: HTMLElement) {
    band = node;
    if (typeof ResizeObserver === "undefined") return;
    const host = node.parentElement;
    const watch = new ResizeObserver(([entry]) => {
      const px = entry?.borderBoxSize?.[0]?.blockSize ?? node.offsetHeight;
      host?.style.setProperty("--tf-dband", `${px}px`);
    });
    watch.observe(node);
    return () => watch.disconnect();
  }
  /** *Back to top*: the page's start, focus on the page so its keys go on. */
  function toTop() {
    const root = band?.closest<HTMLElement>(".tf-main, .tf-bmain");
    if (!root) return;
    root.scrollTop = 0;
    heldAt = null;
    landed = null;
    root.focus({ preventScroll: true });
  }

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

{#snippet fileCard(file: DrawnFile)}
      {@const key = keyOf(file)}
      {@const piped = pipedOf(file)}
      {@const got = lined[key]}
      {@const planned = planOf(file)}
      <TfDiffCard
        bind:this={cards[key]}
        file={piped?.file ?? lineOf(file)}
        waiting={waits(file)}
        counted={counted && (file.made === undefined || got !== undefined)}
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
        viewed={viewed !== null && file.made === undefined}
        done={isViewed(file.path)}
        onviewed={(on) => setViewed(file, on)}
        loaded={loads[key] === true}
        onload={() => (loads = { ...loads, [key]: true })}
        current={current === key}
        rendered={planned?.plan ?? null}
        renderedAs={planned?.path ?? null}
        mode={modeOf(file)}
        onmode={(mode) => (modes = { ...modes, [key]: mode })}
        sideOf={(side) => sideOf(file, side)}
        load={(side) => loadSide(file, side)}
        epoch={sideEpoch}
        ends={sideNames?.(file) ?? null}
        choices={planned?.choices ?? []}
        opened={planned?.opened ?? null}
        pattern={planned ? makeDefaultPattern(planned.opened, planned.path ?? file.path) : ""}
        onpick={(r) => (picked = { ...picked, [key]: r.id })}
        onmakedefault={(r) => planned && void makeDefault(file, planned.opened, r, planned.path ?? file.path)}
        {frame}
        {commands}
        expandable={context !== null && piped === null && file.made === undefined}
        expanded={piped ? undefined : expandedOf(key)}
        onexpand={(hunk, how) => void expand(lineOf(file), hunk, how)}
        tokensOf={(side, cell) => tokensOf(file, side, cell)}
        talk={piped || file.made !== undefined ? quiet : talk}
        {render}
        openAt={file.made === undefined ? openAt : null}
        {repository}
        place={fileWin.row(key)}
      />
{/snippet}

{#snippet mine(body: string)}
  <div class="tf-thread tf-pending"><p><b>You</b> <TfStateChip text="pending" tone="warn" /> {body}</p></div>
{/snippet}

<div class="tf-dview">
<TfFindBar root={() => filesHost} within="in open files" more={heldBack} />
{#if list}<div class="tf-dmark" aria-hidden="true" {@attach stickMark}></div>{/if}
<div class="tf-dtool" class:tf-dband={list} class:tf-stuck={list && stuck} {@attach bandRoom}>
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
  {#if list || anyPipe}
  <div class="tf-fchips">
    {#if list}
      <!-- Always drawn where the diff has its list: worked out per file, it would land late and move *Run pipelines*. -->
      <button type="button" class="tf-drendered" aria-pressed={rendering.on} aria-keyshortcuts="d" title="Show files and folders as their renderers draw them: d" onclick={toggleRendered}
        >Rendered</button
      >
    {/if}
    {#if anyPipe}
      {@const all = pipeAll ?? piping.on}
      <button
        type="button"
        aria-pressed={all}
        title="Run each file's default pipeline on both sides of this diff"
        onclick={() => {
          pipeAll = !all;
          pipeOne = {};
        }}>Run pipelines</button
      >
    {/if}
  </div>
  {/if}
  {#if viewed && files.length > 0}
    {@const count = files.filter((file) => isViewed(file.path)).length}
    <span class="tf-dtool-prog">{count} of {files.length} viewed</span>
  {/if}
  {#if offered && rendering.on && groups.length > 0}
    <!-- The grouped folders' runs (ux26, section 10): only a press runs, at most 20 folders a press. -->
    <span class="tf-drun">
      <span class="tf-drun-n">{thousands(folderRuns.done)} of {thousands(groups.length)} {groups.length === 1 ? "folder" : "folders"} rendered</span>
      <button type="button" class="tf-btn tf-sm" disabled={!folderRuns.going && folderRuns.runnable.length === 0} onclick={runFolders} {@attach keepWidth(runLabel)}>{runLabel}</button>
    </span>
  {/if}
  {#if list}
    <!-- Drawn once the band has stuck: at the diff's start there is nothing above it to go back to. -->
    <button type="button" class="tf-btn tf-sm tf-ghost tf-dtop" title="The page's start: Home" tabindex={stuck ? undefined : -1} onclick={toTop}>Back to top</button>
  {/if}
</div>
{#if cut}
  <p class="tf-dcut">
    This diff is over knobas's ceiling of {thousands(DIFF_CEILING)} lines: it is drawn up to there, and the source's own page has the rest.
  </p>
{/if}
<div class:tf-dgrid={list} class:tf-rail-shut={list && !railOpen} class:tf-dheld={holding} {@attach sideWidth("diff-files")}>
  {#if list}<TfSideEdge side="diff-files" />{/if}
  {#if list}
    <TfRailBar label="File" value={current === null ? "None" : nameOf(current)} open={railOpen} controls="{uid}-files" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} />
    <div class="tf-dnav tf-rail" id="{uid}-files" bind:this={filesRail}>
      <!-- The filter sticks with the list under the band, so a file is found by name from anywhere in the diff. -->
      <div class="tf-dstick">
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
            {@const glyph = glyphOf(row.path)}
            {#if offered && savedFolders.length > 0}
              <!-- A folder a renderer takes (ux26): the Files tree's glyph at the row's end, its room kept while any renderer is saved. -->
              <div class="tf-fdir tf-dfr" {@attach railWin.row(treeKey(row))}
                ><span class="tf-dfr-p">{row.path}</span><span class="tf-dfr-g"
                  >{#if glyph !== null}{@const by = glyph.taking.find((r) => r.id === pickOf(glyph.folder)) ?? glyph.taking[0]}<button
                      type="button"
                      class="tf-dfmark"
                      data-glyph={glyph.folder}
                      title={glyph.taking.length > 1 && pickOf(glyph.folder) === null ? `Compare ${glyph.folder || "the top folder"} as a renderer makes it…` : `Compare ${glyph.folder || "the top folder"} as ${by?.name} makes it`}
                      aria-label={glyph.taking.length > 1 && pickOf(glyph.folder) === null ? `Compare ${glyph.folder || "the top folder"} as a renderer makes it…` : `Compare ${glyph.folder || "the top folder"} as ${by?.name} makes it`}
                      aria-haspopup={glyph.taking.length > 1 && pickOf(glyph.folder) === null ? "menu" : undefined}
                      onclick={(event) => pressGlyph(event, glyph)}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 3.5h11v9h-11z M2.5 10l3-3 2.5 2.5 2-2 3.5 3.5" /><circle cx="10.5" cy="6" r="1" /></svg></button
                    >{/if}</span
                ></div
              >
            {:else}
              <div class="tf-fdir" {@attach railWin.row(treeKey(row))}>{row.path}</div>
            {/if}
          {:else if row.kind === "gdir"}
            <!-- A grouped folder (ux26, mock tag 8): its row where its directory header would be. -->
            <div class="tf-fdir tf-dgdir" data-folder={row.group.folder} {@attach railWin.row(treeKey(row))}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 3.5h11v9h-11z M2.5 10l3-3 2.5 2.5 2-2 3.5 3.5" /><circle cx="10.5" cy="6" r="1" /></svg><TfPath path={row.group.folder === "" ? "The top folder" : row.group.folder} /></div>
          {:else if row.kind === "gband"}
            {@const group = row.group as ShownGroup}
            <button type="button" class="tf-dgband" data-folder={group.folder} title="Show {group.folder || 'the top folder'}'s real files" onclick={() => putBack(group.folder)} {@attach railWin.row(treeKey(row))}
              ><span>{bandWords(group)}</span><span>Real files</span></button
            >
          {:else}
            {@const file = row.file}
            {@const done = isViewed(file.path)}
            {@const badge = badgeOf(file)}
            {@const how = / \((?:[^()]*, )?(not staged|staged|untracked)\)$/.exec(row.name)?.[1] ?? null}
            {@const counts = file.made === undefined ? file : (lined[keyOf(file)]?.file ?? null)}
            <button
              type="button"
              class:tf-viewed={done}
              class:tf-d1={row.depth === 1}
              class:tf-dmade={row.made === true}
              aria-current={current === keyOf(file) ? "true" : undefined}
              title={file.made === undefined ? keyOf(file) : file.path}
              onclick={() => pick(file)}
              {@attach railWin.row(treeKey(row))}
              {@attach menuThing(() => (openAt !== null && file.made === undefined && file.change !== "deleted" ? { kind: "file", repoId: openAt.entityId, path: file.path } : null))}
              ><span class="tf-bd tf-bd-{badge[0] === '+' || badge[0] === '−' ? 'X' : badge[0]}">{badge}</span><span class="tf-fname">{how === null ? row.name : row.name.slice(0, row.name.lastIndexOf(" ("))}</span>{#if how !== null}<span class="tf-state">{how}</span>{/if}<span class="tf-n"
                >{#if file.change !== "binary" && counted && counts !== null}<span class="tf-add-n">+{counts.additions}</span> <span class="tf-del-n">−{counts.deletions}</span>{/if}{done ? " ✓" : ""}</span
              ></button
            >
          {/if}
        {/each}
        {#if railWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach railWin.room("after")}></div>{/if}
      </nav>
      {/if}
      </div>
      <p class="tf-dkeys tf-dim">
        <span><kbd>j</kbd><kbd>k</kbd> files</span><span><kbd>n</kbd><kbd>p</kbd> hunks</span>{#if viewed}<span><kbd>v</kbd> viewed</span>{/if}{#if review}<span
            ><kbd>c</kbd> comment</span
          >{/if}<span><kbd>d</kbd> rendered</span><span><kbd>s</kbd> split</span>
      </p>
    </div>
  {/if}
  <div class="tf-dfiles" {@attach spy} {@attach measure}>
    <div class="tf-dcards" {@attach fileWin.list}>
    {#each cardSegments as seg (seg.key)}
      {#if seg.kind === "file"}
        {@render fileCard(seg.file)}
      {:else if seg.kind === "folder"}
        {@const group = seg.group}
        <div class="tf-dfolder" data-folder={group.folder} {@attach fileWin.row(seg.key)}>
          {#if group.compare !== undefined}
            <TfDiffFolder
              folder={group.folder}
              renderer={group.group.renderer}
              renderers={takingOf(group.folder)}
              compare={group.compare}
              lists={group.lists}
              ends={endsOf(group.group)}
              onpick={(id) => pickFolder(group.folder, id)}
              onreal={() => putBack(group.folder)}
            />
          {/if}
        </div>
      {:else}
        <!-- A grouped folder's generated files, inset under its bar with the info rail (ux26, mock tag 9). -->
        <div class="tf-dunder">
          {#each seg.rows as row (rowKey(row))}
            {#if row.kind === "file"}
              {@render fileCard(row.file)}
            {:else if row.kind === "end"}
              <p class="tf-dfend" data-end="{row.group.folder}{'\u0000'}" {@attach fileWin.row(rowKey(row))}>
                {thousands(row.group.same)} generated {row.group.same === 1 ? "file is" : "files are"} the same on both sides and {row.group.same === 1 ? "is" : "are"} not listed.
              </p>
            {/if}
          {/each}
        </div>
      {/if}
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
{#if glyphMenu !== null}
  {@const at = glyphMenu.folder}
  <!-- Several renderers take the folder and none was picked (ruling 10): choosing one is its pick, and it is grouped. -->
  <TfFolderPickMenu
    bind:open={glyphOpen}
    opener={glyphMenu.opener}
    folder={at}
    taking={glyphMenu.taking}
    onpick={(r) => {
      glyphOpen = false;
      if (folders !== null) void picks.set(folders.entityId, folders.entityId, at, r.id).catch(() => {});
      groupAt(at, r, true);
    }}
  />
{/if}
