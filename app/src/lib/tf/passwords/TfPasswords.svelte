<!--
  Passwords (ADR-0056): a password manager's entries, from the clickdummy at `mockups/passwords-2/`.

  **One read holds the page** (`passwords_held`): every source, folder and a compact row per entry. A folder press and
  a filter keystroke ask nothing; the tree, the panels, a path and the filter's hits are computed here (`model.ts`).
  A return to the page draws from what was read last (`held.svelte.ts`) and reads again behind.

  - **The tree** is one flat list of the rows on show (All entries, then each open fold's folders), one `Windowed`,
    with the Docs rail's look, the folder on show drawn selected. A press selects the folder and leaves its fold as it
    is (#2531); the chevron folds alone; Alt+click folds everything under it. ↑ ↓ walk it, → opens a fold and
    steps in, ← closes it and steps out. One `Tab` stop.
  - **The list**: a folder lists everything under it, its own entries first, then a panel per folder below that holds
    entries; one further down with more than six rows draws five and *Show all*. At the top, *Used recently* first. One
    flat list of lines (a panel's head, a row, *Show all*), one `Windowed`.
  - **The entry** beside it, the list and the entry one block capped at 1,438 px and centred (`TfPwEntry`).
  - **The filter** (`⌘F`) reads the folder on show and the folders under it, every entry at the top; another folder
    clears it. In it ⌘⇧C, S, U, O and A act on the open entry and `Enter` goes to its row (#2531).
  - **Keys:** ↑ ↓ or `j` `k` move in the list; `c` copy the password, `s` show it, `u` the username, `o`
    the one-time code, `a` the address; `Esc` clears the filter, else goes up a folder. `r` does nothing here: a sync
    is something somebody else can read in a log, so it is a button a person aims at.
  - **Leaving the page** hides everything shown. It does not clear the clipboard: the person copied to paste elsewhere.

  The page names no source and no vendor. With more than one password source, each one's folders sit under a top
  named after it: the one place a source's name appears.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { flushSync, tick, untrack } from "svelte";
  import type { Held, PwEntry, PwSource } from "../../ipc/passwords";
  import { Windowed } from "../../shell/windowed.svelte";
  import { inTextField } from "../../shell/keys";
  import { keyHint } from "../../shell/platform";
  import { collectDescendants, isAltClick, TREE_EXPAND_CAP } from "../tree-alt";
  import { filterKeys } from "../filter-keys";
  import Marked from "../filter/Marked.svelte";
  import { heldOrder } from "../filter/ranked.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { stampLabel, clockLabel } from "../../time/format";
  import TfPwEntry from "./TfPwEntry.svelte";
  import TfPwWrite from "./TfPwWrite.svelte";
  import TfPwWeb from "./TfPwWeb.svelte";
  import type { WebOn } from "./web";
  import type { Layers } from "../layers.svelte";
  import { folds, heldNow, readHeld } from "./held.svelte";
  import {
    CUT,
    CUT_OVER,
    CANNOT_READ,
    HAS_CODE,
    NEEDS_APPROVAL,
    bare,
    chainOf,
    commentFolder,
    expiry,
    holding,
    entryMarks,
    homesOf,
    model,
    ordered,
    panels,
    pathWords,
    plural,
    indexBuild,
    scopedEntries,
    searchIndex,
    sourceNode,
    sourceWord,
    treeRows,
    type Entry,
    type Node,
    type Panel,
  } from "./model";
  import { realPasswordPorts, type PasswordPorts } from "./ports";

  let {
    ready,
    rest,
    visible = true,
    timedTicket = null,
    layers,
    sync,
    syncRevision = 0,
    ports = realPasswordPorts,
  }: {
    ready: boolean;
    rest: string[];
    visible?: boolean;
    timedTicket?: string | null;
    layers?: Layers;
    sync?: Snippet;
    syncRevision?: number;
    ports?: PasswordPorts;
  } = $props();

  // ---- the read ----
  // Raw: the read is replaced whole, never changed in place. A deep proxy over 20,000 rows made every read of the
  // model a reactive lookup: the first open took 1.6 s at 6x (the page probe, 2026-11-22).
  let held = $state.raw<Held | null>(heldNow());
  let failed = $state<string | null>(null);
  const m = $derived(held ? model(held) : null);

  async function read(): Promise<void> {
    try {
      held = await readHeld(ports.held);
      failed = null;
    } catch (cause) {
      failed = String((cause as { message?: string })?.message ?? cause);
    }
  }

  // The filter's index (`indexBuild`), built once the page has drawn, in slices of at most 8 ms while the window is
  // idle: not on the first open's path, never one long task a press would wait behind, and built before a first
  // keystroke needs it (one that comes sooner finishes it, `searchIndex`).
  $effect(() => {
    const at = m;
    if (!at || !visible) return;
    const build = indexBuild(at);
    if (build.index) return;
    let live = true;
    let cancel = () => {};
    const slice = (deadline?: IdleDeadline) => {
      if (!live) return;
      const end = performance.now() + Math.min(8, deadline?.timeRemaining() ?? 8);
      if (!build.step(() => performance.now() < end)) next();
    };
    const next = () => {
      if (typeof requestIdleCallback === "function") {
        const id = requestIdleCallback(slice, { timeout: 1000 });
        cancel = () => cancelIdleCallback(id);
      } else {
        const id = setTimeout(slice, 16);
        cancel = () => clearTimeout(id);
      }
    };
    // The frame that is waiting, the second once the first has run: cancelling only the first left the second asking.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => live && next());
    });
    return () => {
      live = false;
      cancelAnimationFrame(frame);
      cancel();
    };
  });

  /**
   * The one-time code's type made once the page is idle (`.tf-pw-warm`): its first use at the display size, weight 800,
   * made a new instance of the face in the layout of the first entry with a code, 26 ms of a 54 ms frame at 6x in
   * knobas's WebView2 window while ↓ was held through the list (the ux30 end gate, 2026-11-24). A hidden sample in that
   * type, laid out after the first frames, makes it before anyone gets there.
   */
  let warm = $state(false);
  $effect(() => {
    if (!m || !visible || untrack(() => warm)) return;
    let cancel = () => {};
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        if (typeof requestIdleCallback === "function") {
          const id = requestIdleCallback(() => (warm = true), { timeout: 1000 });
          cancel = () => cancelIdleCallback(id);
        } else {
          const id = setTimeout(() => (warm = true), 16);
          cancel = () => clearTimeout(id);
        }
      });
    });
    return () => {
      cancelAnimationFrame(frame);
      cancel();
    };
  });

  // Read when the database answers, again when the role's runs end (`syncRevision`), and when shown again.
  $effect(() => {
    void syncRevision;
    if (!ready || !visible) return;
    untrack(() => void read());
  });

  // Leaving the page forgets what it showed; the clipboard is the person's.
  $effect(() => {
    if (visible) return;
    void ports.hide().catch(() => {});
  });

  // ---- where the reader is ----
  /**
   * The folder a press is going to, drawn in the press's own task rather than after the address comes back through
   * the router (four frames at 4x: the page probe, 2026-11-22). The address catching up clears it.
   */
  let going = $state<string | null>(null);
  $effect.pre(() => {
    void rest;
    going = null;
  });
  const where = $derived.by(() => {
    if (going !== null) return { folder: going as string | null, entry: null as string | null };
    const [kind, id] = rest;
    if (kind === "in" && id) return { folder: id, entry: null as string | null };
    if (kind === "source" && id) return { folder: sourceNode(id), entry: null };
    if (kind === "entry" && id) return { folder: null as string | null, entry: id };
    return { folder: "", entry: null };
  });
  /** The entry the reader picked in this folder, when the address names none. */
  let picked = $state<string | null>(null);
  const openId = $derived(where.entry ?? picked);
  const openEntry = $derived(m && openId ? (m.entries.get(openId) ?? null) : null);
  const node = $derived.by((): Node | null => {
    if (!m) return null;
    if (where.folder !== null) return m.nodes.get(where.folder) ?? m.root;
    const e = openEntry;
    return e && e.folder !== null ? (m.nodes.get(e.folder) ?? m.root) : m.root;
  });

  let query = $state("");
  /**
   * Another folder clears the filter (#2531), whichever way it came: a press of the tree, a panel's head, a crumb, the
   * address or Back. Read as the folder's id, so the address catching up with a press (`going`) changes nothing.
   */
  let shownFolder: string | null = null;
  $effect.pre(() => {
    const id = node?.id ?? null;
    if (id === shownFolder) return;
    if (shownFolder !== null) query = "";
    shownFolder = id;
  });
  /**
   * The filter's words are ranked by the app (#2535) over the text of every entry the folder's filter reads
   * (`scopeOf`), a list held once per folder. The index is read on the first word, as before, never on the page's open.
   * Until a key's answer lands the page keeps what it drew, so the first key draws no list the answer then redraws.
   */
  const order = heldOrder<Entry>("secrets");
  const index = $derived(m && query.trim() !== "" ? searchIndex(m) : null);
  /** The entries the folder's filter reads (`scopeOf`), by title: what the app holds and ranks. */
  const scoped = $derived(index !== null && m && node ? scopedEntries(m, node) : []);
  $effect(() => {
    if (index === null) order.ask("", [], () => "");
    else order.ask(query, scoped, index.textOf);
  });
  /** What the page drew last, which a key keeps until its answer lands. */
  let drawnPanels: Panel[] | null = null;
  const shownPanels = $derived.by((): Panel[] => {
    if (!m || !node) return (drawnPanels = []);
    if (index === null) return (drawnPanels = panels(m, node, query));
    if (drawnPanels !== null && !order.settled(query, scoped)) return drawnPanels;
    return (drawnPanels = panels(m, node, query, order.rows(query, scoped, index.textOf)));
  });
  const marking = $derived(index === null ? "" : order.typed(query, scoped));
  /**
   * `Enter` in the filter before the ranking answered its words (#2535): the row it opens is the answer's first, not
   * one of the panels kept meanwhile, so it waits for that answer. Typing on drops it.
   */
  let entering = $state<string | null>(null);
  $effect(() => {
    if (entering === null) return;
    if (entering !== query || index === null) entering = null;
    else if (order.settled(query, scoped)) {
      entering = null;
      // After this flush: `focusOpenRow` draws the row it picks with `flushSync`, which an effect cannot.
      void tick().then(focusOpenRow);
    }
  });

  // ---- the tree ----
  /** Folds the reader opened, kept for the session. */
  const opened = folds;
  const treeOpen = $derived.by(() => {
    const all = new Set(opened);
    if (node) for (let at: Node | null = node.parent; at !== null; at = at.parent) all.add(at.id);
    return all;
  });
  const rows = $derived(m ? treeRows(m, treeOpen) : []);
  const holds = $derived(m && openEntry ? holding(m, openEntry) : new Set<string>());
  const treeWin = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.node.id || "~", estimate: 26, from: 40, scrolls: true });

  /**
   * A press of a folder's row selects it and leaves every fold as it is (#2531, spec #2514's "select-only": a second
   * press on the folder on show folded it away). The chevron and ← → fold one folder; Alt+click folds everything under
   * it (`tree-alt.ts`).
   */
  function pressNode(event: MouseEvent, target: Node): void {
    if (isAltClick(event)) {
      event.preventDefault();
      foldAll(target);
      return;
    }
    const chevron = (event.target as HTMLElement | null)?.closest("[data-twist]") !== null;
    if (chevron) {
      if (opened.has(target.id)) opened.delete(target.id);
      else opened.add(target.id);
      return;
    }
    go(target);
  }

  /**
   * Alt+click: `target` and every folder under it open, or close when `target` is open (All entries, always open: when
   * any folder under it is). Selects nothing. An opening is bounded as every tree's is (`TREE_EXPAND_CAP`); a closing
   * reaches every folder, so none stays open under a closed one.
   */
  function foldAll(target: Node): void {
    const isOpen = target.id === "" ? target.kids.some((k) => opened.has(k.id)) : opened.has(target.id);
    const cap = isOpen ? Number.POSITIVE_INFINITY : TREE_EXPAND_CAP;
    for (const at of collectDescendants([target], (n) => n.kids.filter((k) => k.kids.length > 0), cap)) {
      if (at.id === "") continue;
      if (isOpen) opened.delete(at.id);
      else opened.add(at.id);
    }
  }

  function hashOf(target: Node): string {
    if (target.id === "") return "#/secrets";
    if (target.top) return `#/secrets/source/${encodeURIComponent(target.id.slice(4))}`;
    return `#/secrets/in/${encodeURIComponent(target.id)}`;
  }

  function go(target: Node): void {
    picked = null;
    query = "";
    going = target.id;
    location.hash = hashOf(target);
  }

  function treeKey(event: KeyboardEvent): void {
    const buttons = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("[role=treeitem]")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at === -1) return;
    const row = rows.find((r) => (r.node.id || "~") === buttons[at]!.dataset.id);
    const step = (to: number) => {
      event.preventDefault();
      buttons[Math.max(0, Math.min(buttons.length - 1, to))]?.focus();
    };
    if (event.key === "ArrowDown") step(at + 1);
    else if (event.key === "ArrowUp") step(at - 1);
    else if (event.key === "ArrowRight" && row) {
      event.preventDefault();
      if (row.node.kids.length > 0 && !row.open) opened.add(row.node.id);
      else if (row.open) step(at + 1);
    } else if (event.key === "ArrowLeft" && row) {
      event.preventDefault();
      if (row.open && row.node.id !== "") opened.delete(row.node.id);
      else if (row.node.parent) buttons.find((b) => b.dataset.id === (row.node.parent!.id || "~"))?.focus();
    }
  }

  // ---- the list: one flat list of lines ----
  /**
   * A row is its panel's `n`th, read from `rows` when it is drawn or a key reaches it: a folder's rows are sorted only
   * then (`ordered`), so the top of the page, a panel for every folder, sorts the dozen folders it draws and not all
   * of them (the ux30 end gate, 2026-11-23). A folder's row is keyed by its place, which needs no sort. A hit is keyed
   * by its place as a number: a keystroke that every entry matches lists 20,000, and a text key for each, made and
   * hashed again by the window's heights, was a third of that keystroke's script; a row of the same place shows the
   * next keystroke's hit in place.
   */
  type Line =
    | { kind: "head"; key: string; panel: number; title: string; pre: string; href: string | null; count: number; comment: boolean; first: boolean }
    | { kind: "row"; key: string | number; p: Panel; n: number; path: boolean; last: boolean }
    | { kind: "more"; key: string; node: Node; count: number }
    | { kind: "none"; key: string; text: string };

  /**
   * The folders whose panel the open entry sits beyond the cut of, so that panel shows whole: a string, so `lines`
   * is built again only when it changes, never on every move of the selection (at 20,000 entries a move rebuilt
   * every panel's lines: the page probe, 2026-11-22).
   */
  const lifted = $derived.by((): string => {
    const e = openId && m ? m.entries.get(openId) : undefined;
    if (!e || !m) return "";
    const out: string[] = [];
    for (const home of [e.folder, ...e.also]) {
      const at = home !== null ? m.nodes.get(home) : undefined;
      if (at && at.own.length > CUT_OVER && ordered(m, at).indexOf(e) >= CUT) out.push(at.id);
    }
    return out.join("\n");
  });

  const lines = $derived.by((): Line[] => {
    if (!m || !node) return [];
    const liftedIds = new Set(lifted.split("\n"));
    const out: Line[] = [];
    shownPanels.forEach((p, i) => {
      if (p.kind === "hits") {
        if (p.count === 0) {
          out.push({
            kind: "none",
            key: "none",
            text:
              node.id === ""
                ? `Nothing matches "${query}". The filter looks at titles, usernames, addresses and folder names.`
                : `Nothing in ${node.name} matches "${query}". The filter looks at titles, usernames, addresses and folder names, here and in the folders under it.`,
          });
          return;
        }
        out.push({ kind: "head", key: "h:hits", panel: i, title: `${plural(p.count, "entry", "entries")} in ${node.id === "" ? "every folder" : node.name}`, pre: "", href: null, count: p.count, comment: false, first: true });
        for (let n = 0; n < p.count; n++) out.push({ kind: "row", key: n, p, n, path: true, last: n === p.count - 1 });
        return;
      }
      if (p.kind === "recent") {
        out.push({ kind: "head", key: "h:recent", panel: i, title: "Used recently", pre: "", href: null, count: p.count, comment: false, first: i === 0 });
        p.rows.forEach((e, n) => out.push({ kind: "row", key: `r:recent:${e.id}`, p, n, path: true, last: n === p.count - 1 }));
        return;
      }
      const at = p.node!;
      const rel = chainOf(m, at).slice(chainOf(m, node).length);
      const title = rel.length > 0 ? rel[rel.length - 1]!.name : node.kids.length > 0 ? `Directly in ${node.name}` : node.name;
      const pre = rel.length > 1 ? `${rel.slice(0, -1).map((r) => r.name).join(" / ")} / ` : "";
      const cut = rel.length > 0 && p.count > CUT_OVER && !liftedIds.has(at.id) ? CUT : p.count;
      out.push({ kind: "head", key: `h:${at.id}`, panel: i, title, pre, href: rel.length > 0 ? hashOf(at) : null, count: p.count, comment: commentFolder(at) !== null, first: i === 0 });
      for (let n = 0; n < cut; n++) out.push({ kind: "row", key: `r:${at.id}:${n}`, p, n, path: false, last: n === cut - 1 && cut === p.count });
      if (cut < p.count) out.push({ kind: "more", key: `m:${at.id}`, node: at, count: p.count });
    });
    if (out.length === 0) out.push({ kind: "none", key: "none", text: `Nothing in ${node.name} yet.` });
    return out;
  });
  const listWin = new Windowed({
    count: () => lines.length,
    key: (i) => lines[i]!.key,
    estimate: 52,
    estimateOf: (key) => (typeof key === "number" ? 52 : key.startsWith("h:") ? 50 : key.startsWith("m:") ? 36 : 52),
    from: 60,
  });

  /** A row line's entry. */
  const entryOf = (line: Extract<Line, { kind: "row" }>): Entry => line.p.rows[line.n]!;

  /**
   * Where the entry `id` is listed, as an index into `lines`, or -1. A folder panel is read only when it is one of the
   * entry's folders, so the search sorts none of the others.
   */
  function lineOf(id: string | null): number {
    const e = id !== null && m ? m.entries.get(id) : undefined;
    if (!e || !m) return -1;
    const homes = homesOf(m, e);
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      if (line.kind !== "row" || (line.p.node !== null && !homes.includes(line.p.node))) continue;
      if (entryOf(line) === e) return i;
    }
    return -1;
  }

  /** The first row at or after `from` going by `step`, as an index into `lines`, or -1. */
  function rowFrom(from: number, step: number): number {
    for (let i = from; i >= 0 && i < lines.length; i += step) if (lines[i]!.kind === "row") return i;
    return -1;
  }

  // ---- the entry ----
  /**
   * A key that acts on the open entry (C, S, U, O, A, E) pressed while that entry is still being read: it waits for it,
   * and acts on it once its pane is drawn, never on the entry the pane still shows; another pick drops it.
   */
  let waiting: (() => void) | null = null;
  const records = new Map<string, PwEntry>();
  let record = $state<PwEntry | null>(null);
  let entryPane = $state<TfPwEntry | null>(null);

  async function fetchEntry(id: string): Promise<PwEntry | null> {
    const kept = records.get(id);
    if (kept) return kept;
    const read = await ports.entry(id);
    if (read) records.set(id, read);
    return read;
  }

  /** Ask an entry ahead, when a row takes the pointer, so a press finds it in memory. */
  function ahead(id: string): void {
    if (!records.has(id)) void fetchEntry(id).catch(() => {});
  }

  // A new read of the page drops the records it had: an entry may have changed.
  $effect(() => {
    void held;
    records.clear();
  });

  $effect(() => {
    const id = openId;
    if (!id) {
      record = null;
      return;
    }
    const kept = records.get(id);
    if (kept) {
      record = kept;
      return;
    }
    let live = true;
    void fetchEntry(id).then((read) => {
      if (!live) return;
      record = read;
      const run = waiting;
      waiting = null;
      if (run && read?.id === id) void tick().then(run);
    });
    return () => {
      live = false;
    };
  });

  // The first entry listed opens when the address names none.
  $effect(() => {
    if (where.entry !== null || !m) return;
    void lines;
    if (picked !== null && lineOf(picked) >= 0) return;
    const first = rowFrom(0, 1);
    const line = first >= 0 ? lines[first] : undefined;
    picked = line?.kind === "row" ? entryOf(line).id : null;
  });

  function pick(e: Entry): void {
    waiting = null;
    picked = e.id;
    if (where.entry === null) return;
    // Drawn now, as a press of the tree is (`going`), so the row picked is the open one before the address answers:
    // `Enter` in the filter focuses it (#2531).
    if (node) going = node.id;
    location.hash = node ? hashOf(node) : "#/secrets";
  }

  function move(step: number): void {
    const at = lineOf(openId);
    // From the open row one row on, the last row staying put; with none open, the first row.
    const next = at < 0 ? rowFrom(0, 1) : rowFrom(at + step, step);
    const index = next < 0 ? at : next;
    const line = index >= 0 ? lines[index] : undefined;
    if (line?.kind !== "row") return;
    pick(entryOf(line));
    listWin.reveal(index);
  }

  const source = $derived.by((): PwSource | null => {
    if (!held || !openEntry) return null;
    return held.sources[openEntry.src] ?? null;
  });
  const word = $derived(held && openEntry ? sourceWord(held, openEntry.src) : "the password manager");

  // *Open web interface* in the header (ux30): the folder on show, a source's top or, with one source, the page's top
  // opens the front page; with several sources the page's top has none.
  let headWeb = $state<TfPwWeb | null>(null);
  const webHere = $derived.by((): { source: PwSource; item: string | null; on: WebOn; name: string | null } | null => {
    if (!held || !node) return null;
    if (node.id === "") return held.sources.length === 1 ? { source: held.sources[0]!, item: null, on: "home", name: null } : null;
    const src = held.sources[node.src];
    if (!src) return null;
    if (node.top) return { source: src, item: null, on: "home", name: null };
    return { source: src, item: node.id, on: "folder", name: node.name };
  });
  let retrying = $state(false);

  async function retry(): Promise<void> {
    retrying = true;
    try {
      await read();
    } finally {
      retrying = false;
    }
  }

  // ---- keys ----
  function onkey(event: KeyboardEvent): void {
    if (!visible || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "Escape") {
      if (query !== "") {
        query = "";
        event.preventDefault();
        return;
      }
      if (inTextField(event.target)) return;
      if (node && node.parent) {
        event.preventDefault();
        go(node.parent);
      }
      return;
    }
    if (inTextField(event.target)) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    // W: the open entry in the password manager's web interface, else the folder; Shift W in the browser.
    if (key === "w") {
      event.preventDefault();
      if (record && entryPane) entryPane.openWeb(event.shiftKey);
      else void headWeb?.press(event.shiftKey);
      return;
    }
    const act: Record<string, () => void> = {
      ArrowDown: () => move(1),
      ArrowUp: () => move(-1),
      j: () => move(1),
      k: () => move(-1),
      ...onEntry,
      e: () => {
        if (record && source && record.can_modify && source.offers.writes.includes("edit_entry") && source.down_at === null) writing = "edit";
      },
      n: () => {
        if (newSource) writing = "new";
      },
    };
    // ↑ ↓ in the tree are the tree's.
    if ((key === "ArrowDown" || key === "ArrowUp") && (event.target as HTMLElement | null)?.closest?.(".tf-pw-tree")) return;
    const run = act[key];
    if (!run) return;
    event.preventDefault();
    if (key === "e" || Object.hasOwn(onEntry, key)) actOnEntry(run);
    else run();
  }

  /** What C, S, U, O and A do to the open entry: by their bare key outside a field, by ⌘⇧ and the key in the filter. */
  const onEntry: Record<string, () => void> = {
    c: () => entryPane?.copyPassword(),
    s: () => entryPane?.togglePassword(),
    u: () => void entryPane?.copyUsername(),
    o: () => entryPane?.copyCode(),
    a: () => void entryPane?.address(),
  };

  /** Runs `run` on the open entry, or once it is read when the pane still draws another (`waiting`). */
  function actOnEntry(run: () => void): void {
    if (openId !== null && record?.id !== openId) {
      waiting = run;
      return;
    }
    run();
  }

  /**
   * The filter's own keys (#2531), before ↑ ↓: ⌘⇧C, S, U, O and A do what C, S, U, O and A do outside a field, so a
   * match is acted on without leaving the box; ⌘C, ⌘A and the rest stay the box's own. ⌘ is ⌘ or Ctrl, as the shell
   * reads it (`isBoxChord`), and a layout without Latin letters is read by the key's place. `Enter` takes focus to the
   * open entry's row, where the bare keys answer. Answers whether it took the key.
   */
  function filterChord(event: KeyboardEvent): boolean {
    if (event.altKey || event.isComposing) return false;
    if (event.key === "Enter") {
      if (event.shiftKey || event.metaKey || event.ctrlKey) return false;
      event.preventDefault();
      if (index !== null && !order.settled(query, scoped)) entering = query;
      else focusOpenRow();
      return true;
    }
    if (!(event.metaKey || event.ctrlKey) || !event.shiftKey) return false;
    const letter = /^[a-z]$/i.test(event.key) ? event.key.toLowerCase() : /^Key[A-Z]$/.test(event.code) ? event.code.slice(3).toLowerCase() : "";
    const run = Object.hasOwn(onEntry, letter) ? onEntry[letter] : undefined;
    if (!run) return false;
    event.preventDefault();
    actOnEntry(run);
    return true;
  }

  /**
   * Focus on the open entry's row, drawn first where the window has not drawn it; with none open, or the open one not
   * among the matches, the first row's, which it opens.
   */
  let rowsBox = $state<HTMLElement | null>(null);
  function focusOpenRow(): void {
    let at = lineOf(openId);
    if (at < 0) {
      at = rowFrom(0, 1);
      const line = at >= 0 ? lines[at] : undefined;
      if (line?.kind !== "row") return;
      pick(entryOf(line));
    }
    listWin.reveal(at);
    flushSync();
    rowsBox?.querySelector<HTMLElement>('.tf-pw-row[aria-current="true"]')?.focus();
  }

  // ---- words ----
  const multi = $derived((held?.sources.length ?? 0) > 1);
  const headLine = $derived.by(() => {
    if (!held || !m || !node) return "";
    const own = node.top ? [held.sources[node.src]!] : node.id === "" ? held.sources : [held.sources[node.src]].filter((s): s is PwSource => !!s);
    const lines = own.map((s) => {
      const name = multi && node.id === "" ? `${s.name} ` : "";
      if (s.never_synced) return `${name || "It "}is not synced yet.`;
      const when = s.synced_at ? (s.by_hand ? `by hand at ${clockLabel(s.synced_at, new Date(), true)}` : stampLabel(s.synced_at, new Date(), true)) : "";
      if (s.down_at) return `${name || s.name + " "}did not answer at ${clockLabel(s.down_at, new Date(), true)}; its entries are as synced ${when}.`;
      return `${name ? `${name}synced` : "Synced"} ${when}.`;
    });
    const folders = countFolders(node);
    const count = query
      ? `${plural(shownPanels[0]?.count ?? 0, "entry matches", "entries match")} "${query}".`
      : `${plural(node.total, "entry", "entries")}${folders ? ` in ${plural(folders, "folder")}` : ""}.`;
    const rule = node.id !== "" && commentFolder(node) ? " A usage comment is required for every read here." : "";
    return `${count} ${lines.join(" ")}${rule}`;
  });

  function countFolders(at: Node): number {
    let n = 0;
    const walk = (x: Node) => {
      for (const kid of x.kids) {
        if (!kid.top) n++;
        walk(kid);
      }
    };
    walk(at);
    return n;
  }

  /** A tree row's depth, as `--d`, through the CSSOM: CSP refuses a style attribute. */
  function depth(n: number) {
    return (el: HTMLElement) => {
      el.style.setProperty("--d", String(n));
    };
  }

  // ---- writes (ADR-0056) ----
  let writing = $state<"change" | "new" | "edit" | null>(null);
  /** The source a new entry goes to: the folder's, else the first that performs creates. */
  const newSource = $derived.by((): PwSource | null => {
    const h = held;
    if (!h) return null;
    const at = node && node.src >= 0 ? h.sources[node.src] : null;
    // A folder the account may add to: the one the list stands in, else any of the source's (Pleasant's reader may
    // add to none, and its root takes nothing from anyone: seen in the real app, 2026-11-22).
    const adds = (src: number) => h.folders.some((f) => f.source === src && f.can_add && (!node || node.id === "" || node.top || f.id === node.id));
    const performs = (s: PwSource) => s.offers.writes.includes("create_entry") && s.down_at === null && !s.never_synced && adds(h.sources.indexOf(s));
    if (at) return performs(at) ? at : null;
    return h.sources.find(performs) ?? null;
  });

  async function written(entityId: string, words: string): Promise<void> {
    records.clear();
    push({ text: `${words}.` });
    await read();
    if (where.entry === null) picked = entityId;
    else location.hash = `#/secrets/entry/${encodeURIComponent(entityId)}`;
    records.delete(entityId);
    const fresh = await fetchEntry(entityId).catch(() => null);
    if (fresh && openId === entityId) record = fresh;
  }

  function copied(what: string): void {

    push({ text: `Copied the ${what}.` });
  }
