/**
 * The request column (ux31, sections 14 to 21) with the app's send mocked: the line follows the fields, a request that
 * changes something on another host is asked about in place and nothing is sent until the yes, a typed secret is
 * handed to the app once and the page keeps none, a refusal is drawn, and curl is copied with variables.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const calls: { cmd: string; args: unknown }[] = [];
vi.mock("../../ipc/api", () => ({
  apiServers: async () => ({ servers: [], last: {} }),
  apiSent: async () => [],
  apiHeld: async () => [],
  apiCancel: async () => {},
  apiAnswerBody: async () => new TextEncoder().encode('{"ok":true}'),
  apiSaveAnswer: async () => "x",
  apiClientsFound: async () => ({ bruno: false }),
  apiHoldSecret: async (origin: string, scheme: string, secret: string, user: string | null) => void calls.push({ cmd: "hold", args: { origin, scheme, secret, user } }),
  apiSend: async (send: { id: number; url: string; method: string }) => {
    calls.push({ cmd: "send", args: send });
    if (send.url.includes("refuse")) throw { code: "invalid", message: "This secret is kept for a.example.com. The request goes to b.example.com." };
    return { kind: "answered", id: send.id, status: 201, reason: "Created", version: "HTTP/1.1", headers: [["content-type", "application/json"]], bytes: 11, wire_bytes: 11, to_headers_ms: 3, total_ms: 4, hops: [], cut: false, follow: null, content_type: "application/json" };
  },
}));

const { default: TfOpenApi } = await import("./TfOpenApi.svelte");
const { localEngine } = await import("../../render/openapi-port");
const { confirmedHosts } = await import("./oas.svelte");

const SPEC = JSON.stringify({
  openapi: "3.0.3",
  info: { title: "Shop", version: "1" },
  servers: [{ url: "https://shop.example.com/api" }],
  components: { securitySchemes: { bearer: { type: "http", scheme: "bearer" } } },
  security: [{ bearer: [] }],
  paths: {
    "/orders/{id}": {
      get: { parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }, { name: "expand", in: "query", schema: { type: "string" } }], responses: { "200": { description: "OK" } } },
      post: { parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }], requestBody: { content: { "application/json": { schema: { type: "object", properties: { qty: { type: "integer", minimum: 1 } } } } } }, responses: { "201": { description: "Made" } } },
    },
  },
});

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  calls.length = 0;
  confirmedHosts.clear();
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 8; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label || text(b).startsWith(`${label} `));
function input(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

async function openRequest(at: string) {
  view = mount(TfOpenApi, { target, props: { path: "shop.json", text: SPEC, engine: localEngine(), at, repo: "repo" } });
  await settle();
  button("Request")!.click();
  await settle();
}

test("the line follows every key, a missing path value blocks Send, and a typed value goes out", async () => {
  await openRequest("get /orders/{id}");
  // The method and the address stand in spans of their own.
  const line = () => [...target.querySelectorAll(".tf-oas-line > span")].map((e) => text(e)).join(" ");
  expect(line()).toBe("GET https://shop.example.com/api/orders/{id}");
  expect(button("Send")!.disabled).toBe(true);
  input(target.querySelector<HTMLInputElement>("#oas-p-get\\ \\/orders\\/\\{id\\}-path-id")!, "id:7,x");
  expect(line()).toBe("GET https://shop.example.com/api/orders/id:7,x");
  input(target.querySelector<HTMLInputElement>('[id$="-query-expand"]')!, "lines");
  expect(line()).toBe("GET https://shop.example.com/api/orders/id:7,x?expand=lines");
  input(target.querySelector<HTMLInputElement>('[id^="oas-secret-"]')!, "s3cret-tok");
  button("Send")!.click();
  await settle();
  // The secret went to the app once, for its origin; the request names it and holds none.
  expect(calls.map((c) => c.cmd)).toEqual(["hold", "send"]);
  expect(calls[0]!.args).toEqual({ origin: "https://shop.example.com", scheme: "bearer", secret: "s3cret-tok", user: null });
  expect(JSON.stringify(calls[1]!.args)).not.toContain("s3cret");
  expect(calls[1]!.args).toMatchObject({ method: "GET", auth: { origin: "https://shop.example.com", scheme: "bearer", place: "bearer" }, confirmed_host: null });
  expect(target.querySelector<HTMLInputElement>('[id^="oas-secret-"]')).toBeNull();
  expect(text(target.querySelector(".tf-oas-status"))).toContain("201");
});

test("a POST to another host is asked about in place, and nothing is sent until the yes", async () => {
  await openRequest("post /orders/{id}");
  input(target.querySelector<HTMLInputElement>('[id$="-path-id"]')!, "7");
  button("Send")!.click();
  await settle();
  expect(calls).toEqual([]);
  expect(text(target.querySelector(".tf-oas-ask"))).toContain("POST changes something on shop.example.com. knobas sends it once and does not undo it.");
  button("Cancel")!.click();
  await settle();
  expect(target.querySelector(".tf-oas-ask")).toBeNull();
  expect(calls).toEqual([]);
  button("Send")!.click();
  await settle();
  target.querySelector<HTMLInputElement>(".tf-oas-ask input[type=checkbox]")!.click();
  button("Send POST")!.click();
  await settle();
  expect(calls.at(-1)).toMatchObject({ cmd: "send", args: { method: "POST", confirmed_host: "shop.example.com" } });
  // The body starts as an example made from the schema: an integer of at least 1.
  expect((JSON.parse((calls.at(-1)!.args as { body: string }).body) as { qty: number }).qty).toBeGreaterThanOrEqual(1);
  // Don't ask again for this host until knobas quits: the next send goes at once.
  calls.length = 0;
  button("Send")!.click();
  await settle();
  expect(target.querySelector(".tf-oas-ask")).toBeNull();
  expect(calls.at(-1)).toMatchObject({ cmd: "send", args: { confirmed_host: "shop.example.com" } });
});

test("a refusal before anything was sent is drawn as Not sent, with its words", async () => {
  const spec = SPEC.replace("https://shop.example.com/api", "https://refuse.example.com");
  view = mount(TfOpenApi, { target, props: { path: "shop.json", text: spec, engine: localEngine(), at: "get /orders/{id}", repo: "repo" } });
  await settle();
  button("Request")!.click();
  await settle();
  input(target.querySelector<HTMLInputElement>('[id$="-path-id"]')!, "1");
  button("Send")!.click();
  await settle();
  expect(text(target.querySelector(".tf-oas-fail"))).toBe("Not sent.This secret is kept for a.example.com. The request goes to b.example.com.");
});

test("Copy as curl copies the request with the secret as a variable", async () => {
  const copied: string[] = [];
  Object.defineProperty(navigator, "clipboard", { value: { writeText: async (t: string) => void copied.push(t) }, configurable: true });
  await openRequest("get /orders/{id}");
  input(target.querySelector<HTMLInputElement>('[id$="-path-id"]')!, "7");
  button("Copy as curl")!.click();
  await settle();
  expect(copied[0]).toBe("curl -X GET 'https://shop.example.com/api/orders/7' \\\n  -H \"Authorization: Bearer $TOKEN\"");
});
