/**
 * A request from the API explorer (ux31, sections 14 and 21): the value a field starts with, the address as it goes
 * out, a missing path value named, the body left off a GET, a secret as dots on the line and as a variable in curl,
 * and the curl text surviving the shell.
 */
import { execFileSync } from "node:child_process";

import { expect, test } from "vitest";

import type { Operation, Scheme } from "./openapi";
import { bodyText, curl, outgoing, requestLine, startDraft, startValue, type RequestDraft } from "./openapi-request";

const op = (o: Partial<Operation>): Operation => ({
  i: 0, method: "get", path: "/app/rest/builds/{buildLocator}", summary: "", description: "", operationId: null, deprecated: false,
  parameters: [], body: null, responses: [{ status: "200", description: "", content: [{ type: "application/json", schema: null, examples: [] }], headers: [], links: [], ref: null }],
  security: [], servers: [], callbacks: [], externalDocs: null, webhook: false, base: "spec.json", ...o,
});
const scheme = (s: Partial<Scheme>): Scheme => ({ name: "key", type: "apiKey", scheme: "", in: "query", keyName: "api_key", bearerFormat: "", description: "", flows: [], ...s });

test("a field starts with the parameter's example, then its schema's example, default, first enum value, else empty", () => {
  expect(startValue({ example: "ex", schema: { example: "sx", default: "d", enum: ["e"] } })).toBe("ex");
  expect(startValue({ example: undefined, schema: { example: "sx", default: "d", enum: ["e"] } })).toBe("sx");
  expect(startValue({ example: undefined, schema: { default: 20, enum: [10] } })).toBe("20");
  expect(startValue({ example: undefined, schema: { enum: ["open", "closed"] } })).toBe("open");
  expect(startValue({ example: undefined, schema: { type: "string" } })).toBe("");
});

test("the address keeps a locator's colon and comma, leaves an unticked query row out, and names a missing path value", () => {
  const o = op({ parameters: [
    { name: "buildLocator", in: "path", required: true, deprecated: false, description: "", schema: { type: "string" }, example: undefined, ref: null },
    { name: "fields", in: "query", required: false, deprecated: false, description: "", schema: { type: "string" }, example: "id,status", ref: null },
    { name: "tag", in: "query", required: false, deprecated: false, description: "", schema: { type: "array" }, example: undefined, ref: null },
  ] });
  const draft = startDraft(o, {}, null);
  draft.params[0]!.value = "id:4711,branch:main";
  let out = outgoing(o, draft, "https://tc.example.com/", null);
  expect(out.url).toBe("https://tc.example.com/app/rest/builds/id:4711,branch:main");
  expect(out.missing).toEqual([]);
  draft.params[1]!.on = true;
  draft.params[2]!.on = true;
  draft.params[2]!.value = "a, b";
  out = outgoing(o, draft, "https://tc.example.com", null);
  expect(out.url).toBe("https://tc.example.com/app/rest/builds/id:4711,branch:main?fields=id%2Cstatus&tag=a&tag=b");
  draft.params[0]!.value = "";
  expect(outgoing(o, draft, "https://tc.example.com", null).missing).toEqual(["buildLocator"]);
});

test("a GET carries no body; a POST carries its body and its type; a relative or empty server blocks the send", () => {
  const post = op({ method: "post", path: "/v1/charges", body: { description: "", required: true, ref: null, content: [{ type: "application/json", schema: null, examples: [] }] } });
  const draft: RequestDraft = { server: { kind: "spec", index: 0, variables: {} }, scheme: null, params: [], media: "application/json", body: '{"a":1}' };
  const out = outgoing(post, draft, "https://api.example.com", null);
  expect(out.body).toBe('{"a":1}');
  expect(out.headers).toContainEqual(["Content-Type", "application/json"]);
  expect(out.safe).toBe(false);
  expect(out.local).toBe(false);
  expect(outgoing(op({}), draft, "https://api.example.com", null).body).toBeNull();
  expect(outgoing(op({}), draft, "/v2", null).blocked).toMatch(/relative/);
  expect(outgoing(op({}), draft, "", null).blocked).toMatch(/names no server/);
  expect(outgoing(op({}), draft, "http://127.0.0.1:8111", null).local).toBe(true);
  expect(outgoing(op({}), draft, "http://127.evil.example", null).local).toBe(false);
});

test("a query key is dots on the line and $API_KEY in curl; a bearer token and Basic are variables too", () => {
  const draft = startDraft(op({}), {}, "key");
  draft.params = [];
  const withKey = outgoing(op({ path: "/x" }), draft, "https://api.example.com", scheme({}));
  expect(requestLine(withKey)).toBe("GET https://api.example.com/x?api_key=••••••••");
  expect(curl(withKey)).toContain('"https://api.example.com/x?api_key=$API_KEY"');
  const bearer = outgoing(op({ path: "/x" }), draft, "https://api.example.com", scheme({ name: "b", type: "http", scheme: "bearer" }));
  expect(curl(bearer)).toContain('-H "Authorization: Bearer $TOKEN"');
  const basic = outgoing(op({ path: "/x" }), draft, "https://api.example.com", scheme({ name: "u", type: "http", scheme: "basic" }));
  expect(curl(basic)).toContain('-u "$USER:$PASSWORD"');
  const cookie = outgoing(op({ path: "/x" }), draft, "https://api.example.com", scheme({ in: "cookie", keyName: "sid" }));
  expect(curl(cookie)).toContain('-H "Cookie: sid=$API_KEY"');
});

test("a body with a single quote survives the shell as curl writes it", () => {
  const post = op({ method: "post", path: "/x" });
  const draft: RequestDraft = { server: { kind: "spec", index: 0, variables: {} }, scheme: null, params: [], media: "application/json", body: `{"name":"O'Brien","n":"$HOME \\ \`x\`"}` };
  const text = curl(outgoing(post, draft, "https://api.example.com", null));
  const data = text.split("--data ")[1]!;
  // printf through sh -c reads the word back as the body it was.
  expect(execFileSync("sh", ["-c", `printf %s ${data}`]).toString()).toBe(draft.body);
});

test("a form body is name=value lines", () => {
  expect(bodyText("application/x-www-form-urlencoded", { amount: 2000, currency: "eur", meta: { a: 1 } })).toBe("amount=2000\ncurrency=eur");
});

test("no writer ever prints a secret's value: an outgoing request holds none to print", () => {
  for (const s of [scheme({}), scheme({ in: "header", keyName: "X-Key" }), scheme({ name: "b", type: "http", scheme: "bearer" }), scheme({ name: "u", type: "http", scheme: "basic" })]) {
    const out = outgoing(op({ path: "/x" }), startDraft(op({}), {}, s.name), "https://api.example.com", s);
    const all = `${requestLine(out)}\n${curl(out)}\n${JSON.stringify(out)}`;
    expect(all).not.toMatch(/tok|secret-value|s3cret/);
    for (const [, v] of out.headers) if (typeof v !== "string") expect(Object.keys(v).sort()).toEqual(v.cookie === undefined ? ["scheme", "secret"] : ["cookie", "scheme", "secret"]);
  }
});
