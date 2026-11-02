/**
 * `?fake-ipc`'s attachment bytes and download (spec #2396, #2405):
 * `attachment_bytes` answers PAY-231's two screenshots with a small picture
 * each, and the other files with a line of text; `download_attachment`
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

/** Two 96×32 pictures, one per screenshot, so the two previews differ. */
const PICTURES = [
  "iVBORw0KGgoAAAANSUhEUgAAAGAAAAAgCAIAAABiouoDAAAAW0lEQVR42u3YsQ0AIAwDsF7CicxczMLCCTxQcUBrKWMmT1Fi35NmrJmmWz8AAQIECBAgQGWBQPz7gAABAgQIEKC6QCAsaUCAAAECBMgfZCha0oAAAQIECFCr/gP3NZ9LUPv2+wAAAABJRU5ErkJggg==",
  "iVBORw0KGgoAAAANSUhEUgAAAGAAAAAgCAIAAABiouoDAAAAW0lEQVR42u3YIREAIBBE0UtCRDTByITBIAhAAYYA3Jv5ctWTG7uHHiEABAgQIECA/gUqrV4ba17LtgcECBAgQIAAAQIECBAgQIAAAfoNyCXmUQQECBAgQICSdgCzYkA8nT/7cAAAAABJRU5ErkJggg==",
];

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
      if (mime.startsWith("image/")) return { mime: "image/png", data: PICTURES[Number(file.id) % 2]! };
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
