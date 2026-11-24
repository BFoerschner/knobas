/**
 * What is XML (the XML view's design, 2026-11-23, section 4): a file opens in the XML view where its text says so.
 *
 * 1. A file of any name whose text starts with an XML declaration (`<?xml`, after a byte-order mark and white space).
 * 2. A file whose ending is one of {@link XML_ENDINGS} and whose first thing that is no comment, instruction or DOCTYPE
 *    is a start tag.
 *
 * Not *any file whose first thing is a start tag*: a `.svelte` or `.vue` file starts with `<script>`. Not *the ending
 * alone*: a `.config` or `.manifest` that is JSON or INI would open on an error. The sniff reads at most
 * {@link SNIFF_BYTES} of the text, and is what start-up holds of the view.
 */
import { XML_ENDINGS } from "./languages";

export { XML_ENDINGS };

/** How much of a file's text the sniff reads. */
export const SNIFF_BYTES = 4096;

const ENDINGS = new Set(XML_ENDINGS);

/** The file's ending, lower case, or `""`. */
function endingOf(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** Where the text's first thing stands, past a byte-order mark and white space. */
function firstAt(text: string): number {
  let at = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  while (at < text.length) {
    const c = text.charCodeAt(at);
    if (c !== 32 && c !== 9 && c !== 10 && c !== 13) break;
    at += 1;
  }
  return at;
}

/** Whether a name may start at `c`: a letter, `_` or `:` (ASCII; a name past it is a start tag all the same). */
function nameStart(c: number): boolean {
  return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95 || c === 58 || c > 127;
}

/** Whether `head` starts with an XML declaration. */
export function hasDeclaration(head: string): boolean {
  const at = firstAt(head);
  if (!head.startsWith("<?xml", at)) return false;
  const after = head.charCodeAt(at + 5);
  return after === 32 || after === 9 || after === 10 || after === 13 || after === 63;
}

/** Whether the first thing of `head` that is no comment, instruction or DOCTYPE is a start tag. */
export function startsWithTag(head: string): boolean {
  let at = firstAt(head);
  while (at < head.length) {
    if (head.charCodeAt(at) !== 60) return false;
    if (head.startsWith("<!--", at)) {
      const end = head.indexOf("-->", at + 4);
      if (end === -1) return false;
      at = end + 3;
    } else if (head.startsWith("<?", at)) {
      const end = head.indexOf("?>", at + 2);
      if (end === -1) return false;
      at = end + 2;
    } else if (head.startsWith("<!DOCTYPE", at)) {
      // An internal subset in brackets may hold `>`: the declaration ends at the first `>` outside them.
      let depth = 0;
      let k = at + 9;
      for (; k < head.length; k += 1) {
        const c = head.charCodeAt(k);
        if (c === 91) depth += 1;
        else if (c === 93) depth -= 1;
        else if (c === 62 && depth <= 0) break;
      }
      if (k >= head.length) return false;
      at = k + 1;
    } else return nameStart(head.charCodeAt(at + 1));
    while (at < head.length) {
      const c = head.charCodeAt(at);
      if (c !== 32 && c !== 9 && c !== 10 && c !== 13) break;
      at += 1;
    }
  }
  return false;
}

/** Whether the file at `path` holding `text` is XML: a declaration at any name, or an XML ending and a start tag. */
export function isXmlFile(text: string, path: string): boolean {
  const head = text.length > SNIFF_BYTES ? text.slice(0, SNIFF_BYTES) : text;
  if (hasDeclaration(head)) return true;
  return ENDINGS.has(endingOf(path)) && startsWithTag(head);
}
