/**
 * The ticket page's inline images under `?fake-ipc` (#2406): fills the page's
 * attachment image read (`tf/job/attachment-image.ts`) with a drawn
 * screenshot per image file, or a read that always fails, as an unreachable
 * Jira does (`?fake-attachments=unreachable`, seeded by `fake/entity.ts`).
 * #2405's bytes command takes the seam over with its own fake handler.
 */
import type { Attachment } from "../../ipc/entity";
import { fillAttachmentImages } from "../../tf/job/attachment-image";

const WIDTH = 480;
const HEIGHT = 180;

/** A screenshot-like picture of a log panel, as a 24-bit BMP `data:` URL: a dark title bar, grey lines, one red. */
function picture(seed: number): string {
  const row = Math.ceil((WIDTH * 3) / 4) * 4;
  const bytes = new Uint8Array(54 + row * HEIGHT);
  const view = new DataView(bytes.buffer);
  bytes.set([0x42, 0x4d]);
  view.setUint32(2, bytes.length, true);
  view.setUint32(10, 54, true);
  view.setUint32(14, 40, true);
  view.setInt32(18, WIDTH, true);
  view.setInt32(22, -HEIGHT, true); // top-down rows
  view.setUint16(26, 1, true);
  view.setUint16(28, 24, true);
  for (let y = 0; y < HEIGHT; y++) {
    const line = Math.floor((y - 44) / 18);
    const inLine = y >= 44 && (y - 44) % 18 < 8 && line < 7;
    const length = 160 + ((line * 97 + seed * 31) % 260);
    for (let x = 0; x < WIDTH; x++) {
      let rgb = [0xf4, 0xf2, 0xec];
      if (y < 28) rgb = [0x2b, 0x2f, 0x36];
      else if (inLine && x >= 20 && x < 20 + length) rgb = line === 3 ? [0xc0, 0x39, 0x2b] : [0x8a, 0x8f, 0x98];
      const at = 54 + y * row + x * 3;
      bytes[at] = rgb[2]!;
      bytes[at + 1] = rgb[1]!;
      bytes[at + 2] = rgb[0]!;
    }
  }
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:image/bmp;base64,${btoa(binary)}`;
}

/** Fills the read as Jira reachable (every image file draws) or unreachable (every read fails); returns the undo. */
export function fakeAttachmentImages(state: "reachable" | "unreachable"): () => void {
  return fillAttachmentImages(async (_, file: Attachment) => {
    if (state === "unreachable") throw new Error("source: unreachable: error sending request for url");
    if (!(file.mime ?? "").startsWith("image/")) throw new Error(`${file.name} is no image`);
    return picture(Number(file.id) || 0);
  });
}
