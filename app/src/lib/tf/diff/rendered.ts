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
import type { DiffFile } from "../../ipc/entity";
import type { Opened } from "../../render/defaults";
import type { Renderer } from "../../render/registry";

export type Side = "old" | "new";

export interface RenderedPlan {
  renderer: Renderer;
  /** The sides drawn, before first: one for an added or deleted file. */
  sides: Side[];
}

/** What the caller can read: each side's text whole (expanded context), each side's bytes (every rendered side). */
export interface Readable {
  whole: boolean;
  bytes: boolean;
}

/** How a file's card shows it: its lines (or a binary file's *Bytes* note), or rendered in the stage. */
export type DiffMode = "lines" | "rendered";
/** How an image changed on both sides is drawn in the stage: side by side, swiped across, or onion-skinned. */
export type ImageMode = "sides" | "swipe" | "onion";

/** One side as read: its text or bytes, no file on that side (an added or deleted binary), or why it could not be read. */
export type SideGot = { kind: "drawn"; content: string | Uint8Array } | { kind: "absent" } | { kind: "failed"; why: string };

const COMMAND = "command:";

/** The sides a file has: one for an added or deleted file, both otherwise. */
export function sidesOfChange(file: DiffFile): Side[] {
  return file.change === "added" ? ["new"] : file.change === "deleted" ? ["old"] : ["old", "new"];
}

/** Whether `renderer` can draw `file` on both sides, given what the caller can read. */
function drawsBoth(file: DiffFile, renderer: Renderer, can: Readable): boolean {
  if (!renderer.diff || renderer.output === "lines") return false;
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
