/**
 * A line of a spec's text mapped to its place (ux31): an Everything hit inside a spec opens the explorer at the
 * operation or schema that holds the hit's line, in YAML and in JSON.
 */
import { expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import { DEEP_30_JSON, PETS_30_YAML, STORE_20_JSON } from "./openapi-fixture";
import { pathAtLine } from "./openapi-line";

/** The 1-based line of the first line of `text` holding `needle`, after line `from`. */
function lineOf(text: string, needle: string, from = 0): number {
  const lines = text.split("\n");
  for (let i = from; i < lines.length; i++) if (lines[i]!.includes(needle)) return i + 1;
  throw new Error(needle);
}

function placeAt(text: string, path: string, line: number): string | null {
  const files = new SpecFiles(path);
  if (!files.open(text).ok) throw new Error("no spec");
  return files.placeAt(line);
}

test("YAML: a line inside an operation, a path item's parameters, a schema's property, and the head", () => {
  const t = PETS_30_YAML;
  expect(placeAt(t, "a.yaml", lineOf(t, "How many to return"))).toBe("get /pets");
  expect(placeAt(t, "a.yaml", lineOf(t, "summary: Create a pet"))).toBe("post /pets");
  // A path item's own parameters: its first operation.
  expect(placeAt(t, "a.yaml", lineOf(t, "name: petId"))).toBe("get /pets/{petId}");
  expect(placeAt(t, "a.yaml", lineOf(t, "enum: [available, sold]"))).toBe("schemas/Pet");
  expect(placeAt(t, "a.yaml", lineOf(t, "pets:", lineOf(t, "    Owner:")))).toBe("schemas/Owner");
  expect(placeAt(t, "a.yaml", lineOf(t, "title: Pet Store"))).toBeNull();
  // A block scalar's text that looks like a key is still the description's.
  expect(pathAtLine("info:\n  description: |\n    Note: this is words\n  title: T\n", 3)).toEqual(["info", "description"]);
});

test("JSON: an operation's line, a definition's, and a line in a nested schema", () => {
  expect(placeAt(STORE_20_JSON, "s.json", lineOf(STORE_20_JSON, '"X-Trace"'))).toBe("post /orders");
  expect(placeAt(STORE_20_JSON, "s.json", lineOf(STORE_20_JSON, '"qty"'))).toBe("schemas/Order");
  expect(placeAt(DEEP_30_JSON, "d.json", lineOf(DEEP_30_JSON, '"last4"'))).toBe("schemas/Card");
  expect(placeAt(DEEP_30_JSON, "d.json", lineOf(DEEP_30_JSON, '"Delete a customer"'))).toBe("delete /v1/customers/{customer}");
  expect(placeAt(DEEP_30_JSON, "d.json", 1)).toBeNull();
  expect(pathAtLine('{\n  "a": {\n    "b": [\n      1,\n      {"c": 2}\n    ]\n  }\n}', 5)).toEqual(["a", "b", 1, "c"]);
});
