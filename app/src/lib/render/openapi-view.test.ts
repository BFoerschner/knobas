/**
 * What the OpenAPI view draws from an index (ux18): its groups, the filter's rows and a schema's one line; and which
 * files the registry gives the OpenAPI renderer, by what they hold.
 */
import { expect, test } from "vitest";

import { SpecFiles, type SpecIndex } from "./openapi";
import { BROKEN_YAML, COMPOSE_YAML, PETS_30_YAML, STORE_20_JSON, bigSpec } from "./openapi-fixture";
import { groupsOf, haystack, refName, rowsOf, summary, terms, type Row } from "./openapi-view";
import { openWith } from "./defaults";
import { OPENAPI, allRenderers, renderersFor } from "./registry";

function indexOf(text: string): SpecIndex {
  const answer = new SpecFiles("spec.yaml").open(text);
  if (!answer.ok) throw new Error(answer.error);
  return answer.index;
}

const none = { ops: new Set<number>(), refs: new Set<string>(), schemas: new Set<string>() };
const hays = (index: SpecIndex) => new Map(index.ops.map((op) => [op, haystack(op)]));
const shape = (rows: Row[]) =>
  rows.map((r) =>
    r.kind === "group" ? `# ${r.group.name} ${r.shown}` : r.kind === "op" ? `${r.op.method} ${r.op.path}` : r.kind === "schema" ? `s ${r.name}` : r.kind === "shead" ? `# Schemas ${r.shown}` : r.kind,
  );

test("endpoints group by tag in the spec's order; those no tag holds group by their path's first segment", () => {
  const index = indexOf(PETS_30_YAML);
  expect(groupsOf(index).map((g) => [g.name, g.code, g.ops.length])).toEqual([
    ["pets", false, 4],
    ["store", false, 1],
    ["/health", true, 1],
  ]);
  expect(shape(rowsOf(groupsOf(index), hays(index), index.schemas, [], none))).toEqual([
    "# pets 4",
    "get /pets",
    "post /pets",
    "get /pets/{petId}",
    "delete /pets/{petId}",
    "# store 1",
    "get /store/inventory",
    "# /health 1",
    "get /health",
    "# Schemas 3",
    "s Pet",
    "s Owner",
    "s Error",
  ]);
});

test("the filter keeps the endpoints every word finds in their method, path, summary, tag or operationId, and the schemas it names", () => {
  const index = indexOf(PETS_30_YAML);
  const groups = groupsOf(index);
  const rows = (typed: string) => shape(rowsOf(groups, hays(index), index.schemas, terms(typed), none));
  expect(rows("delete")).toEqual(["# pets 1", "delete /pets/{petId}"]);
  expect(rows("GET petid")).toEqual(["# pets 1", "get /pets/{petId}"]);
  expect(rows("inventory")).toEqual(["# store 1", "get /store/inventory"]);
  expect(rows("listpets")).toEqual(["# pets 1", "get /pets"]);
  expect(rows("store")).toEqual(["# store 1", "get /store/inventory"]);
  expect(rows("owner")).toEqual(["# Schemas 1", "s Owner"]);
  expect(rows("nothing-like-this")).toEqual([]);
});

test("an open endpoint and an open schema have their detail row under them", () => {
  const index = indexOf(PETS_30_YAML);
  const rows = rowsOf(groupsOf(index), hays(index), index.schemas, [], { ops: new Set([1]), refs: new Set(), schemas: new Set(["Owner"]) });
  expect(shape(rows).slice(1, 4)).toEqual(["get /pets", "post /pets", "detail"]);
  expect(shape(rows).slice(-3)).toEqual(["s Owner", "sbody", "s Error"]);
  // Every key is its own: the windowed list keys rows by them.
  expect(new Set(rows.map((r) => r.k)).size).toBe(rows.length);
});

