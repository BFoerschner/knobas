/**
 * A Confluence page's **sections**, and the rule that says which of them
 * knobas will let a reader edit (spec #272, issue #286).
 *
 * A section is a heading of level one to three and everything until the next
 * heading of the same or higher level. That is the unit the design settled on
 * (design spec, Rec 08-24 answer to Q4): no full editor, no whole-page
 * textarea, one heading's worth of prose at a time.
 *
 * # Everything here is pure, and that is the point
 *
 * Boundaries in, boundaries out; no IPC, no Svelte, no DOM. The refusal rule
 * is the half that decides whether a reader is offered an edit at all, so it
 * is testable on its own rather than through a component that has to be
 * mounted to be asked a question.
 *
 * # Why this scans rather than walking {@link parseStorageFormat}'s tree
 *
 * It reuses that parser where the parser fits and does not where it does not,
 * and the line is **offsets**.
 *
 * * For the *words* -- a section's heading label and its body as text -- the
 *   tree is exactly right and is used: {@link parseStorageFormat} already
 *   knows which elements are content, which are macros, and how to decode an
 *   entity without manufacturing a tag out of prose. A second answer to those
 *   questions would be a second thing to get wrong.
 * * For the *boundaries* it cannot be: a `StorageNode` carries no index into
 *   the string it came from, and an edit has to re-assemble the **whole** body
 *   with everything outside the edited section byte for byte as it was. A page
 *   whose untouched half was re-serialised from a tree would lose every macro,
 *   every attribute and every layout the tree deliberately drops. So the scan
 *   below finds indices, and it finds them with the parser's **own**
 *   {@link readTag}, so "where does this tag end" has one answer in this app.
 *
 * The scan below therefore repeats that parser's skip of CDATA and comments,
 * and it repeats it because the two answer **differently** on purpose. The
 * parser renders a code macro's CDATA as text, since that is what the reader
 * should see; this scan steps over it, because a `<h2>` inside somebody's code
 * sample is their example of markup and not a heading of this page. One shared
 * scanner would need a flag saying which of the two it was being, which is a
 * worse thing to get wrong than a repeated four-line skip.
 *
 * # What a text edit keeps, and what it does not
 *
 * A section's body goes to the reader as text and comes back as paragraphs and
 * line breaks. Bold, links, lists and code blocks inside the section are
 * **not** rebuilt -- {@link PageSection.flattens} is true when the section
 * holds any of them, so the surface can say so before the reader commits
 * rather than after. This is not a third refusal: the two refusals are the
 * ticket's (a macro, a table), and they are about a section knobas must not
 * touch at all. Flattening is about one it may touch, honestly labelled.
 */

import { parseStorageFormat, readTag, type StorageNode } from "./storage-format";

/** Why a section may not be edited from knobas. */
export type SectionRefusal =
  /**
   * The section holds an `ac:` element -- a macro, an image, a Confluence
   * link, a task list. Editing round-trips the body through text, and a macro
   * has no text form: it would come back as the placeholder's words, which is
   * a silent deletion of whatever the macro rendered.
   */
  | "macro"
  /**
   * The section holds a table -- or *is inside one*. Rows and cells have no
   * line-oriented text form either, and a table flattened into paragraphs is
   * not a table that can be put back.
   *
   * The second half is why {@link TABLE} is a family and not the one tag. A
   * heading in a table cell whose section ends at a heading in the *next* cell
   * has no `<table>` in its own slice -- only a `</td><td>` crossing -- and
   * that crossing is inside the range {@link replaceSectionBody} replaces, so
   * an edit offered there would delete the cell boundary and merge two cells
   * into one. A closing tag whose opening is outside the section is the signal
   * that the section is not a whole element's worth of anything.
   */
  | "table";

/** One section of a page body. */
export interface PageSection {
  /** Position in {@link pageSections}' answer, so a caller can key on it. */
  index: number;
  /** 1, 2 or 3 -- the heading's own level. */
  level: number;
  /** The heading's words, entity-decoded. */
  heading: string;
  /** The section's body as text: what the reader edits. */
  text: string;
  /** `null` when the section may be edited; otherwise why it may not. */
  refusal: SectionRefusal | null;
  /**
   * True when the body holds block structure a text edit will not rebuild --
   * a list, a quote, a code block, a nested heading. The section is still
   * editable; the surface is expected to say what will happen.
   */
  flattens: boolean;
  /** Index of the `<` that opens the heading. */
  start: number;
  /** Index just past the heading's close tag: where the body begins. */
  bodyStart: number;
  /** Index just past the section -- the next heading's `<`, or the end. */
  end: number;
}

/** Heading levels that open a section. Four to six are content, not structure. */
const SECTION_LEVELS = /^h([123])$/;

