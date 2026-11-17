/**
 * An OpenAPI spec read for its view (ux18): what is recognised as a spec, what its index and operations hold for
 * Swagger 2.0 and OpenAPI 3.0 and 3.1 in YAML and JSON, how refs resolve (in the file, into another file, never to
 * a URL or out of the repository), and what a broken or unsupported file answers.
 */
import { expect, test } from "vitest";

import { SpecFiles, parseDoc, pointer, refTarget, type Operation, type SpecIndex } from "./openapi";
import { isOpenApi, sniffSpec } from "./openapi-sniff";
import { BROKEN_YAML, COMPOSE_YAML, ITEMS_YAML, ORDER_YAML, PETS_30_YAML, SPLIT_31_YAML, STORE_20_JSON, TINY_30_JSON, bigSpec } from "./openapi-fixture";

function opened(path: string, text: string): { files: SpecFiles; index: SpecIndex } {
  const files = new SpecFiles(path);
  const answer = files.open(text);
  if (!answer.ok) throw new Error(answer.error);
  return { files, index: answer.index };
}

test("a spec is recognised by its top-level openapi or swagger key, in YAML or JSON, whatever the file is called", () => {
  expect(sniffSpec(PETS_30_YAML)).toEqual({ key: "openapi", version: "3.0.3" });
  expect(sniffSpec(SPLIT_31_YAML)).toEqual({ key: "openapi", version: "3.1.0" });
  expect(sniffSpec(STORE_20_JSON)).toEqual({ key: "swagger", version: "2.0" });
  expect(sniffSpec(TINY_30_JSON)).toEqual({ key: "openapi", version: "3.0.0" });
  // Quoted keys, a byte-order mark, a document marker and comments first.
  expect(isOpenApi(`\uFEFF# api\n---\n"openapi": '3.1.0'\ninfo: {}\n`)).toBe(true);
  // The version key after other keys, and after strings that hold braces and escaped quotes.
  expect(isOpenApi(`{"info": {"title": "a \\" { b", "x": [1, {"openapi": "3.0.0"}]}, "openapi": "3.1.0"}`)).toBe(true);
});

test("a file whose openapi key is not at its top, or that names no version knobas draws, is not a spec", () => {
  expect(isOpenApi(COMPOSE_YAML)).toBe(false);
  expect(isOpenApi(`{"name": "pkg", "config": {"openapi": "3.0.0"}}`)).toBe(false);
  expect(isOpenApi(`{"name": "pkg", "config": {"x": 1, "openapi": "3.0.0"}}`)).toBe(false);
  expect(isOpenApi(`  openapi: 3.0.0\n`)).toBe(true);
  expect(isOpenApi(`info:\n  openapi: 3.0.0\n`)).toBe(false);
  expect(isOpenApi(`openapi: 4.0.0\n`)).toBe(false);
  expect(isOpenApi(`swagger: "1.2"\n`)).toBe(false);
  expect(isOpenApi(`# openapi: 3.0.0\n`)).toBe(false);
  expect(isOpenApi("")).toBe(false);
});

test("OpenAPI 3.0 in YAML: the head, the tags in the spec's order, every operation and the schemas", () => {
  const { index } = opened("api/openapi.yaml", PETS_30_YAML);
  expect(index).toMatchObject({
    key: "openapi",
    version: "3.0.3",
    title: "Pet Store",
    apiVersion: "1.4.0",
    servers: [{ url: "https://api.example.com/v1", description: "Production" }],
    tags: [
      { name: "pets", description: "Everything about pets" },
      { name: "store", description: "" },
    ],
    schemas: ["Pet", "Owner", "Error"],
    schemaBase: "#/components/schemas/",
    problems: [],
  });
  expect(index.description).toContain("The **pet** store.");
  expect(index.ops.map((op) => op.anchor)).toEqual(["get /pets", "post /pets", "get /pets/{petId}", "delete /pets/{petId}", "get /store/inventory", "get /health"]);
  expect(index.ops[3]).toMatchObject({ deprecated: true, tags: ["pets"], summary: "Remove a pet" });
  expect(index.ops[5]!.tags).toEqual([]);
});

