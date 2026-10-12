/**
 * A block's tone as the app's own colour class (`app.css`: `.ok`, `.amber`,
 * `.fail`, `.link`, `.muted`). ADR-0020: the adapter names a tone and the
 * shell maps it to its theme's token -- here, once, for every block.
 */
import type { Tone } from "../../ipc/entity";

const CLASS: Record<Tone, string> = {
  ok: "ok",
  warn: "amber",
  err: "fail",
  info: "link",
  muted: "muted",
};

export function toneClass(tone: Tone | null): string {
  return tone ? CLASS[tone] : "";
}
