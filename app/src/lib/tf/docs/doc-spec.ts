/**
 * Whether a Docs page's code block is an API spec (ux31, section 13): its language says YAML or JSON or nothing, and
 * the sniff finds an `openapi` or `swagger` version key at its top. A guess past the head (`deep`) is no offer: a
 * snippet holding a spec nested inside something else is not one. Asked once a block: a page is read again on every
 * draw, and a block's text does not change while it is the same block.
 */
import { sniffSpec } from "../../render/openapi-sniff";

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
