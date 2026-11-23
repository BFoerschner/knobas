/**
 * The XML view's fixtures (its design, 2026-11-23, section 30): the two openTRANS orders and the POM, made by the
 * clickdummy's data script from the XSD design's own example (`fixtures/doc/SOURCE.md`), read from the repository's
 * `fixtures/doc/`; and, for what they lack, a few lines each.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = resolve(__dirname, "../../../../fixtures/doc");

/** A file of `fixtures/doc/`, as text. */
export const fixture = (name: string): string => readFileSync(resolve(ROOT, name), "utf8");
/** A file of `fixtures/doc/`, as bytes. */
export const fixtureBytes = (name: string): Uint8Array => new Uint8Array(readFileSync(resolve(ROOT, name)));

/** A document written on one line. */
export const ONE_LINE = `<?xml version="1.0"?><a xmlns="urn:x"><b n="1">one</b><b n="2">two  spaces</b><c/></a>`;
/** A document that is not well formed at line 4. */
export const BROKEN = ["<ORDER>", "  <ITEM>", "    <PRICE>12</PRICE>", "  </PRICES>", "</ORDER>"].join("\n");
/** A comment of several lines, CDATA, a default namespace reset to none, two prefixes for one namespace. */
export const CORNERS = [
  '<r xmlns="urn:one" xmlns:a="urn:two" xmlns:b="urn:two">',
  "  <!-- a comment",
  "       of three lines -->",
  "  <a:x>1</a:x>",
  "  <b:x>2</b:x>",
  '  <plain xmlns="">',
  "    <inner/>",
  "  </plain>",
  "  <code><![CDATA[a < b && c]]></code>",
  '  <img src="data:image/png;base64,iVBORw0KGgo=" />',
  "</r>",
].join("\n");
