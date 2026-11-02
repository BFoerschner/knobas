/**
 * What the ticket page's Attachments section says about one file (#2404):
 * its kind, drawn in mono where a file has no preview (#2405), and its line
 * of who, when and how big. The list is the mirror's
 * (`EntityDetail.attachments`); nothing here reads the bytes.
 */
import type { Attachment } from "../../ipc/entity";
import { formatBytes } from "../../sources/diagnostics";
import { dayKey } from "../../time/day";

/** A short word of letters and digits: what a kind may be drawn as. */
const WORD = /^[a-z0-9]+$/i;

/**
 * The file's kind as a short capitalised word: the name's extension (`PDF`,
 * `PNG`) where it has one of at most four characters, else the media type's
 * subtype where that is one plain word (`text/markdown` → `MARKDOWN`), else
 * `FILE`. A leading dot (`.env`) is a hidden file's name, not an extension.
 */
export function attachmentKind(file: Pick<Attachment, "name" | "mime">): string {
  const dot = file.name.lastIndexOf(".");
  const extension = dot > 0 ? file.name.slice(dot + 1) : "";
  if (extension.length > 0 && extension.length <= 4 && WORD.test(extension)) return extension.toUpperCase();
  const subtype = file.mime?.split("/")[1]?.trim() ?? "";
  if (subtype !== "" && WORD.test(subtype)) return subtype.toUpperCase();
  return "FILE";
}

/**
 * Who attached the file, on which day in the reader's calendar, and how big
 * it is, joined by commas; what the source did not say is left out
 * (ADR-0007), never guessed.
 */
export function attachmentLine(file: Pick<Attachment, "author" | "created" | "size">): string {
  const when = file.created === null ? null : new Date(file.created);
  return [
    file.author,
    when !== null && !Number.isNaN(when.getTime()) ? dayKey(when) : null,
    file.size === null ? null : formatBytes(file.size),
  ]
    .filter((part): part is string => part !== null && part !== "")
    .join(", ");
}
