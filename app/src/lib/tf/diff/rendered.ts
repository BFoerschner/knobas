/**
 * Rendered diffs (#2279, spec #2234 step 6e): a file whose renderer draws
 * diffs (`Renderer.diff`, not Source, whose diff is the line diff) offers a
 * *Rendered* mode beside its lines. It is the file drawn by that renderer
 * twice, before and after side by side; an image also as a swipe.
 *
 * What it draws comes from the caller's reader (`DiffContext`): a text file
 * read whole at each side's commit, an image's bytes. An added or deleted
 * text file's one side is its hunk, so it draws where no reader is offered,
 * unless the ceiling cut it. A modified file without a reader offers no
 * rendered mode: its hunks hold only the lines near the change.
 */
import type { DiffFile } from "../../ipc/entity";
import type { Renderer } from "../../render/registry";

export type Side = "old" | "new";

export interface RenderedPlan {
  renderer: Renderer;
  /** The sides drawn, before first: one for an added or deleted file. */
  sides: Side[];
  /** An image changed on both sides can also be swiped across. */
  swipe: boolean;
}

/** What the caller can read: each side's text whole, each side's bytes. */
export interface Readable {
  whole: boolean;
  bytes: boolean;
}

/** How a file's card shows it: its lines (or a binary file's *Bytes* note), rendered side by side, or swiped. */
export type DiffMode = "lines" | "rendered" | "swipe";

/** One side as read: its text or bytes, no file on that side (an added or deleted binary), or why it could not be read. */
export type SideGot = { kind: "drawn"; content: string | Uint8Array } | { kind: "absent" } | { kind: "failed"; why: string };

const COMMAND = "command:";

/** The rendered mode a diff's file offers with the renderer it opens with, or `null` for none. */
export function renderedPlan(file: DiffFile, renderer: Renderer | null, can: Readable): RenderedPlan | null {
  if (renderer === null || !renderer.diff || renderer.output === "lines") return null;
  const binary = file.change === "binary";
  const sides: Side[] = file.change === "added" ? ["new"] : file.change === "deleted" ? ["old"] : ["old", "new"];
  if (binary) {
    // A command renderer is handed the file as text (`TfCommandOutput`): a binary file's bytes are not that.
    if (!can.bytes || renderer.input !== "bytes" || renderer.id.startsWith(COMMAND)) return null;
  } else if (!can.whole && !sides.every((side) => hunkText(file, side) !== null)) {
    return null;
  }
  return { renderer, sides, swipe: renderer.id === "image" && sides.length === 2 };
}

/** An added file's new side, or a deleted file's old one, from its hunks: `null` where they hold less than the file. */
export function hunkText(file: DiffFile, side: Side): string | null {
  const whole = side === "new" ? file.change === "added" : file.change === "deleted";
  if (!whole || file.truncated) return null;
  return file.hunks.flatMap((hunk) => hunk.lines.map((line) => line.text)).join("\n");
}
