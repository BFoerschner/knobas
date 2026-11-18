/**
 * A Docs page kept hidden, laid out at Reading's new size while the app is idle (the ux18 Windows pass, Fable's ruling,
 * `docs/decisions/2026-11-18-what-the-ux18-windows-pass-left-over.md`).
 *
 * A kept page is hidden by `display: none`, so the page shown again lays itself out whole. After Reading's size changed
 * in Settings › Fonts, that return took 229 / 372 ms at CPU 4x / 6x in Edge on Windows, against 116 / 200 at 100 %:
 * Windows gives every glyph its metrics at a size it has not drawn, and every word of the page is shaped again. On the
 * Mac it is 4 ms more. Laying out a copy of the page's text beforehand, off screen, does that work while the person
 * is still in Settings: the return was 173 / 282 ms after it.
 *
 * So, once Reading's size has held still for {@link QUIET_MS}, each hidden kept Docs page (the most recently shown
 * first, {@link MOST_CHARS} characters in all) has its body's top-level blocks copied, about {@link PIECE_CHARS}
 * characters a piece, into an off-screen box under shallow copies of the body's ancestors, so its own rules draw it;
 * one piece a task, when the app is idle, each laid out by one read and taken down in the same task. A copy carries no
 * id, image, frame or diagram. Another size change stops the warm and starts the wait again; a page shown, dropped or
 * read again since is left alone. It writes nothing to the page it copies.
 */
import { fonts as appFonts, type Fonts } from "../fonts.svelte";

/** How long Reading's size holds still before the warm starts: stepped keys warm one size. */
export const QUIET_MS = 300;
/** About this many characters a piece: 22 / 35 ms at CPU 4x / 6x on Windows, inside a key's line. */
export const PIECE_CHARS = 2000;
/** At most this many characters a warm, over every page. */
export const MOST_CHARS = 24000;

/** What a copy leaves out: nothing in it draws Reading's text, and an image or a frame would load. */
const LEFT_OUT = "img, picture, iframe, video, audio, object, embed, canvas, svg, textarea, input, select, button";

/** One task's work: some of a page's body blocks, in the section that holds them when they are its children. */
export interface Piece {
  /** The kept page (`.tf-kept`) the blocks are in. */
  page: Element;
  /** Its body (`.tf-doc`). */
  doc: Element;
  /** The block that holds `nodes` where it was cut into pieces, else `null`. */
  within: Element | null;
  nodes: Element[];
}

/** Where the warm finds its pages and lays its copies out. */
export interface WarmRoom {
  /** The hidden kept Docs pages, the most recently shown first. */
  pages(): Element[];
  /** The main pane, whose width a copy takes. */
  main(): HTMLElement | null;
}

/** When the warm runs: after a wait, and when the app is idle. Each returns its cancel. */
export interface WarmClock {
  after(go: () => void, ms: number): () => void;
  idle(go: () => void): () => void;
}

const charsOf = (node: Element) => node.textContent.length;

/** `page`'s body cut into pieces of about {@link PIECE_CHARS} characters, at most `budget` in all. */
export function piecesOf(page: Element, budget: number): { pieces: Piece[]; chars: number } {
  const doc = page.querySelector(".tf-doc");
  const pieces: Piece[] = [];
  if (doc === null) return { pieces, chars: 0 };
  let chars = 0;
  const group = (nodes: Element[], within: Element | null) => {
    let open: Element[] = [];
    let size = 0;
    const close = () => {
      if (open.length > 0) pieces.push({ page, doc, within, nodes: open });
      open = [];
      size = 0;
    };
    for (const node of nodes) {
      if (chars >= budget) break;
      const n = charsOf(node);
      if (n === 0) continue;
      // A block much longer than a piece is cut into its own blocks, once.
      if (within === null && n > PIECE_CHARS * 2 && node.children.length > 1) {
        close();
        group([...node.children], node);
        continue;
      }
      open.push(node);
      size += n;
      chars += n;
      if (size >= PIECE_CHARS) close();
    }
    close();
  };
  group([...doc.children].filter((node) => !node.classList.contains("tf-doc-more")), null);
  return { pieces, chars };
}

/** A copy of `node` with no id, and nothing of {@link LEFT_OUT}. */
function cleanCopy(node: Element, deep: boolean): Element {
  const copy = node.cloneNode(deep) as Element;
  copy.removeAttribute("id");
  copy.removeAttribute("hidden");
  if (deep) {
    for (const one of copy.querySelectorAll("[id]")) one.removeAttribute("id");
    for (const one of copy.querySelectorAll(LEFT_OUT)) one.remove();
  }
  return copy;
}