/**
 * Which rendered elements end a line of text.
 *
 * The one list, and {@link FLATTENED} is derived from it: "what ends a line"
 * and "what a text edit cannot rebuild" are two questions with overlapping
 * answers, and two hand-written sets would drift the day somebody adds a tag
 * to one of them.
 */
const BLOCK: ReadonlySet<string> = new Set([
  "p",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "li",
  "br",
  "hr",
  "blockquote",
  "pre",
  "tr",
  "table",
]);

/**
 * Every tag that only ever appears inside a table.
 *
 * The whole family and not `table` alone, because a section's slice is what
 * the refusal reads and a slice can hold a cell without holding the table it
 * belongs to -- see {@link SectionRefusal}'s `"table"`. A `</td>` in a section
 * is a table the section is *in*, which is the same answer as a table the
 * section holds: not an edit knobas makes.
 */
const TABLE: ReadonlySet<string> = new Set([
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "td",
  "th",
  "caption",
  "colgroup",
  "col",
]);

/** What {@link toStorage} can put back: a paragraph and a line break. */
const REBUILDABLE: ReadonlySet<string> = new Set(["p", "br"]);

/**
 * Block structure a text edit turns into paragraphs rather than rebuilding.
 *
 * Every block {@link toStorage} cannot write, plus `code` -- which is inline
 * rather than a block, and is still formatting an edit would flatten.
 */
const FLATTENED: ReadonlySet<string> = new Set(
  [...BLOCK, "code"].filter((tag) => !REBUILDABLE.has(tag)),
);

/**
 * The sections of a storage-format body, in document order.
 *
 * Total, like every other reader of this markup: a body that would not parse
 * is a page a reader still has to be able to open, so an unclosed heading runs
 * to the end of the document and a bare `<` in prose is prose.
 *
 * Text before the first heading belongs to no section and is therefore not
 * editable from here. That is deliberate rather than an omission: a preamble
 * has no heading to name it, and an *Edit* button with nothing to call itself
 * is a button a reader cannot aim.
 */
export function pageSections(storage: string): PageSection[] {
  const opens = headingOpens(storage);
  return opens.map((heading, index) => {
    const next = opens.find((other, at) => at > index && other.level <= heading.level);
    const end = next?.start ?? storage.length;
    const body = storage.slice(heading.bodyStart, end);
    const whole = storage.slice(heading.start, end);
    return {
      index,
      level: heading.level,
      heading: textOf(parseStorageFormat(storage.slice(heading.innerStart, heading.innerEnd))),
      text: textOf(parseStorageFormat(body)),
      refusal: refusalIn(whole),
      flattens: flattensIn(body),
      start: heading.start,
      bodyStart: heading.bodyStart,
      end,
    };
  });
}

/**
 * The whole body, with one section's prose replaced by `text`.
 *
 * **The whole body**, and that is the load-bearing word: `WriteOp::UpdatePage`
 * carries what Confluence's content `PUT` will store, and that request replaces
 * the record. A caller that sent only the edited section would delete the rest
 * of the page. Everything outside `[section.bodyStart, section.end)` is copied
 * verbatim, so the macros, tables and layouts this module refuses to touch
 * survive an edit to their neighbour byte for byte.
 *
 * The heading itself is outside the replaced range too: a section edit changes
 * a section's prose, never its name.
 */
export function replaceSectionBody(
  storage: string,
  section: PageSection,
  text: string,
): string {
  return storage.slice(0, section.bodyStart) + toStorage(text) + storage.slice(section.end);
}

/**
 * Plain text in, storage format out -- the frontend half of the pair whose
 * other half is `knobas_source_confluence::storage::from_text`.
 *
 * Two implementations rather than one because they serve two ops with two
 * dialects on the wire: a `Comment` carries what the reader typed and the
 * adapter renders it, while an `UpdatePage` carries **storage format already**,
 * since most of what it carries is a page's untouched markup that no adapter
 * may re-render. The rules are the same, deliberately, so a comment and a
 * section edit made of the same keystrokes read the same way in the wiki:
 *
 * * `&`, `<` and `>` escaped, `&` **first** -- escaping `<` first would then
 *   escape the `&` of the `&lt;` it just wrote.
 * * A blank line starts a new `<p>`; a single newline is a `<br/>`.
 * * Text that is only whitespace produces the empty string, so an emptied
 *   section is an empty section rather than a page full of empty paragraphs.
 */
export function toStorage(text: string): string {
  return text
    .replace(/\r\n/g, "\n")
    .split("\n\n")
    .map((paragraph) => withoutBlankEnds(paragraph.split("\n").map((line) => line.trimEnd())))
    .filter((lines) => lines.length > 0)
    .map((lines) => `<p>${lines.map(escape).join("<br/>")}</p>`)
    .join("");
}

/**
 * `lines` with its leading and trailing blank runs removed.
 *
 * A run of three newlines is one paragraph break, not one break and an empty
 * paragraph, and a body that begins or ends with them begins and ends with
 * words.
 */
