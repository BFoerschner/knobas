/**
 * XML and JSON compared by element (C1, the XML view's design section 28): formatting is no change, a change says
 * both values at its path, a part that came or went is one row at its top, a part that stands elsewhere is moved,
 * and repeated parts are matched by what names them, then whole, then in order.
 */
import { expect, test } from "vitest";
import { compareDocs, nth } from "./doc-compare";

const rows = (a: string, b: string, kind: "xml" | "json" = "xml") => compareDocs(a, b, kind).rows.map((r) => `${r.mark} ${r.path}${r.say === "" ? "" : ` | ${r.say}`}`);

test("XML: indentation, attribute order, quotes, a prefix spelled differently and <a/> against <a></a> are no change", () => {
  const a = `<p:order xmlns:p="urn:o" id="1" kind="std"><p:line n="1">pen</p:line><p:note/></p:order>`;
  const b = `<?xml version="1.0"?>
<order xmlns="urn:o" kind='std'   id="1">
    <line n="1">pen</line>
    <note></note>
</order>`;
  const c = compareDocs(a, b, "xml");
  expect(c.rows).toEqual([]);
  expect(c.same).toBe(3);
});

test("XML: a changed value and attribute say both values; what came or went is one row at its top", () => {
  const a = `<o><h><date>2026-01-02</date></h><items><i sku="A"><q>1</q></i></items><gone><x/><y/></gone></o>`;
  const b = `<o><h><date>2026-01-03</date><cur>EUR</cur></h><items><i sku="B"><q>1</q></i></items></o>`;
  expect(rows(a, b)).toEqual([
    "changed /o/h/date | text: 2026-01-02 → 2026-01-03",
    "added /o/h/cur | EUR",
    "changed /o/items/i | @sku: A → B",
    "removed /o/gone | 2 parts inside",
  ]);
});

test("XML: an element that stands elsewhere among its siblings is moved, and still compared", () => {
  const a = `<o><a>1</a><b>2</b><c>3</c></o>`;
  const b = `<o><c>3</c><a>1</a><b>20</b></o>`;
  expect(rows(a, b)).toEqual(["moved /o/c | 3rd → 1st", "changed /o/b | text: 2 → 20"]);
});

test("XML: repeated items are matched by their id, then whole, then in order", () => {
  const item = (id: string, q: string) => `<item id="${id}"><q>${q}</q></item>`;
  // By id: reordered and one changed.
  expect(rows(`<l>${item("1", "5")}${item("2", "6")}</l>`, `<l>${item("2", "7")}${item("1", "5")}</l>`)).toEqual(["moved /l/item[1] | 2nd → 1st", "changed /l/item[1]/q | text: 6 → 7"]);
  // With no id, an edited item in the middle is changed, not removed and added.
  const plain = (qs: string[]) => `<l>${qs.map((q) => `<item><q>${q}</q></item>`).join("")}</l>`;
  expect(rows(plain(["1", "2", "3"]), plain(["1", "9", "3"]))).toEqual(["changed /l/item[2]/q | text: 2 → 9"]);
  // An identical item taken out of the middle is one removal.
  expect(rows(plain(["1", "2", "3"]), plain(["1", "3"]))).toEqual(["removed /l/item[2] | 1 part inside"]);
});

test("JSON: key order and spacing are no change; a value, an item and a member say their jq path", () => {
  const a = `{"orders":[{"id":1,"total":10},{"id":2,"total":20}],"currency":"EUR"}`;
  const b = `{
  "currency": "EUR",
  "orders": [ {"total": 10, "id": 1}, {"id": 2, "total": 25.0}, {"id": 3, "total": 5} ],
  "note": null
}`;
  expect(rows(a, b, "json")).toEqual(["changed .orders[1].total | value: 20 → 25", "added .orders[2] | 2 parts inside", "added .note | null"]);
  expect(compareDocs(a, a.replace(`"currency":"EUR"`, `"currency" : "EUR"`), "json").rows).toEqual([]);
});

test("JSON: array items named by an id are moved, an object's members never", () => {
  const a = `{"x":1,"y":2,"list":[{"id":"a"},{"id":"b"},{"id":"c"}]}`;
  const b = `{"y":2,"x":1,"list":[{"id":"c"},{"id":"a"},{"id":"b"}]}`;
  expect(rows(a, b, "json")).toEqual(["moved .list[0] | 3rd → 1st"]);
});

test("a side that is not well formed, or too large, says why and compares nothing; a file only after is one row", () => {
  const broken = compareDocs("<a><b></a>", "<a/>", "xml");
  expect(broken.rows).toEqual([]);
  expect(broken.bad.old).toMatch(/^not well formed/);
  expect(compareDocs(`{"a":`, "{}", "json").bad.old).toMatch(/^not JSON/);
  expect(compareDocs(null, "<a><b/></a>", "xml").rows.map((r) => `${r.mark} ${r.path}`)).toEqual(["added /a"]);
});

test("a place among siblings in words", () => {
  expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 103, 111].map((n) => nth(n))).toEqual(["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "103rd", "111th"]);
});

test("XML: items named by an id are told apart by it, even when each also changed; spaces around text are formatting", () => {
  const item = (id: string, q: string) => `<item id="${id}"><q>${q}</q></item>`;
  expect(rows(`<l>${item("1", "5")}${item("2", "6")}</l>`, `<l>${item("2", "7")}${item("1", "8")}</l>`)).toEqual([
    "moved /l/item[1] | 2nd → 1st",
    "changed /l/item[1]/q | text: 6 → 7",
    "changed /l/item[2]/q | text: 5 → 8",
  ]);
  expect(rows(`<a> pen </a>`, `<a>pen</a>`)).toEqual([]);
});

test("XML: an attribute added, changed and removed, each with its values", () => {
  expect(rows(`<a x="1" y="2"/>`, `<a y="3" z="4"/>`)).toEqual(["changed /a | @y: 2 → 3 · @z: added · @x: removed"]);
});
