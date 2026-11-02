/**
 * `?fake-ipc`'s attachment bytes and download (spec #2396, #2405):
 * `attachment_bytes` answers PAY-231's two screenshots with a drawn picture
 * of a log panel each (#2406's, moved here when the two joined), and the other files with a line of text; `download_attachment`
 * answers the path a download would land at. Both look the file up in the
 * fixture's attachment list, as the real commands look it up in the mirror's.
 *
 * `?fake-attachments=unreachable` makes both reject as a Jira that cannot be
 * reached does, `?fake-attachments=forbidden` as one that answers 403: the
 * page's stale strip and placeholders, for the shell tests and the shots.
 */
import type { Attachment, AttachmentBytes } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { CORPUS } from "./corpus";

/** The cap the real command reads under: 5 MiB (`ATTACHMENT_CAP`). */
const CAP = 5 * 1024 * 1024;

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

type Failing = "unreachable" | "forbidden" | null;

/** The file `attachmentId` of `entityId` as the fixture lists it, or the real command's refusal. */
function listed(entityId: string, attachmentId: string): Attachment {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (!entry) throw { code: "not_found", message: `${entityId} is not in the mirror`, source_id: null };
  const files = (entry.payload["attachments"] ?? []) as Attachment[];
  const file = files.find((candidate) => candidate.id === attachmentId);
  if (!file) throw { code: "not_found", message: `${entityId} lists no attachment ${attachmentId}`, source_id: null };
  return file;
}

/** The rejection `?fake-attachments=` asks for, if it asks for one. */
function failure(failing: Failing, entityId: string, file: Attachment): unknown {
  const source = entityId.split(":")[0] ?? "";
  if (failing === "unreachable") return { code: "unreachable", message: "unreachable: connection refused (127.0.0.1:5399)", source_id: source };
  if (failing === "forbidden")
    return { code: "unauthorized", message: `${source} does not permit this account to read ${file.name} (HTTP 403)`, source_id: source };
  return null;
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const asked = params.get("fake-attachments");
  const failing: Failing = asked === "unreachable" || asked === "forbidden" ? asked : null;
  return {
    attachment_bytes: (args): AttachmentBytes => {
      const entityId = String(args["entityId"]);
      const file = listed(entityId, String(args["attachmentId"]));
      const refused = failure(failing, entityId, file);
      if (refused !== null) throw refused;
      if (file.size !== null && file.size > CAP)
        throw { code: "invalid", message: `${file.name} is over 5 MiB; knobas previews an attachment up to 5 MiB`, source_id: null };
      const mime = file.mime ?? "application/octet-stream";
      if (mime.startsWith("image/")) return { mime: "image/bmp", data: picture(Number(file.id) || 0).slice("data:image/bmp;base64,".length) };
      return { mime, data: btoa(`${file.name}\n`) };
    },
    download_attachment: (args) => {
      const entityId = String(args["entityId"]);
      const file = listed(entityId, String(args["attachmentId"]));
      const refused = failure(failing, entityId, file);
      if (refused !== null) throw refused;
      return `/Users/mara/Downloads/${file.name}`;
    },
  };
}