test("an operation: the path's parameters with its own, its body and responses, their refs in the file followed", () => {
  const { files } = opened("api/openapi.yaml", PETS_30_YAML);
  const show = files.operation(2)!;
  expect(show.parameters).toEqual([
    { name: "petId", in: "path", required: true, deprecated: false, description: "", schema: { type: "string" }, example: undefined, ref: null },
  ]);
  const list = files.operation(0)!;
  expect(list.parameters.map((p) => `${p.in}:${p.name}`)).toEqual(["query:limit"]);
  expect(list.responses.map((r) => r.status)).toEqual(["200", "default"]);
  // `default` was a ref to components.responses: followed, so it reads whole.
  expect(list.responses[1]).toMatchObject({ description: "Something went wrong", ref: null, content: [{ type: "application/json", schema: { $ref: "#/components/schemas/Error" } }] });
  expect(list.responses[0]!.headers).toEqual([{ name: "x-next", description: "A link to the next page", schema: { type: "string" } }]);
  const create = files.operation(1)!;
  expect(create.body).toMatchObject({ required: true, content: [{ type: "application/json", schema: { $ref: "#/components/schemas/Pet" }, examples: [{ name: "", value: { id: 1, name: "Rex" } }] }] });
});

test("Swagger 2.0 in JSON: host and basePath are the server, the body parameter is the request body, produces is the content", () => {
  const { files, index } = opened("swagger.json", STORE_20_JSON);
  expect(index).toMatchObject({ key: "swagger", version: "2.0", servers: [{ url: "https://store.example.com/api" }], schemas: ["Order"], schemaBase: "#/definitions/" });
  const op = files.operation(0) as Operation;
  expect(op.parameters.map((p) => `${p.in}:${p.name}`)).toEqual(["header:X-Trace"]);
  expect(op.parameters[0]!.schema).toEqual({ type: "string" });
  expect(op.body).toMatchObject({ required: true, content: [{ type: "application/json", schema: { $ref: "#/definitions/Order" } }] });
  expect(op.responses[0]).toMatchObject({ status: "200", content: [{ type: "application/json", schema: { $ref: "#/definitions/Order" }, examples: [{ value: { id: 7 } }] }] });
});

test("OpenAPI 3.0 in JSON and 3.1 in YAML read the same way", () => {
  expect(opened("tiny.json", TINY_30_JSON).index.ops.map((op) => op.anchor)).toEqual(["get /ping"]);
  const split = opened("api/spec.yaml", SPLIT_31_YAML).index;
  expect(split.ops.map((op) => op.anchor)).toEqual(["get /orders", "get /far", "get /out"]);
  // A path item in another file is listed by its path and read when opened.
  expect(split.pathRefs).toEqual([{ path: "/items", ref: "./paths/items.yaml" }]);
  // 3.1 needs no paths.
  expect(new SpecFiles("w.yaml").open(`openapi: 3.1.0\ninfo: {title: W, version: "1"}\nwebhooks: {}\n`).ok).toBe(true);
});

test("a ref into another file asks for it, then resolves from it; refs inside it are read from there", () => {
  const { files } = opened("api/spec.yaml", SPLIT_31_YAML);
  expect(files.resolve("api/spec.yaml", "./schemas/order.yaml#/Order")).toEqual({ need: "api/schemas/order.yaml" });
  expect(files.addFile("api/schemas/order.yaml", ORDER_YAML)).toEqual({ ok: true });
  const order = files.resolve("api/spec.yaml", "./schemas/order.yaml#/Order");
  expect(order).toMatchObject({ ok: true, base: "api/schemas/order.yaml" });
  // `#/Line` inside order.yaml is order.yaml's.
  expect(files.resolve("api/schemas/order.yaml", "#/Line")).toMatchObject({ ok: true, node: { type: "object" }, base: "api/schemas/order.yaml" });
  expect(files.pathItem({ path: "/items", ref: "./paths/items.yaml" })).toEqual({ need: "api/paths/items.yaml" });
  files.addFile("api/paths/items.yaml", ITEMS_YAML);
  const items = files.pathItem({ path: "/items", ref: "./paths/items.yaml" });
  expect(items).toMatchObject({ ok: true, ops: [{ method: "get", path: "/items", summary: "List items", base: "api/paths/items.yaml" }] });
});

test("a ref to a URL is never followed, and one out of the repository is refused", () => {
  const { files } = opened("api/spec.yaml", SPLIT_31_YAML);
  expect(files.resolve("api/spec.yaml", "https://example.com/schemas/far.json")).toEqual({ ok: false, error: "https://example.com/schemas/far.json is outside the repository; knobas does not fetch it." });
  expect(files.resolve("api/spec.yaml", "../../../outside.yaml#/X")).toEqual({ ok: false, error: "../../../outside.yaml#/X points outside the repository." });
  expect(refTarget("a/b/spec.yaml", "../c/./d.yaml#/X")).toEqual({ ok: true, file: "a/c/d.yaml", pointer: "/X" });
  expect(refTarget("spec.yaml", "/top.yaml")).toEqual({ ok: true, file: "top.yaml", pointer: "" });
});

