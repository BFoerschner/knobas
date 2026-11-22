/**
 * What the worker reads now (ux31, sections 8, 9 and 12): servers with variables and per operation, security
 * schemes and requirements, links, callbacks, webhooks, 2.0 form bodies, `info`'s facts; and the shape of a schema
 * (`allOf` merged, a choice told), the *Used by* index, a value made from a schema, and a page in one answer.
 */
import { expect, test } from "vitest";

import { SpecFiles, answerCall, type Operation, type SpecIndex } from "./openapi";
import { DEEP_30_JSON, FORM_20_JSON, MODEL_31_JSON, PETS_30_YAML } from "./openapi-fixture";

function opened(text: string, path = "spec.json"): { files: SpecFiles; index: SpecIndex } {
  const files = new SpecFiles(path);
  const answer = files.open(text);
  if (!answer.ok) throw new Error(answer.error);
  return { files, index: answer.index };
}

const opOf = (files: SpecFiles, index: SpecIndex, anchor: string): Operation => {
  const row = [...index.ops, ...index.webhooks].find((o) => o.anchor === anchor);
  if (row === undefined) throw new Error(anchor);
  return files.operation(row.i)!;
};

test("a server's variables, the operation's own servers, its path item's, else the spec's", () => {
  const { files, index } = opened(MODEL_31_JSON);
  expect(index.servers).toEqual([
    { url: "https://{tenant}.example.test/{base}", description: "Hosted", variables: { tenant: { default: "acme", enum: ["acme", "globex"], description: "" }, base: { default: "v2", enum: [], description: "" } } },
  ]);
  expect(opOf(files, index, "post /things").servers.map((s) => s.url)).toEqual(["https://write.example.test"]);
  expect(opOf(files, index, "get /things").servers.map((s) => s.url)).toEqual(["https://things.example.test"]);
  expect(opOf(files, index, "get /things/{id}").servers.map((s) => s.url)).toEqual(["https://{tenant}.example.test/{base}"]);
});

test("a 2.0 spec's host, basePath and schemes are its servers; with no host the address is relative", () => {
  const { index } = opened(FORM_20_JSON);
  expect(index.servers).toEqual([{ url: "/v2", description: "", variables: {} }]);
});

test("security schemes of each kind, and requirements: [{}] is or nothing, two schemes together are one requirement", () => {
  const { files, index } = opened(MODEL_31_JSON);
  expect(index.security.map((s) => [s.name, s.type, s.scheme, s.in, s.keyName])).toEqual([
    ["bearer", "http", "bearer", "", ""],
    ["basic", "http", "basic", "", ""],
    ["key", "apiKey", "", "query", "api_key"],
    ["oauth", "oauth2", "", "", ""],
    ["oidc", "openIdConnect", "", "", ""],
  ]);
  expect(index.security[0]!.bearerFormat).toBe("JWT");
  expect(index.security[3]!.flows).toEqual(["authorizationCode"]);
  expect(opOf(files, index, "get /things").security).toEqual([[]]);
  expect(opOf(files, index, "post /things").security).toEqual([["key"], ["basic", "key"]]);
  // The spec's own requirement where the operation names none.
  expect(opOf(files, index, "get /things/{id}").security).toEqual([["bearer"]]);
  // 2.0's `basic` reads as an http scheme.
  expect(opened(FORM_20_JSON).index.security.map((s) => [s.name, s.type, s.scheme, s.in, s.keyName])).toEqual([
    ["basic", "http", "basic", "", ""],
    ["key", "apiKey", "", "header", "X-API-Key"],
  ]);
});

