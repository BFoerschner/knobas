/**
 * What a right-click is on (2026-11-16, `docs/decisions/2026-11-16-right-click-menus.md`): the thing under the pointer,
 * or under focus for the context menu key and Shift+F10, read from the page as it is drawn, with nothing read from the
 * backend. The menu opens in the frame it is asked for; an item that needs more reads it when the menu opens and never
 * waits for it.
 *
 * Most things are already named by the page: a row is a link to the thing's address (`#/ticket/…`, `#/pr/…`,
 * `#/repo/<id>/files/<path>`, `#/build/<pipeline>/<run>`), so the address is the thing. A row that is no link (a
 * branch, a test, a monitor, an Inbox line, a diff's file) carries a tag, {@link menuThing}, set by the page that draws
 * it. The nearest tag or link up from the pressed element wins, and a table's row is the thing its first link names,
 * so a button inside a row gives the row's menu.
 *
 * Three are read from where they stand on the page whose address names their repository (`#/repo/<id>/…`), so the
 * page that draws them needs no tag: a folder of the file tree (`.tf-fdir[data-path]`), a line of the file view
 * (`.tf-fileview .tf-l[data-n]`) and an image of a rendered file that is a file of the repository (`img[data-md-path]`).
 * And a page's heading is that page's thing (`own`: its menu has no *Open*, the reader is there).
 *
 * A file of a generated folder (ADR-0046) is no file of the repository: it is a `made` thing, read from its row's address
 * (`#/repo/<id>/rendered/…/files/<path>`), a tag on a generated folder's row, or a line of a generated file's view, and
 * nothing on it ever acts on the real file of its path (ADR-0060 amended 2026-11-25).
 */
import { parseTfHash } from "../route";
import { placeOf, spaceAt, spaceId, spaceTitle } from "../docs/docs";
import { heldTree } from "../docs/tree-store";

export type Thing =
  /** An item of a source, opened at its address. `kind` is the address's kind, not the source's word. */
  | { kind: "ticket" | "pr" | "repo" | "branch" | "page" | "asset" | "note" | "set" | "item"; id: string; hash: string }
  /**
   * A pipeline. `unread` where the Builds rail draws it (#2527): the unread failed runs of it the rail holds, its last
   * twelve, read when the menu opens; the menu's *Mark N runs as read* marks exactly those. A link elsewhere has none.
   */
  | { kind: "pipeline"; id: string; hash: string; unread?: readonly string[] }
  /** A folder of the Builds rail (#2527): what it says, and the unread failed runs the rail holds of every pipeline under it. */
  | { kind: "pipeline-folder"; name: string; unread: readonly string[] }
  /**
   * A monitor; `alerting` where its row shows an open alert, so *Acknowledge* is offered with no read, and `ack` the
   * row's own *Ack*, so its *Acked* and its toast follow.
   */
  | { kind: "monitor"; id: string; hash: string; alerting?: boolean; ack?: () => void }
  /** A commit; `repoId` when the address names its repository. */
  | { kind: "commit"; id: string; repoId: string | null; hash: string }
  /**
   * A run of a pipeline; `pipelineId` when the address names it. `failed` where the page draws it as a finished failed
   * run (`data-failed` on its row or heading), which can be marked read or unread (ux18).
   */
  | { kind: "run"; id: string; pipelineId: string | null; hash: string; failed?: boolean }
  /** A file or a folder of a repository's tree, by its path in the repository. */
  | { kind: "file" | "folder"; repoId: string; path: string }
  /** A line of a file, from 1. */
  | { kind: "line"; repoId: string; path: string; line: number }
  /**
   * A file or a folder of a generated folder (ADR-0046): what `renderer` made of the repository's `folder`, `rel` from
   * the generated folder's top (`""` the top), at `line` where one is pressed. Never a file of the repository.
   */
  | { kind: "made"; repoId: string; renderer: string; folder: string; rel: string; dir: boolean; line: number | null }
  /**
   * A line of a document view that holds a value (the XML view's design, 2026-11-23, sections 15 and 22): the line's own
   * rows, and the value's. `does` is the view's own: it opens the value viewer, and asks the query. `link` is false in a
   * document laid out by knobas, whose line numbers are not the file's.
   */
  | { kind: "value"; repoId: string; path: string; line: number; link: boolean; does: ValueDoes }
  /** A branch drawn as a row of a repository's Branches tab: its name, not an entity. */
  | { kind: "branch-name"; repoId: string; name: string }
  /** A test of a run, by its name. */
  | { kind: "test"; name: string }
  /** A link to a page on the web, in synced text or drawn by the app. */
  | { kind: "web"; url: string }
  /**
   * A Docs space (PR #2479, 2026-11-17): `id` its `spaceId` (`<source>/<key>`) and `name` as Docs draws it, read off the
   * tree Docs already holds (`tree-store.ts`), so it can be made Docs' home or hidden. A space the tree does not hold is
   * no thing.
   */
  | { kind: "space"; id: string; name: string; hash: string }
  /** An Inbox line, by its key: the Inbox page's own writes on it (`ops`, the backend's offer on the line). */
  | { kind: "inbox"; key: string; title: string; open: string | null; ops: readonly string[]; entityId: string | null; sourceId: string | null };