function withoutBlankEnds(lines: string[]): string[] {
  let first = 0;
  let last = lines.length;
  while (first < last && lines[first]!.trim() === "") first += 1;
  while (last > first && lines[last - 1]!.trim() === "") last -= 1;
  return lines.slice(first, last);
}

/** The three characters that are markup in character data. */
function escape(raw: string): string {
  return raw.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

interface HeadingOpen {
  level: number;
  /** The `<` of the open tag. */
  start: number;
  /** Just past the open tag's `>`. */
  innerStart: number;
  /** The `<` of the close tag, or the end of the document. */
  innerEnd: number;
  /** Just past the close tag, or the end of the document. */
  bodyStart: number;
}

/**
 * Every `h1`–`h3` open tag, with the extent of its own heading text.
 *
 * A heading that is never closed runs to the end of the document, and its body
 * is then empty -- the safe direction, since the alternative is an edit that
 * writes prose in place of a heading nobody closed.
 */
function headingOpens(storage: string): HeadingOpen[] {
  const found: HeadingOpen[] = [];
  let open: HeadingOpen | null = null;
  let openName = "";
  for (const tag of tags(storage)) {
    const level = SECTION_LEVELS.exec(tag.name)?.[1];
    if (tag.closing) {
      if (open !== null && tag.name === openName) {
        open.innerEnd = tag.at;
        open.bodyStart = tag.after;
        found.push(open);
        open = null;
      }
      continue;
    }
    if (level === undefined || tag.selfClosing) continue;
    if (open !== null) {
      // A heading opened inside an unclosed heading: close the first one where
      // the second starts rather than losing it.
      open.innerEnd = tag.at;
      open.bodyStart = tag.at;
      found.push(open);
    }
    openName = tag.name;
    open = {
      level: Number(level),
      start: tag.at,
      innerStart: tag.after,
      innerEnd: storage.length,
      bodyStart: storage.length,
    };
  }
  if (open !== null) found.push(open);
  return found;
}

/** Why `slice` may not be edited, or `null`. */
function refusalIn(slice: string): SectionRefusal | null {
  for (const tag of tags(slice)) {
    // A macro outranks a table, so a section holding both says the thing the
    // reader can act on least: `Open in browser` either way, but the sentence
    // names what is actually there first.
    if (tag.name.startsWith("ac:")) return "macro";
    if (TABLE.has(tag.name)) return "table";
  }
  return null;
}

/** Whether `slice` holds block structure a text edit will not rebuild. */
function flattensIn(slice: string): boolean {
  for (const tag of tags(slice)) if (FLATTENED.has(tag.name)) return true;
  return false;
}

/**
 * Every tag in `source`, with its own start index, skipping what is not markup.
 *
 * CDATA and comments are stepped over whole: a `<h2>` inside a code macro's
 * CDATA is somebody's example, not a heading, and a commented-out table is not
 * a table. A `<` that names nothing (`3 < 4` in prose) is one character of
 * text, and stepping past the `>` a naive scan found would step past the
 * *next* tag with it.
 */
function* tags(source: string): Generator<{ name: string; closing: boolean; selfClosing: boolean; at: number; after: number }> {
  let at = 0;
  while (at < source.length) {
    const open = source.indexOf("<", at);
    if (open === -1) return;
    if (source.startsWith("<![CDATA[", open)) {
      const end = source.indexOf("]]>", open);
      at = end === -1 ? source.length : end + 3;
      continue;
    }
    if (source.startsWith("<!--", open)) {
      const end = source.indexOf("-->", open);
      at = end === -1 ? source.length : end + 3;
      continue;
    }
    const tag = readTag(source, open);
    if (!tag) return;
    if (tag.name === "") {
      at = open + 1;
      continue;
    }
    at = tag.after;
    yield {
      name: tag.name,
      closing: tag.closing,
      selfClosing: tag.selfClosing,
      at: open,
      after: tag.after,
    };
  }
}

/**
 * A parsed body as the text a reader edits.
 *
 * Blocks are separated by a blank line, so the text round-trips through
 * {@link toStorage} back into the same paragraphs. A macro placeholder
 * contributes **nothing**: a section holding one is refused anyway, and a
 * heading label that happened to carry one should read as its words rather
 * than as the word "macro".
 */
function textOf(nodes: StorageNode[]): string {
  const parts: string[] = [];
  let line = "";
  const flush = () => {
    const trimmed = line.trim();
    if (trimmed !== "") parts.push(trimmed);
    line = "";
  };
  const walk = (list: StorageNode[]) => {
    for (const node of list) {
      if (node.kind === "text") {
        line += node.text;
        continue;
      }
      if (node.kind === "macro") continue;
      if (BLOCK.has(node.tag)) {
        flush();
        walk(node.children);
        flush();
        continue;
      }
      walk(node.children);
    }
  };
  walk(nodes);
  flush();
  return parts.join("\n\n");
}