</script>

<svelte:window onkeydown={onkey} />

{#if writing !== null && m}
  {@const writeSource = writing === "new" ? newSource : source}
  {#if writeSource}
    <TfPwWrite
      kind={writing}
      source={writeSource}
      entry={writing === "new" ? null : record}
      folder={node}
      {m}
      {timedTicket}
      {layers}
      send={ports.write}
      copySecret={ports.copySecret}
      onclose={() => (writing = null)}
      ondone={(id, words) => void written(id, words)}
    />
  {/if}
{/if}

{#snippet mark()}
  <svg class="tf-pw-mark" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" role="img" aria-label="Usage comment required"
    ><title>A usage comment is required for every read here</title><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" /></svg
  >
{/snippet}

{#snippet row(e: Entry, path: boolean, last: boolean)}
  {@const x = held?.sources[e.src]?.offers.expiry ? expiry(e.expires) : null}
  {@const lit = query.trim() === "" ? null : entryMarks(e, order.marks(e))}
  <button
    type="button"
    class="tf-pw-row"
    class:tf-pw-last={last}
    aria-current={e.id === openId ? "true" : undefined}
    onclick={() => pick(e)}
    onpointerenter={() => ahead(e.id)}
  >
    <span class="tf-pw-t"><Marked text={e.title} typed={marking} pairs={lit?.title ?? []} /></span>
    <span class="tf-pw-s"
      >{#if e.user}<span class="tf-mono"><Marked text={e.user} typed={marking} pairs={lit?.user ?? []} /></span>{/if}{#if path && m}<span
          ><Marked text={pathWords(m, e.folder)} typed={marking} pairs={lit?.path ?? []} /></span
        >{:else if e.address}<span class="tf-mono"><Marked text={bare(e.address)} typed={marking} pairs={lit?.address ?? []} /></span>{/if}{#if !e.user && !e.address && !path}&nbsp;{/if}</span
    >
    <span class="tf-pw-e"
      >{#if x && x.tone}<span class="tf-chip tf-{x.tone}">{x.text}</span>{/if}{#if e.flags & NEEDS_APPROVAL}<span class="tf-lab">approval</span>{/if}{#if e.flags & HAS_CODE}<span class="tf-lab">code</span>{/if}{#if e.flags & CANNOT_READ}<span class="tf-lab">no read</span>{/if}{#if e.changed}<span class="tf-num">{e.changed.slice(0, 10)}</span>{/if}</span
    >
  </button>
{/snippet}

<div class="tf-pw-page">
  {#if !held || !m || !node}
    <div class="tf-pw-none">
      {#if failed}<p class="tf-stg-errt">{failed}</p>{/if}
    </div>
  {:else if held.sources.length === 0}
    <div class="tf-pw-none">
      <div class="tf-empty">
        <h1>No secrets source yet</h1>
        <p>
          Add your secrets source in Settings › Sources, the way Jira or Gitea is added. Its entries are listed here and found by the filter. A protected value stays where it is:
          knobas reads one only when you show or copy it.
        </p>
        <a class="tf-btn tf-primary" href="#/settings/sources">Add a secrets source</a>
      </div>
    </div>
  {:else if held.sources.length === 1 && held.sources[0]!.never_synced}
    {@const only = held.sources[0]!}
    <div class="tf-pw-none">
      <div class="tf-empty">
        <h1>{only.name} is not synced yet</h1>
        <p>{only.by_hand ? "It syncs only when you press Sync now. " : ""}A sync lists its entries and their folders. It reads no passwords.</p>
        {@render sync?.()}
      </div>
    </div>
  {:else}
    <div class="tf-pw-body">
      <aside class="tf-rd-tree tf-pw-tree">
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="tf-dl-body" role="tree" aria-label="Folders" tabindex="-1" onkeydown={treeKey} {@attach treeWin.list}>
          {#if treeWin.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach treeWin.room("before")}></div>{/if}
          {#each treeWin.slice(rows) as r (r.node.id || "~")}
            {@const here = r.node === node}
            {@const src = r.node.src >= 0 ? held.sources[r.node.src] : null}
            <button
              type="button"
              role="treeitem"
              class="tf-pw-node"
              class:tf-pw-top={r.node.top}
              class:tf-holds={!r.open && r.node.kids.length > 0 && holds.has(r.node.id) && !here}
              {@attach depth(Math.max(0, r.node.depth))}
              aria-level={Math.max(0, r.node.depth) + 1}
              aria-expanded={r.node.kids.length > 0 && r.node.id !== "" ? r.open : undefined}
              aria-selected={here}
              aria-current={here ? "true" : undefined}
              tabindex={here ? 0 : -1}
              data-id={r.node.id || "~"}
              onclick={(event) => pressNode(event, r.node)}
              {@attach treeWin.row(r.node.id || "~")}
              ><span class="tf-pw-tw" data-twist={r.node.kids.length > 0 && r.node.id !== "" ? "" : undefined}
                >{r.node.kids.length > 0 && r.node.id !== "" ? (r.open ? "▾" : "▸") : ""}</span
              ><span class="tf-pw-nm">{r.node.name}</span>{#if r.node.comment}{@render mark()}{/if}{#if r.node.top && src?.down_at}<span
                  class="tf-dot tf-err"
                  role="img"
                  aria-label="Not reachable"
                  title="Did not answer at {clockLabel(src.down_at, new Date(), true)}"
                ></span>{/if}{#if r.node.top && src?.never_synced}<span class="tf-pw-n">not synced</span>{:else}<span class="tf-pw-n">{r.node.total}</span>{/if}</button
            >
          {/each}
          {#if treeWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach treeWin.room("after")}></div>{/if}
        </div>
      </aside>
      <section class="tf-pw-work">
        <div class="tf-pw-bench">
          <header class="tf-fhead tf-pw-head">
            <div>
              <h1>{node.id === "" ? "Secrets" : node.name}</h1>
              <p class="tf-sub">{headLine}</p>
            </div>
            <div class="tf-acts">
              {#if newSource}<button type="button" class="tf-btn" aria-label="New entry" aria-keyshortcuts="N" onclick={() => (writing = "new")}>New entry<kbd>N</kbd></button>{/if}
              {#if webHere}{#key `${webHere.source.id}/${webHere.item}`}<TfPwWeb bind:this={headWeb} source={webHere.source} item={webHere.item} on={webHere.on} name={webHere.name} {ports} key={record === null} />{/key}{/if}
              {@render sync?.()}
            </div>
          </header>
          <div class="tf-pw-cols">
            <div class="tf-pw-listcol">
              <label class="tf-fsearch tf-pw-filter"
                ><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
                  ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
                ><input
                  type="text"
                  placeholder={node.id === "" ? "Filter every folder" : `Filter ${node.name}`}
                  aria-label={node.id === "" ? "Filter the entries of every folder" : `Filter the entries of ${node.name} and the folders under it`}
                  autocomplete="off"
                  spellcheck="false"
                  bind:value={query}
                  {@attach filterKeys()}
                  onkeydown={(event) => {
                    if (filterChord(event)) return;
                    if (event.key === "ArrowDown") {
                      event.preventDefault();
                      move(1);
                    } else if (event.key === "ArrowUp") {
                      event.preventDefault();
                      move(-1);
                    }
                  }}
                /><kbd>{keyHint("⌘F")}</kbd></label
              >
              <div class="tf-pw-rows" bind:this={rowsBox} {@attach listWin.list}>
                {#each listWin.slice(lines) as line (line.key)}
                  {#if line.kind === "head"}
                    <header class="tf-pw-lhead" class:tf-pw-first={line.first} {@attach listWin.row(line.key)}>
                      <h3>
                        {#if line.href}<a href={line.href}>{#if line.pre}<span class="tf-pw-pre">{line.pre}</span>{/if}{line.title}</a>{:else}{line.title}{/if}
                      </h3>
                      {#if line.comment}{@render mark()}{/if}
                      <span class="tf-count">{line.count}</span>
                    </header>
                  {:else if line.kind === "row"}
                    <div class="tf-pw-lrow" class:tf-pw-last={line.last} {@attach listWin.row(line.key)}>{@render row(entryOf(line), line.path, line.last)}</div>
                  {:else if line.kind === "more"}
                    <div class="tf-pw-lrow tf-pw-last" {@attach listWin.row(line.key)}>
                      <button type="button" class="tf-pw-more" onclick={() => go(line.node)}>Show all {line.count} in {line.node.name}</button>
                    </div>
                  {:else}
                    <div class="tf-panel" {@attach listWin.row(line.key)}><div class="tf-empty tf-pw-blank"><p>{line.text}</p></div></div>
                  {/if}
                {/each}
              </div>
              <div class="tf-pw-keys" aria-label="Keys">
                <span><kbd>↑</kbd> <kbd>↓</kbd> move</span><span><kbd>{keyHint("⌘F")}</kbd> filter</span><span><kbd>C</kbd> copy the password</span><span><kbd>U</kbd> username</span><span
                  ><kbd>O</kbd> one-time code</span
                ><span><kbd>Esc</kbd> up a folder</span>
              </div>
            </div>
            <div class="tf-pw-entrycol">
              {#if warm}<div class="tf-pw-entry tf-pw-warm" aria-hidden="true"><span class="tf-pw-code">000<i>000</i></span></div>{/if}
              {#if record && source && openEntry}
                {#key record.id}
                  <TfPwEntry bind:this={entryPane} entry={record} {source} {m} {word} {timedTicket} {visible} {ports} onretry={() => void retry()} {retrying} {copied} onchange={() => (writing = "change")} onedit={() => (writing = "edit")} {layers} ondown={() => void read()} />
                {/key}
              {:else}
                <div class="tf-panel"><div class="tf-empty tf-pw-blank"><p>{query ? "No entry matches the filter." : "No entry is open."}</p></div></div>
              {/if}
            </div>
          </div>
        </div>
      </section>
    </div>
  {/if}
</div>