/** What a document view does for a value's rows. */
export interface ValueDoes {
  /** *Look at this value*, on knobas's guess or as text. */
  look: () => void;
  /** *Send it through* a step: `base64_decode`, `url_decode`, `unescape`, `jwt`; `null` opens the viewer with no step. */
  through: (step: "base64_decode" | "url_decode" | "unescape" | "jwt" | null) => void;
  /** Copies the line's element as an XPath that finds it alone. */
  copyXPath: () => void;
  /** *Query for elements like this*. */
  like: () => void;
}

/** What a thing was found as: `own` is the page's own, from its heading. */
export interface Found {
  thing: Thing;
  own: boolean;
  /** The element that stands for it, which keeps its hover look while the menu is open. */
  node: Element;
}

const TAGS = new WeakMap<Element, { read: () => Thing | null; claims: boolean }>();

/**
 * `{@attach menuThing(() => thing)}`: the element is that thing for a right-click, for a row that is no link to it.
 * Read when the menu opens, so the row's current values count. `claims`: the row is its thing wherever it is pressed,
 * links inside it included (an Inbox line, whose sentence links what it is about).
 */
export function menuThing(read: () => Thing | null, { claims = false }: { claims?: boolean } = {}) {
  return (node: Element) => {
    TAGS.set(node, { read, claims });
    return () => {
      TAGS.delete(node);
    };
  };
}

/**
 * The thing an address names, or `null` for an address that is a place and no thing (a section, a tab of one, a
 * setting). The one reader of addresses here: what `route.ts` parses, `job.ts`' `itemHash` makes.
 */
export function thingOfHash(hash: string): Thing | null {
  if (!hash.startsWith("#")) return null;
  const route = parseTfHash(hash);
  switch (route.view) {
    case "ticket":
      return { kind: "ticket", id: route.id, hash: `#/ticket/${encodeURIComponent(route.id)}` };
    case "job":
      return route.key === null ? null : { kind: "ticket", id: route.key, hash: `#/ticket/${encodeURIComponent(route.key)}` };
    case "item":
      return { kind: "item", id: route.id, hash };
    case "note":
      return { kind: "note", id: route.id, hash };
    case "tickets":
    case "unknown":
    case "site":
      return null;
    case "full":
      break;
  }
  const [first, second, ...more] = route.rest;
  if (first === undefined || first === "") return null;
  const at = (head: string, id: string) => `#/${head}/${encodeURIComponent(id)}`;
  switch (route.head) {
    case "pr":
      return { kind: "pr", id: first, hash: at("pr", first) };
    case "branch":
      return { kind: "branch", id: first, hash: at("branch", first) };
    case "commit":
      return { kind: "commit", id: first, repoId: null, hash: at("commit", first) };
    case "page":
      return { kind: "page", id: first, hash: at("page", first) };
    case "docs": {
      const tree = heldTree.tree;
      if (first !== "space" || second === undefined || tree === null) return null;
      const space = spaceAt(placeOf(route.rest), tree);
      return space === undefined ? null : { kind: "space", id: spaceId(space), name: spaceTitle(space, tree.spaces), hash };
    }
    case "asset":
      return { kind: "asset", id: first, hash: at("asset", first) };
    case "sets":
      return { kind: "set", id: first, hash: at("sets", first) };
    case "builds":
      return { kind: "pipeline", id: first, hash: at("builds", first) };
    case "build":
      return second === undefined
        ? { kind: "run", id: first, pipelineId: null, hash: at("build", first) }
        : { kind: "run", id: second, pipelineId: first, hash: `${at("build", first)}/${encodeURIComponent(second)}` };
    case "monitors":
      return first === "at" && second ? { kind: "monitor", id: second, hash: `#/monitors/at/${encodeURIComponent(second)}` } : null;
    case "repo": {
      if (second === "files" && more.length > 0) return { kind: "file", repoId: first, path: more.join("/") };
      if (second === "rendered" && more.length >= 2) return madeOfRoute(first, more);
      if (second === "line" && more.length > 1) {
        const line = Number(more[0]);
        const path = more.slice(1).join("/");
        return Number.isInteger(line) && line > 0 ? { kind: "line", repoId: first, path, line } : { kind: "file", repoId: first, path };
      }
      if (second === "commits" && more[0]) return { kind: "commit", id: more[0], repoId: first, hash: `${at("repo", first)}/commits/${encodeURIComponent(more[0])}` };
      return { kind: "repo", id: first, hash: at("repo", first) };
    }
    default:
      return null;
  }
}

