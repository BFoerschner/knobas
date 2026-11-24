import { expect, test } from "vitest";
import { exampleFileName, exampleMeta, exampleOfPlace, forgetHeld, heldOptions, holdOptions } from "./example-held.svelte";

test("an example's place names its element; every other place, none", () => {
  expect(exampleOfPlace("example/ORDER")).toBe("ORDER");
  expect(exampleOfPlace("example/bmecat:ARTICLE")).toBe("bmecat:ARTICLE");
  for (const other of ["el/ORDER", "example/", "examples/ORDER", "", null, undefined]) expect(exampleOfPlace(other)).toBeNull();
});

test("its file is named as the maker names it, the element's local name and the seed", () => {
  expect(exampleFileName("ORDER", 4711)).toBe("ORDER-example-4711.xml");
  expect(exampleFileName("bmecat:ARTICLE", 7)).toBe("ARTICLE-example-7.xml");
});

test("the header says how it was made, in the made-by comment's words, its lines once known, and that it is not saved", () => {
  expect(exampleMeta({ seed: 4711, optional: "required" }, 212)).toBe("Example made by knobas, seed 4711, required parts only. 212 lines. Not saved.");
  expect(exampleMeta({ seed: 9, optional: "all" }, 1234)).toBe("Example made by knobas, seed 9, every optional part. 1,234 lines. Not saved.");
  expect(exampleMeta({ seed: 9, optional: "all" }, null)).toBe("Example made by knobas, seed 9, every optional part. Not saved.");
});

test("settings are held by their schema's key until forgotten", () => {
  forgetHeld();
  const o = { seed: 99, optional: "all", repeats: 2, lang: "de", picks: {} } as const;
  holdOptions("r\na.xsd\n1:2", o);
  expect(heldOptions("r\na.xsd\n1:2")).toBe(o);
  expect(heldOptions("r\na.xsd\n1:3")).toBeUndefined();
  forgetHeld();
  expect(heldOptions("r\na.xsd\n1:2")).toBeUndefined();
});
