/**
 * **Find over what a page has drawn** (design system, the key table): the matches of some words in a page's text nodes, for
 * `TfFindBar`. Each page with no box has its own renderer, so this reads the DOM they draw and changes none of it.
 *
 * It finds across the pieces a line is drawn in, so a word split by syntax colours is one match, and never across a block's
 * edge. It leaves out what isn't the page's words: a control's label, a line number, what is hidden, and a closed fold.
 *
 * Marks draw with the CSS Custom Highlight API. Where a webview has none, only the current match shows, as a selection, and
 * only once it is stepped to: a selection takes the caret out of the box, so it can't draw under typing.
 */

/** What holds no words of the page. */
const LEFT_OUT =
  'button, input, select, textarea, script, style, svg, [hidden], [aria-hidden="true"], .tf-no, .tf-findhold, details:not([open]) > :not(summary)';
/** Tags that flow within a line, so text either side of one reads as one run. */
const INLINE = new Set(["A", "ABBR", "B", "BDI", "BDO", "CITE", "CODE", "DEL", "DFN", "EM", "I", "INS", "KBD", "MARK", "Q", "S", "SAMP", "SMALL", "SPAN", "STRONG", "SUB", "SUP", "TIME", "U", "VAR"]);

const MATCHES = "tf-find";
const CURRENT = "tf-find-now";

/** The words as a pattern: as typed, any case, with any run of spaces matching any other. */
function patternOf(words: string): RegExp | null {
  const trimmed = words.trim();
  if (trimmed === "") return null;
  return new RegExp(trimmed.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\s+/g, "\\s+"), "giu");
}

function blockOf(from: Element | null, known: Map<Element, Element | null>): Element | null {
  if (from === null) return null;
  const seen = known.get(from);
  if (seen !== undefined) return seen;
  const block = INLINE.has(from.tagName) ? blockOf(from.parentElement, known) : from;
  known.set(from, block);
  return block;
}

/** A range over `run`'s text from `start` to `end`, as offsets into the run joined up. */
function rangeOver(run: readonly Text[], start: number, end: number): Range {
  const range = document.createRange();
  let passed = 0;
  for (const node of run) {
    const after = passed + node.data.length;
    if (start >= passed && start < after) range.setStart(node, start - passed);
    if (end > passed && end <= after) {
      range.setEnd(node, end - passed);
      break;
    }
    passed = after;
  }
  return range;
}

/** The drawn matches of `words` in `root`, in the page's order. */
export function findIn(root: Element, words: string): Range[] {
  const pattern = patternOf(words);
  if (pattern === null) return [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.nodeType === Node.TEXT_NODE ? NodeFilter.FILTER_ACCEPT : (node as Element).matches(LEFT_OUT) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_SKIP),
  });
  const found: Range[] = [];
  const known = new Map<Element, Element | null>();
  const search = (run: readonly Text[]): void => {
    const text = run.map((node) => node.data).join("");
    for (const match of text.matchAll(pattern)) {
      const range = rangeOver(run, match.index, match.index + match[0].length);
      // What the stylesheet hides has no box. A layout-free test has no way to ask.
      if (typeof range.getClientRects !== "function" || range.getClientRects().length > 0) found.push(range);
    }
  };
  let run: Text[] = [];
  let block: Element | null = null;
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const here = blockOf(node.parentElement, known);
    if (here !== block && run.length > 0) {
      search(run);
      run = [];
    }
    block = here;
    run.push(node as Text);
  }
  search(run);
  return found;
}

/** Whether this webview draws marks without touching the DOM. `tauri.conf.json` names no minimum macOS, so it may not. */
export function canMark(): boolean {
  return typeof Highlight === "function" && typeof CSS !== "undefined" && CSS.highlights !== undefined;
}

/** Draw the matches, `current` apart. `stepped`: it was stepped to, so a webview with no marks may show it as a selection. */
export function drawMatches(matches: readonly Range[], current: number, stepped = false): void {
  const now = matches[current];
  if (!canMark()) {
    if (stepped && now !== undefined) getSelection()?.setBaseAndExtent(now.startContainer, now.startOffset, now.endContainer, now.endOffset);
    return;
  }
  const rest = new Highlight();
  matches.forEach((range, at) => {
    if (at !== current) rest.add(range);
  });
  CSS.highlights.set(MATCHES, rest);
  const shown = new Highlight();
  if (now !== undefined) shown.add(now);
  shown.priority = 1;
  CSS.highlights.set(CURRENT, shown);
}

/** No marks at all. */
export function clearMatches(): void {
  if (!canMark()) return;
  CSS.highlights.delete(MATCHES);
  CSS.highlights.delete(CURRENT);
}

/** What ⌘F starts the box with: the page's selected text on one line, and where it is. Text selected in a field is the field's own. */
export function selectedWords(): { words: string; range: Range } | null {
  const selection = getSelection();
  if (selection === null || selection.isCollapsed || selection.rangeCount === 0) return null;
  const words = selection.toString().trim();
  if (words === "" || words.includes("\n") || words.length > 200) return null;
  return { words, range: selection.getRangeAt(0).cloneRange() };
}
