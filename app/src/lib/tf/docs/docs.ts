/**
 * The Docs reader's rules (#859, requirements §10), pure: which page an
 * address opens, what the tree holds, the header's words and the anchors.
 *
 * Prototype-6 draws all three Docs addresses with one screen: `#/docs` opens
 * the page read last, `#/docs/space/<key>` that space's first page, and
 * `#/page/<id>[/<section>]` the page itself. A key two sources hold is
 * addressed `#/docs/space/<source>:<key>` (#1455), as a ticket key two sources
 * hold is `#/job/<source>:<key>` (#1476).
 */
import type {
  DocBacklink,
  DocBlock,
  DocForYou,
  DocHit,
  DocInline,
  DocNode,
  DocPage,
  DocSnippet,
  DocSpace,
  DocsTree,
  PanelKind,
  Reached,
  SectionRefusal,
} from "../../ipc/entity";
import { originLabel } from "../job/job";
import { isDone } from "../tickets/tickets";

/** Where a Docs address points. */
export type DocsPlace =
  | { kind: "last" }
  | { kind: "space"; source: string | null; key: string }
  | { kind: "page"; id: string; section: number | null };

/**
 * Read the rest of a Docs address. The route keeps what follows the first
 * segment, and both `#/docs/…` and `#/page/…` arrive here: `space` names a
 * space, and anything else is a page's entity id (which always holds a `:`).
 * A space segment holding a `:` names its source before the first one, as an
 * entity id does (a source id holds no `:`).
 */
