/**
 * The XML view's fixtures (its design, 2026-11-23, section 30): the two openTRANS orders and the POM, made by the
 * clickdummy's data script from the XSD design's own example (`fixtures/doc/SOURCE.md`), read from the repository's
 * `fixtures/doc/`; and, for what they lack, a few lines each.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/** The fixtures as the seed pushes them onto `tidewater/ledger-api` (`testenv/seed-gitea.sh`, step 4c). */
const ROOT = resolve(__dirname, "../../../../fixtures/doc/files/tidewater/ledger-api");
/** Where each fixture stands in the repository. */
const AT: Record<string, string> = {
  "order-4710.xml": "orders/order-4710.xml",
  "order-4711.xml": "orders/order-4711.xml",
  "pom.xml": "build/pom.xml",
  "kugellager-6204-2rs.png": "media/kugellager-6204-2rs.png",
  "rahmenvertrag-2026.pdf": "contracts/rahmenvertrag-2026.pdf",
};

/** A fixture of `fixtures/doc/`, by its name, as text. */
export const fixture = (name: string): string => readFileSync(resolve(ROOT, AT[name] ?? name), "utf8");
/** A fixture of `fixtures/doc/`, by its name, as bytes. */
export const fixtureBytes = (name: string): Uint8Array => new Uint8Array(readFileSync(resolve(ROOT, AT[name] ?? name)));

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