test("a ref cycle resolves one step at a time: the schema that holds itself is the node it points at", () => {
  const { files } = opened("api/openapi.yaml", PETS_30_YAML);
  const pet = files.resolve("api/openapi.yaml", "#/components/schemas/Pet");
  expect(pet).toMatchObject({ ok: true });
  const parent = (pet as { node: { properties: { parent: { $ref: string } } } }).node.properties.parent;
  expect(files.resolve("api/openapi.yaml", parent.$ref)).toEqual(pet);
  // A JSON pointer reads escaped names.
  expect(pointer({ "a/b": { "c~d": 1 } }, "/a~1b/c~0d")).toBe(1);
});

test("a file that does not parse says why and on which line, YAML or JSON", () => {
  const broken = new SpecFiles("broken.yaml").open(BROKEN_YAML);
  expect(broken).toMatchObject({ ok: false, line: 7 });
  expect(parseDoc(`{\n  "openapi": "3.0.0",\n  "info": {]\n}`, "x.json")).toMatchObject({ ok: false, line: 3 });
});

test("a spec knobas cannot draw says what it lacks", () => {
  const no = (text: string) => {
    const answer = new SpecFiles("x.yaml").open(text);
    return answer.ok ? "ok" : answer.error;
  };
  expect(no(`openapi: 3.2.0\ninfo: {title: X}\npaths: {}\n`)).toBe("knobas draws OpenAPI 3.0 and 3.1 and Swagger 2.0; this file says openapi: 3.2.0.");
  expect(no(`openapi: 3.0.0\npaths: {}\n`)).toBe("The spec has no info object, which every spec must have.");
  expect(no(`openapi: 3.0.0\ninfo: {title: X}\n`)).toBe("The spec has no paths, which OpenAPI 3.0 and Swagger 2.0 require.");
  expect(no(`openapi: 3.0.0\ninfo: {title: X}\npaths: [1]\n`)).toBe("The spec's paths is not a mapping of paths to their operations.");
  expect(no(`- openapi\n`)).toBe("The file is not a mapping at its top, so it is not an OpenAPI spec.");
  // `openapi: 3.0` unquoted is the number 3 to YAML; the text's own words decide.
  expect(no(`openapi: 3.0\ninfo: {title: X}\npaths: {}\n`)).toBe("ok");
  // An operation that is not an object is left out and said.
  const { index } = opened("x.yaml", `openapi: 3.0.0\ninfo: {title: X}\npaths:\n  /a:\n    get: 1\n    put: {summary: P}\n`);
  expect(index.problems).toEqual(["GET /a is not an operation, so it is left out."]);
  expect(index.ops.map((op) => op.anchor)).toEqual(["put /a"]);
});

test("a spec of 5,000 operations is indexed whole, each operation read on its own", () => {
  const { files, index } = opened("big.yaml", bigSpec(5000));
  expect(index.ops).toHaveLength(5000);
  expect(index.tags).toHaveLength(25);
  expect(index.schemas).toHaveLength(1250);
  expect(files.operation(4999)).toMatchObject({ method: "delete", path: "/resource1249/{id}", parameters: [{ name: "id", in: "path" }] });
});

test("another file a ref read is kept with the least lately used dropped past the bound; the spec itself stays", () => {
  const files = new SpecFiles("spec.yaml", 2);
  files.open(TINY_30_JSON.replace('"Tiny"', '"Kept"'));
  files.addFile("a.yaml", "A: 1\n");
  files.addFile("b.yaml", "B: 1\n");
  expect(files.resolve("spec.yaml", "./a.yaml#/A")).toMatchObject({ ok: true, node: 1 });
  files.addFile("c.yaml", "C: 1\n");
  expect([files.has("spec.yaml"), files.has("a.yaml"), files.has("b.yaml"), files.has("c.yaml")]).toEqual([true, true, false, true]);
});

test("a spec opened again is not parsed again: its document is the one read before", () => {
  const first = new SpecFiles("api/openapi.yaml");
  first.open(PETS_30_YAML);
  const again = new SpecFiles("api/openapi.yaml");
  again.open(PETS_30_YAML);
  const info = (files: SpecFiles) => (files.resolve("api/openapi.yaml", "#/info") as { node: unknown }).node;
  expect(info(again)).toBe(info(first));
});