export function placeOf(rest: string[]): DocsPlace {
  const [head, next] = rest;
  if (head === undefined || head === "") return { kind: "last" };
  if (head === "space") {
    if (!next) return { kind: "last" };
    const colon = next.indexOf(":");
    return colon > 0
      ? { kind: "space", source: next.slice(0, colon), key: next.slice(colon + 1) }
      : { kind: "space", source: null, key: next };
  }
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
 * `#/docs` with nothing read yet opens the first space's first page. A space
 * named with its source is matched through {@link spaceId}; a bare key opens
 * the first space holding it, and so does a segment whose `:` belongs to the
 * key rather than naming a source.
 */
export function pageToOpen(place: DocsPlace, tree: DocsTree): string | null {
  if (place.kind === "page") return place.id;
  if (place.kind === "space") {
    const { source, key } = place;
    const named = source === null ? undefined : spaceId({ source_id: source, key });
    const space =
      tree.spaces.find((s) => spaceId(s) === named) ??
      tree.spaces.find((s) => s.key === (source === null ? key : `${source}:${key}`));
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

/**
 * For you, narrowed to a space: its key and its source, since two sources can
 * share a key (#1556). An item's source is the head of its `<source>:<id>`.
 * No space leaves the list whole.
 */
export function forYouIn(items: DocForYou[], space: SpaceRef | undefined): DocForYou[] {
  if (space === undefined) return items;
  const { source_id: source, key } = space;
  return items.filter((f) => f.space_key === key && f.entity_id.startsWith(`${source}:`));
}

/** A space as its source and key name it. */
type SpaceRef = { source_id: string; key: string };

/** A space's stable identity in the picker: source and key. */
export function spaceId(space: SpaceRef): string {
  return `${space.source_id}/${space.key}`;
}

/**
 * The address of a space: `#/docs/space/<key>`, or `#/docs/space/<source>:<key>`
 * when another of `spaces` holds the key too (#1455, the shape #1476 gave a
 * job), so the address never opens the other source's space.
 */
export function spaceHash(spaces: readonly SpaceRef[], space: SpaceRef): string {
  const shared = spaces.some((s) => s.key === space.key && s.source_id !== space.source_id);
  return `#/docs/space/${encodeURIComponent(shared ? `${space.source_id}:${space.key}` : space.key)}`;
}

/** The picker's words for a space: *ENG Engineering (12)*. */
export function spaceLabel(space: DocSpace): string {
  return `${space.key} ${space.name ?? ""}`.trim() + ` (${space.pages.length})`;
}

/**
 * A space's pages by parent (`null` for the top of the space), in the order
 * the space lists them: built in one pass, so a tree drawn from it costs the
 * page count once rather than once per node (#2200: a 4,000-page space drew
 * 16M comparisons a draw through a filter per node).
 */
export function childrenByParent(space: DocSpace): Map<string | null, DocNode[]> {
  const under = new Map<string | null, DocNode[]>();
  for (const page of space.pages) {
    const list = under.get(page.parent);
    if (list === undefined) under.set(page.parent, [page]);
    else list.push(page);
  }
  return under;
}

/**
 * The folds that hold `entityId`, so they start open: the page itself and
 * every page above it, walked up its parents once (#2200; a walk down every
 * fold's subtree cost the space's page count per node).
 */
export function foldsHolding(space: DocSpace, entityId: string | null): Set<string> {
  const held = new Set<string>();
  if (entityId === null) return held;
  const parentOf = new Map(space.pages.map((p) => [p.entity_id, p.parent]));
  if (!parentOf.has(entityId)) return held;
  // The backend breaks parent loops; the set stops the walk on a bad answer all the same.
  for (let at: string | null | undefined = entityId; at != null && !held.has(at); at = parentOf.get(at)) held.add(at);
  return held;
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

export function blockWords(block: DocBlock): string {
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

/** The place a queued edit of section `number` draws in (`tf/queued.svelte.ts`). */
export function sectionKind(number: number): string {
  return `section ${number}`;
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

/** How a mention reads: `@` and the display name's first word, else the account. */
export function mentionText(mention: Extract<DocInline, { type: "mention" }>): string {
  const first = mention.name?.split(/\s+/)[0];
  return `@${first || mention.user || "someone"}`;
}

/** A display name's first word, the way prototype-6 names a person in a line. */
export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

/** The person a line names: *you* for the reader, else the first name, else the account. */
export function shortWho(user: string | null, name: string | null, me: string[]): string {
  if (user !== null && me.includes(user)) return "you";
  return name ? firstName(name) : (user ?? "someone");
}

/**
 * The relation *Link to <job>* draws from a page to the job's ticket:
 * `knobas_app::docs::LINK_RELATION`, the one *Link it to <job>* draws.
 */
export const LINK_RELATION = "documents";

/**
 * Who created the page and when, for the meta line's *Created by* (#2080):
 * the version numbered 1 in its history, else the page's own editor while the
 * page is at version 1; `null` when neither says.
 */
export function createdBy(page: DocPage): { who: string; at: string | null } | null {
  const first = page.history.find((v) => v.number === 1);
  if (first) return { who: shortWho(first.by, first.by_name, page.me), at: first.at };
  if (page.version === 1) return { who: shortWho(page.editor, page.editor_name, page.me), at: page.updated_at };
  return null;
}

/**
 * How many backlinks the backend loads at most (the `limit 50` of
 * `knobas_app::docs`'s `BACKLINKS` query): at this many there may be more, so
 * *Linked from* says `50+`.
 */
const BACKLINK_LIMIT = 50;

/** *Linked from*'s heading: the number of backlinks, `50+` at the backend's limit (#2080). */
export function linkedFromHeading(count: number): string {
  if (count >= BACKLINK_LIMIT) return `Linked from ${BACKLINK_LIMIT}+ pages`;
  return `Linked from ${count} ${count === 1 ? "page" : "pages"}`;
}

/** One fold of *Linked from*: a space's backlinks, or the ones with no space key (`key` `null`). */
export interface BacklinkGroup {
  key: string | null;
  /** The space's name from the tree, `null` where the tree names none. */
  name: string | null;
  links: DocBacklink[];
}

/**
 * *Linked from* by space (#2080): one group per space key, ordered by key,
 * its pages by title; the backlinks with no key last. A space is named from
 * the tree's spaces of the page's own source, since two sources can share a key.
 */
export function backlinkGroups(backlinks: DocBacklink[], spaces: readonly DocSpace[], sourceId: string): BacklinkGroup[] {
  const byKey = new Map<string | null, DocBacklink[]>();
  for (const link of backlinks) byKey.set(link.space_key, [...(byKey.get(link.space_key) ?? []), link]);
  const keys = [...byKey.keys()].filter((k): k is string => k !== null).sort((a, b) => a.localeCompare(b));
  const group = (key: string | null): BacklinkGroup => ({
    key,
    name: key === null ? null : (spaces.find((s) => s.source_id === sourceId && s.key === key)?.name ?? null),
    links: [...byKey.get(key)!].sort((a, b) => a.title.localeCompare(b.title)),
  });
  return [...keys.map(group), ...(byKey.has(null) ? [group(null)] : [])];
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

// -- the search (#860) and the find bar --------------------------------------

/**
 * A needle as the reader matches it, the backend's rule (`knobas_app::docs`,
 * prototype-6's): lowercased, and a word longer than four letters without one
 * `ing`, `es`, `ed` or `s` at its end, so "reviews" finds "review".
 */
export function stem(needle: string): string {
  const lower = needle.toLowerCase();
  if (lower.length <= 4) return lower;
  for (const suffix of ["ing", "es", "ed", "s"]) {
    if (lower.endsWith(suffix)) return lower.slice(0, -suffix.length);
  }
  return lower;
}

/** The stems of the find bar's text: each word, empty ones dropped. */
export function stemsOf(find: string): string[] {
  return [...new Set(find.split(/\s+/).filter((w) => w.length > 1).map(stem))];
}

const WORD = /[\p{L}\p{N}]/u;

/**
 * `text` as runs, each stem's occurrence and the rest of its word marked:
 * what the reader draws as `<mark>`.
 */
export function markText(text: string, stems: string[]): { text: string; hit: boolean }[] {
  if (stems.length === 0 || text === "") return [{ text, hit: false }];
  const lower = text.toLowerCase();
  const hit = new Array<boolean>(text.length).fill(false);
  for (const s of stems) {
    if (s === "") continue;
    let at = lower.indexOf(s);
    while (at >= 0) {
      let end = at + s.length;
      while (end < text.length && WORD.test(text[end]!)) end += 1;
      hit.fill(true, at, end);
      at = lower.indexOf(s, at + s.length);
    }
  }
  const out: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i += 1) {
    const last = out.at(-1);
    if (last && last.hit === hit[i]) last.text += text[i];
    else out.push({ text: text[i]!, hit: hit[i]! });
  }
  return out;
}

/** How often the stems occur in `text`. */
export function countIn(text: string, stems: string[]): number {
  const lower = text.toLowerCase();
  let n = 0;
  for (const s of stems) {
    if (s === "") continue;
    let at = lower.indexOf(s);
    while (at >= 0) {
      n += 1;
      at = lower.indexOf(s, at + s.length);
    }
  }
  return n;
}

/** One section of a page body: section 0 is what comes before the first heading. */
export interface Section {
  number: number;
  heading: Extract<DocBlock, { type: "heading" }> | null;
  blocks: DocBlock[];
}

/** A body's sections, prototype-6's numbering: the first heading opens section 1. */
export function sectionsOf(blocks: DocBlock[]): Section[] {
  const out: Section[] = [{ number: 0, heading: null, blocks: [] }];
  for (const block of blocks) {
    if (block.type === "heading") out.push({ number: out.length, heading: block, blocks: [] });
    else out.at(-1)!.blocks.push(block);
  }
  return out;
}

/**
 * The windows the reader folds its outline above the page in (#2081, R-10.9):
 * narrower than 1240px. `css/docs.css` writes the same query for the same
 * band, and `tf-css.test.ts` holds the two together.
 */
export const READER_NARROW = "(max-width: 1239px)";

/**
 * The windows the reader's tree is a drawer over the page in (#2084, R-10.9):
 * narrower than 900px. `css/docs.css` writes the same query for the same
 * band, and `tf-css.test.ts` holds the two together.
 */
export const READER_DRAWER = "(max-width: 899px)";

/** The outline's count per section: matches in its heading and its blocks. */
export function sectionMatches(blocks: DocBlock[], stems: string[]): Map<number, number> {
  const out = new Map<number, number>();
  if (stems.length === 0) return out;
  for (const section of sectionsOf(blocks)) {
    if (section.heading === null) continue;
    const words = [inlineWords(section.heading.text), ...section.blocks.map(blockWords)].join(" ");
    const n = countIn(words, stems);
    if (n > 0) out.set(section.number, n);
  }
  return out;
}

/** The search field's scope, in the placeholder and the header line: *all spaces* or the space's name. */
export function scopeName(space: DocSpace | undefined): string {
  return space === undefined ? "all spaces" : (space.name ?? space.key);
}

/** The hits' header line: *5 pages in all spaces, 1 ms*. */
export function hitsLine(total: number, scope: string, ms: number): string {
  return `${total} page${total === 1 ? "" : "s"} in ${scope}, ${Math.max(1, ms)} ms`;
}

/** What a snippet opens with: its section, or *Comment by Priya*. */
export function snippetLabel(snippet: DocSnippet): string | null {
  if (snippet.comment_by !== null) {
    const first = firstName(snippet.comment_by);
    return first ? `Comment by ${first}:` : "Comment:";
  }
  return snippet.section === null ? null : `${snippet.section}:`;
}

/** A hit's last line: *3 matches, Priya, today 10:40* (*you* for the reader). */
export function hitLine(hit: DocHit, me: string[], when: (iso: string | null) => string): string {
  const who = shortWho(hit.editor, hit.editor_name, me);
  return `${hit.matches} match${hit.matches === 1 ? "" : "es"}, ${who}, ${when(hit.updated_at)}`;
}

/** Why a page is *For you*: *Priya mentioned you*. */
export function forYouWhy(item: DocForYou): string {
  const first = item.by_name ? firstName(item.by_name) : "";
  return first ? `${first} mentioned you` : "You are mentioned here";
}

/** The disabled *Edit section*'s reason, prototype-6's sentence with the source's name. */
export function refusalReason(refusal: SectionRefusal, source: string): string {
  if (refusal === "formatting") {
    return `This section has formatting or links that a text edit would lose. knobas edits text-only sections; edit this one in ${source}.`;
  }
  return `This section has a table, code or a macro. knobas edits text-only sections; edit this one in ${source}.`;
}

/** What *Copy link* copies: the knobas address, then the page in the source. */
export function copyLinkText(page: Pick<DocPage, "entity_id" | "web_url">): string {
  return [pageHash(page.entity_id), page.web_url].filter((x): x is string => x !== null).join("\n");
}

// ---------------------------------------------------------------------------
// *This job* (#863, R-2.8): the job's pages, grouped by the reason it reaches them.
// ---------------------------------------------------------------------------

/** One page of the job: its title and where it sits, *OPS › Runbooks*. */
export interface JobPage {
  entity_id: string;
  title: string;
  where: string;
}

/** The job's pages under one reason: *Linked to PAY-231*, *Parent PAY-230*. */
export interface JobPageGroup {
  reason: string;
  pages: JobPage[];
}

/**
 * Why the job reaches a page, as its group's heading: one of the parent's own
 * links is *Parent PAY-230* (the job sections' label); a page linked to the
 * ticket is *Linked to PAY-231*; a page further down the walk names its first
 * step, *Via <title>*, when `titles` has the step's title.
 */
export function reachReason(reached: Reached, job: string, titles: ReadonlyMap<string, string>): string {
  const origin = originLabel(reached);
  if (origin !== null) return origin;
  const step = reached.step === reached.entity_id ? undefined : titles.get(reached.step);
  return step === undefined ? `Linked to ${job}` : `Via ${step}`;
}

/** The ids of the reached pages the tree holds. */
export function reachedPages(reach: readonly Reached[], tree: DocsTree): Set<string> {
  const ids = new Set(reach.map((r) => r.entity_id));
  return new Set(tree.spaces.flatMap((s) => s.pages.filter((p) => ids.has(p.entity_id)).map((p) => p.entity_id)));
}

/** A page's place: its space's key, then its ancestors' titles from the top. */
function whereOf(space: DocSpace, page: DocNode): string {
  const byId = new Map(space.pages.map((p) => [p.entity_id, p]));
  const path: string[] = [];
  const seen = new Set<string>([page.entity_id]);
  for (let up = page.parent; up !== null && !seen.has(up); ) {
    seen.add(up);
    const node = byId.get(up);
    if (!node) break;
    path.unshift(node.title);
    up = node.parent;
  }
  return [space.key, ...path].join(" › ");
}

/**
 * The job's pages the tree holds, grouped by reason; the groups in the order
 * their first page comes in the tree, the pages in tree order. A reached id
 * the tree does not hold (not a page, or not synced) is left out.
 */
export function jobPageGroups(
  reach: readonly Reached[],
  tree: DocsTree,
  job: string,
  titles: ReadonlyMap<string, string> = new Map(),
): JobPageGroup[] {
  const why = new Map(reach.map((r) => [r.entity_id, reachReason(r, job, titles)]));
  const groups = new Map<string, JobPage[]>();
  for (const space of tree.spaces) {
    for (const page of space.pages) {
      const reason = why.get(page.entity_id);
      if (reason === undefined) continue;
      const list = groups.get(reason) ?? [];
      list.push({ entity_id: page.entity_id, title: page.title, where: whereOf(space, page) });
      groups.set(reason, list);
    }
  }
  return [...groups].map(([reason, pages]) => ({ reason, pages }));
}
