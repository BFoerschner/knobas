/**
 * A document's lines (the XML view's design, 2026-11-23, section 6). Pure.
 *
 * **A file written on one line is laid out**: where the longest line is over {@link LAY_OUT_OVER} characters and holds
 * more than one start tag, the view draws the document laid out by knobas: an element a line, two spaces a level, a
 * leaf's text untouched, mixed content left on its line. Whether to is decided on the page from the text at hand, before
 * the first frame ({@link wantsLayout}); the layout itself is the worker's ({@link layOutXml}). Its rows are the file's
 * rows in the same order, so a value's place in the file is found by its row.
 */
import { CDATA, COMMENT, DOC, EL, PI, TEXT, type XmlOutline } from "./xml-outline";

/** A line longer than this, holding more than one start tag, makes a file one to lay out. */
export const LAY_OUT_OVER = 2000;

/** Whether `text` is a document written on one line: its longest line is over 2,000 characters and holds two start tags. */
export function wantsLayout(text: string): boolean {
  let from = 0;
  for (;;) {
    const at = text.indexOf("\n", from);
    const end = at === -1 ? text.length : at;
    if (end - from > LAY_OUT_OVER) {
      let tags = 0;
      for (let i = text.indexOf("<", from); i !== -1 && i < end; i = text.indexOf("<", i + 1)) {
        const c = text.charCodeAt(i + 1);
        if ((c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95) tags += 1;
        if (tags > 1) return true;
      }
    }
    if (at === -1) return false;
    from = at + 1;
  }
}

/** The document laid out: an element a line, two spaces a level, a leaf's text untouched, mixed content on its line. */
export function layOutXml(o: XmlOutline): string {
  const t = o.text;
  const out: string[] = [];
  // What stands before the root as written (the declaration, a DOCTYPE), its own lines kept.
  const first = o.first[0]!;
  const prolog = first === -1 ? t : t.slice(0, o.start[first]);
  for (const line of prolog.split("\n")) if (line.trim() !== "") out.push(line.trim());
  const pad = (depth: number) => "  ".repeat(Math.max(0, depth - 1));
  const write = (id: number) => {
    const type = o.type[id];
    const depth = o.depth[id]!;
    if (type !== EL) {
      const raw = t.slice(o.start[id], o.end[id]);
      if (type === TEXT && raw.trim() === "") return;
      out.push(pad(depth) + (type === TEXT ? raw.trim() : raw));
      return;
    }
    // A leaf, or an element with text beside its elements (mixed content): as written, on its line.
    let mixed = false;
    let elements = 0;
    for (let c = o.first[id]!; c !== -1; c = o.next[c]!) {
      if (o.type[c] === TEXT || o.type[c] === CDATA) mixed = true;
      if (o.type[c] === EL || o.type[c] === COMMENT || o.type[c] === PI) elements += 1;
    }
    if (elements === 0 || mixed) {
      out.push(pad(depth) + t.slice(o.start[id], o.end[id]));
      return;
    }
    out.push(pad(depth) + t.slice(o.start[id], o.open[id]));
    for (let c = o.first[id]!; c !== -1; c = o.next[c]!) write(c);
    out.push(pad(depth) + t.slice(o.close[id], o.end[id]));
  };
  for (let c = first; c !== -1; c = o.next[c]!) if (o.type[c] !== DOC) write(c);
  return out.join("\n") + "\n";
}