/**
 * A generated folder's address after `#/repo/<id>/rendered/`: `<renderer>/<folder>` (`.` the top), then `files/<path>`,
 * `line/<n>/<path>` or `at/<place>/<path>`; the folder itself with nothing after it.
 */
function madeOfRoute(repoId: string, rest: readonly string[]): Thing | null {
  const [renderer, folderSeg, how, ...tail] = rest;
  if (renderer === undefined || folderSeg === undefined) return null;
  const folder = folderSeg === "." ? "" : folderSeg;
  const made = (rel: string, dir: boolean, line: number | null): Thing => ({ kind: "made", repoId, renderer, folder, rel, dir, line });
  if (how === undefined) return made("", true, null);
  if (how === "files" && tail.length > 0) return made(tail.join("/"), false, null);
  if (how === "line" && tail.length > 1) {
    const line = Number(tail[0]);
    return made(tail.slice(1).join("/"), false, Number.isInteger(line) && line > 0 ? line : null);
  }
  if (how === "at" && tail.length > 1) return made(tail.slice(1).join("/"), false, null);
  return null;
}

/** The repository the page's address stands in (`#/repo/<id>/…`), or `null`. */
function repoOfPage(hash: string): string | null {
  const route = parseTfHash(hash);
  return route.view === "full" && route.head === "repo" && route.rest[0] ? route.rest[0] : null;
}

/** The file the page's address shows (`#/repo/<id>/files/<path>` or at a line), or `null`. */
function fileOfPage(hash: string): string | null {
  const thing = thingOfHash(hash);
  return thing !== null && (thing.kind === "file" || thing.kind === "line") ? thing.path : null;
}

/**
 * Whether a right-click on `node` belongs to the web view: a field a person types in, where its own items (spelling,
 * paste) are the point. Not a `select`: it takes no typing, and the web views offer it their page menu (2026-11-17).
 */
export function isEditable(node: Element | null): boolean {
  const field = node?.closest?.("input, textarea, [contenteditable]:not([contenteditable='false'])");
  if (!field) return false;
  if (field instanceof HTMLInputElement) {
    // A box that takes no typing is a control: a checkbox's right-click has nothing of the platform's to offer.
    return !["checkbox", "radio", "button", "submit", "reset", "range", "color", "file", "image", "hidden"].includes(field.type);
  }
  return true;
}

/**
 * The thing `node` is, or is inside: the nearest tag or link up from it, else what its place on the page names.
 * `hash` is the page's address, `origin` the app's.
 */
export function thingAt(node: Element | null, hash: string = location.hash, origin: string = location.origin): Found | null {
  const found = thingAtOnly(node, hash, origin);
  // A run the page draws as failed says so where it stands (ux18): its row or its heading carries `data-failed`.
  if (found !== null && found.thing.kind === "run" && found.node.closest("[data-failed]") !== null) {
    return { ...found, thing: { ...found.thing, failed: true } };
  }
  return found;
}

