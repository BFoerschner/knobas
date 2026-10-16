/**
 * The Docs reader's rules (#859, requirements §10), pure: which page an
 * address opens, what the tree holds, the header's words and the anchors.
 *
 * Prototype-6 draws all three Docs addresses with one screen: `#/docs` opens
 * the page read last, `#/docs/space/<key>` that space's first page, and
 * `#/page/<id>[/<section>]` the page itself.
 */
import type { DocBlock, DocInline, DocNode, DocPage, DocSpace, DocsTree, PanelKind } from "../../ipc/entity";
import { isDone } from "../tickets/tickets";

/** Where a Docs address points. */
export type DocsPlace =
  | { kind: "last" }
  | { kind: "space"; key: string }
  | { kind: "page"; id: string; section: number | null };

/**
 * Read the rest of a Docs address. The route keeps what follows the first
 * segment, and both `#/docs/…` and `#/page/…` arrive here: `space` names a
 * space, and anything else is a page's entity id (which always holds a `:`).
 */
export function placeOf(rest: string[]): DocsPlace {
  const [head, next] = rest;
  if (head === undefined || head === "") return { kind: "last" };
  if (head === "space") return next ? { kind: "space", key: next } : { kind: "last" };
  const section = next !== undefined && /^\d+$/.test(next) ? Number(next) : null;
  return { kind: "page", id: head, section };
}

/** The address of a page, or of one of its sections. */
export function pageHash(entityId: string, section?: number): string {
  const base = `#/page/${encodeURIComponent(entityId)}`;
  return section === undefined ? base : `${base}/${section}`;
}

/** A space's first page: the first one at the top of its tree. */
export function firstPage(space: DocSpace): DocNode | undefined {
  return space.pages.find((p) => p.parent === null) ?? space.pages[0];
}

/**
 * The page an address opens, or `null` when the tree has nothing to open.
 * `#/docs` with nothing read yet opens the first space's first page.
 */
export function pageToOpen(place: DocsPlace, tree: DocsTree): string | null {
  if (place.kind === "page") return place.id;
  if (place.kind === "space") {
    const space = tree.spaces.find((s) => s.key === place.key);
    return space ? (firstPage(space)?.entity_id ?? null) : null;
  }
  if (tree.last_read !== null) return tree.last_read;
  const first = tree.spaces[0];
  return first ? (firstPage(first)?.entity_id ?? null) : null;
}

/** The space holding a page, if the tree has it. */
export function spaceOfPage(tree: DocsTree, entityId: string): DocSpace | undefined {
  return tree.spaces.find((s) => s.pages.some((p) => p.entity_id === entityId));
}

/** A space's stable identity in the picker: source and key. */
export function spaceId(space: { source_id: string; key: string }): string {
  return `${space.source_id}/${space.key}`;
}

/** The picker's words for a space: *ENG Engineering (12)*. */
export function spaceLabel(space: DocSpace): string {
  return `${space.key} ${space.name ?? ""}`.trim() + ` (${space.pages.length})`;
}

/** The pages directly under `parent` (`null` for the top of the space). */
export function childrenOf(space: DocSpace, parent: string | null): DocNode[] {
  return space.pages.filter((p) => p.parent === parent);
}

/** Whether `entityId` is `node` or below it, so the fold holding it opens. */
export function holds(space: DocSpace, node: DocNode, entityId: string, seen = new Set<string>()): boolean {
  if (node.entity_id === entityId) return true;
  // The backend breaks parent loops; a set here keeps a bad answer from recursing forever.
  if (seen.has(node.entity_id)) return false;
  seen.add(node.entity_id);
  return childrenOf(space, node.entity_id).some((child) => holds(space, child, entityId, seen));
}

/** The words of a run of inlines. */
export function inlineWords(text: DocInline[]): string {
  return text
    .map((i) => {
      switch (i.type) {
        case "text":
        case "link":
        case "page_link":
          return i.text;
        case "issue":
          return i.key;
        case "mention":
          return mentionText(i);
        case "break":
          return " ";
      }
    })
    .join("");
}

function blockWords(block: DocBlock): string {
  switch (block.type) {
    case "heading":
    case "paragraph":
      return inlineWords(block.text);
    case "list":
      return block.items.flat().map(blockWords).join(" ");
    case "code":
      return block.code;
    case "table":
      return block.rows
        .flat()
        .map((c) => inlineWords(c.text))
        .join(" ");
    case "panel":
      return block.blocks.map(blockWords).join(" ");
    case "placeholder":
      return "";
  }
}

/** *N min read*: 220 words a minute, never less than one (prototype-6). */
export function readingMinutes(page: DocPage): number {
  const words = page.blocks
    .map(blockWords)
    .join(" ")
    .split(/\s+/)
    .filter(Boolean).length;
  return Math.max(1, Math.round(words / 220));
}

/**
 * The section number of each heading, prototype-6's numbering: section 0 is
 * whatever comes before the first heading, so the first heading opens
 * section 1. Keyed by the heading's index in the page's top-level blocks.
 */
export function sectionNumbers(blocks: DocBlock[]): Map<number, number> {
  const out = new Map<number, number>();
  let n = 0;
  blocks.forEach((block, index) => {
    if (block.type === "heading") out.set(index, ++n);
  });
  return out;
}

/** How a mention reads: `@` and the display name's first word, else the account. */
export function mentionText(mention: Extract<DocInline, { type: "mention" }>): string {
  const first = mention.name?.split(/\s+/)[0];
  return `@${first || mention.user || "someone"}`;
}

/** *You* for the reader, else the person as the source names them. */
export function person(user: string | null, name: string | null, me: string[]): string {
  if (user !== null && me.includes(user)) return "You";
  return name ?? user ?? "Someone";
}

/**
 * A ticket chip's status tone: ok for the *Done* column's status, by the one
 * rule the Tickets view already has (`isDone`), else plain. No other status
 * word is read: the source names no tone for a status.
 */
export function statusTone(status: string | null): "ok" | null {
  return isDone(status) ? "ok" : null;
}

/** The shell's tone each panel kind is drawn in (CONTEXT.md **Tone**). */
const PANEL_TONE: Record<PanelKind, "info" | "warn" | "muted" | "ok"> = {
  info: "info",
  warning: "warn",
  note: "muted",
  tip: "ok",
};

/** The shell's tone a panel kind is drawn in. */
export function panelTone(kind: PanelKind): "info" | "warn" | "muted" | "ok" {
  return PANEL_TONE[kind];
}

/** A panel's label: its title, else its tone in the reader's words. */
export function panelLabel(block: Extract<DocBlock, { type: "panel" }>): string {
  if (block.title) return block.title;
  return { info: "Info", note: "Note", warning: "Warning", tip: "Tip" }[block.kind];
}

/** Whether a table's first row is all header cells, so it draws as `thead`. */
export function headed(rows: { header: boolean }[][]): boolean {
  const first = rows[0];
  return first !== undefined && first.length > 0 && first.every((c) => c.header);
}