test("5,000 operations make 5,000 rows under 25 tags, and a filter over them keeps the keys unique", () => {
  const index = indexOf(bigSpec(5000));
  const groups = groupsOf(index);
  const all = rowsOf(groups, hays(index), [], [], none);
  expect(all.filter((r) => r.kind === "op")).toHaveLength(5000);
  expect(all.filter((r) => r.kind === "group")).toHaveLength(25);
  const some = rowsOf(groups, hays(index), [], terms("delete resource12"), none);
  expect(some.filter((r) => r.kind === "op").map((r) => (r.kind === "op" ? r.op.path : ""))).toContain("/resource12/{id}");
  expect(new Set(all.map((r) => r.k)).size).toBe(all.length);
});

test("a schema's one line says its type in words, a ref by name, and null where 3.0 or 3.1 allows it", () => {
  expect(summary({ type: "integer", format: "int64" })).toMatchObject({ type: "integer, int64", ref: null });
  expect(summary({ type: "array", items: { $ref: "#/components/schemas/Pet" } })).toMatchObject({ type: "array of", ref: "#/components/schemas/Pet" });
  expect(summary({ type: "array", items: { type: "string" } }).type).toBe("array of string");
  expect(summary({ type: ["string", "null"] })).toMatchObject({ type: "string", nullable: true });
  expect(summary({ type: "string", nullable: true }).nullable).toBe(true);
  expect(summary({ oneOf: [{}, {}, {}] }).type).toBe("one of 3");
  expect(summary({ properties: { a: {} } }).type).toBe("object");
  expect(summary({ enum: ["a", 1] })).toMatchObject({ type: "enum", enumValues: ["a", "1"] });
  expect(refName("#/components/schemas/Pet")).toBe("Pet");
  expect(refName("./schemas/order.yaml#/Order")).toBe("Order");
  expect(refName("../common.yaml")).toBe("common.yaml");
});

test("the registry offers OpenAPI for a spec at any name and opens it by default, and never for other YAML or JSON", () => {
  const spec = (path: string, text: string) => openWith({ path, binary: false, text }, allRenderers(), []);
  expect(spec("openapi.yaml", PETS_30_YAML).renderer).toBe(OPENAPI);
  expect(spec("api/v1/spec.json", STORE_20_JSON).renderer).toBe(OPENAPI);
  expect(spec("petstore.oas.yml", PETS_30_YAML).renderer).toBe(OPENAPI);
  expect(spec("api-spec", PETS_30_YAML).offered.map((r) => r.id)).toEqual(["openapi", "source"]);
  // A file that looks like a spec at its top and does not parse is still offered: the view says why.
  expect(spec("broken.yaml", BROKEN_YAML).renderer).toBe(OPENAPI);
  expect(spec("docker-compose.yml", COMPOSE_YAML).offered.map((r) => r.id)).toEqual(["source"]);
  // Markdown keeps a README that happens to start with `openapi:`; OpenAPI is offered beside it.
  expect(spec("README.md", PETS_30_YAML).offered.map((r) => r.id)).toEqual(["markdown", "openapi", "source"]);
  // Without its text (a binary file, a diff's file list) OpenAPI takes nothing.
  expect(openWith({ path: "openapi.yaml", binary: false }, allRenderers(), []).offered.map((r) => r.id)).toEqual(["source"]);
  expect(renderersFor({ path: "openapi.yaml", binary: true, text: PETS_30_YAML }).map((r) => r.id)).toEqual([]);
});

test("a default that maps a pattern to OpenAPI draws only the specs among its files", () => {
  const mapped = [{ pattern: "*.yaml", renderer: "openapi", pipeline: null }];
  expect(openWith({ path: "x.yaml", binary: false, text: PETS_30_YAML }, allRenderers(), mapped).renderer).toBe(OPENAPI);
  expect(openWith({ path: "x.yaml", binary: false, text: COMPOSE_YAML }, allRenderers(), mapped).renderer?.id).toBe("source");
});
