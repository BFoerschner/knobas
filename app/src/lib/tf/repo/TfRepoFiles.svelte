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

  With no file in the address the top directory's README opens.

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
-->
<script lang="ts">
  import { flushSync, tick, untrack } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { EVENTS, ipcErrorMessage } from "../../ipc";
  import type { CheckoutView, TreeEntry, ViewBlock, ViewFresh, ViewSection } from "../../ipc/entity";
  import type { WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import { renderPort, type HighlightJob, type LineTokens, type RenderPort } from "../../render/port";
  import { allRenderers, imageUrl, type Renderer } from "../../render/registry";
  import { makeDefaultPattern, openWith, withDefault } from "../../render/defaults";
  import { rendererDefaults as appDefaults, type RendererDefaults } from "../../render/defaults.svelte";
  import TfRendererSwitch from "../render/TfRendererSwitch.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfDiagram from "../render/TfDiagram.svelte";
  import { findCount, stepMatch } from "../builds/run-reads";
  import { fileHits, firstHitOn, lineOfHit, needles, segments } from "./file-lines";
  import { echoOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfCommitFile from "./TfCommitFile.svelte";
  import TfCommandOutput from "../render/TfCommandOutput.svelte";
  import { commandIdOf, ipcCommands, type CommandPort } from "../../render/command";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
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
  import { waitPhase } from "../waiting";

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
  }: {
    ports: Pick<RepoPorts, "entityView" | "submitWrite" | "listen">;
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
  } = $props();

  /** Each directory read so far at `at`, by its path (`""` the top). */
  let listings = $state<Record<string, TreeEntry[]>>({});
  let treeFailed = $state<string | null>(null);
  const open = new SvelteSet<string>();
  let file = $state<FileBlock | null>(null);
  let fileFailed = $state<string | null>(null);
  /** The file shown is being read: the one drawn before stays, dimmed, until it answers (#2246). */
  let fileReading = $state(false);
  /** Directory reads asked and not answered, at the current `at`. */
  let asked = new Set<string>();
  /**
   * Reads a deep link asked before a listing named their section (#2244), by
   * section, at the current `at`: the listing that names one takes it.
   */
  let early = new Map<string, Promise<ViewSection | undefined>>();
  let generation = 0;

  const join = (dir: string, name: string) => (dir === "" ? name : `${dir}/${name}`);
  const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");
  const nameOf = (full: string) => full.slice(full.lastIndexOf("/") + 1);

  /** The entry `full` names in its parent's listing, once that is read. */
  function entryOf(full: string): TreeEntry | undefined {
    return listings[parentOf(full)]?.find((entry) => entry.name === nameOf(full));
  }

  /** The section `section` names, read at `where`: a directory's listing or a file and its actions. */
  async function readSection(section: string, where = at): Promise<ViewSection | undefined> {
    const view = await ports.entityView(where, section);
    return view?.sections[0];
  }

  /**
   * What an early read is held by: the place, not the section, since a guess
   * names no commit and the listing's section does (#2241).
   */
  const dirKey = (dir: string) => `dir\u0000${dir}`;
  const fileKey = (full: string) => `file\u0000${full}`;

  /** The read of `place` asked early, once, else -- or where that guess was refused -- a new one of `section`. */
  function take(section: string, place: string, where = at): Promise<ViewSection | undefined> {
    const read = early.get(place);
    if (read === undefined) return readSection(section, where);
    early.delete(place);
    return read.catch(() => readSection(section, where));
  }

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
        listings[dir] = block?.type === "tree" ? block.entries : [];
      },
      (error) => {
        if (mine === generation) treeFailed = ipcErrorMessage(error);
      },
    );
  }

  // A new branch is a new tree.
  $effect(() => {
    void at;
    generation += 1;
    asked = new Set();
    early = new Map();
    listings = {};
    treeFailed = null;
    untrack(() => readDir("", "files"));
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
  // and the file at once (#2244); the file draws as soon as it answers, and
  // the tree fills in as its listings arrive.
  $effect(() => {
    const where = at;
    const target = path;
    if (target === null) return;
    return untrack(() => {
      if (listings[parentOf(target)] !== undefined) return;
      const likely = likelySections(target);
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
        // Its directory's listing says whether there is such a file.
        spent,
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
    if (shown === null || listings[parentOf(shown)] === undefined) return null;
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
    drawnSection = section;
    fileReading = false;
    fileFailed = null;
    draft = null;
    settleCommit(file);
  }

  // The file, once its directory has said which section opens it; a deep
  // link's early read that drew it already stands.
  $effect(() => {
    const section = fileSection;
    const where = at;
    // Drawn already, by a deep link's early read or before a step away and back.
    if (section !== null && section === untrack(() => drawnSection) && untrack(() => file) !== null) {
      fileReading = false;
      fileFailed = null;
      return;
    }
    // A deep link's guess drew this file before its listing named the section: it stands for it.
    if (
      section !== null &&
      section !== MISSING &&
      drawnSection === GUESSED &&
      untrack(() => file)?.path === untrack(() => shown)
    ) {
      drawnSection = section;
      fileReading = false;
      fileFailed = null;
      return;
    }
    fileFailed = null;
    // Waiting for the directory that says which section opens the file; with no file to show, nothing is drawn.
    const showing = untrack(() => shown) !== null;
    fileReading = showing;
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
    take(section, fileKey(untrack(() => shown) ?? ""), where).then(
      (read) => {
        if (live && !(file !== null && drawnSection === section)) drawFile(read, section);
      },
      (error) => {
        if (!live) return;
        fileReading = false;
        file = null;
        drawnSection = null;
        fileFailed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  /**
   * A directory read failed before the file's section was known: no answer is
   * coming, so the file pane neither waits on skeletons nor stays dimmed; the
   * tree draws the error.
   */
  const fileStranded = $derived(treeFailed !== null && fileSection === null);

  const lines = $derived(file === null || file.binary ? [] : fileLines(file.text));

  /** Files of more lines than this draw only the lines in view and a margin (#2251). */
  const WHOLE_UP_TO = 2_000;
  // A line is 13 px at 1.6.
  const win = new Windowed<number>({ count: () => lines.length, key: (i) => i, estimate: 20.8, from: WHOLE_UP_TO + 1 });
  let code = $state<HTMLElement | null>(null);

  /** What takes the file and what it opens with (#2254, #2263): the default for its pattern, else the registry's first. */
  const opened = $derived(file === null ? null : openWith(file, allRenderers(), defaults.list));
  /** The renderer picked in the switcher per file, for as long as the tab lives (#2263). */
  const picks = new SvelteMap<string, string>();
  /** What draws the file now: the one picked, else the default. */
  const renderer = $derived.by((): Renderer | null => {
    if (opened === null || file === null) return null;
    const picked = picks.get(file.path);
    return opened.offered.find((r) => r.id === picked) ?? opened.renderer;
  });
  const drawAs = $derived(renderer?.id ?? null);
  /** The pattern *Make default* writes: the one that chose the default, unless broader than the file's extension. */
  const switchPattern = $derived(file === null || opened === null ? "" : makeDefaultPattern(opened, file.path));

  async function makeDefault(path: string, chosen: Renderer) {
    try {
      await defaults.save(withDefault(defaults.list, switchPattern, chosen.id));
      picks.delete(path);
    } catch (cause) {
      push({ text: `Could not make ${chosen.name} the default: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  const image = $derived(
    drawAs === "image" && file !== null && !file.binary ? imageUrl(new TextEncoder().encode(file.text), file.path) : null,
  );

  /**
   * Each line's tokens as the worker sent them, by line index; not reactive
   * itself, `painted` counts the answers so the drawn rows read them again.
   */
  let tokensOf: (LineTokens | undefined)[] = [];
  let painted = $state(0);
  let highlighting: HighlightJob | null = null;
  $effect(() => {
    const read = file;
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
    return readSection(section, where).then(
      (read) => {
        if (section !== fileSection || where !== at) return;
        const block = read?.blocks[0];
        if (block?.type !== "file") return;
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
        const before = (listings[""] ?? []).map((entry) => entry.section).join("\u0000");
        if (block.entries.map((entry) => entry.section).join("\u0000") === before) return;
        generation += 1;
        asked = new Set([""]);
        early = new Map();
        listings = { "": block.entries };
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
      if (live && fresh.entity_id === where && fresh.section === "files") void rereadTop();
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
    const reads: Promise<unknown>[] = [rereadTop()];
    for (const dir of Object.keys(listings)) {
      if (dir === "") continue;
      const section = entryOf(dir)?.section;
      if (section == null) continue;
      const read = readSection(section).then(
        (read) => {
          const block = read?.blocks[0];
          if (mine === generation && block?.type === "tree") listings[dir] = block.entries;
        },
        // The directory keeps what it drew.
        () => {},
      );
      reads.push(read);
    }
    return Promise.all(reads);
  }

  /** A relative link in a Markdown file: the file here, at its line where it names one. */
  const fileLink = (to: string, at: number | null) => (at === null ? fileHash(repoId, to) : lineHash(repoId, to, at));

  function toggle(dir: string) {
    if (open.has(dir)) open.delete(dir);
    else open.add(dir);
  }
</script>

{#snippet listing(dir: string, depth: number)}
  {#each listings[dir] ?? [] as entry (entry.name)}
    {@const full = join(dir, entry.name)}
    {#if entry.kind === "dir"}
      <button
        type="button"
        class="tf-fdir tf-d{Math.min(depth, 8)}"
        aria-expanded={open.has(full)}
        onclick={() => toggle(full)}>{entry.name}/</button
      >
      {#if open.has(full)}
        {@render listing(full, depth + 1)}
      {/if}
    {:else if entry.section !== null}
      <a
        href={fileHash(repoId, full)}
        class="tf-d{Math.min(depth, 8)}"
        class:tf-changed={changed.has(full)}
        aria-current={full === shown ? "true" : undefined}
        title={changed.has(full) ? "Changed in your checkout" : undefined}>{entry.name}</a
      >
    {:else}
      <span class="tf-fsub tf-d{Math.min(depth, 8)}">{entry.name}</span>
    {/if}
  {/each}
{/snippet}

<div class="tf-repo">
  <nav class="tf-ftree" aria-label="Files">
    {#if treeFailed !== null}
      <p class="tf-cfail">{treeFailed}</p>
    {/if}
    <TfWaiting phase={waitPhase({ drawn: listings[""] !== undefined || treeFailed !== null, reading: false, failed: false })} columns={1} count={8}>
      {@render listing("", 0)}
    </TfWaiting>
  </nav>
  <div class="tf-fileview">
    {#if file !== null && editing && editable && branch !== null && !fileReading}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-chip tf-warn">editing on {branch}</span>
        <span class="tf-acts">
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
    {:else}
      <TfWaiting phase={waitPhase({ drawn: file !== null || fileStranded, reading: fileReading && !fileStranded, failed: fileFailed !== null })} error={fileFailed} failClass="tf-fpad" shape="lines">
      {#if file !== null}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-dim tf-small"
          >{file.binary ? `${file.size} bytes` : `${lines.length} ${lines.length === 1 ? "line" : "lines"}`}{branch !== null ? `, on ${branch}` : ""}</span
        >
        <span class="tf-acts">
          {#if opened !== null && renderer !== null}
            {@const at = file.path}
            <TfRendererSwitch
              {opened}
              shown={renderer}
              pattern={switchPattern}
              onpick={(r) => picks.set(at, r.id)}
              onmakedefault={(r) => void makeDefault(at, r)}
            />
          {/if}
          {#if editable}
            <a class="tf-btn tf-sm" href={editHash(repoId, file.path)}>Edit</a>
          {:else}
            <button type="button" class="tf-btn tf-sm" aria-disabled="true" title={notEditable}>Edit</button>
          {/if}
          <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}>History</a>
          <TfOpenIn entityId={repoId} {title} {checkout} file={file.path} />
        </span>
      </header>
      {#if queued !== null}
        <div class="tf-banner tf-warn">
          A commit of this file {queued.state === "sent" ? "was sent" : "is queued"}: “{queued.text}”. It shows here once {sourceName(source)} has it.
        </div>
      {/if}
      {#if changed.has(file.path)}
        <div class="tf-banner tf-warn">
          Your checkout has an uncommitted change to this file.
          <span class="tf-acts"><a class="tf-btn tf-sm" href={repoHash(repoId, "local")}>See local changes</a></span>
        </div>
      {/if}
      {#if file.truncated}
        <div class="tf-banner tf-info">
          The file is longer than knobas shows.
          {#if file.url}<span class="tf-acts"><a class="tf-btn tf-sm" href={file.url}>The rest on the source</a></span>{/if}
        </div>
      {/if}
      {#if file.binary}
        <div class="tf-empty"><p>A binary file, not drawn.</p></div>
      {:else if image !== null}
        <div class="tf-fimage"><img src={image} alt={file.path} /></div>
      {:else if drawAs === "markdown"}
        <TfMarkdown text={file.text} base={parentOf(file.path)} {fileLink} {render} />
      {:else if drawAs === "mermaid"}
        <TfDiagram engine="mermaid" code={file.text} {render} />
      {:else if renderer !== null && drawAs === renderer.id && commandIdOf(renderer) !== null}
        <TfCommandOutput {renderer} text={file.text} {commands} />
      {:else}
        <div class="tf-findbar">
          <label class="tf-fsearch">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
              ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
            >
            <input placeholder="Find in file" aria-label="Find in file" value={typed} oninput={findIn} onkeydown={findKeys} />
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
