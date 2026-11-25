/**
 * Whether a Docs page's code block is an API spec (ux31, section 13): its language says YAML or JSON or nothing, and
 * the sniff finds an `openapi` or `swagger` version key at its top. A guess past the head (`deep`) is no offer: a
 * snippet holding a spec nested inside something else is not one. Asked once a block: a page is read again on every
 * draw, and a block's text does not change while it is the same block.
 */
import { sniffSpec } from "../../render/openapi-sniff";
import { isXsd } from "../../render/xsd-sniff";
import { isXmlFile } from "../../render/xml-sniff";
import { isJsonFile } from "../../render/json-sniff";

/** The languages a spec block may say. `null` and `""` are a block that says none. */
const LANGUAGES = new Set(["", "yaml", "yml", "json"]);

const asked = new Map<string, boolean>();
const ASKED_MOST = 200;

export function specBlock(language: string | null, code: string): boolean {
  if (!LANGUAGES.has((language ?? "").trim().toLowerCase())) return false;
  const known = asked.get(code);
  if (known !== undefined) return known;
  const sniffed = sniffSpec(code);
  const is = sniffed !== null && sniffed.deep !== true;
  if (asked.size >= ASKED_MOST) asked.delete(asked.keys().next().value!);
  asked.set(code, is);
  return is;
}

/** How long an XML or JSON block is before it offers its view (the XML view's design, section 28). */
export const DOC_BLOCK_LINES = 40;
/** The languages an XML block may say; a schema's may also say `xsd`. */
const XML_LANGUAGES = new Set(["", "xml", "xsd"]);
const JSON_LANGUAGES = new Set(["", "json", "jsonc"]);
const kinds = new Map<string, "xsd" | "xml" | "json" | null>();

/**
 * What a Docs page's code block may be drawn as, besides its code (D1): a schema (*Schema*, at any length), an XML or
 * a JSON document of {@link DOC_BLOCK_LINES} lines and more (*XML view*, *JSON view*), or nothing. Its language says so
 * or says nothing, and the sniff agrees; a block another language names is never read. An API spec is `specBlock`'s.
 */
export function docBlock(language: string | null, code: string): "xsd" | "xml" | "json" | null {
  const lang = (language ?? "").trim().toLowerCase();
  if (!XML_LANGUAGES.has(lang) && !JSON_LANGUAGES.has(lang)) return null;
  // Asked by language and text: the same text under another language is another answer.
  const key = `${lang}\u0000${code}`;
  const known = kinds.get(key);
  if (known !== undefined) return known;
  let kind: "xsd" | "xml" | "json" | null = null;
  if (XML_LANGUAGES.has(lang) && isXsd(code)) kind = "xsd";
  else if (lineCount(code) >= DOC_BLOCK_LINES) {
    if (XML_LANGUAGES.has(lang) && isXmlFile(code, "block.xml")) kind = "xml";
    else if (JSON_LANGUAGES.has(lang) && isJsonFile(code, "block.json")) kind = "json";
  }
  if (kinds.size >= ASKED_MOST) kinds.delete(kinds.keys().next().value!);
  kinds.set(key, kind);
  return kind;
}

function lineCount(s: string): number {
  let n = 1;
  for (let i = s.indexOf("\n"); i !== -1 && n < DOC_BLOCK_LINES; i = s.indexOf("\n", i + 1)) n += 1;
  return n;
}
