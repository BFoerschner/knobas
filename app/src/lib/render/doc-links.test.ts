/** What opens in a line (the XML view's design, 2026-11-23, sections 13 and 14). */
import { expect, test } from "vitest";

import { joinPath, linksIn, pathLike, piecesOf } from "./doc-links";

const files = new Set(["media/kugellager-6204-2rs.png", "schemas/opentrans/opentrans_2_1.xsd", "README.md"]);
const has = (p: string) => files.has(p);
const kinds = (line: string) => linksIn(line, "orders/order-4711.xml", has).map((l) => `${l.kind}:${l.to}${l.image ? ":image" : ""}`);

test("an address ending in a full stop or a bracket loses it", () => {
  expect(kinds("<REMARKS>See https://media.tidewater.example/a.png.</REMARKS>")).toEqual(["web:https://media.tidewater.example/a.png:image"]);
  expect(kinds("(https://track.example/s/1)")).toEqual(["web:https://track.example/s/1"]);
});

test("a path is a link only where the repository holds a file there, from the document's folder first", () => {
  expect(kinds("<MIME_SOURCE>../media/kugellager-6204-2rs.png</MIME_SOURCE>")).toEqual(["path:media/kugellager-6204-2rs.png:image"]);
  expect(kinds("<MIME_SOURCE>../media/gone.png</MIME_SOURCE>")).toEqual([]);
  expect(kinds("<a>README.md</a>")).toEqual(["path:README.md"]);
  // Until the repository's paths are read, no path is a link.
  expect(linksIn("<a>README.md</a>", "x.xml", null)).toEqual([]);
});

test("a path that climbs out of the repository is none, and image/png is no path", () => {
  expect(joinPath("orders/a.xml", "../../x.png")).toBeNull();
  expect(pathLike("image/png")).toBe(false);
  expect(pathLike("a/b")).toBe(false);
  expect(pathLike("1.2.3")).toBe(false);
  expect(pathLike("pom.xml")).toBe(true);
  expect(pathLike("https://a.example/x.png")).toBe(false);
});

test("xmlns and the two halves of schemaLocation are their own kinds", () => {
  const line = `<ORDER xmlns="urn:ot" xmlns:xsi="urn:xsi" xsi:schemaLocation="urn:ot ../schemas/opentrans/opentrans_2_1.xsd">`;
  expect(kinds(line)).toEqual(["namespace:urn:ot", "namespace:urn:xsi", "namespace:urn:ot", "schema:../schemas/opentrans/opentrans_2_1.xsd"]);
  expect(kinds(`<project xsi:noNamespaceSchemaLocation="https://maven.example/x.xsd">`)).toEqual(["schema:https://maven.example/x.xsd"]);
});

test("a line's runs are cut where a link starts and ends", () => {
  const links = linksIn("go https://a.example/x now", "x.xml", null);
  const pieces = piecesOf([{ text: "go https:/" }, { text: "/a.example/x now" }], links);
  expect(pieces.map((p) => [p.text, p.link])).toEqual([
    ["go ", null],
    ["https:/", 0],
    ["/a.example/x", 0],
    [" now", null],
  ]);
});
