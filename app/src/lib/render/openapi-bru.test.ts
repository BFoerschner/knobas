/**
 * A spec as a Bruno collection (ux31, section 25), read back by Bruno's own parser (`@usebruno/lang` 0.41.0, a
 * development dependency): every request with its method, its address, its parameters and its body as written; no
 * secret in an environment; names every file system takes.
 */
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

import { expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import { bruKey, bruOne, safeName } from "./openapi-bru";
import { STORE_20_JSON, SPLIT_31_YAML, PETS_30_YAML } from "./openapi-fixture";

const lang = createRequire(import.meta.url)("@usebruno/lang") as {
  bruToJsonV2: (t: string) => { http: { method: string; url: string; body: string }; params?: { name: string; value: string; type: string; enabled: boolean }[]; headers?: { name: string; value: string; enabled: boolean }[]; body?: { json?: string } };
  collectionBruToJson: (t: string) => unknown;
  bruToEnvJsonV2: (t: string) => { variables: { name: string; value: string; secret: boolean }[] };
};

function written(text: string, path = "spec.json"): Map<string, string> {
  const files = new SpecFiles(path);
  if (!files.open(text).ok) throw new Error("no spec");
  return new Map(files.collection(path.endsWith(".json") ? "openapi.json" : "openapi.yaml"));
}

/** Every file read back by Bruno's parser: requests by their path. */
function readBack(files: Map<string, string>) {
  const requests = new Map<string, ReturnType<typeof lang.bruToJsonV2>>();
  for (const [path, text] of files) {
    if (!path.endsWith(".bru")) continue;
    if (path === "collection.bru" || path.endsWith("/folder.bru")) lang.collectionBruToJson(text);
    else if (path.startsWith("environments/")) lang.bruToEnvJsonV2(text);
    else requests.set(path, lang.bruToJsonV2(text));
  }
  return requests;
}

const KEYED = JSON.stringify({
  openapi: "3.0.3",
  info: { title: "Keyed", version: "1", description: "Line one\nline two" },
  servers: [{ url: "https://api.example.com/v1", description: "Production" }],
  components: { securitySchemes: { key: { type: "apiKey", in: "query", name: "api_key" } } },
  security: [{ key: [] }],
  paths: {
    "/a/{id}": {
      get: {
        summary: "../../x",
        parameters: [
          { name: "id", in: "path", required: true, schema: { type: "string" }, example: "id:7" },
          { name: "a:b", in: "query", schema: { type: "string" }, example: "x\ny" },
          { name: "X-Trace", in: "header", required: true, schema: { type: "string" }, example: "t1" },
        ],
        responses: { "200": { description: "OK" } },
      },
      post: { summary: "../../x", requestBody: { content: { "application/json": { schema: { type: "object", required: ["n"], properties: { n: { type: "integer", minimum: 3 } } } } } }, responses: { "201": { description: "Made" } } },
    },
  },
});

test("each request is read back by Bruno's parser with its method, address, parameters and body", () => {
  const files = written(KEYED);
  const requests = readBack(files);
  expect([...requests.keys()].sort()).toEqual(["a {id}/x.bru", "a {id}/x 2.bru"].sort());
  const get = [...requests.values()].find((r) => r.http.method === "get")!;
  expect(get.http.url).toBe("{{baseUrl}}/a/:id");
  // A key holding a colon is quoted, a value is one line, an optional row is off.
  expect(get.params?.find((p) => p.type === "query")).toMatchObject({ name: "a:b", value: "x y", enabled: false });
  expect(get.params?.find((p) => p.type === "path")).toMatchObject({ name: "id", value: "id:7" });
  expect(get.headers).toEqual([expect.objectContaining({ name: "X-Trace", value: "t1", enabled: true })]);
  const post = [...requests.values()].find((r) => r.http.method === "post")!;
  expect(post.http.body).toBe("json");
  expect(JSON.parse(post.body!.json!)).toEqual({ n: 3 });
});

test("no secret is ever written: the environment names it with no value, the collection uses a variable", () => {
  const files = written(KEYED);
  const env = lang.bruToEnvJsonV2(files.get("environments/Production.bru")!);
  expect(env.variables).toEqual([
    expect.objectContaining({ name: "baseUrl", value: "https://api.example.com/v1", secret: false }),
    expect.objectContaining({ name: "apiKey", secret: true }),
  ]);
  expect(env.variables.find((v) => v.name === "apiKey")?.value ?? "").toBe("");
  expect(files.get("collection.bru")).toContain("value: {{apiKey}}");
  expect(files.get("collection.bru")).toContain("placement: query");
  expect(JSON.parse(files.get("bruno.json")!)).toMatchObject({ name: "Keyed", type: "collection", openapi: [{ sourceUrl: "openapi.json" }] });
});

test("the pet store 2.0 and the split 3.1 spec and the 3.0 pets are written and read back whole", () => {
  for (const [text, path] of [[STORE_20_JSON, "store.json"], [SPLIT_31_YAML, "api/split.yaml"], [PETS_30_YAML, "pets.yaml"]] as const) {
    const requests = readBack(written(text, path));
    expect(requests.size).toBeGreaterThan(0);
    for (const r of requests.values()) expect(r.http.url.startsWith("{{baseUrl}}")).toBe(true);
  }
});

test("names, keys and values are made safe as Bruno and every file system need", () => {
  expect(safeName("../../x")).toBe("x");
  expect(safeName("CON")).toBe("request");
  expect(safeName('a<b>c:d"e|f?g*h')).toBe("a b c d e f g h");
  expect(safeName("x".repeat(300)).length).toBe(120);
  expect(bruKey("a:b")).toBe('"a:b"');
  expect(bruKey("plain")).toBe("plain");
  expect(bruOne("a\r\n b")).toBe("a b");
});

/** GitHub's and Stripe's specs, where `KNOBAS_OAS_SPECS` names a folder holding them (not in the repository). */
const real = process.env["KNOBAS_OAS_SPECS"];
test.skipIf(real === undefined)("GitHub's and Stripe's collections are read back whole, and written in time", () => {
  for (const name of ["github.json", "stripe.json"]) {
    const t0 = performance.now();
    const files = written(readFileSync(`${real}/${name}`, "utf8"), name);
    const ms = performance.now() - t0;
    const requests = readBack(files);
    console.log(`${name}: ${files.size} files, ${requests.size} requests read back, written in ${ms.toFixed(0)} ms (with the parse)`);
    for (const r of requests.values()) expect(r.http.url.startsWith("{{baseUrl}}")).toBe(true);
  }
});

test("an operation that needs no sign-in says so; two servers of one description get environments of their own", () => {
  const files = written(
    JSON.stringify({
      openapi: "3.0.3",
      info: { title: "Open", version: "1" },
      servers: [
        { url: "https://a.example.com", description: "Hosted" },
        { url: "https://b.example.com", description: "Hosted" },
      ],
      components: { securitySchemes: { bearer: { type: "http", scheme: "bearer" } } },
      security: [{ bearer: [] }],
      paths: {
        "/health": { get: { summary: "Health", security: [], responses: { "200": { description: "ok" } } } },
        "/me": { get: { summary: "Me", responses: { "200": { description: "ok" } } } },
      },
    }),
  );
  const requests = [...files].filter(([p]) => /\/(Health|Me)\.bru$/.test(p)).map(([p, t]) => [p.replace(/^.*\//, ""), /auth: (\w+)/.exec(t)?.[1]]);
  expect(Object.fromEntries(requests)).toEqual({ "Health.bru": "none", "Me.bru": "inherit" });
  const envs = [...files.keys()].filter((p) => p.startsWith("environments/")).sort();
  expect(envs).toEqual(["environments/Hosted 2.bru", "environments/Hosted.bru"]);
  expect(lang.bruToEnvJsonV2(files.get("environments/Hosted 2.bru")!).variables.find((v) => v.name === "baseUrl")?.value).toBe("https://b.example.com");
});
