/**
 * The request column (ux31, sections 14 to 21) with the app's send mocked: the line follows the fields, a request that
 * changes something on another host is asked about in place and nothing is sent until the yes, a typed secret is
 * handed to the app once and the page keeps none, a refusal is drawn, and curl is copied with variables.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

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
      get: { parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }, { name: "expand", in: "query", schema: { type: "string" } }, { name: "limit", in: "query", schema: { type: "integer", default: 20 } }], responses: { "200": { description: "OK" } } },
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

test("each field stands in one row with its label: the name, a query row's tick, then the value", async () => {
  // The real app drew labels a row off their fields (2026-11-22): a stray cell in the shared grid moved every pair
  // after it. A row is one element now, so a field's label is in its own row whatever stands above it.
  await openRequest("get /orders/{id}");
  const grid = target.querySelector(".tf-oas-req .tf-oas-grid")!;
  for (const child of grid.children) expect(child.matches(".tf-oas-prow, .tf-oas-sect, .tf-oas-hint"), child.outerHTML.slice(0, 80)).toBe(true);
  const rows = [...grid.querySelectorAll<HTMLElement>(":scope > .tf-oas-prow")];
  const fields = [...grid.querySelectorAll<HTMLElement>("input:not([type=checkbox]), select, .tf-oas-row2")];
  expect(fields.length).toBeGreaterThanOrEqual(4);
  for (const field of fields) {
    const row = field.closest(".tf-oas-prow")!;
    expect(rows, field.id).toContain(row);
    // Two cells, the label first; the label names its own field.
    expect(row.children).toHaveLength(2);
    const [label, value] = [...row.children];
    expect(label!.matches("label, .tf-oas-k")).toBe(true);
    expect(value!.contains(field)).toBe(true);
    if (label instanceof HTMLLabelElement && field.id !== "") expect(label.htmlFor).toBe(field.id);
  }
  const byName = (name: string) => rows.find((row) => text(row.firstElementChild) === name)!;
  // The path's `id` and the query's `expand`, each beside its own field, the query row's tick before its name.
  expect(byName("id").querySelector<HTMLInputElement>("input.tf-oas-inp")!.id).toMatch(/-path-id$/);
  const expand = byName("expand");
  expect(expand.querySelector<HTMLInputElement>("input.tf-oas-inp")!.id).toMatch(/-query-expand$/);
  expect(expand.firstElementChild!.firstElementChild!.matches("input[type=checkbox]")).toBe(true);
  expect(expand.querySelector("input[type=checkbox]")!.getAttribute("aria-label")).toBe("Send expand");
  // A row lays its two cells on the grid's own columns (jsdom draws nothing, so the rule is read).
  const css = readFileSync(join(process.cwd(), "src/lib/tf/css/openapi.css"), "utf8");
  expect(css).toMatch(/\n\.tf-oas-prow \{ grid-column: 1 \/ -1; display: grid; grid-template-columns: subgrid;/);
  expect(css).toMatch(/\n\.tf-oas-grid \{ display: grid; grid-template-columns: [^;]+ minmax\(0, 1fr\);/);
});

test("a query row the spec fills starts unticked and in second ink, and a tick or a key sends it", async () => {
  // Fable's ruling (2026-11-22, option A): the design's `on: required`; the value only shows what one looks like.
  await openRequest("get /orders/{id}");
  const line = () => [...target.querySelectorAll(".tf-oas-line > span")].map((e) => text(e)).join(" ");
  const row = () => target.querySelector<HTMLInputElement>('[id$="-query-limit"]')!.closest(".tf-oas-prow")!;
  const tick = () => row().querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(row().querySelector<HTMLInputElement>("input.tf-oas-inp")!.value).toBe("20");
  expect(tick().checked).toBe(false);
  expect(row().classList.contains("tf-oas-off")).toBe(true);
  expect(line()).toBe("GET https://shop.example.com/api/orders/{id}");
  tick().click();
  flushSync();
  expect(row().classList.contains("tf-oas-off")).toBe(false);
  expect(line()).toBe("GET https://shop.example.com/api/orders/{id}?limit=20");
  tick().click();
  flushSync();
  expect(row().classList.contains("tf-oas-off")).toBe(true);
  input(row().querySelector<HTMLInputElement>("input.tf-oas-inp")!, "50");
  expect(tick().checked).toBe(true);
  expect(row().classList.contains("tf-oas-off")).toBe(false);
  expect(line()).toBe("GET https://shop.example.com/api/orders/{id}?limit=50");
  // The rule draws it: an off row's value in second ink.
  expect(readFileSync(join(process.cwd(), "src/lib/tf/css/openapi.css"), "utf8")).toContain(".tf-oas-prow.tf-oas-off > .tf-oas-inp { color: var(--ink-2); }");
});

test("an empty path value says required, and is marked once a send is asked for", async () => {
  await openRequest("get /orders/{id}");
  const id = () => target.querySelector<HTMLInputElement>('[id$="-path-id"]')!;
  expect(id().placeholder).toBe("required");
  expect(id().classList.contains("tf-oas-missing")).toBe(false);
  expect(text(target.querySelector(".tf-oas-send"))).toContain("Fill in id.");
  target.querySelector(".tf-oas")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }));
  await settle();
  expect(calls).toEqual([]);
  expect(id().classList.contains("tf-oas-missing")).toBe(true);
  expect(id().getAttribute("aria-invalid")).toBe("true");
  input(id(), "7");
  expect(id().classList.contains("tf-oas-missing")).toBe(false);
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
