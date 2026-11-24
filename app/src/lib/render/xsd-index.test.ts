import { expect, test, vi } from "vitest";

import { heldIndex } from "./xsd-index";

const XSD = (ns: string) => `<xs:schema xmlns:xs="${"http:/"}/www.w3.org/2001/XMLSchema" targetNamespace="${ns}"/>`;

test("a listing's index is read once and shared, by its folder; another listing (another commit) reads again", async () => {
  const files = new Map([
    ["a/one.xsd", XSD("urn:one")],
    ["b/two.xsd", XSD("urn:two")],
  ]);
  const read = vi.fn((p: string) => Promise.resolve(files.get(p) ?? null));
  const listing = [...files.keys()];
  const first = await heldIndex("r", listing, "a/x.xsd", read);
  expect(first.declares).toEqual([
    ["a/one.xsd", "urn:one"],
    ["b/two.xsd", "urn:two"],
  ]);
  expect(read).toHaveBeenCalledTimes(2);
  // The same listing from the same folder: nothing read again.
  expect(await heldIndex("r", listing, "a/y.xml", read)).toBe(first);
  expect(read).toHaveBeenCalledTimes(2);
  // Another folder reads the nearest first, so it is its own index; another listing is another commit.
  await heldIndex("r", listing, "b/x.xsd", read);
  await heldIndex("r", [...listing], "a/x.xsd", read);
  expect(read).toHaveBeenCalledTimes(6);
});
