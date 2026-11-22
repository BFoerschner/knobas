import { expect, test } from "vitest";
import { sameApi, specChanges, stableJson } from "./openapi-changes";

const pet = { type: "object", required: ["id"], properties: { id: { type: "integer" }, name: { type: "string" }, owner: { $ref: "#/components/schemas/Owner" } } };
const owner = { type: "object", properties: { name: { type: "string" } } };
const BASE = {
  openapi: "3.0.3",
  info: { title: "Pets", version: "1" },
  paths: {
    "/pets": {
      get: { summary: "List pets", responses: { "200": { description: "ok", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/Pet" } } } } } } },
      post: { summary: "Add a pet", requestBody: { content: { "application/json": { schema: { $ref: "#/components/schemas/Pet" } } } }, responses: { "201": { description: "made" } } },
    },
    "/owners": { get: { summary: "Owners", description: "All owners.", responses: { "200": { description: "ok", content: { "application/json": { schema: { $ref: "#/components/schemas/Owner" } } } } } } },
    "/health": { get: { summary: "Health", responses: { "200": { description: "ok" } } } },
  },
  components: { schemas: { Pet: pet, Owner: owner } },
};
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

test("a reworded description is a change of its operation only", () => {
  const after = clone(BASE);
  after.paths["/owners"].get.description = "Every owner.";
  expect(specChanges(BASE, after)).toEqual({ ops: { added: [], removed: [], changed: [{ key: "get /owners" }] }, schemas: { added: [], removed: [], changed: [] } });
});

test("a schema's new required field marks the schema and the two operations that name it, through it", () => {
  const after = clone(BASE);
  after.components.schemas.Pet.required = ["id", "name"];
  const c = specChanges(BASE, after);
  expect(c.schemas).toEqual({ added: [], removed: [], changed: ["Pet"] });
  expect(c.ops.changed).toEqual([
    { key: "get /pets", via: "Pet" },
    { key: "post /pets", via: "Pet" },
  ]);
});

test("a change two refs down names the nearest changed schema; an operation that changed itself is not said to change through one", () => {
  const after = clone(BASE);
  after.components.schemas.Owner.properties.name = { type: "string", maxLength: 80 } as never;
  after.paths["/pets"].post.summary = "Add one pet";
  const c = specChanges(BASE, after);
  expect(c.schemas.changed).toEqual(["Owner"]);
  expect(c.ops.changed).toEqual([{ key: "get /pets", via: "Owner" }, { key: "post /pets" }, { key: "get /owners", via: "Owner" }]);
});

test("a reordered document, keys and all, is no change", () => {
  const after = { components: { schemas: { Owner: { properties: owner.properties, type: "object" }, Pet: pet } }, paths: Object.fromEntries(Object.entries(BASE.paths).reverse()), info: BASE.info, openapi: "3.0.3" };
  const c = specChanges(BASE, after);
  expect(sameApi(c)).toBe(true);
  expect(stableJson({ b: 1, a: [2, { d: 3, c: 4 }] })).toBe(stableJson({ a: [2, { c: 4, d: 3 }], b: 1 }));
  // An array's order is the API's: reordered, it is a change.
  expect(stableJson([1, 2])).not.toBe(stableJson([2, 1]));
});

test("operations and schemas added and removed are said by key, and a path item's own parameters count for its operations", () => {
  const after = clone(BASE) as typeof BASE & { paths: Record<string, Record<string, unknown>> };
  delete (after.paths as Record<string, unknown>)["/health"];
  after.paths["/pets/{id}"] = { get: { summary: "One pet", responses: { "200": { description: "ok" } } } };
  (after.components.schemas as Record<string, unknown>).Tag = { type: "string" };
  delete (after.components.schemas as Record<string, unknown>).Owner;
  (after.paths["/owners"] as Record<string, unknown>).parameters = [{ name: "limit", in: "query", schema: { type: "integer" } }];
  const c = specChanges(BASE, after);
  expect(c.ops.added).toEqual(["get /pets/{id}"]);
  expect(c.ops.removed).toEqual(["get /health"]);
  expect(c.ops.changed.map((x) => x.key)).toContain("get /owners");
  // Pet names Owner, now gone; its own JSON is the same, so it is no change of its own.
  expect(c.schemas).toEqual({ added: ["Tag"], removed: ["Owner"], changed: [] });
});

test("a cycle of refs ends, and 2.0's definitions are its schemas", () => {
  const node = { type: "object", properties: { parent: { $ref: "#/definitions/Node" }, leaf: { $ref: "#/definitions/Leaf" } } };
  const before = { swagger: "2.0", paths: { "/nodes": { get: { responses: { "200": { description: "ok", schema: { $ref: "#/definitions/Node" } } } } } }, definitions: { Node: node, Leaf: { type: "string" } } };
  const after = clone(before);
  after.definitions.Leaf = { type: "string", format: "uuid" } as never;
  const c = specChanges(before, after);
  expect(c.schemas.changed).toEqual(["Leaf"]);
  expect(c.ops.changed).toEqual([{ key: "get /nodes", via: "Leaf" }]);
  // The cycle alone, nothing changed: no operation is marked.
  expect(sameApi(specChanges(before, clone(before)))).toBe(true);
});

test("an operation reaching only a cycle of unchanged schemas, while another schema changed, ends unmarked", () => {
  const node = { type: "object", properties: { parent: { $ref: "#/components/schemas/Node" } } };
  const before = { openapi: "3.0.3", paths: { "/nodes": { get: { responses: { "200": { description: "ok", content: { "application/json": { schema: { $ref: "#/components/schemas/Node" } } } } } } } }, components: { schemas: { Node: node, Other: { type: "string" } } } };
  const after = clone(before);
  after.components.schemas.Other = { type: "string", format: "date" } as never;
  const c = specChanges(before, after);
  expect(c.schemas.changed).toEqual(["Other"]);
  expect(c.ops.changed).toEqual([]);
});

test("a 3.1 webhook is compared as an operation by its method and name", () => {
  const before = { openapi: "3.1.0", webhooks: { newPet: { post: { summary: "A pet was added" } } } };
  const after = { openapi: "3.1.0", webhooks: { newPet: { post: { summary: "A pet came" } } } };
  expect(specChanges(before, after).ops.changed).toEqual([{ key: "post newPet" }]);
});