/** Whether `piece` may still be copied: its page hidden and held, and its blocks where they were. */
export function stillHidden(piece: Piece): boolean {
  return piece.page.isConnected && piece.page.hasAttribute("hidden") && piece.doc.isConnected && piece.nodes.every((node) => node.isConnected);
}

/**
 * Lays `piece` out once, off screen, at the main pane's width, under shallow copies of its body's ancestors, and takes
 * it down. `false` when its page has been shown, dropped or read again since.
 */
export function layOut(piece: Piece, main: HTMLElement | null): boolean {
  if (!stillHidden(piece)) return false;
  const host = document.createElement("div");
  host.className = "tf-warm";
  host.setAttribute("aria-hidden", "true");
  host.inert = true;
  const width = main?.clientWidth || 1200;
  host.style.cssText = `position:fixed;left:-30000px;top:0;width:${width}px;visibility:hidden;pointer-events:none;contain:layout paint style`;
  // The ancestors below `<body>`, outermost first, so a rule that names one (`.tf-main > .tf-kept`, `.tf-doc`) draws the
  // copy as the page.
  const chain: Element[] = [];
  for (let at: Element | null = piece.doc; at !== null && at !== document.body; at = at.parentElement) chain.unshift(at);
  let parent: Element = host;
  for (const ancestor of chain) {
    const copy = cleanCopy(ancestor, false);
    if (ancestor !== piece.doc && copy instanceof HTMLElement) {
      // The ancestors' own boxes (a grid, a scroller, a pane's height) would size the body otherwise than the page does.
      copy.style.cssText = "display:block;position:static;width:auto;height:auto;min-height:0;max-height:none;overflow:visible;margin:0";
    }
    parent.append(copy);
    parent = copy;
  }
  if (piece.within !== null) {
    const within = cleanCopy(piece.within, false);
    parent.append(within);
    parent = within;
  }
  for (const node of piece.nodes) parent.append(cleanCopy(node, true));
  document.body.append(host);
  try {
    void host.getBoundingClientRect();
    void (parent as HTMLElement).offsetHeight;
  } finally {
    host.remove();
  }
  return true;
}

/** The app's kept pages and main pane. */
export const documentRoom: WarmRoom = {
  pages: () => [...document.querySelectorAll(".tf-main > .tf-kept[hidden]")].filter((page) => page.querySelector(".tf-doc") !== null).reverse(),
  main: () => document.querySelector<HTMLElement>(".tf-main"),
};

/** A real clock; idle where the engine says so, and never under jsdom, which lays nothing out. */
export const idleClock: WarmClock = {
  after(go, ms) {
    const timer = setTimeout(go, ms);
    return () => clearTimeout(timer);
  },
  idle(go) {
    if (typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) return () => {};
    if (typeof requestIdleCallback === "function") {
      const id = requestIdleCallback(go, { timeout: 2000 });
      return () => cancelIdleCallback(id);
    }
    // WebKit before 18.4 has no `requestIdleCallback`.
    const timer = setTimeout(go, 50);
    return () => clearTimeout(timer);
  },
};

/** Warms the hidden Docs pages after each change of Reading's size, until the returned function is called. */
export function warmDocs(of: Fonts = appFonts, room: WarmRoom = documentRoom, clock: WarmClock = idleClock): () => void {
  let cancel: (() => void) | null = null;
  let run = 0;
  const stop = () => {
    cancel?.();
    cancel = null;
    run += 1;
  };
  const start = () => {
    const mine = ++run;
    const queue: Piece[] = [];
    let budget = MOST_CHARS;
    for (const page of room.pages()) {
      if (budget <= 0) break;
      const { pieces, chars } = piecesOf(page, budget);
      queue.push(...pieces);
      budget -= chars;
    }
    const left = new Set<Element>();
    const step = () => {
      cancel = null;
      if (mine !== run) return;
      let piece = queue.shift();
      while (piece !== undefined && left.has(piece.page)) piece = queue.shift();
      if (piece === undefined) return;
      // A page shown, dropped or read again since is left alone, all of it.
      if (!layOut(piece, room.main())) left.add(piece.page);
      if (queue.length > 0) cancel = clock.idle(step);
    };
    if (queue.length > 0) cancel = clock.idle(step);
  };
  const off = of.onSize((role) => {
    if (role !== "read") return;
    stop();
    cancel = clock.after(() => {
      cancel = null;
      start();
    }, QUIET_MS);
  });
  return () => {
    stop();
    off();
  };
}
