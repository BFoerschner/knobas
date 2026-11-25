/**
 * Rendered diffs (#2279, spec #2234 step 6e; ux26's rendered comparisons, `docs/decisions/2026-11-20-a-comparison-is-
 * drawn-rendered-and-may-end-at-the-working-tree.md`): a file one of whose renderers draws both sides
 * (`Renderer.diff`, not Source, whose diff is the line diff) offers *Lines | Rendered* in its header. *Rendered* is the
 * file drawn by that renderer twice, before and after side by side, in the stage (`TfDiffRendered`); an image also as a
 * swipe or an onion skin, chosen in the stage's caption.
 *
 * What it draws comes from the caller's reader (`DiffContext`): each side's bytes at its end, read at the clone first
 * (`entity_file_bytes`), a text renderer decoding them. An added or deleted text file's one side is its hunk, so it
 * draws where no reader is offered, unless the ceiling cut it. A modified file without a byte reader offers no rendered
 * mode: its hunks hold only the lines near the change.
 */
import type { ArchiveIndex, DiffFile } from "../../ipc/entity";
import type { Opened } from "../../render/defaults";
import type { Renderer } from "../../render/registry";
import { ns } from "../../render/xml-names";

export type Side = "old" | "new";

export interface RenderedPlan {
  renderer: Renderer;
  /** The sides drawn, before first: one for an added or deleted file. */
  sides: Side[];
}

/**
 * What the caller can read: each side's text whole (expanded context), each side's bytes (every rendered side), each
 * side's archive listed (a JAR's comparison, `DiffContext.archive`).
 */
export interface Readable {
  whole: boolean;
  bytes: boolean;
  archive?: boolean;
}

/** How a file's card shows it: its lines (or a binary file's *Bytes* note), or rendered in the stage. */
export type DiffMode = "lines" | "rendered";
/** How an image changed on both sides is drawn in the stage: side by side, swiped across, or onion-skinned. */
export type ImageMode = "sides" | "swipe" | "onion";

/**
 * One side as read: its text or bytes (a JAR's: its listing), no file on that side (an added or deleted binary), or why
 * it could not be read.
 */
export type SideGot = { kind: "drawn"; content: string | Uint8Array | ArchiveIndex } | { kind: "absent" } | { kind: "failed"; why: string };

/** Whether a side's content is an archive's listing (a JAR's side), not its text or bytes. */
export function isListing(content: string | Uint8Array | ArchiveIndex): content is ArchiveIndex {
  // `ArrayBuffer.isView`, not `instanceof`: bytes made in another realm (a worker, a test's encoder) are no `Uint8Array` here.
  return typeof content === "object" && !ArrayBuffer.isView(content);
}

const COMMAND = "command:";

/** The sides a file has: one for an added or deleted file, both otherwise. */
export function sidesOfChange(file: DiffFile): Side[] {
  return file.change === "added" ? ["new"] : file.change === "deleted" ? ["old"] : ["old", "new"];
}

/** Whether `renderer` can draw `file` on both sides, given what the caller can read. */
function drawsBoth(file: DiffFile, renderer: Renderer, can: Readable): boolean {
  if (!renderer.diff || renderer.output === "lines") return false;
  // A JAR is compared by its two listings, read by the backend (`archive_open`), never through the byte read.
  if (renderer.id === "jar") return can.archive === true;
  if (file.change === "binary") {
    // A command renderer is handed the file as text (`TfCommandOutput`): a binary file's bytes are not that.
    return can.bytes && renderer.input === "bytes" && !renderer.id.startsWith(COMMAND);
  }
  return can.bytes || sidesOfChange(file).every((side) => hunkText(file, side) !== null);
}

/**
 * The renderers offered for `file` that can draw both sides, the one it opens with first, then the registry's order.
 * So an `.html` file, which opens as Source, offers *Page*.
 */
export function renderedChoices(file: DiffFile, opened: Opened, can: Readable): Renderer[] {
  const first = opened.renderer;
  const rest = opened.offered.filter((r) => r !== first);
  return (first === null ? rest : [first, ...rest]).filter((r) => drawsBoth(file, r, can));
}

/** The rendered mode a diff's file offers with `renderer`, or `null` where it cannot draw both sides. */
export function renderedPlan(file: DiffFile, renderer: Renderer | null, can: Readable): RenderedPlan | null {
  if (renderer === null || !drawsBoth(file, renderer, can)) return null;
  return { renderer, sides: sidesOfChange(file) };
}

