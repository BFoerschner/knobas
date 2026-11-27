/**
 * An answer's words in the API explorer (ux31, section 19): the tone its status takes, its size in words, the file it
 * goes by and the view it is drawn by (#2533). Pure, for its tests and its mutants.
 */
import type { PipeStep } from "../../ipc/entity";
import { isJsonFile } from "../../render/json-sniff";
import { isXmlMedia } from "../../render/openapi-xml";
import { isXmlFile } from "../../render/xml-sniff";

/** A status's tone: `ok` under 300, `warn` under 400, `err` from 400. An outcome that happened, so it has one. */
export function statusTone(status: number): "ok" | "warn" | "err" {
  return status < 300 ? "ok" : status < 400 ? "warn" : "err";
}

/** The ending an answer's file takes for its content type. */
function endingOf(contentType: string): string {
  const t = contentType.split(";")[0]!.trim().toLowerCase();
  if (/(^|[/+])json$/.test(t)) return "json";
  if (/html/.test(t)) return "html";
  if (isXmlMedia(t)) return "xml";
  if (/gzip$/.test(t)) return "gz";
  if (/zstd$/.test(t)) return "zst";
  if (t.startsWith("text/")) return "txt";
  return "bin";
}

/**
 * The file an answer goes by (#2533): `answer-200.json`. Its ending is what the pipeline bar's defaults and a saved
 * answer read, as a file's would be.
 */
export function answerName(status: number, contentType: string): string {
  return `answer-${status}.${endingOf(contentType)}`;
}

/** How an answer's body is drawn: the JSON view, the XML view, or its text as written. */
export type BodyKind = "json" | "xml" | "text";

/**
 * The view an answer's body is drawn by where the person picks none (#2533): the one its content type names, else what
 * its text is (a `text/plain` body that is JSON), read by the file name it goes by (a pipeline's run renames it). HTML
 * is only ever its text: knobas never renders an answer's page, nor reads it as an XML document.
 */
export function bodyKind(contentType: string, name: string, text: string): BodyKind {
  if (/html/i.test(contentType)) return "text";
  const ending = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
  if (ending === "html" || ending === "htm") return "text";
  if (ending === "json" && isJsonFile(text, name)) return "json";
  if (ending === "xml" && isXmlFile(text, name)) return "xml";
  if (isJsonFile(text, "answer.json")) return "json";
  if (isXmlFile(text, "answer.xml")) return "xml";
  return "text";
}

/** What the person set on an answer's body: the pipeline's steps (`null` while the bar is shut) and *Render as*. */
export interface BodyView {
  steps: PipeStep[] | null;
  as: BodyKind | null;
}