function thingAtOnly(node: Element | null, hash: string, origin: string): Found | null {
  for (let at: Element | null = node; at !== null; at = at.parentElement) {
    const tag = TAGS.get(at);
    if (tag?.claims) {
      const thing = tag.read();
      if (thing) return { thing, own: false, node: at };
    }
  }
  for (let at: Element | null = node; at !== null; at = at.parentElement) {
    const tag = TAGS.get(at);
    if (tag) {
      const thing = tag.read();
      if (thing) return { thing, own: false, node: at };
    }
    if (at instanceof HTMLAnchorElement && at.hasAttribute("href")) {
      const href = at.getAttribute("href") ?? "";
      if (href.startsWith("#")) {
        // A link to a place (a tab, a section) is no thing: the row around it may still be one.
        const thing = thingOfHash(href);
        // A generated folder's row is its generated file, never the repository's file of its path.
        if (thing && thing.kind !== "made" && at.closest("[data-made]")) return null;
        if (thing) return { thing, own: false, node: at };
        continue;
      }
      const url = webUrl(at.href, origin);
      if (url) return { thing: { kind: "web", url }, own: false, node: at };
      continue;
    }
    const repoId = repoOfPage(hash);
    // A row of a generated folder (ADR-0046) is its address or its tag, read above; never the real file or folder of
    // its path.
    if ((at as HTMLElement).dataset?.["made"] !== undefined) return null;
    if (repoId !== null) {
      const path = (at as HTMLElement).dataset?.["path"];
      if (at.classList.contains("tf-fdir") && path) return { thing: { kind: "folder", repoId, path }, own: false, node: at };
      const md = at instanceof HTMLImageElement ? at.dataset["mdPath"] : undefined;
      if (md) return { thing: { kind: "file", repoId, path: md.replace(/^\//, "") }, own: false, node: at };
      const n = (at as HTMLElement).dataset?.["n"];
      const file = fileOfPage(hash);
      if (n && file !== null && at.classList.contains("tf-l") && at.closest(".tf-fileview")) {
        return { thing: { kind: "line", repoId, path: file, line: Number(n) }, own: false, node: at };
      }
      // A line of a generated file's view: the generated file at that line.
      const made = n && at.classList.contains("tf-l") && at.closest(".tf-fileview") ? thingOfHash(hash) : null;
      if (made !== null && made.kind === "made" && !made.dir) return { thing: { ...made, line: Number(n) }, own: false, node: at };
    }
    // A table's row is the thing its first link names: its actions' cell holds no link to it.
    if (at.tagName === "TR") {
      for (const link of at.querySelectorAll<HTMLAnchorElement>('a[href^="#"]')) {
        const thing = thingOfHash(link.getAttribute("href") ?? "");
        if (thing) return { thing, own: false, node: link };
      }
    }
    // A page's heading is the page's own thing.
    if (at.tagName === "H1" && at.closest(".tf-main")) {
      const thing = thingOfHash(hash);
      if (thing) return { thing: thing.kind === "line" ? { kind: "file", repoId: thing.repoId, path: thing.path } : thing, own: true, node: at };
    }
    // A float's own menu, a dialog: what is under them is not theirs.
    if (at.classList.contains("tf-floats") || at.getAttribute("role") === "dialog") break;
  }
  return null;
}

/** `href` when it is an http(s) page of another origin than the app's, else `null`. */
export function webUrl(href: string, origin: string): string | null {
  try {
    const url = new URL(href);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.origin === origin ? null : url.href;
  } catch {
    return null;
  }
}

/** The selected words, when the press is inside them; `null` otherwise. A press beside a selection is not on it. */
export function selectionAt(node: Node | null, selection: Selection | null): string | null {
  if (selection === null || selection.isCollapsed || selection.rangeCount === 0 || node === null) return null;
  const text = selection.toString().trim();
  if (text === "") return null;
  for (let index = 0; index < selection.rangeCount; index++) {
    if (selection.getRangeAt(index).intersectsNode(node)) return text;
  }
  return null;
}