/** A name a spec goes by: `openapi`, `swagger` or `.oas.` in it, ending `.json`, `.yaml` or `.yml`. */
const SPEC_NAME = /(openapi|swagger)[^/]*\.(json|ya?ml)$|\.oas\.(json|ya?ml)$/i;
const SPEC_EXT = /\.(json|ya?ml)$/i;
/** A version key at a line's start, as a YAML spec's top level writes it. */
const SPEC_KEY = /^(["']?)(openapi|swagger)\1[ \t]*:/;

/**
 * Text a diff's file holds that tells whether it is an OpenAPI or Swagger spec, with no read (2026-11-22, Fable's ruling
 * 9): an added or deleted file whole, the first hunk where it starts at line 1, a hunk's line that is a YAML spec's
 * version key, or a name a spec goes by. `undefined` where none tells: such a file offers no OpenAPI (a modified spec
 * with a plain name, changed away from its first lines, keeps *Lines*).
 */
export function specHint(file: DiffFile): string | undefined {
  // A schema is planned from its name (S7): a `.xsd` needs no hint from its hunks; one that is no schema says so in the stage.
  if (/\.xsd$/i.test(file.path) && file.change !== "binary") return XSD_HINT;
  if (!SPEC_EXT.test(file.path) || file.change === "binary") return undefined;
  if (SPEC_NAME.test(file.path)) return "openapi: 3.0.0";
  for (const side of ["new", "old"] as const) {
    const whole = hunkText(file, side);
    if (whole !== null) return whole;
  }
  const first = file.hunks[0];
  if (first !== undefined && first.lines[0]?.new === 1)
    return first.lines
      .filter((l) => l.kind !== "del")
      .map((l) => l.text)
      .join("\n");
  for (const hunk of file.hunks) for (const line of hunk.lines) if (SPEC_KEY.test(line.text)) return line.text;
  return undefined;
}

/**
 * What an XML or JSON file's plan is sniffed by where no text tells (C1): its name alone. A JSON file read at the
 * checkout is sniffed from its side first (an API spec, `TfDiffView`'s `sniffable`), so its caller asks only otherwise.
 */
export function docHint(file: DiffFile): string | undefined {
  if (file.change === "binary") return undefined;
  if (/\.(xml|xsl|xslt|pom|csproj|wsdl|svg|plist|xaml|resx|config)$/i.test(file.path)) return '<?xml version="1.0"?><a/>';
  if (/\.(json|geojson|webmanifest|har|ipynb|jsonl|ndjson)$/i.test(file.path)) return "{}";
  return undefined;
}

/** What a `.xsd`'s plan is sniffed by: an XML Schema's first tag. */
const XSD_HINT = `<xs:schema xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"/>`;

/** An added file's new side, or a deleted file's old one, from its hunks: `null` where they hold less than the file. */
export function hunkText(file: DiffFile, side: Side): string | null {
  const whole = side === "new" ? file.change === "added" : file.change === "deleted";
  if (!whole || file.truncated) return null;
  return file.hunks.flatMap((hunk) => hunk.lines.map((line) => line.text)).join("\n");
}

/** A document stage's height, from the window's: between 360 and 640 px, 56 % of the window between (`.tf-rstage`). */
export function docStage(innerHeight: number): number {
  return Math.min(640, Math.max(360, 0.56 * innerHeight));
}
/** An image stage's height (`.tf-rstage-img`). */
export const IMAGE_STAGE = 320;
/** The caption row's height and the foot's, around the stage (`.tf-rcap`, `.tf-rfoot`). */
export const CAPTION = 29;
export const FOOT = 31;
/** The stage's height for a renderer: known before any side is read, so the card's room is right in its first frame. */
export function stageOf(renderer: Renderer, innerHeight: number): number {
  return renderer.id === "image" ? IMAGE_STAGE : docStage(innerHeight);
}
/** A side's end in a caption: a sha's first seven digits, anything else as written. */
export function endWords(end: string): string {
  return /^[0-9a-f]{40}([0-9a-f]{24})?$/i.test(end) ? end.slice(0, 7) : end;
}

/** A file card's header (`.tf-fh`), as the windowed list reckons it. */
export const CARD_HEAD = 42;
/**
 * The room of a card open rendered: its header, the caption, the stage and the foot, all known before a side is read,
 * so the windowed list places every card right in the first frame (`TfDiffView`'s `fileRoom`).
 */
export function renderedRoom(renderer: Renderer, innerHeight: number): number {
  return CARD_HEAD + CAPTION + stageOf(renderer, innerHeight) + FOOT;
}

/** Runs of line numbers, from 1, both ends counted, in order. */
export type LineRanges = [number, number][];

/**
 * The lines a change touched on each side (ux26, a rendered Markdown file's changed blocks, ruling 3): the removed lines
 * by their old numbers, the added ones by their new numbers, each run of them one range. The diff's own answer: no
 * second diff is worked out.
 */
export function changedLines(file: DiffFile): { old: LineRanges; new: LineRanges } {
  const out: { old: LineRanges; new: LineRanges } = { old: [], new: [] };
  const add = (into: LineRanges, n: number) => {
    const last = into.at(-1);
    if (last !== undefined && n <= last[1] + 1) last[1] = Math.max(last[1], n);
    else into.push([n, n]);
  };
  for (const hunk of file.hunks)
    for (const line of hunk.lines) {
      if (line.kind === "del" && line.old !== null) add(out.old, line.old);
      else if (line.kind === "add" && line.new !== null) add(out.new, line.new);
    }
  return out;
}

/** Whether the lines `from` to `to` (both counted) meet any of `ranges`. */
export function meetsLines(from: number, to: number, ranges: LineRanges): boolean {
  return ranges.some(([a, b]) => a <= to && b >= from);
}