test("links, callbacks, external docs, a webhook, and info's contact, licence and terms", () => {
  const { files, index } = opened(MODEL_31_JSON);
  const list = opOf(files, index, "get /things");
  expect(list.responses[0]!.links).toEqual([
    { name: "first", operationId: "getThing", operationRef: "", description: "The first thing" },
    { name: "ref", operationId: "listThings", operationRef: "", description: "More" },
  ]);
  const make = opOf(files, index, "post /things");
  expect(make.callbacks).toEqual(["made"]);
  expect(make.externalDocs).toEqual({ url: "https://example.test/make", description: "" });
  expect(index.webhooks.map((w) => [w.i, w.anchor, w.webhook])).toEqual([[3, "post thingMade", true]]);
  const hook = opOf(files, index, "post thingMade");
  expect(hook).toMatchObject({ webhook: true, summary: "A thing was made" });
  expect(hook.body?.content.map((m) => m.type)).toEqual(["application/json"]);
  expect(index.info).toEqual({
    contact: { name: "API team", email: "api@example.test", url: "https://example.test/team" },
    license: { name: "MIT", url: "https://opensource.org/licenses/MIT" },
    terms: "https://example.test/terms",
    externalDocs: { url: "https://example.test/docs", description: "Guide" },
  });
});

test("2.0 formData parameters are a form body: multipart where one is a file, else urlencoded", () => {
  const { files, index } = opened(FORM_20_JSON);
  const photo = opOf(files, index, "post /pets/{id}/photo");
  expect(photo.parameters.map((p) => `${p.in}:${p.name}`)).toEqual(["path:id"]);
  expect(photo.body).toMatchObject({ required: true, content: [{ type: "multipart/form-data", schema: { type: "object", required: ["file"] } }] });
  const props = (photo.body!.content[0]!.schema as { properties: Record<string, { type: string; format?: string }> }).properties;
  expect(props.file).toMatchObject({ type: "string", format: "binary" });
  expect(props.caption).toMatchObject({ type: "string", description: "Words under it" });
  expect(opOf(files, index, "post /pets/{id}").body?.content[0]?.type).toBe("application/x-www-form-urlencoded");
});

test("allOf is merged: the parts' properties in order, their required lists joined, the first description kept", () => {
  const { files } = opened(DEEP_30_JSON);
  const sh = files.shape(files.root, { $ref: "#/components/schemas/Card" });
  expect(sh.props.map((p) => [p.name, p.required])).toEqual([
    ["id", true],
    ["object", false],
    ["name", true],
    ["last4", false],
  ]);
  expect(sh.props[0]!.words).toBe("The id.");
  const charge = files.shape(files.root, { $ref: "#/components/schemas/Charge" });
  expect(charge.props.map((p) => p.name)).toEqual(["id", "object", "amount", "customer", "description", "source", "parent"]);
  expect(charge.props.find((p) => p.name === "amount")?.required).toBe(true);
});

test("a choice of more than one is told apart; a choice of a schema and null, or empty, is that schema", () => {
  const { files } = opened(DEEP_30_JSON);
  const charge = files.shape(files.root, { $ref: "#/components/schemas/Charge" });
  const source = files.shape(files.root, charge.props.find((p) => p.name === "source")!.node);
  expect(source.variants).toMatchObject({ key: "oneOf", discriminator: "object" });
  expect(source.variants!.list).toHaveLength(2);
  const description = files.shape(files.root, charge.props.find((p) => p.name === "description")!.node);
  expect(description.variants).toBeNull();
  expect(description.facts).toMatchObject({ type: "string", maxLength: 5000 });
  const orNull = files.shape(files.root, { oneOf: [{ $ref: "#/components/schemas/Base" }, { type: "null" }] });
  expect(orNull.variants).toBeNull();
  expect(orNull.props.map((p) => p.name)).toEqual(["id", "object"]);
});

test("a schema that holds itself answers its row and stops", () => {
  const { files } = opened(DEEP_30_JSON);
  const charge = files.shape(files.root, { $ref: "#/components/schemas/Charge" });
  const parent = charge.props.find((p) => p.name === "parent")!;
  expect(parent.node).toEqual({ $ref: "#/components/schemas/Charge" });
  const { files: pets } = opened(PETS_30_YAML, "api/openapi.yaml");
  expect(pets.shape(pets.root, { $ref: "#/components/schemas/Pet" }).props.map((p) => p.name)).toContain("parent");
});

