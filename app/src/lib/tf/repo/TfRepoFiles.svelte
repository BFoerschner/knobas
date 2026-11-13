<!--
  The repository page's *Files* tab (#838, R-8.5): the tree on the left, the
  file on the right, as prototype-6's `.repo`.

  The tree is the source's, a directory at a time (`entity_view` with the
  section a listing names, #711): the top directory first, then each directory
  opened, and every directory above the file the address names. A file the
  local checkout changed is marked `M`; open, it warns above its text. The
  file is drawn with its line numbers; *History* goes to Commits. *Open in…*
  (#844) opens the file in the checkout, with an application's file command.

  *Edit* (#845, R-8.6) is the file's address under `edit/`: the file turns
  into an editor with line numbers under the banner that the commit goes to
  the source and the checkout is never written, and *Commit change…* opens
  `TfCommitFile`. Only a file the source gave a blob sha for (#883) can be
  edited, since that is what the commit is checked against; a binary or cut
  file cannot either, and *Edit* says why. A commit queued this session is a
  banner on the file, its echo (`tf/queued.svelte.ts`, #2105): *queued*, then
  *sent*, until the source holds a newer blob, the file read again on the
  refresh event and a run's end.

  With no file in the address the top directory's README opens, under the
  branch's last commit (`TfRepoLastCommit`, #2257): the landing is the tree,
  the last commit and the README, with no second listing of the top.

  **Read from the checkout first** (ADR-0041, the smoothness pass of PR
  #2479): where the repository has a checkout that holds the commit the
  mirror names for the branch, the whole tree is read there at once
  (`entity_local_tree`) and drawn before the source answers, its files read
  there too (`entity_local_file`), under section ids the tab makes itself
  (`LOCAL`), never the source's. The source's top is still read behind it:
  where it stands at the same commit nothing moves and the tree says it was
  checked; where the branch moved since the last sync the source's tree
  replaces the checkout's in place, the open directories and the file kept,
  and a file whose text changed says so. A checkout that lacks the commit is
  fetched into behind the read (`checkout:fetched`), so the next open is
  local.

  **Read at a commit** (#2241): the source reads the tree at the branch's tip
  and the sections its listings name stand at that commit, so a file opened
  from them costs the source its own two reads, and the app keeps it. Whether
  a commit straight onto the branch is taken is the page's `refused_writes`
  answer (`refusal`), not part of the file's read. A refresh, a commit's echo
  and the app's `view:fresh` for the top directory (#2242) read the top again;
  where the tip moved its sections are new, and the tree and the file are read
  at the new commit, the file drawn kept, dimmed, meanwhile.

  A file over 2,000 lines is a windowed list (#2251, `shell/windowed.svelte.ts`):
  only the lines near the viewport are drawn, the rest are room above and
  below. A line link (`line/<n>/<path>`) draws its line, marks it and scrolls
  it to the middle. Copying a selection across lines copies their text from
  the file, not from the drawn nodes, so the lines between need not be drawn.

  The file is drawn by the first renderer that accepts it (`render/registry.ts`,
  #2254): *Source* draws the lines plain at once and colours them as the
  highlighting worker answers, the lines in view first; *Image* draws an SVG;
  a command renderer (#2262) runs on the backend and draws in a sandboxed
  frame (`../render/TfCommandOutput.svelte`).
  A Markdown file is drawn by the Markdown component (#2255): its relative
  links open files here, external ones go to the browser. Which renderer a
  file opens with is the default per file pattern (#2263, `render/defaults.ts`);
  the header's *Render as ▾* (`../render/TfRendererSwitch.svelte`) switches
  this file to any other renderer that takes it, for as long as the tab lives,
  and makes one the default for the file's pattern.
  *Find in file* (#2299) marks every hit, "N of M"; `Enter` and `Shift+Enter`
  step through them, drawing a hit's line first where the window left it out.

  **Go to file and the keys** (#2311): the tree's head is *Go to file*, which
  `T` focuses; typed into, it lists the paths holding every word among every
  file at the commit -- the source's whole tree, read once the field is
  first used and kept by its sha -- and `Enter` opens the first. A tree the
  source cut is searched as far as it was read, and says so; a source with
  no whole tree is searched among the directories read so far. In the tree,
  `↑`/`↓` move between the rows, `←`/`→` fold and unfold, `Enter` opens; no
  key is taken while a field has focus, while the page is held hidden, nor
  while a dialog or the launcher is open over it.
  The file's header says its size, its lines or kind and the commit it is
  read at; *More* (⋯) holds *Edit*, *History* and its page on the source.
-->
<script lang="ts">
  import { filterKeys } from "../filter-keys";
  import { flushSync, tick, untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { EVENTS, ipcErrorMessage } from "../../ipc";
  import { openLink } from "../../shell/external-links";
  import type { CheckoutView, Fetched, LocalDir, TreeEntry, TreeSections, ViewBlock, ViewFresh, ViewSection } from "../../ipc/entity";
  import type { WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import { renderPort, type HighlightJob, type LineTokens, type RenderPort } from "../../render/port";
  import { allRenderers, BYTES_CAP, type Renderer } from "../../render/registry";
  import { defaultPipeline, makeDefaultPattern, openWith, withDefault, withPipeline } from "../../render/defaults";
  import { rendererDefaults as appDefaults, type RendererDefaults } from "../../render/defaults.svelte";
  import TfRendererSwitch from "../render/TfRendererSwitch.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfDiagram from "../render/TfDiagram.svelte";
  import TfBytesImage from "../render/TfBytesImage.svelte";
  import TfPdf from "../render/TfPdf.svelte";
  import { findCount, stepMatch } from "../builds/run-reads";
  import { fileHits, firstHitOn, lineOfHit, needles, segments } from "./file-lines";
  import { echoOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfCommitFile from "./TfCommitFile.svelte";
  import TfCommandOutput from "../render/TfCommandOutput.svelte";
  import TfFilePipeline from "../render/TfFilePipeline.svelte";
  import type { PipeRun, PipeStep } from "../../ipc/entity";
  import { inputOf, pipedFile, pipelinePattern, savedAs, sized, stepName, suggestion } from "../../render/pipeline";
  import {
    commandSteps as appCommandSteps,
    namedPipelines as appPipelines,
    type CommandSteps,
    type NamedPipelines,
  } from "../../render/pipelines.svelte";
  import { commandIdOf, ipcCommands, type CommandPort } from "../../render/command";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { ago } from "../../shell/time";
  import TfRepoLastCommit from "./TfRepoLastCommit.svelte";
  import { drawnFile, forgetDrawnFiles, hasDrawnFile, keepDrawnFile } from "./drawn-files";
  import {
    REPO_FILES,
    ancestors,
    commitKind,
    editHash,
    fileHash,
    fileLines,
    lineHash,
    likelySections,
    pickDefaultFile,
    repoHash,
    selectedText,
    sourceName,
    type TextPlace,
  } from "./repo";
  import TfWaiting from "../TfWaiting.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { waitPhase } from "../waiting";
  import { inTextField } from "../../shell/keys";
  import { layers } from "../layers.svelte";
  import { checkoutListing, fileMeta, goToFile, treeKey, treeRows } from "./file-tree";
  import TfFileMore from "./TfFileMore.svelte";
  import TfFindBar from "../TfFindBar.svelte";

  type FileBlock = Extract<ViewBlock, { type: "file" }>;

  let {
    ports,
    source = null,
    repoId,
    at,
    branch,
    path,
    line = null,
    changed,
    editing = false,
    job = null,
    checkoutBranch = null,
    checkout = undefined,
    title = "",
    refreshed = 0,
    refusal = null,
    render = renderPort(),
    commands = ipcCommands,
    defaults = appDefaults,
    pipelines = appPipelines,
    stepCommands = appCommandSteps,
    treeSections = null,
    ondrew,
  }: {
    ports: Pick<RepoPorts, "entityView" | "submitWrite" | "listen" | "fileBytes" | "piped" | "localDir" | "localPaths" | "localFile">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    repoId: string;
    /** The entity the tree is read at: the repository, or the branch picked. */
    at: string;
    /** The branch's name, for *on <branch>*. */
    branch: string | null;
    /** The file the address names, or `null`. */
    path: string | null;
    /** The line a line link marks, from 1; `null` for none. */
    line?: number | null;
    /** The paths the local checkout changed. */
    changed: ReadonlySet<string>;
    /** The address is the file's edit (`edit/<path>`). */
    editing?: boolean;
    /** The job's key, for the commit's message. */
    job?: string | null;
    /** The branch the local checkout is on, `null` without one. */
    checkoutBranch?: string | null;
    /** The repository's checkout, for *Open in…*; `undefined` while it is read. */
    checkout?: CheckoutView | undefined;
    /** The repository's `owner/name`. */
    title?: string;
    /** The page's refreshes (#2135): each reads the open directories and the file again over what the tab drew. */
    refreshed?: number;
    /** Why the source refuses a commit straight onto the branch (#985, #2241), `null` where it takes one. */
    refusal?: string | null;
    /** The engines renderers draw with (#2254): the app's workers, a fake in tests. */
    render?: RenderPort;
    /** Runs a command renderer (#2262): the IPC, a fake in tests. */
    commands?: CommandPort;
    /** The default renderer per file pattern (#2263): the app's copy, a fresh one in tests. */
    defaults?: RendererDefaults;
    /** The named pipelines (#2265): the app's copy, a fresh one in tests. */
    pipelines?: NamedPipelines;
    /** The person's command steps (#2265): the app's copy, a fresh one in tests. */
    stepCommands?: CommandSteps;
    /** How the source names the tree's sections by path (#2298, the repository's `EntityDetail.tree`); `null` asks a deep link's sections only once its listings name them. */
    treeSections?: TreeSections | null;
    /** Told once the tree has drawn, or could not be: the header holds its first look until then (the inner-pages pass). */
    ondrew?: () => void;
  } = $props();

  $effect(() => {
    if (listings[""] !== undefined || treeFailed !== null) untrack(() => ondrew?.());
  });

  /**
   * Each directory read so far at `at`, by its path (`""` the top). Replaced whole, never written into: a clone's tree
   * is every directory at once, and a deep proxy over ten thousand entries cost a folder's press more than its rows
   * (the big-folder pass, 2026-11-13).
   */
  let listings = $state.raw<Record<string, TreeEntry[]>>({});
  /**
   * The checkout's tree at the mirror's tip (ADR-0041), while the listings are its: the commit it was read at, each
   * folder read when it opens (the big-folder pass, 2026-11-13); `checked` once the source's top stood at the same commit.
   */
  let local = $state<{ commit: string; checked: boolean; asOf: string | null } | null>(null);
  /** A folder the source listed past the whole tree it reads at once, one at a time (`read_alone`): the tree says so. */
  let readAlone = $state(false);
  /** Folders the source cut at its cap, by path: how many entries it shows, and its page there. */
  let cutDirs = $state.raw<Record<string, { shown: number; url: string | null }>>({});
  /** The branch moved past the checkout's copy, and the file the reader had open changed with it. */
  let movedNote = $state<string | null>(null);
  /** The file drawn from the checkout when the source's newer tree replaced it: its path and blob. */
  let movedFrom: { path: string; sha: string | null } | null = null;
  let treeFailed = $state<string | null>(null);
  /** The section the top listing names for every file at its commit (#2311), `null` where the source has none. */
  let pathsSection = $state<string | null>(null);
  const open = new SvelteSet<string>();
  let file = $state<FileBlock | null>(null);
  let fileFailed = $state<string | null>(null);
  /** The file shown is being read: the one drawn before stays, dimmed, until it answers (#2246). */
  let fileReading = $state(false);
  /** The commit the source's top directory named when it answered, at the current `at`; `null` before, or where it named none. */
  let sourceTop: string | null = null;
  /** When knobas read that top, where it was what knobas held (`held_at`, ADR-0042); `null` for a read made now. */
  let sourceTopHeld: string | null = null;
  /**
   * The file drawn stands at a commit before the tree's, and its read at the tree's commit is on its way (the forge-waits
   * pass, 2026-11-13: after a sync moved the branch, the file waited 2.1 s on Gitea, dimmed): drawn undimmed meanwhile,
   * its header saying from when it is, and replaced in place when the clone or the source answers. `failed` where the
   * source did not.
   */
  let fileBehind = $state<{ heldAt: string; failed: string | null } | null>(null);
  /** When knobas read each answer of the source's this tab drew: `held_at` where it was held, else when it came. */
  const readAtOf = new WeakMap<ViewSection, string>();
  /** Directory reads asked and not answered, at the current `at`. */
  let asked = new Set<string>();
  // Each file the tab drew is kept by the listing's section that opens it (`drawn-files.ts`, the inner-pages pass).
  /** The read the file drawn came from, which a listing's section can stand for once it names it. */
  let drawnRead: ViewSection | undefined;
  /**
   * Keeps the file `read` drew under `section`, where the section names where the file stands: a listing's, at a commit.
   * A file read by its path alone may have changed by the next look, so that is read again, as before.
   */
  function keepFile(section: string, read: ViewSection | undefined, path: string) {
    if (section === GUESSED || section.startsWith(LOCAL) || section === MISSING) return;
    // Where the source declares no scheme there is no telling a listing's section from a read by path: none is kept.
    if (treeSections == null || section === `${treeSections.file}${path}`) return;
    keepDrawnFile(at, section, read);
  }
  /**
   * Reads a deep link asked before a listing named their section (#2244), by
   * section, at the current `at`: the listing that names one takes it.
   */
  let early = new Map<string, Promise<ViewSection | undefined>>();
  let generation = 0;

  /** A tree of more rows than this draws only the rows near the view (the big-folder pass, 2026-11-13). */
  const TREE_WINDOW_FROM = 120;

  const join = (dir: string, name: string) => (dir === "" ? name : `${dir}/${name}`);
  const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");
  const nameOf = (full: string) => full.slice(full.lastIndexOf("/") + 1);

  /** A deep link's early read of its file has failed, or there is none: nothing but the listings can draw it now. */
  let guessGone = $state(false);

  /** The entry `full` names in its parent's listing, once that is read. */
  function entryOf(full: string): TreeEntry | undefined {
    return listings[parentOf(full)]?.find((entry) => entry.name === nameOf(full));
  }

  /** The section `section` names, read at `where`: a directory's listing or a file and its actions. */
  async function readSection(section: string, where = at): Promise<ViewSection | undefined> {
    if (section.startsWith(LOCAL)) return readLocalSection(section, where);
    const view = await ports.entityView(where, section);
    const read = view?.sections[0];
    if (read !== undefined) {
      const held = view !== null && "held_at" in view && typeof view.held_at === "string" ? view.held_at : null;
      readAtOf.set(read, held ?? new Date().toISOString());
      if (held !== null) heldReads.add(read);
    }
    return read;
  }

  /** Whether `read` was answered from what knobas held, not read now: the backend said when (`held_at`). */
  const heldReads = new WeakSet<ViewSection>();
  const heldBefore = (read: ViewSection) => heldReads.has(read);

  /** The source's top was what knobas held from before the sync the clone's tree stands at: the clone's is newer. */
  function olderThanClone(heldAt: string | null, asOf: string | null): boolean {
    return heldAt !== null && asOf !== null && Date.parse(heldAt) < Date.parse(asOf);
  }

  // ---- The checkout's copy (ADR-0041) ----

  /** What begins every section id the tab makes for the checkout's tree: a control character, which no source's id holds. */
  const LOCAL = "\u0001local\u0000";
  const localId = (commit: string, kind: "dir" | "file" | "paths", full: string) => `${LOCAL}${commit}\u0000${kind}\u0000${full}`;
  function parseLocal(section: string): { commit: string; kind: string; full: string } | null {
    const [commit, kind, full] = section.slice(LOCAL.length).split("\u0000");
    return commit === undefined || kind === undefined || full === undefined ? null : { commit, kind, full };
  }

  /** One folder of the checkout's tree as the listing the tab draws, its entries opened by sections the tab makes. */
  const localListing = (commit: string, entries: LocalDir["entries"]) => checkoutListing(entries, (kind, full) => localId(commit, kind, full));

  /**
   * A section the tab made for the checkout's tree: a folder or every path, read from the tree the backend keeps (the
   * big-folder pass, 2026-11-13: the page holds what it draws, not the whole tree), or a file read at the checkout. A
   * folder the checkout's read could not hold whole (a tree past two million entries) is the source's listing.
   */
  async function readLocalSection(section: string, where: string): Promise<ViewSection | undefined> {
    const named = parseLocal(section);
    if (named === null) return undefined;
    const wrap = (block: ViewBlock): ViewSection => ({ id: section, title: "Files", count: null, deferred: false, blocks: [block] });
    if (named.kind === "file") {
      const read = ports.localFile;
      const block = read === undefined ? null : await read(where, named.commit, named.full);
      if (block === null) throw new Error(`Your checkout could not read ${named.full}.`);
      return wrap(block);
    }
    if (named.kind === "paths") {
      const read = ports.localPaths;
      const answer = read === undefined ? null : await read(where, named.commit);
      if (answer === null) throw new Error("Your checkout could not list its files.");
      const entries = answer.entries.map((entry) => ({ name: entry.path, kind: entry.kind, size: entry.size, section: localId(named.commit, "file", entry.path) }));
      return wrap({ type: "tree", trail: [], entries, truncated: answer.truncated, url: null });
    }
    const read = ports.localDir;
    const answer = read === undefined ? null : await read(where, named.commit, named.full);
    if (answer === null) throw new Error(`Your checkout could not read ${named.full || "its top folder"}.`);
    if (!answer.complete) {
      // Past what the checkout's read holds: the source's listing of the folder, by the scheme it declares.
      if (treeSections == null) throw new Error(`${named.full} is past what knobas reads of your checkout at once.`);
      return readSection(`${treeSections.dir}${named.full}`, where);
    }
    return wrap({ type: "tree", trail: [], entries: localListing(named.commit, answer.entries), truncated: false, url: null });
  }

  /**
   * The checkout's tree at `where` (its top, at the mirror's tip), drawn where the source has not answered yet, or
   * taking over from the source's where it stands at the same commit.
   */
  function readLocal(where: string, mine: number) {
    const read = ports.localDir;
    if (read === undefined) return;
    read(where, null, "").then(
      (top) => (top === null ? false : land(top, where, mine)),
      // No checkout's copy: the source's reads draw the tree, as before.
      () => false,
    );
  }

  /**
   * The checkout's top `top`, at the mirror's tip, drawn as the tree: at once where nothing is drawn yet, or taking over
   * from the source's tree drawn first. `true` where it was taken.
   */
  async function land(top: LocalDir, where: string, mine: number, synced = false): Promise<boolean> {
    const read = ports.localDir;
    if (read === undefined || !top.complete || mine !== generation || where !== at) return false;
    // The source's top answered first (the app keeps it, so on a later open it beats `git ls-tree`): where it stands
    // at the clone's commit, the clone draws the tree after all, and its files are read there, not on the forge (the
    // inner-pages pass, 2026-11-12). At another commit the branch moved since the sync, and the source's tree stands;
    // unless the source's answer was what knobas held from before the sync the clone stands at (the forge-waits pass,
    // 2026-11-13: after a sync the page drew the old commit's tree, and read the old commit's files, until Gitea answered).
    const first = listings[""] !== undefined;
    // `synced`: a sync since moved the mirror's tip past the clone's tree drawn, so the clone's is the newer.
    const newer = synced || olderThanClone(sourceTopHeld, top.as_of);
    if (first && sourceTop !== top.commit && !newer) return false;
    const drawn: Record<string, TreeEntry[]> = { "": localListing(top.commit, top.entries) };
    if (first) {
      // Every folder the source's tree has drawn is read from the clone first and swapped in with the top, so no row
      // moves (the big-folder pass); one the clone cannot answer whole leaves the source's tree standing.
      const dirs = Object.keys(listings).filter((dir) => dir !== "");
      const more = await Promise.all(dirs.map((dir) => read(where, top.commit, dir).catch(() => null)));
      if (mine !== generation || where !== at || (sourceTop !== top.commit && !newer)) return false;
      for (const [i, dir] of dirs.entries()) {
        const answer = more[i];
        // A folder the newer commit no longer holds is left out; any other the clone cannot answer leaves the source's tree.
        if (answer === null && newer) continue;
        if (answer == null || !answer.complete) return false;
        drawn[dir] = localListing(top.commit, answer.entries);
      }
      // Taking over from the source's tree: its directory reads still under way would write the source's sections
      // over the clone's.
      generation += 1;
      asked = new Set(Object.keys(drawn));
      early = new Map();
    }
    local = { commit: top.commit, checked: first && sourceTop === top.commit, asOf: top.as_of };
    listings = drawn;
    pathsSection = localId(top.commit, "paths", "");
    return true;
  }

  /**
   * The clone at the mirror's tip again, where its tree is drawn (the forge-waits pass, 2026-11-13): on a return or a
   * refresh, and when a fetch behind lands. A sync that moved the branch moved the mirror's tip with it, and where the
   * clone holds the new commit its tree takes over in place at once, the file read there, before the source's read
   * behind says the branch moved (3 s on a forge that takes 1 s a request, and the file read from the forge after it).
   * Where the clone lacks it, the read fetches it behind (ADR-0041), and `checkout:fetched` asks again.
   */
  function rereadLocal(): Promise<unknown> {
    const held = local;
    const read = ports.localDir;
    const [where, mine] = [at, generation];
    if (held === null || read === undefined) return Promise.resolve();
    return read(where, null, "").then(
      (top) => {
        if (top === null || top.commit === held.commit || local?.commit !== held.commit || mine !== generation) return;
        // A tip the mirror stored after the one drawn: the clone's tree is newer than what the tab shows.
        if (held.asOf === null || top.as_of === null || Date.parse(top.as_of) <= Date.parse(held.asOf)) return;
        void land(top, where, mine, true);
      },
      () => {},
    );
  }

  /**
   * The source's top, read behind the checkout's copy: at the same commit it checks it and nothing moves; at another
   * the branch moved since the last sync, and the source's tree replaces the copy, the open directories and the file
   * read again through its sections. `true` where it was taken.
   */
  function settleLocal(block: Extract<ViewBlock, { type: "tree" }>): boolean {
    const held = local;
    if (held === null) return false;
    if (block.commit === undefined || block.commit === held.commit) {
      local = { ...held, checked: block.commit !== undefined };
      return true;
    }
    // What knobas held from before the sync the clone's tree stands at: older than the clone's, and read again behind.
    if (olderThanClone(sourceTopHeld, held.asOf)) return true;
    // The branch moved past the clone's tree. A sync since moves the mirror's tip with it (the forge-waits pass,
    // 2026-11-13: the tab went to the source's tree here and read every file on the forge, 2.1 s for a large one): the
    // clone is asked at the mirror's tip first, and where it stands at the source's commit it takes over in place. Where
    // it lacks that tip the read fetches it behind (ADR-0041), and `checkout:fetched` hands the tree back to it.
    const mine = generation;
    const toSource = () => {
      if (mine !== generation || local !== held) return;
      local = null;
      movedFrom = file === null ? null : { path: file.path, sha: file.sha ?? null };
      generation += 1;
      asked = new Set([""]);
      early = new Map();
      listings = { "": block.entries };
      pathsSection = block.paths ?? null;
    };
    const read = ports.localDir;
    if (read === undefined) {
      toSource();
      return true;
    }
    const where = at;
    read(where, null, "").then(
      async (top) => {
        if (top !== null && top.commit === block.commit && (await land(top, where, mine))) return;
        toSource();
      },
      toSource,
    );
    return true;
  }

  /**
   * What an early read is held by: the place, not the section, since a guess
   * names no commit and the listing's section does (#2241).
   */
  const dirKey = (dir: string) => `dir\u0000${dir}`;
  const fileKey = (full: string) => `file\u0000${full}`;
  /** A read of the checkout's asked early, apart from the source's guesses at the same place. */
  const localKey = (place: string) => `local\u0000${place}`;

  /** The read of `place` asked early, once, else -- or where that guess was refused -- a new one of `section`. */
  function take(section: string, place: string, where = at): Promise<ViewSection | undefined> {
    // A deep link's early guess at the source would only be slower than the checkout; the checkout's own read of a folder
    // the pointer rested on is taken (the big-folder pass).
    if (section.startsWith(LOCAL)) place = localKey(place);
    const read = early.get(place);
    if (read === undefined) return readSection(section, where);
    early.delete(place);
    return read.catch(() => readSection(section, where));
  }

  /**
   * The pointer resting on a row of the tree reads what a press on it opens, so the source's round trip runs while the
   * hand is on its way to the press (the inner-pages pass, 2026-11-12): a file or a directory a press would read from
   * the source, asked once, after the pointer has stood on it a moment, so one swept across the tree asks nothing. The
   * press takes the read under way (`take`). A folder of the checkout's copy is read early too (the big-folder pass:
   * the tab reads a folder when it opens, not the whole tree at once); its files need none.
   */
  const REST_MS = 80;
  let resting: ReturnType<typeof setTimeout> | null = null;
  function restOn(full: string, section: string | null, kind: "file" | "dir") {
    unrest();
    if (section === null || (section.startsWith(LOCAL) && kind === "file")) return;
    const place = section.startsWith(LOCAL) ? localKey(dirKey(full)) : kind === "file" ? fileKey(full) : dirKey(full);
    if (early.has(place) || (kind === "dir" ? asked.has(full) || listings[full] !== undefined : hasDrawnFile(at, section) || full === shown)) return;
    resting = setTimeout(() => {
      resting = null;
      if (!early.has(place)) askEarly(section, place);
    }, REST_MS);
  }
  function unrest() {
    if (resting !== null) clearTimeout(resting);
    resting = null;
  }
  $effect(() => unrest);

  /** Asks for `section` now, for the listing that names `place` later. */
  function askEarly(section: string, place: string): Promise<ViewSection | undefined> {
    const read = readSection(section);
    // A wrong guess fails unheard; the listing's own section is read instead.
    read.catch(() => {});
    early.set(place, read);
    return read;
  }

  function readDir(dir: string, section: string) {
    if (asked.has(dir) || listings[dir] !== undefined) return;
    asked.add(dir);
    const mine = generation;
    take(section, dirKey(dir)).then(
      (read) => {
        if (mine !== generation) return;
        const block = read?.blocks[0];
        if (dir === "" && block?.type === "tree") {
          sourceTop = block.commit ?? null;
          sourceTopHeld = read === undefined || !heldBefore(read) ? null : (readAtOf.get(read) ?? null);
        }
        if (dir === "" && block?.type === "tree" && settleLocal(block)) return;
        listings = { ...listings, [dir]: block?.type === "tree" ? block.entries : [] };
        if (block?.type === "tree" && block.read_alone) readAlone = true;
        if (block?.type === "tree" && block.truncated) cutDirs = { ...cutDirs, [dir]: { shown: block.entries.length, url: block.url } };
        if (dir === "") pathsSection = block?.type === "tree" ? (block.paths ?? null) : null;
      },
      (error) => {
        // The checkout's copy stands where the source cannot answer.
        if (mine === generation && !(dir === "" && local !== null)) treeFailed = ipcErrorMessage(error);
      },
    );
  }

  // A new branch is a new tree.
  $effect(() => {
    void at;
    generation += 1;
    sourceTop = null;
    sourceTopHeld = null;
    fileBehind = null;
    asked = new Set();
    early = new Map();
    listings = {};
    treeFailed = null;
    pathsSection = null;
    local = null;
    readAlone = false;
    cutDirs = {};
    movedNote = null;
    movedFrom = null;
    untrack(() => {
      readLocal(at, generation);
      readDir("", "files");
    });
  });

  // A fetch behind the checkout's read landed (ADR-0041): the checkout can draw the tree now, and where the source drew it
  // first at the same commit, it takes over, so the next folder opens from the clone and not from the forge (the
  // big-folder pass, 2026-11-13: a page opened while the clone was behind kept reading every folder from the forge).
  $effect(() => {
    const where = at;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void listen(EVENTS.checkoutFetched, ({ payload }) => {
      const fetched = payload as Fetched;
      if (!live || fetched.entity_id !== where) return;
      const held = untrack(() => local);
      if (held === null) readLocal(where, generation);
      else if (held.commit !== fetched.commit) void rereadLocal();
    }).then(
      (unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      },
      () => {},
    );
    return () => {
      live = false;
      stop?.();
    };
  });

  // The directories above the file the address names open.
  $effect(() => {
    if (path !== null) for (const dir of ancestors(path)) open.add(dir);
  });

  // Every open directory is read once its parent is.
  $effect(() => {
    for (const dir of open) {
      const entry = entryOf(dir);
      if (entry?.kind === "dir" && entry.section !== null) readDir(dir, entry.section);
    }
  });

  // A link to a file below directories not read yet asks for each of them
  // and the file at once (#2244), by the scheme its source declares (#2298);
  // the file draws as soon as it answers, and the tree fills in as its
  // listings arrive.
  $effect(() => {
    const where = at;
    const target = path;
    const declared = treeSections;
    if (target === null) return;
    return untrack(() => {
      if (listings[parentOf(target)] !== undefined) return;
      guessGone = false;
      const likely = likelySections(target, declared);
      if (likely === null) {
        guessGone = true;
        return;
      }
      for (const [dir, section] of Object.entries(likely.dirs))
        if (!asked.has(dir) && listings[dir] === undefined && !early.has(dirKey(dir))) askEarly(section, dirKey(dir));
      let live = true;
      const asking = askEarly(likely.file, fileKey(target));
      // Answered, it is spent: a later open of the file reads it again.
      const spent = () => {
        if (early.get(fileKey(target)) === asking) early.delete(fileKey(target));
      };
      asking.then(
        (read) => {
          spent();
          if (!live || where !== at || shown !== target) return;
          if (file !== null && file.path === target) return;
          drawFile(read, fileSection ?? GUESSED);
        },
        // Its directory's listing says whether there is such a file; one no listing holds has no directory to say so.
        () => {
          spent();
          if (live && where === at && shown === target) guessGone = true;
        },
      );
      return () => {
        live = false;
      };
    });
  });

  const shown = $derived(path ?? pickDefaultFile(listings[""] ?? []));

  const MISSING = "\u0000missing";

  /**
   * The section that opens the file shown, once its directory is read;
   * `missing` where the directory holds no such file.
   */
  const fileSection = $derived.by((): string | null => {
    if (shown === null) return null;
    // A directory on the way that its parent's listing does not hold: the file cannot be there, and its own listing never comes.
    for (const dir of ancestors(shown)) {
      if (dir !== "" && guessGone && listings[dir] === undefined && listings[parentOf(dir)] !== undefined && entryOf(dir) === undefined) return MISSING;
    }
    if (listings[parentOf(shown)] === undefined) return null;
    const entry = entryOf(shown);
    return entry === undefined || entry.section === null || entry.kind === "dir" ? MISSING : entry.section;
  });
  /** The section the file drawn was read through; the file drawn stays, dimmed, while the next one is read. */
  let drawnSection: string | null = null;
  /** `drawnSection` of a file a deep link's guess drew before its listing named its section. */
  const GUESSED = "\u0000guessed";

  function drawFile(read: ViewSection | undefined, section: string) {
    const block = read?.blocks[0];
    file = block?.type === "file" ? block : null;
    const was = movedFrom;
    movedFrom = null;
    movedNote =
      was !== null && file !== null && file.path === was.path && was.sha !== null && file.sha !== was.sha
        ? `${branch ?? "This branch"} moved on ${sourceName(source)} since the last sync, and this file changed with it: the newest is shown.`
        : null;
    drawnSection = section;
    drawnRead = read;
    if (file !== null) keepFile(section, read, file.path);
    fileReading = false;
    fileBehind = null;
    fileFailed = null;
    draft = null;
    settleCommit(file);
  }

  // The file, once its directory has said which section opens it; a deep
  // link's early read that drew it already stands.
  $effect(() => {
    const section = fileSection;
    const where = at;
    // Drawn already, by a deep link's early read or before a step away and back; or the checkout's tree took over from
    // the source's at the commit the file was read at, so it is the same file (the big-folder pass, 2026-11-13).
    const named = section?.startsWith(LOCAL) ? parseLocal(section) : null;
    const held = untrack(() => file);
    if (named !== null && named.kind === "file" && held !== null && held.path === named.full && held.commit === named.commit) {
      drawnSection = section;
      fileReading = false;
      fileFailed = null;
      return;
    }
    if (section !== null && section === untrack(() => drawnSection) && untrack(() => file) !== null) {
      fileReading = false;
      fileFailed = null;
      return;
    }
    // A deep link's guess drew this file before its listing named the section: it stands for it, where it stands at the
    // tree's commit. A guess read at the commit before a sync does not stand for the clone's tree at the commit after it
    // (the forge-waits pass, 2026-11-13: a page opened after a sync kept the file before it).
    const guessed = untrack(() => file);
    const guessAt = untrack(() => local?.commit ?? null) ?? sourceTop;
    if (
      section !== null &&
      section !== MISSING &&
      drawnSection === GUESSED &&
      guessed?.path === untrack(() => shown) &&
      !(typeof guessed?.commit === "string" && guessAt !== null && guessed.commit !== guessAt)
    ) {
      drawnSection = section;
      keepFile(section, drawnRead, untrack(() => shown) ?? "");
      fileReading = false;
      fileFailed = null;
      return;
    }
    fileFailed = null;
    // Waiting for the directory that says which section opens the file; with no file to show, nothing is drawn.
    const showing = untrack(() => shown) !== null;
    // The file drawn is this one at a commit before the tree's (a sync or a push moved the branch): it stays drawn,
    // undimmed, saying from when it is, until the read at the new commit replaces it (the forge-waits pass, 2026-11-13).
    const before = untrack(() => file);
    const treeAt = untrack(() => local?.commit ?? null) ?? sourceTop;
    const behind =
      section !== null &&
      section !== MISSING &&
      before !== null &&
      before.path === untrack(() => shown) &&
      typeof before.commit === "string" &&
      treeAt !== null &&
      before.commit !== treeAt;
    fileReading = showing && !behind;
    // The clone answers in a few milliseconds: only a read of the source is said to be waited on.
    fileBehind = behind && !section.startsWith(LOCAL) ? { heldAt: (drawnRead && readAtOf.get(drawnRead)) ?? new Date().toISOString(), failed: null } : null;
    if (!showing) {
      file = null;
      drawnSection = null;
    }
    if (section === null) return;
    if (section === MISSING) {
      fileReading = false;
      file = null;
      drawnSection = null;
      fileFailed = `There is no file ${untrack(() => shown)} on ${untrack(() => branch) ?? "this branch"}.`;
      return;
    }
    let live = true;
    const drawnBefore = drawnFile(where, section);
    if (drawnBefore !== undefined) {
      drawFile(drawnBefore, section);
      return;
    }
    take(section, fileKey(untrack(() => shown) ?? ""), where).then(
      (read) => {
        if (live && !(file !== null && drawnSection === section)) drawFile(read, section);
      },
      (error) => {
        if (!live) return;
        fileReading = false;
        // The file knobas drew stays, saying the source did not answer; nothing is read again until the next look.
        if (fileBehind !== null) {
          fileBehind = { ...fileBehind, failed: ipcErrorMessage(error) };
          return;
        }
        file = null;
        drawnSection = null;
        fileFailed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  /** What the header says of a file drawn from a commit before the tree's: from when it is, and what is being read. */
  function behindLine(behind: { heldAt: string; failed: string | null }, commit: string | null): string {
    const when = ago(behind.heldAt, new Date());
    const read = `As knobas read it ${when === "just now" ? "a moment ago" : when}${commit === null ? "" : `, at ${commit.slice(0, 7)}`}.`;
    return behind.failed === null ? `${read} Checking ${sourceName(source)}…` : `${read} ${sourceName(source)} did not answer: ${behind.failed}`;
  }

  /**
   * A directory read failed before the file's section was known: no answer is
   * coming, so the file pane neither waits on skeletons nor stays dimmed; the
   * tree draws the error.
   */
  const fileStranded = $derived(treeFailed !== null && fileSection === null);

  /**
   * **Pipelines** (#2264): each file's steps by path, for as long as the tab lives; none while editing. A pipeline
   * runs in the backend over the file's bytes at its commit; what it finished as is drawn in the file's place, by
   * the renderer its name takes (`events.json.gz` through gunzip is `events.json`). Until it answers, and when a
   * step fails, the file is drawn as read.
   */
  const pipes = new SvelteMap<string, PipeStep[]>();
  /**
   * The named pipeline the file's pattern opens it through (#2265, `defaultPipeline`), read from the app's copy
   * each time: an edit in Settings changes what runs. Edited in the bar, the edit (in `pipes`) stands instead.
   */
  const fromDefault = $derived(file === null || editing ? null : defaultPipeline(file.path, defaults.list, pipelines.list));
  const steps = $derived(file === null || editing ? null : (pipes.get(file.path) ?? fromDefault?.pipeline.steps ?? null));
  /** What a closed bar held, by path: *Pipeline* brings the steps back, as the reader built them. */
  const closedPipes = new SvelteMap<string, PipeStep[]>();
  /** The panel the reader opened on this file, which *Close* or `Esc` puts away before `Esc` leaves the page. */
  const pipeOpened = $derived(file !== null && !editing && pipes.has(file.path) ? file.path : null);
  let pipeFace = $state<HTMLButtonElement | null>(null);
  let suggestFace = $state<HTMLButtonElement | null>(null);

  function closePipe(at: string) {
    const held = pipes.get(at);
    if (held !== undefined && held.length > 0) closedPipes.set(at, held);
    pipes.delete(at);
  }
  const shownDefault = $derived(
    file !== null && fromDefault !== null && !pipes.has(file.path) ? { name: fromDefault.pipeline.name, pattern: fromDefault.pattern } : null,
  );

  async function savePipeline(path: string, name: string, pattern: string) {
    const chain = $state.snapshot(pipes.get(path) ?? []);
    try {
      const stored = await pipelines.save(savedAs(pipelines.list, name, chain));
      const id = stored.find((p) => p.name.toLowerCase() === name.toLowerCase())?.id;
      if (id === undefined) throw new Error(`${name} was not stored`);
      await defaults.save(withPipeline(defaults.list, pattern, id));
      pipes.delete(path);
    } catch (cause) {
      push({ text: `Could not save the pipeline: ${ipcErrorMessage(cause)}`, tone: "err" });
      throw cause;
    }
  }

  /**
   * **Suggestion chips** (#2265): with no pipeline on the file (none added, none a default), bytes that look gzip,
   * zstd or base64 offer their unwrap step; it is never applied until clicked. *Not now* puts it away for the tab.
   * A binary file's head is read through the byte read (kept by sha) only where no renderer reads its bytes.
   */
  const notNow = new SvelteSet<string>();
  let sniffed = $state<{ key: string; head: Uint8Array } | null>(null);
  $effect(() => {
    const read = bytesRead;
    const key = bytesAt;
    // Only a binary file nothing else draws: a PDF or an image reads its own bytes (and a PDF past its cap none).
    if (read === null || key === null || steps !== null || renderer?.input === "bytes") return;
    if (untrack(() => sniffed?.key) === key) return;
    let live = true;
    read().then(
      (bytes) => {
        if (live) sniffed = { key, head: bytes.slice(0, 8) };
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });
  const suggested = $derived.by(() => {
    if (file === null || editing || steps !== null || ports.piped === undefined || notNow.has(file.path)) return null;
    const head = file.binary && sniffed !== null && sniffed.key === bytesAt ? sniffed.head : null;
    return suggestion(file, head);
  });
  let piped = $state<{ path: string; run: PipeRun } | null>(null);
  let piping = $state(false);
  let pipeRefused = $state<string | null>(null);
  $effect(() => {
    const read = file;
    const chain = steps;
    const run = ports.piped;
    if (read === null || chain === null || chain.length === 0 || run === undefined) {
      piped = null;
      piping = false;
      pipeRefused = null;
      return;
    }
    if (untrack(() => piped?.path) !== read.path) piped = null;
    let live = true;
    piping = true;
    run(at, read.path, read.commit ?? null, $state.snapshot(chain)).then(
      (answer) => {
        if (!live) return;
        piped = { path: read.path, run: answer };
        piping = false;
        pipeRefused = null;
      },
      (cause: unknown) => {
        if (!live) return;
        piped = null;
        piping = false;
        pipeRefused = ipcErrorMessage(cause);
      },
    );
    return () => {
      live = false;
    };
  });
  /** The run that answered for the file open now; another file's, until the effect above clears it, is not. */
  const ran = $derived(piped !== null && file !== null && piped.path === file.path ? piped.run : null);
  const pipeFailure = $derived(ran !== null && ran.outcome === "failed" ? ran.failure : null);
  /**
   * **A text file past the source's cut** (spec #2234 story 16): Gitea's file view stops at 10,000 lines or 1 MB, so
   * such a file is read whole through the byte read (#2256, up to `BYTES_CAP`) at the commit it was read at, and the
   * windowed viewer draws all of it. Until the bytes answer, or where they cannot be read, the cut stands and says so.
   */
  const wholeAt = $derived(
    file === null || file.binary || !file.truncated || file.size > BYTES_CAP || ports.fileBytes === undefined
      ? null
      : JSON.stringify([at, file.path, file.commit ?? null]),
  );
  let whole = $state<{ key: string; text: string } | null>(null);
  $effect(() => {
    const key = wholeAt;
    const read = ports.fileBytes;
    if (key === null || read === undefined || untrack(() => whole?.key) === key) return;
    const [entityId, path, commit] = JSON.parse(key) as [string, string, string | null];
    let live = true;
    read(entityId, path, commit).then(
      (bytes) => {
        if (live) whole = { key, text: new TextDecoder().decode(bytes) };
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });
  /** The file as read, whole where the byte read answered for a cut one. */
  const shownFile = $derived(file !== null && whole !== null && whole.key === wholeAt ? { ...file, text: whole.text, truncated: false } : file);

  /** The file as it is drawn: the pipeline's output where it finished, else the file as read. */
  const drawn = $derived.by((): FileBlock | null => {
    if (shownFile === null) return null;
    if (steps === null || steps.length === 0 || ran === null || ran.outcome !== "done") return shownFile;
    return { ...shownFile, ...pipedFile(ran) };
  });

  const lines = $derived(drawn === null || drawn.binary ? [] : fileLines(drawn.text));

  /** Files of more lines than this draw only the lines in view and a margin (#2251). */
  const WHOLE_UP_TO = 2_000;
  // A line is 13 px at 1.6.
  const win = new Windowed<number>({ count: () => lines.length, key: (i) => i, estimate: 20.8, from: WHOLE_UP_TO + 1 });
  let code = $state<HTMLElement | null>(null);
  /** A rendered file's words (a README, a diagram, a command's output), for ⌘F's find bar: a person reading it means words, not *Go to file*'s names (Round 17). */
  let rendered = $state<HTMLElement | null>(null);

  /** What takes the file and what it opens with (#2254, #2263): the default for its pattern, else the registry's first. */
  const opened = $derived(drawn === null ? null : openWith(drawn, allRenderers(), defaults.list));
  /** The renderer picked in the switcher per file, for as long as the tab lives (#2263). */
  const picks = new SvelteMap<string, string>();
  /** What draws the file now: the one picked, else the default. */
  const renderer = $derived.by((): Renderer | null => {
    if (opened === null || drawn === null) return null;
    const picked = picks.get(drawn.path);
    return opened.offered.find((r) => r.id === picked) ?? opened.renderer;
  });
  const drawAs = $derived(renderer?.id ?? null);
  /** The pattern *Make default* writes: the one that chose the default, unless broader than the file's extension. */
  const switchPattern = $derived(drawn === null || opened === null ? "" : makeDefaultPattern(opened, drawn.path));

  async function makeDefault(path: string, chosen: Renderer) {
    try {
      await defaults.save(withDefault(defaults.list, switchPattern, chosen.id));
      picks.delete(path);
    } catch (cause) {
      push({ text: `Could not make ${chosen.name} the default: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  /** A text file drawn as an image (an SVG): its bytes, for `TfBytesImage`, which sanitizes an SVG. */
  const image = $derived.by(() => {
    if (drawAs !== "image" || drawn === null || drawn.binary) return null;
    const bytes = new TextEncoder().encode(drawn.text);
    return { bytes, read: () => Promise.resolve(bytes) };
  });

  /**
   * Each line's tokens as the worker sent them, by line index; not reactive
   * itself, `painted` counts the answers so the drawn rows read them again.
   */
  let tokensOf: (LineTokens | undefined)[] = [];
  let painted = $state(0);
  let highlighting: HighlightJob | null = null;
  $effect(() => {
    const read = drawn;
    if (read === null || read.binary || drawAs !== "source") return;
    const got: (LineTokens | undefined)[] = [];
    tokensOf = got;
    untrack(() => (painted += 1));
    const started = render.highlight(
      { text: read.text, path: read.path, focus: untrack(() => [win.start, win.end]) },
      {
        onLines(from, answered) {
          if (tokensOf !== got) return;
          for (let i = 0; i < answered.length; i += 1) got[from + i] = answered[i];
          painted += 1;
        },
      },
    );
    highlighting = started;
    return () => {
      started.cancel();
      if (highlighting === started) highlighting = null;
    };
  });
  // The lines in view moved: the worker does them next.
  $effect(() => {
    const [from, to] = [win.start, win.end];
    untrack(() => highlighting?.focus(from, to));
  });

  /** *Find in file* (#2299): what is typed, its hits, and the hit stepped to. */
  let typed = $state("");
  let current = $state(0);
  const found = $derived(needles(typed));
  const hits = $derived(fileHits(lines, found));

  /** Line `index`'s segments: its tokens and hits, `null` to draw its text as it is. */
  function shade(text: string, index: number) {
    void painted;
    return segments(text, tokensOf[index], found);
  }

  function findIn(event: Event) {
    typed = (event.currentTarget as HTMLInputElement).value;
    current = 0;
  }

  /** Steps to the next or previous hit, drawing its line if the window left it out, and scrolls it to the middle. */
  function jump(step: 1 | -1) {
    if (hits.total === 0) return;
    current = stepMatch(current, step, hits.total);
    win.reveal(lineOfHit(hits, current));
    flushSync();
    code?.querySelector("mark.tf-curm")?.scrollIntoView?.({ block: "center" });
  }

  function findKeys(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    jump(event.shiftKey ? -1 : 1);
  }

  /**
   * Another file opens at its top. The shell's one scroller (`.tf-main`) is shared by every file, so without this a
   * file opened from the tree started at the depth the last one was read to: a PDF below its first page, a short file
   * at its end. A re-read of the same file, and a link to a line (the effect below), leave the scroll be.
   */
  const shownPage = pageVisible();
  let fileview = $state<HTMLElement | null>(null);
  let openedPath: string | null = null;
  $effect(() => {
    const opening = file?.path ?? null;
    if (opening === null || opening === openedPath) return;
    openedPath = opening;
    const view = untrack(() => fileview);
    const main = view?.closest(".tf-main");
    if (!view || !main || untrack(() => line) !== null || !shownPage()) return;
    // The tree's sticky top (`code.css`, `.tf-ftreebox`): the file's top comes to rest beside the tree's.
    const above = view.getBoundingClientRect().top - main.getBoundingClientRect().top - 12;
    if (above < 0) main.scrollTop += above;
  });

  /** The file and line last scrolled to, so a refresh of the file leaves the reader where they are. */
  let shownLine: string | null = null;
  $effect(() => {
    const at = line;
    const read = file;
    if (at === null || read === null || read.binary || editing) return;
    const key = `${read.path}\u0000${at}`;
    if (key === shownLine) return;
    shownLine = key;
    void tick().then(() => {
      win.reveal(at - 1);
      flushSync();
      code?.querySelector(".tf-l.tf-at")?.scrollIntoView?.({ block: "center" });
    });
  });

  /** Where `node` at `offset` sits in the file's text, if it is inside a drawn line's text. */
  function placeOf(node: Node | null, offset: number): TextPlace | null {
    const el = node instanceof Element ? node : (node?.parentElement ?? null);
    const row = el?.closest<HTMLElement>(".tf-l");
    if (!row || !code?.contains(row)) return null;
    const n = Number(row.dataset["n"]) - 1;
    const body = row.lastElementChild;
    if (body === null || !body.contains(node)) return { line: n, offset: 0 };
    const before = document.createRange();
    before.setStart(body, 0);
    before.setEnd(node!, offset);
    return { line: n, offset: Math.min(before.toString().length, lines[n]?.length ?? 0) };
  }

  /** Copies the file's own text between the selection's ends: no line numbers, every line between. */
  function copyLines(event: ClipboardEvent) {
    const selection = document.getSelection();
    if (selection === null || selection.isCollapsed || event.clipboardData === null) return;
    const from = placeOf(selection.anchorNode, selection.anchorOffset);
    const to = placeOf(selection.focusNode, selection.focusOffset);
    if (from === null || to === null) return;
    event.clipboardData.setData("text/plain", selectedText(lines, from, to));
    event.preventDefault();
  }

  /** Why the file shown cannot be edited, `null` where it can. */
  const notEditable = $derived(
    file === null
      ? null
      : file.binary
        ? "A binary file is not edited here"
        : file.truncated
          ? "The file is longer than knobas shows, so it is not edited here"
          : file.sha === null
            ? "The source did not say which version of the file this is, so a commit could not be checked against it"
            : null,
  );
  const editable = $derived(file !== null && notEditable === null);

  /** The editor's text, `null` until the reader types: the file as read. */
  let draft = $state<string | null>(null);
  const text = $derived(draft ?? file?.text ?? "");
  const lineCount = $derived(text.split("\n").length);
  const numbers = $derived(Array.from({ length: lineCount }, (_, n) => n + 1).join("\n"));
  let committing = $state(false);

  // Leaving the edit drops what was typed, as *Cancel* does.
  $effect(() => {
    if (!editing) draft = null;
  });

  function openCommit() {
    if (file === null || text === file.text) {
      push({ text: "Nothing changed yet." });
      return;
    }
    committing = true;
  }

  /** The file's commit echo, while the file read is still the blob it was made against. */
  const queued = $derived.by(() => {
    if (file === null || branch === null) return null;
    const echo = echoOn(repoId, commitKind(branch, file.path));
    return echo !== null && echo.base === file.sha ? echo : null;
  });

  /** A read of `read` drops the file's commit echo once the source holds a newer blob. */
  function settleCommit(read: FileBlock | null) {
    const on = untrack(() => branch);
    if (read === null || on === null) return;
    settleEchoes(repoId, (echo) => echo.kind === commitKind(on, read.path) && echo.base !== read.sha);
  }

  // The refresh after the commit, or a run of its source's end, reads the
  // file again (#2105); what the reader typed stays.
  $effect(() => untrack(() => watchEchoes(REPO_FILES, rereadFile, { listen: ports.listen })));

  function rereadFile(): Promise<unknown> {
    const section = fileSection;
    const where = at;
    if (section === null || section === MISSING) return Promise.resolve();
    // The file a commit made is at the branch's new tip: the top says which.
    void rereadTop();
    // A file kept under the section that names its commit (`keepFile`) cannot have changed: a branch that moved names
    // another section in the top read again, and the file is read there. Reading it again here asked the source on every
    // run's end, and a page opened meanwhile waited on it (the WSL block, 2026-11-14: 0.2 to 2.2 s on Gitea).
    if (hasDrawnFile(where, section)) return Promise.resolve();
    forgetDrawnFiles(where);
    return readSection(section, where).then(
      (read) => {
        if (section !== fileSection || where !== at) return;
        const block = read?.blocks[0];
        if (block?.type !== "file") return;
        keepFile(section, read, block.path);
        file = block;
        settleCommit(block);
      },
      // The file keeps what it drew; the next read tries again.
      () => {},
    );
  }

  // A refresh of the page reads every directory drawn and the file again,
  // over what the tab drew; what the reader typed stays.
  onRefreshed(
    () => refreshed,
    () => Promise.all([rereadTree(), rereadFile()]),
  );

  /**
   * The top directory again (#2241, #2242): where its sections name a new
   * commit, the branch moved, and the tree is read again at it -- the open
   * directories by the sections the new top names, the file by its new
   * section once its directory answers, drawn over what the tab shows.
   */
  function rereadTop(): Promise<unknown> {
    const mine = generation;
    return readSection("files").then(
      (read) => {
        const block = read?.blocks[0];
        if (mine !== generation || block?.type !== "tree") return;
        // The commit the source's tree stands at now, which a clone's tree must stand at to take over (the big-folder
        // pass, 2026-11-13: the first answer can be the app's kept read at the commit before a push).
        const was = sourceTop;
        sourceTop = block.commit ?? null;
        sourceTopHeld = read === undefined || !heldBefore(read) ? null : (readAtOf.get(read) ?? null);
        if (settleLocal(block)) return;
        const before = (listings[""] ?? []).map((entry) => entry.section).join("\u0000");
        if (block.entries.map((entry) => entry.section).join("\u0000") !== before) {
          generation += 1;
          asked = new Set([""]);
          early = new Map();
          listings = { "": block.entries };
          pathsSection = block.paths ?? null;
        }
        // The clone may hold the new commit already (a fetch behind an earlier read): its tree takes over if so.
        if (sourceTop !== was) readLocal(at, generation);
      },
      // The tree keeps what it drew.
      () => {},
    );
  }

  // The app read the top directory again behind its answer, and it moved (#2242).
  $effect(() => {
    const where = at;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void listen(EVENTS.viewFresh, ({ payload }) => {
      const fresh = payload as ViewFresh;
      if (live && fresh.entity_id === where && fresh.section === "files" && !fresh.failed) void rereadTop();
    }).then(
      (unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      },
      // No bridge to hear it on (a test without `listen`): nothing is told.
      () => {},
    );
    return () => {
      live = false;
      stop?.();
    };
  });

  function rereadTree(): Promise<unknown> {
    const mine = generation;
    const reads: Promise<unknown>[] = [rereadLocal(), rereadTop()];
    for (const dir of Object.keys(listings)) {
      if (dir === "") continue;
      const section = entryOf(dir)?.section;
      if (section == null) continue;
      const read = readSection(section).then(
        (read) => {
          const block = read?.blocks[0];
          if (mine === generation && block?.type === "tree") listings = { ...listings, [dir]: block.entries };
        },
        // The directory keeps what it drew.
        () => {},
      );
      reads.push(read);
    }
    return Promise.all(reads);
  }

  /** A Markdown file's relative images, read at the commit the file was (#2256); one function per file and `at`. */
  const imageRead = $derived.by(() => {
    const read = ports.fileBytes;
    const entityId = at;
    const commit = file?.commit ?? null;
    return read === undefined ? null : (path: string) => read(entityId, path, commit);
  });
  /**
   * A file's bytes, at the commit it was read at (#2256), for the renderer that reads bytes (an image, a PDF,
   * #2260): every binary file, and a file the source read as text that such a renderer draws (an ASCII-only PDF has
   * no NUL in its head, so the source calls it text). Keyed by what it reads, so a re-read of the same file hands the
   * renderer the same function and it reads nothing again.
   */
  const bytesAt = $derived(
    file === null || !(file.binary || renderer?.input === "bytes") ? null : JSON.stringify([at, file.path, file.commit ?? null]),
  );
  const bytesRead = $derived.by(() => {
    const read = ports.fileBytes;
    if (bytesAt === null || read === undefined) return null;
    const [entityId, path, commit] = JSON.parse(bytesAt) as [string, string, string | null];
    return () => read(entityId, path, commit);
  });
  /** A relative link in a Markdown file: the file here, at its line where it names one. */
  const fileLink = (to: string, at: number | null) => (at === null ? fileHash(repoId, to) : lineHash(repoId, to, at));

  function toggle(dir: string) {
    if (open.has(dir)) open.delete(dir);
    else open.add(dir);
  }

  // ---- Go to file and the tree's keys (#2311) ----

  /** What is typed into *Go to file*. */
  let goTyped = $state("");
  let goField = $state<HTMLInputElement | null>(null);
  let tree = $state<HTMLElement | null>(null);
  /** Every file at the commit, as the source's whole tree answered, by the section it was read through. */
  let every = $state<{ section: string; paths: string[]; truncated: boolean } | null>(null);
  let everyReading = $state(false);
  let everyFailed = $state<string | null>(null);

  /** The whole tree, once: read when *Go to file* is first used at this commit, kept by the app by its sha. */
  function readEvery() {
    const section = pathsSection;
    const where = at;
    if (section === null || every?.section === section || everyReading) return;
    everyReading = true;
    everyFailed = null;
    readSection(section, where).then(
      (read) => {
        everyReading = false;
        if (where !== at || section !== pathsSection) return;
        const block = read?.blocks[0];
        every = block?.type === "tree" ? { section, paths: block.entries.map((entry) => entry.name), truncated: block.truncated } : { section, paths: [], truncated: false };
      },
      (error) => {
        everyReading = false;
        if (where === at) everyFailed = ipcErrorMessage(error);
      },
    );
  }

  // The commit moved (another branch, a push): the next use reads its whole tree.
  $effect(() => {
    void pathsSection;
    if (untrack(() => goTyped.trim()) !== "") untrack(readEvery);
  });

  /** The files the directories read so far hold: what is searched without a whole tree. */
  const known = $derived(
    Object.entries(listings).flatMap(([dir, entries]) => entries.filter((entry) => entry.kind !== "dir" && entry.section !== null).map((entry) => join(dir, entry.name))),
  );
  const searched = $derived(every !== null && every.section === pathsSection ? every : null);
  const goFound = $derived(goToFile(searched?.paths ?? known, goTyped));
  const going = $derived(goTyped.trim() !== "");

  function openPath(full: string) {
    goTyped = "";
    location.hash = fileHash(repoId, full);
  }

  function goKeys(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      const first = goFound[0];
      if (first !== undefined) openPath(first);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      tree?.querySelector<HTMLElement>(going ? "a.tf-gotof" : "[data-path]")?.focus();
    }
  }

  /** The tree's own keys: only while focus is on one of its rows, so a field keeps every key. */
  function treeKeys(event: KeyboardEvent) {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || inTextField(event.target)) return;
    if (going) {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const links = [...(tree?.querySelectorAll<HTMLElement>("a.tf-gotof") ?? [])];
      const here = links.indexOf(event.target as HTMLElement);
      if (event.key === "ArrowUp" && here <= 0) goField?.focus();
      else links[event.key === "ArrowDown" ? Math.min(links.length - 1, here + 1) : here - 1]?.focus();
      return;
    }
    // On the tree itself: the row that had focus left the window, and the keys go on from it.
    const from =
      event.target === tree ? focusedPath : event.target instanceof HTMLElement ? (event.target.closest<HTMLElement>("[data-path]")?.dataset["path"] ?? null) : null;
    const move = treeKey(rows, from, event.key);
    if (move === null) return;
    event.preventDefault();
    if ("fold" in move) open.delete(move.fold);
    else if ("unfold" in move) open.add(move.unfold);
    else {
      // A row past the window is drawn first.
      treeWin.reveal(rows.findIndex((row) => row.path === move.focus));
      flushSync();
      const row = [...(tree?.querySelectorAll<HTMLElement>("[data-path]") ?? [])].find((el) => el.dataset["path"] === move.focus);
      row?.focus();
      row?.scrollIntoView?.({ block: "nearest" });
    }
  }

  /**
   * The tree's rows, flat (every listing read, an open directory's entries after it), drawn through a windowed list
   * (the big-folder pass, 2026-11-13): the rows in view and a screenful each side, the rest room above and below, so
   * a folder of ten thousand entries opens in a frame. A row is 24 px (`code.css`, `.tf-ftree a`).
   */
  const rows = $derived(treeRows(listings, open));
  const treeWin = new Windowed<string>({ count: () => (going ? 0 : rows.length), key: (i) => rows[i]!.path, estimate: 24, from: TREE_WINDOW_FROM });
  /** The row that last had focus, while focus is in the tree. */
  let focusedPath: string | null = null;
  function treeFocus(event: FocusEvent) {
    focusedPath = event.target instanceof HTMLElement ? (event.target.closest<HTMLElement>("[data-path]")?.dataset["path"] ?? focusedPath) : focusedPath;
  }
  function treeBlur(event: FocusEvent) {
    // Focus left the tree; a row the window took out of the DOM keeps it (`treeRow`). Chrome tells a focused row's removal
    // as a `focusout` while it is still connected, so whether it went is asked once this task's DOM work is over.
    const to = event.relatedTarget;
    const from = event.target;
    if (to instanceof Node && tree?.contains(to)) return;
    queueMicrotask(() => {
      if (from instanceof Node && from.isConnected && !(document.activeElement !== null && tree?.contains(document.activeElement))) focusedPath = null;
    });
  }
  /** On each drawn row: measured by the window, and where it held focus when the window took it, the tree keeps focus. */
  const treeRow = (path: string) => (node: HTMLElement) => {
    const measured = treeWin.row(path)(node);
    return () => {
      measured();
      // The DOM goes before the teardown. Chrome fixes focus up only at its next rendering update, so the row may still
      // be the active element here; jsdom has dropped it to the body already.
      const active = document.activeElement;
      if (focusedPath === path && (active === node || active === document.body) && tree?.isConnected) tree.focus({ preventScroll: true });
    };
  };

  // `T` focuses *Go to file* from anywhere on the tab, as the mockup's hint says; never while typing.
  $effect(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page or tab keep-alive holds hidden (#2247) takes no keys.
      if (!shownPage()) return;
      if (event.key !== "t" && event.key !== "T") return;
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
      if (inTextField(event.target) || inTextField(document.activeElement)) return;
      // A dialog or the launcher over the page keeps its keys and its focus.
      if (layers.open.some((layer) => layer.kind !== "panel") || goField === null) return;
      event.preventDefault();
      goField.focus();
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
</script>

{#snippet icon(kind: "dir" | "open" | "file")}
  <svg class="tf-ficon" viewBox="0 0 16 16" aria-hidden="true"
    >{#if kind === "dir"}<path d="M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z" />{:else if kind === "open"}<path
        d="M2.5 4.5h4l1.5 1.5h5.5v1.5M2.5 4.5v8h10l1.5-5h-10l-1.5 5"
      />{:else}<path d="M4 2h5l3 3v9H4z M9 2v3h3" />{/if}</svg
  >
{/snippet}

{#snippet listing()}
  <div class="tf-ftrows" {@attach treeWin.list}>
    {#each treeWin.slice(rows) as row (row.path)}
      {@const full = row.path}
      {#if row.kind === "dir"}
        <button
          type="button"
          class="tf-fdir tf-d{Math.min(row.depth, 8)}"
          data-path={full}
          aria-expanded={row.open}
          onpointerenter={() => restOn(full, row.section, "dir")}
          onpointerleave={unrest}
          onclick={() => toggle(full)}
          {@attach treeRow(full)}>{@render icon(row.open ? "open" : "dir")}{row.name}</button
        >
      {:else if row.kind === "file"}
        <a
          href={fileHash(repoId, full)}
          data-path={full}
          class="tf-d{Math.min(row.depth, 8)}"
          class:tf-changed={changed.has(full)}
          aria-current={full === shown ? "true" : undefined}
          onpointerenter={() => restOn(full, row.section, "file")}
          onpointerleave={unrest}
          title={changed.has(full) ? "Changed in your checkout" : undefined}
          {@attach treeRow(full)}>{@render icon("file")}{row.name}</a
        >
      {:else}
        <span class="tf-fsub tf-d{Math.min(row.depth, 8)}" data-path={full} tabindex="-1" {@attach treeRow(full)}>{@render icon("file")}{row.name}</span>
      {/if}
    {/each}
  </div>
{/snippet}

<div class="tf-repo">
  <!-- The tree stays beside every file (#2257, the mockup of #2252): its head folds it, its rows scroll inside it. -->
  <aside class="tf-ftreebox">
    <header>
      <label class="tf-minif tf-gotofile">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input
          placeholder="Go to file"
          aria-label="Go to file"
          bind:this={goField}
          value={goTyped}
          oninput={(event) => {
            goTyped = event.currentTarget.value;
            readEvery();
          }}
          onfocus={readEvery}
          onkeydown={goKeys}
          {@attach filterKeys({ aside: true })}
        />
        {#if !going}<kbd>T</kbd>{/if}
      </label>
      <button type="button" class="tf-iconbtn tf-sm" title="Collapse all" disabled={open.size === 0} onclick={() => open.clear()}
        ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4-3 4 3M4 10l4 3 4-3" /></svg></button
      >
    </header>
    <!-- The rows are the links and buttons; the arrow keys move between them (#2311). -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <nav class="tf-ftree" aria-label="Files" tabindex="-1" bind:this={tree} onkeydown={treeKeys} onfocusin={treeFocus} onfocusout={treeBlur}>
      {#if going}
        <!-- Go to file's answer in the rows' place (#2311): every path at the commit that holds what is typed. -->
        {#each goFound as found (found)}
          {@const cut = found.lastIndexOf("/") + 1}
          <a class="tf-gotof" href={fileHash(repoId, found)} aria-current={found === shown ? "true" : undefined} onclick={() => (goTyped = "")}
            >{@render icon("file")}<span>{#if cut > 0}<span class="tf-pdir">{found.slice(0, cut)}</span>{/if}{found.slice(cut)}</span></a
          >
        {/each}
        {#if everyFailed !== null}
          <p class="tf-gonote">Could not read every path ({everyFailed}); searched the directories opened so far.</p>
        {:else if everyReading && searched === null}
          <p class="tf-gonote">Reading every path… meanwhile the directories opened so far.</p>
        {:else if pathsSection === null}
          <p class="tf-gonote">The source lists no whole tree: searched the directories opened so far.</p>
        {/if}
        {#if goFound.length === 0 && !everyReading}
          <p class="tf-gonote">No file matches “{goTyped.trim()}”. <button type="button" class="tf-linkish" onclick={() => (goTyped = "")}>Clear</button></p>
        {/if}
        {#if searched?.truncated}
          <p class="tf-gonote">The tree is larger than knobas reads at once: searched its first {searched.paths.length.toLocaleString("en-US")} files.</p>
        {/if}
      {:else}
        {#if treeFailed !== null}
          <p class="tf-cfail">{treeFailed}</p>
        {/if}
        <TfWaiting phase={waitPhase({ drawn: listings[""] !== undefined || treeFailed !== null, reading: false, failed: false })} columns={1} count={8}>
          {@render listing()}
        </TfWaiting>
      {/if}
    </nav>
    {#if local !== null && !going}
      <p class="tf-gonote">
        From your clone at <span class="tf-mono">{local.commit.slice(0, 7)}</span>{local.checked ? `, the same as on ${sourceName(source)}` : ", as of the last sync"}.
      </p>
    {/if}
    <!-- What the source could not list at once says so under the tree, never among its rows (the big-folder pass, 2026-11-13). -->
    {#if readAlone && local === null && !going}
      <p class="tf-gonote">Folders past this repository's first 20,000 entries are read one at a time.</p>
    {/if}
    {#if !going}
      {#each Object.entries(cutDirs).filter(([dir]) => dir === "" || open.has(dir)) as [dir, cut] (dir)}
        <p class="tf-gonote">
          <span class="tf-mono">{dir || title || "The top folder"}</span> shows its first {cut.shown.toLocaleString("en-US")} entries; the rest are
          {#if cut.url}<a href={cut.url} onclick={openLink}>on {sourceName(source)}</a>{:else}on {sourceName(source)}{/if}.
        </p>
      {/each}
    {/if}
    {#if listings[""] !== undefined}
      <footer class="tf-ftree-keys"><span><kbd>T</kbd> go to file</span><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>←</kbd><kbd>→</kbd> fold</span></footer>
    {/if}
  </aside>
  <div class="tf-fcol">
  {#if path === null}<TfRepoLastCommit {ports} {repoId} {at} {refreshed} />{/if}
  <div class="tf-fileview" bind:this={fileview}>
    {#if file !== null && editing && editable && branch !== null && !fileReading}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-chip tf-warn">Editing on <span class="tf-mono">{branch}</span></span>
        <span class="tf-trail">
          <a class="tf-btn tf-sm tf-ghost" href={fileHash(repoId, file.path)}>Cancel</a>
          <button type="button" class="tf-btn tf-sm tf-primary" onclick={openCommit}>Commit change…</button>
        </span>
      </header>
      <div class="tf-banner tf-info">
        <span>The edit is committed to <span class="tf-mono">{branch}</span> on {sourceName(source)}. Your local checkout is never written to.</span>
      </div>
      <div class="tf-editor">
        <div class="tf-nums" aria-hidden="true">{numbers}</div>
        <textarea
          spellcheck="false"
          rows={lineCount}
          aria-label="Edit {file.path}"
          value={text}
          oninput={(event) => (draft = (event.currentTarget as HTMLTextAreaElement).value)}
        ></textarea>
      </div>
    {:else if shown === null && listings[""] !== undefined}
      <div class="tf-empty"><p>Pick a file in the tree.</p></div>
    {:else if fileSection === MISSING}
      <TfNothingHere message="{branch ?? 'This branch'} has no file at this path." href={repoHash(repoId)} action="All files" />
    {:else}
      <TfWaiting phase={waitPhase({ drawn: file !== null || fileStranded, reading: fileReading && !fileStranded, failed: fileFailed !== null })} error={fileFailed} failClass="tf-fpad" shape="lines">
      {#if file !== null}
      {@const cut = file.path.lastIndexOf("/") + 1}
      <header>
        <span class="tf-p">{#if cut > 0}<span class="tf-pdir">{file.path.slice(0, cut)}</span>{/if}{file.path.slice(cut)}</span>
        {#if fileBehind !== null}
          <!-- The held line (ADR-0042) in the meta's own place, so nothing under the header moves when it comes or goes. -->
          <span class="tf-dim tf-small tf-fmeta" role="status">{behindLine(fileBehind, file.commit ?? null)}</span>
        {:else}
          <span class="tf-dim tf-small tf-fmeta">{fileMeta(file, lines.length)}</span>
        {/if}
        <span class="tf-trail">
          {#if opened !== null && renderer !== null}
            {@const at = drawn?.path ?? file.path}
            <TfRendererSwitch
              {opened}
              shown={renderer}
              pattern={switchPattern}
              onpick={(r) => picks.set(at, r.id)}
              onmakedefault={(r) => void makeDefault(at, r)}
            />
          {/if}
          {#if steps === null && ports.piped !== undefined && suggested === null}
            {@const at = file.path}
            <button type="button" class="tf-btn tf-sm" bind:this={pipeFace} onclick={() => pipes.set(at, closedPipes.get(at) ?? [])}>Pipeline</button>
          {/if}
          <TfOpenIn entityId={repoId} {title} {checkout} file={file.path} page={file.url} />
          <TfFileMore edit={editHash(repoId, file.path)} {notEditable} history={repoHash(repoId, "commits")} />
        </span>
      </header>
      {#if queued !== null}
        <div class="tf-banner tf-warn">
          A commit of this file {queued.state === "sent" ? "was sent" : "is queued"}: “{queued.text}”. It shows here once {sourceName(source)} has it.
        </div>
      {/if}
      {#if movedNote !== null}
        <div class="tf-banner tf-info" role="status">{movedNote}</div>
      {/if}
      {#if changed.has(file.path)}
        <div class="tf-banner tf-warn">
          Your checkout has an uncommitted change to this file.
          <span class="tf-trail"><a class="tf-btn tf-sm" href={repoHash(repoId, "local")}>See local changes</a></span>
        </div>
      {/if}
      {#if shownFile?.truncated}
        <div class="tf-banner tf-info">
          The file is longer than knobas shows.
          {#if file.url}<span class="tf-trail"><a class="tf-btn tf-sm" href={file.url} onclick={openLink}>The rest on the source</a></span>{/if}
        </div>
      {/if}
      {#if suggested !== null}
        {@const at = file.path}
        {@const chip = suggested}
        <div class="tf-suggest" role="status">
          <b>These bytes look {chip.kind}.</b>
          <span>Nothing has been applied; you see the file as it is.</span>
          <span class="tf-trail">
            <button type="button" class="tf-btn tf-sm tf-primary" bind:this={suggestFace} onclick={() => pipes.set(at, [chip.step])}>Unwrap with {stepName(chip.step)}</button>
            <button
              type="button"
              class="tf-btn tf-sm tf-ghost"
              onclick={() => {
                notNow.add(at);
                // The line goes with the press, and *Pipeline* takes its place.
                void tick().then(() => pipeFace?.focus());
              }}>Not now</button
            >
          </span>
        </div>
      {/if}
      {#if steps !== null}
        {@const at = file.path}
        <TfFilePipeline
          {steps}
          commands={stepCommands.list}
          saved={shownDefault}
          savePattern={pipelinePattern(file.path)}
          savedName={fromDefault?.pipeline.name ?? ""}
          onsave={(name, pattern) => savePipeline(at, name, pattern)}
          path={file.path}
          shownAs={drawn !== null && drawn !== file ? `${renderer?.name ?? "Source"} · ${drawn.path.slice(drawn.path.lastIndexOf("/") + 1)}` : (renderer?.name ?? "Source")}
          failedAt={pipeFailure?.index ?? null}
          running={piping}
          onchange={(next) => pipes.set(at, next)}
          onreset={() => {
            closedPipes.delete(at);
            // Back to the default where there is one, else an empty bar for new steps.
            if (fromDefault !== null) pipes.delete(at);
            else pipes.set(at, []);
          }}
          onclose={pipeOpened !== null && fromDefault === null && shownPage() ? () => closePipe(at) : null}
          closeTo={() => pipeFace ?? suggestFace}
          edited={pipes.has(at)}
        />
        {#if pipeFailure !== null}
          <div class="tf-stepfail" role="alert">
            <b>Step {pipeFailure.index + 1}, {pipeFailure.name}, failed.</b>
            <div class="tf-sfcols">
              <div><b>What it said</b><pre>{pipeFailure.error}</pre></div>
              <div><b>Its input, {inputOf(steps, pipeFailure.index, stepCommands.list)}, {sized(pipeFailure.input.size)}</b><pre>{pipeFailure.input.head}</pre></div>
            </div>
          </div>
        {:else if pipeRefused !== null}
          <div class="tf-banner tf-err" role="alert">The pipeline could not read the file: {pipeRefused}</div>
        {/if}
      {/if}
      {#if drawn === shownFile && bytesRead !== null && drawAs === "pdf"}
        <TfPdf read={bytesRead} size={file.size} {render} />
      {:else if drawn === file && file.binary && bytesRead !== null && drawAs === "image"}
        <TfBytesImage read={bytesRead} path={file.path} size={file.size} />
      {:else if drawn === null || drawn.binary}
        <div class="tf-empty"><p>A binary file, not drawn.</p></div>
      {:else if image !== null}
        <TfBytesImage read={image.read} path={drawn.path} size={image.bytes.length} />
      {:else if drawAs === "markdown"}
        <div class="tf-rendered" bind:this={rendered}><TfMarkdown text={drawn.text} base={parentOf(file.path)} {fileLink} {render} image={imageRead} /></div>
        <TfFindBar root={() => rendered} within="in this file" />
      {:else if drawAs === "mermaid" || drawAs === "d2" || drawAs === "graphviz"}
        <TfDiagram engine={drawAs} code={drawn.text} {render} />
      {:else if renderer !== null && drawAs === renderer.id && commandIdOf(renderer) !== null}
        <div class="tf-rendered" bind:this={rendered}><TfCommandOutput {renderer} text={drawn.text} {commands} /></div>
        <TfFindBar root={() => rendered} within="in this file" />
      {:else}
        <div class="tf-findbar">
          <label class="tf-fsearch">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
              ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
            >
            <input placeholder="Find in file" aria-label="Find in file" value={typed} oninput={findIn} onkeydown={findKeys} {@attach filterKeys()} />
            <span class="tf-dim tf-lqn" aria-live="polite">{findCount(typed, hits.total, current)}</span>
          </label>
          <span class="tf-nav2">
            <button type="button" aria-label="Previous match" onclick={() => jump(-1)}>↑</button><button
              type="button"
              aria-label="Next match"
              onclick={() => jump(1)}>↓</button
            >
          </span>
        </div>
        <pre class="tf-code" bind:this={code} oncopy={copyLines} {@attach win.list}>{#each win.slice(lines) as text, i (win.start + i)}{@const n = win.start + i + 1}{@const segs = shade(text, n - 1)}<div class="tf-l" class:tf-at={n === line} data-n={n} {@attach win.row(n - 1)}><span>{n}</span><span>{#if segs === null}{text || " "}{:else}{@const base = firstHitOn(hits, n - 1) ?? 0}{#each segs as seg, at (at)}{#if seg.hit !== null}<mark class={seg.kind === "" ? undefined : `tf-hl-${seg.kind}`} class:tf-curm={base + seg.hit === current}>{seg.text}</mark>{:else if seg.kind !== ""}<span class="tf-hl-{seg.kind}">{seg.text}</span>{:else}{seg.text}{/if}{/each}{/if}</span></div>{/each}</pre>
      {/if}
      {/if}
      </TfWaiting>
    {/if}
  </div>
  </div>
</div>

{#if committing && file !== null && file.sha !== null && branch !== null}
  <TfCommitFile
    {repoId}
    {branch}
    path={file.path}
    before={file.text}
    after={text}
    baseSha={file.sha}
    {refusal}
    {job}
    {checkoutBranch}
    {ports}
    {source}
    onclose={() => (committing = false)}
  />
{/if}
