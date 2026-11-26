/**
 * What is an XML Schema (the XSD design, 2026-11-23, section 4): a `*.xsd` file whose first element is `schema` in the
 * XML Schema namespace. The sniff reads at most {@link XSD_SNIFF_BYTES}, past a byte-order mark, the declaration,
 * comments, instructions and a DOCTYPE with its internal subset, takes the first start tag and asks whether its local
 * name is `schema` and its prefix (or the default namespace) is bound to XML Schema's namespace on that tag. It is all
 * start-up holds of the schema explorer.
 */
import { XS } from "./xsd-xml";

/** How much of a file's text the sniff reads. */
export const XSD_SNIFF_BYTES = 64 * 1024;

/** The first start tag of `text` (its text up to its `>`), past what may stand before it; `null` for none. */
export function firstTag(text: string): string | null {
  const head = text.length > XSD_SNIFF_BYTES ? text.slice(0, XSD_SNIFF_BYTES) : text;
  let i = head.charCodeAt(0) === 0xfeff ? 1 : 0;
  for (;;) {
    const lt = head.indexOf("<", i);
    if (lt === -1) return null;
    if (head.startsWith("<?", lt)) {
      const end = head.indexOf("?>", lt + 2);
      if (end === -1) return null;
      i = end + 2;
    } else if (head.startsWith("<!--", lt)) {
      const end = head.indexOf("-->", lt + 4);
      if (end === -1) return null;
      i = end + 3;
    } else if (head.startsWith("<!", lt)) {
      // A DOCTYPE: its internal subset in brackets may hold `>`, and a quoted value may hold either.
      let depth = 0;
      let quote = 0;
      let j = lt + 2;
      for (; j < head.length; j += 1) {
        const c = head.charCodeAt(j);
        if (quote !== 0) {
          if (c === quote) quote = 0;
        } else if (c === 34 || c === 39) quote = c;
        else if (c === 91) depth += 1;
        else if (c === 93) depth -= 1;
        else if (c === 62 && depth <= 0) break;
      }
      if (j >= head.length) return null;
      i = j + 1;
    } else {
      let quote = 0;
      let j = lt + 1;
      for (; j < head.length; j += 1) {
        const c = head.charCodeAt(j);
        if (quote !== 0) {
          if (c === quote) quote = 0;
        } else if (c === 34 || c === 39) quote = c;
        else if (c === 62) break;
      }
      return head.slice(lt, j);
    }
  }
}

/** The tag's attributes as written, by name. */
function attrsOf(tag: string): Map<string, string> {
  const out = new Map<string, string>();
  const ATTR = /([^\s=<>/]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  for (let m = ATTR.exec(tag); m !== null; m = ATTR.exec(tag)) out.set(m[1]!, m[2] ?? m[3] ?? "");
  return out;
}

/** Whether the first tag of `text` is an XML Schema's `schema` element, its namespace bound on that tag. */
export function isXsd(text: string): boolean {
  const tag = firstTag(text);
  if (tag === null) return false;
  const m = /^<([^\s/>]+)/.exec(tag);
  if (m === null) return false;
  const q = m[1]!;
  const colon = q.indexOf(":");
  const local = colon === -1 ? q : q.slice(colon + 1);
  if (local !== "schema") return false;
  const bind = colon === -1 ? "xmlns" : `xmlns:${q.slice(0, colon)}`;
  return attrsOf(tag).get(bind) === XS;
}

/**
 * The `targetNamespace` a schema's first tag declares: `""` for none, `undefined` where the text is no schema. What the
 * repository's index keeps for each `.xsd`.
 */
export function sniffNamespace(text: string): string | undefined {
  if (!isXsd(text)) return undefined;
  return attrsOf(firstTag(text)!).get("targetNamespace") ?? "";
}