test("Used by lists an operation that names a schema in a parameter, a body and a response once; Part of lists its holders", () => {
  const spec = {
    openapi: "3.0.3",
    info: { title: "U", version: "1" },
    paths: {
      "/a": {
        post: {
          parameters: [{ name: "q", in: "query", schema: { $ref: "#/components/schemas/Pet" } }],
          requestBody: { content: { "application/json": { schema: { $ref: "#/components/schemas/Pet" } } } },
          responses: { "200": { $ref: "#/components/responses/Pets" } },
        },
      },
      "/b": { get: { responses: { "200": { description: "Owner", content: { "application/json": { schema: { $ref: "#/components/schemas/Owner" } } } } } } },
    },
    components: {
      responses: { Pets: { description: "Pets", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/Pet" } } } } } },
      schemas: { Pet: { type: "object" }, Owner: { type: "object", properties: { pets: { type: "array", items: { $ref: "#/components/schemas/Pet" } }, self: { $ref: "#/components/schemas/Owner" } } } },
    },
  };
  const { files } = opened(JSON.stringify(spec));
  expect(files.uses("Pet")).toEqual({ ops: [0], schemas: ["Owner"] });
  // A schema never holds itself in Part of.
  expect(files.uses("Owner")).toEqual({ ops: [1], schemas: [] });
});

test("a value made from a schema: required and first properties, read-only ones left out, formats in their shape", () => {
  const { files } = opened(MODEL_31_JSON);
  expect(files.sample(files.root, { $ref: "#/components/schemas/Thing" }, "request")).toEqual({ name: "lamp", made: "2026-11-22T14:05:00Z" });
  const { files: deep } = opened(DEEP_30_JSON);
  const charge = deep.sample(deep.root, { $ref: "#/components/schemas/Charge" }) as Record<string, unknown>;
  expect(charge.amount).toBeGreaterThanOrEqual(1);
  // The cycle closes as null, not forever.
  expect(charge.parent).toBeDefined();
});

test("a page is one answer: an operation and the shapes it shows unopened, or a schema with its uses and an example", () => {
  const { files } = opened(DEEP_30_JSON);
  const sessions = new Map([[1, files]]);
  const op = answerCall(sessions, 1, { kind: "page", place: "post /v1/charges" }) as { kind: string; op: Operation; shapes: Record<string, unknown> };
  expect(op.kind).toBe("op");
  expect(Object.keys(op.shapes).sort()).toEqual(["b:application/json", "b:application/x-www-form-urlencoded", "r:200:application/json"]);
  const list = answerCall(sessions, 1, { kind: "page", place: "get /v1/charges" }) as { shapes: Record<string, { props: unknown[] }> };
  // An array's items are drawn under it at once.
  expect(Object.keys(list.shapes)).toEqual(["r:200:application/json", "r:200:application/json/[]"]);
  const schema = answerCall(sessions, 1, { kind: "page", place: "schemas/Named" }) as { kind: string; uses: { schemas: string[] }; sample: unknown };
  expect(schema.kind).toBe("schema");
  expect(schema.uses.schemas).toEqual(["Card", "Bank", "Customer"]);
  expect(answerCall(sessions, 1, { kind: "page", place: "get /nowhere" })).toEqual({ kind: "missing" });
});

test("a schema's example closes where the schema comes round again, and a choice of a schema and empty is that schema", () => {
  const { files } = opened(DEEP_30_JSON);
  const sessions = new Map([[1, files]]);
  const page = answerCall(sessions, 1, { kind: "page", place: "schemas/Charge" }) as { sample: Record<string, unknown> };
  expect(page.sample.parent).toBeNull();
  expect(page.sample.description).toBe("Some words about it.");
});
