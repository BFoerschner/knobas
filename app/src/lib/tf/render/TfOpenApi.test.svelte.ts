/**
 * The API explorer (ux31) on this thread's engine: the pane and its columns, the rail by tag, path and schema, the
 * overview, an operation's page, a schema's page, a ref followed and *Back to*, picks that replace and links that
 * push, the filter and its keys, refs into other files read only when asked, a broken spec, places an address
 * names, and 5,000 operations drawn as a window.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfOpenApi from "./TfOpenApi.svelte";
import { columnsFor, paneHeight } from "./oas-layout";
import { localEngine, type SpecEngine } from "../../render/openapi-port";
import { installFindKey } from "../filter-keys";
import { BROKEN_YAML, DEEP_30_JSON, FORM_20_JSON, MODEL_31_JSON, ORDER_YAML, PETS_30_YAML, SPLIT_31_YAML, bigSpec } from "../../render/openapi-fixture";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  Element.prototype.scrollBy = function () {} as Element["scrollBy"];
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(path: string, text: string, extra: Record<string, unknown> = {}) {
  view = mount(TfOpenApi, { target, props: { path, text, engine: localEngine(), ...extra } });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 8; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rail = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-list .tf-oas-row")];
const railText = () => rail().map((b) => text(b));
const row = (label: string) => rail().find((b) => text(b) === label || text(b).startsWith(`${label} `))!;
const page = () => target.querySelector(".tf-oas-page")!;
const button = (label: string, within: ParentNode = target) => [...within.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);
const names = (within: ParentNode = page()) => [...within.querySelectorAll(":scope .tf-oas-fname")].map((el) => text(el));

function type(value: string) {
  const box = target.querySelector<HTMLInputElement>(".tf-oas-railhead input")!;
  box.value = value;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function key(el: Element, k: string, more: KeyboardEventInit = {}) {
  el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...more }));
  flushSync();
}

test("the pane holds two columns under 1,480 px, a request column from 1,480 and an answer's from 2,500", () => {
  expect(columnsFor(1479)).toBe(2);
  expect(columnsFor(1480)).toBe(3);
  expect(columnsFor(2499)).toBe(3);
  expect(columnsFor(2500)).toBe(4);
  // The window's height, less the box's top at scroll 0, the status bar and 14 px; never under 380.
  expect(paneHeight(868, 290, 24)).toBe(868 - 290 - 24 - 14);
  expect(paneHeight(600, 290, 24)).toBe(380);
});

test("the pane's height is set at mount and on resize, never on scroll", async () => {
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(900);
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  const box = target.querySelector<HTMLElement>(".tf-oas")!;
  expect(box.style.getPropertyValue("--oas-h")).toBe(`${900 - 14}px`);
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(700);
  window.dispatchEvent(new Event("scroll"));
  expect(box.style.getPropertyValue("--oas-h")).toBe(`${900 - 14}px`);
  window.dispatchEvent(new Event("resize"));
  expect(box.style.getPropertyValue("--oas-h")).toBe(`${700 - 14}px`);
  vi.restoreAllMocks();
});

test("a spec opens on its overview, and a small one's rail has every fold open", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Pet Store");
  expect(text(page().querySelector(".tf-oas-meta"))).toContain("6 endpoints in 3 groups, 3 schemas");
  expect(text(target.querySelector(".tf-oas-count"))).toBe("6 endpoints");
  expect(railText()).toEqual([
    "Overview",
    "pets 4",
    "GET /pets",
    "POST /pets",
    "GET /pets/{petId}",
    "DEL /pets/{petId}",
    "store 1",
    "GET /store/inventory",
    "/health 1",
    "GET /health",
  ]);
  expect(row("Overview").getAttribute("aria-current")).toBe("true");
  // Nothing on the page calls the API: the server is text.
  expect(names().slice(0, 1)).toEqual(["https://api.example.com/v1"]);
});

test("a large spec opens with every fold shut: its first screen is the table of contents", async () => {
  draw("big.yaml", bigSpec(5000));
  await settle();
  expect(rail().length).toBeLessThan(30);
  expect(railText()[0]).toBe("Overview");
  expect(railText()[1]).toBe("tag0 200");
  expect(text(target.querySelector(".tf-oas-count"))).toBe("5,000 endpoints");
  // Two folds of 200 opened draw a window of their rows, not 400.
  row("tag0").click();
  flushSync();
  row("tag1").click();
  flushSync();
  expect(row("tag0").getAttribute("aria-expanded")).toBe("true");
  expect(rail().length).toBeLessThan(200);
});

test("a pick in the rail shows the operation, replaces the place, and its parameters stand in the order path, query, header", async () => {
  const places: string[] = [];
  const spec = {
    openapi: "3.0.3",
    info: { title: "Order", version: "1" },
    paths: {
      "/a/{id}": {
        get: {
          parameters: [
            { name: "X-Trace", in: "header", schema: { type: "string" } },
            { name: "q", in: "query", schema: { type: "string" } },
            { name: "id", in: "path", required: true, schema: { type: "string" } },
          ],
          responses: { "200": { description: "OK" } },
        },
      },
    },
  };
  draw("o.json", JSON.stringify(spec), { onplace: (p: string, how: string) => places.push(`${how} ${p}`) });
  await settle();
  row("GET /a/{id}").click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /a/{id}");
  expect(names()).toEqual(["id", "q", "X-Trace"]);
  expect(places).toEqual(["replace get /a/{id}"]);
  expect(row("GET /a/{id}").getAttribute("aria-current")).toBe("true");
});

test("the pick's pill is drawn before the worker answers; the page swaps whole when it does", async () => {
  const local = localEngine();
  let hold: (() => void) | null = null;
  const engine: SpecEngine = async (session, call) => {
    if (call.kind === "page") await new Promise<void>((go) => (hold = go));
    return local(session, call);
  };
  draw("api/openapi.yaml", PETS_30_YAML, { engine });
  await settle();
  row("GET /health").click();
  flushSync();
  expect(row("GET /health").getAttribute("aria-current")).toBe("true");
  // The old page stands until the answer is in.
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Pet Store");
  hold!();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /health");
});

test("a page is one worker call, and an operation visited again asks nothing", async () => {
  const local = localEngine();
  const calls: string[] = [];
  const engine: SpecEngine = (session, call) => {
    calls.push(call.kind);
    return local(session, call);
  };
  draw("api/openapi.yaml", PETS_30_YAML, { engine });
  await settle();
  calls.length = 0;
  row("POST /pets").click();
  await settle();
  expect(calls).toEqual(["page"]);
  row("GET /health").click();
  await settle();
  calls.length = 0;
  row("POST /pets").click();
  await settle();
  expect(calls).toEqual([]);
  expect(text(page().querySelector(".tf-oas-h"))).toBe("POST /pets");
});

test("a ref's name follows to the schema's page, which pushes the place; Back returns to the operation at its scroll", async () => {
  const places: string[] = [];
  draw("api/openapi.yaml", PETS_30_YAML, { onplace: (p: string, how: string) => places.push(`${how} ${p}`) });
  await settle();
  row("GET /pets/{petId}").click();
  await settle();
  page().scrollTop = 120;
  button("Pet", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Pet");
  expect(places.at(-1)).toBe("push schemas/Pet");
  expect(text(page().querySelector(".tf-oas-back"))).toBe("Back to GET /pets/{petId}");
  // The schema's page: merged properties, an example made from it, and who uses it.
  expect(names().slice(0, 5)).toEqual(["id", "name", "status", "parent", "owner"]);
  expect(text(page())).toContain("Example Made from the schema");
  expect(text(page())).toContain("Used by 3");
  expect(text(page())).toContain("Part of 1");
  // A followed name on the schema's page follows again, and Back steps back twice.
  button("Owner", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-back"))).toBe("Back to Pet");
  button("Back to Pet", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Pet");
  button("Back to GET /pets/{petId}", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /pets/{petId}");
  expect(page().scrollTop).toBe(120);
  expect(page().querySelector(".tf-oas-back")).toBeNull();
});

test("a pick in the rail after a followed ref leaves no way back", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  row("GET /pets/{petId}").click();
  await settle();
  button("Pet", page())!.click();
  await settle();
  expect(page().querySelector(".tf-oas-back")).not.toBeNull();
  row("GET /health").click();
  await settle();
  expect(page().querySelector(".tf-oas-back")).toBeNull();
});

test("a caret opens a schema in place, once its shape is in; a schema that holds itself says so and does not open", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  target.querySelectorAll<HTMLButtonElement>(".tf-oas-modes button")[2]!.click();
  flushSync();
  row("Pet").click();
  await settle();
  const owner = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "owner")!;
  // The whole row toggles, not just the caret.
  owner.querySelector<HTMLElement>(".tf-oas-fname")!.click();
  await settle();
  expect(names(owner.querySelector(".tf-oas-in")!)).toEqual(["name", "pets"]);
  const parent = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "parent")!;
  expect(parent.querySelector(".tf-oas-fr.tf-oas-can")).toBeNull();
  expect(text(parent)).toContain("It holds itself: follow Pet to read it.");
});

test("a schema of more than 80 properties draws 80 and Show the other", async () => {
  const properties: Record<string, unknown> = {};
  for (let i = 0; i < 81; i++) properties[`p${i}`] = { type: "string" };
  draw("w.json", JSON.stringify({ openapi: "3.0.3", info: { title: "W", version: "1" }, paths: {}, components: { schemas: { Wide: { type: "object", properties } } } }), { at: "schemas/Wide" });
  await settle();
  expect(names().length).toBe(80);
  button("Show the other 1 properties", page())!.click();
  flushSync();
  expect(names().length).toBe(81);
});

test("allOf is drawn merged and a oneOf is a choice the reader picks among", async () => {
  draw("p.json", DEEP_30_JSON, { at: "schemas/Charge" });
  await settle();
  expect(names().slice(0, 7)).toEqual(["id", "object", "amount", "customer", "description", "source", "parent"]);
  const source = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "source")!;
  // Stripe's `anyOf [X, ""]` reads as the schema, or empty.
  const description = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "description")!;
  expect(text(description.querySelector(".tf-oas-ftype"))).toBe("string, or empty");
  source.querySelector<HTMLElement>(".tf-oas-fr")!.click();
  await settle();
  // The choice heads the opened block in one line; the picked choice's rows follow in the same block.
  expect(text(source.querySelector(".tf-oas-in > .tf-oas-pick"))).toBe("One of CardBank told apart by object");
  expect([...source.querySelectorAll(".tf-oas-pick button")].map((b) => text(b))).toEqual(["Card", "Bank"]);
  expect(names(source.querySelector(".tf-oas-in")!)).toContain("last4");
  button("Bank", source)!.click();
  await settle();
  expect(names(source.querySelector(".tf-oas-in")!)).toContain("iban");
});

test("a body with two media types draws one and switches; a picked status survives a visit to another operation", async () => {
  draw("p.json", DEEP_30_JSON);
  await settle();
  row("POST /v1/charges").click();
  await settle();
  const body = [...page().querySelectorAll("section")].find((s) => text(s.querySelector("h3")).startsWith("Request body"))!;
  expect([...body.querySelectorAll(".tf-seg button")].map((b) => [text(b), b.getAttribute("aria-pressed")])).toEqual([
    ["application/x-www-form-urlencoded", "true"],
    ["application/json", "false"],
  ]);
  button("application/json", body)!.click();
  flushSync();
  expect(button("application/json", body)!.getAttribute("aria-pressed")).toBe("true");
  const statuses = () => [...page().querySelectorAll(".tf-oas-lab .tf-seg button")].map((b) => [text(b), b.getAttribute("aria-pressed")]);
  expect(statuses()).toEqual([
    ["200", "true"],
    ["default", "false"],
  ]);
  button("default", page())!.click();
  flushSync();
  row("GET /v1/charges").click();
  await settle();
  row("POST /v1/charges").click();
  await settle();
  expect(statuses()).toEqual([
    ["200", "false"],
    ["default", "true"],
  ]);
});

test("Stripe's shape: every path under /v1 and no tags groups by the part after it, each row its path from there", async () => {
  draw("p.json", DEEP_30_JSON);
  await settle();
  expect(railText().slice(0, 5)).toEqual(["Overview", "/v1/charges 3", "GET /v1/charges", "POST /v1/charges", "GET /v1/charges/{charge}"]);
});

test("the overview draws servers with variables, how to sign in in words, groups and About", async () => {
  draw("m.json", MODEL_31_JSON);
  await settle();
  const overview = text(page());
  expect(overview).toContain("{tenant}. Default acme. One of acme globex");
  const schemes = [...page().querySelectorAll("#tf-oas-signin .tf-oas-fl")].map((f) => [text(f.querySelector(".tf-oas-fname")), text(f.querySelector(".tf-oas-ftype"))]);
  expect(schemes).toEqual([
    ["bearer", "A bearer token (JWT)"],
    ["basic", "A user name and a password (Basic)"],
    ["key", "A key in the query api_key"],
    ["oauth", "OAuth 2.0: an access token, which you paste"],
    ["oidc", "OpenID Connect: an access token, which you paste"],
  ]);
  const about = [...page().querySelectorAll(".tf-oas-kv dt")].map((dt) => [text(dt), text(dt.nextElementSibling)]);
  expect(about).toEqual([
    ["Contact", "API team, api@example.test, https://example.test/team"],
    ["Licence", "MIT"],
    ["Terms", "https://example.test/terms"],
    ["More", "https://example.test/docs"],
  ]);
  // A webhook is listed in a fold of its own and drawn as an operation's page.
  row("POST thingMade").click();
  await settle();
  expect(text(page().querySelector(".tf-oas-meta"))).toContain("Webhook");
});

test("a 2.0 spec's servers and its formData as a form body", async () => {
  draw("f.json", FORM_20_JSON, { at: "post /pets/{id}/photo" });
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("POST /pets/{id}/photo");
  const body = [...page().querySelectorAll("section")].find((s) => text(s.querySelector("h3")).startsWith("Request body"))!;
  expect(text(body)).toContain("multipart/form-data");
  expect(names(body)).toEqual(["caption", "file"]);
});

test("the filter draws a flat list; clearing it returns the folds and the scroll", async () => {
  draw("big.yaml", bigSpec(400));
  await settle();
  row("tag3").click();
  flushSync();
  const list = target.querySelector<HTMLElement>(".tf-oas-list")!;
  list.scrollTop = 84;
  type("resource7");
  await settle();
  expect(railText().slice(0, 4)).toEqual(["GET /resource7/{id}", "PUT /resource7/{id}", "POST /resource7/{id}", "DEL /resource7/{id}"]);
  expect(text(target.querySelector(".tf-oas-count"))).toMatch(/^\d+ of 400$/);
  type("");
  await settle();
  expect(row("tag3").getAttribute("aria-expanded")).toBe("true");
  expect(list.scrollTop).toBe(84);
});

test("⌘F goes to the filter with its words selected; ↓ then Enter opens the first row", async () => {
  const off = installFindKey();
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  type("health");
  const box = target.querySelector<HTMLInputElement>(".tf-oas-railhead input")!;
  // jsdom lays nothing out: the box is drawn, as the ladder asks.
  box.getClientRects = () => [{}] as unknown as DOMRectList;
  box.blur();
  key(window as unknown as Element, "f", { metaKey: true });
  expect(document.activeElement).toBe(box);
  expect([box.selectionStart, box.selectionEnd]).toEqual([0, 6]);
  key(box, "ArrowDown");
  const list = target.querySelector<HTMLElement>(".tf-oas-list")!;
  expect(document.activeElement).toBe(list);
  key(list, "Enter");
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /health");
  off();
});

test("the rail's keys: arrows move, ← shuts a fold or goes to its head, → opens it", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  const list = target.querySelector<HTMLElement>(".tf-oas-list")!;
  list.focus();
  key(list, "ArrowDown");
  key(list, "ArrowDown");
  await settle();
  expect(text(target.querySelector(".tf-oas-cursor"))).toBe("GET /pets");
  key(list, "ArrowLeft");
  await settle();
  expect(text(target.querySelector(".tf-oas-cursor"))).toBe("pets 4");
  key(list, "ArrowLeft");
  expect(row("pets").getAttribute("aria-expanded")).toBe("false");
  key(list, "ArrowRight");
  expect(row("pets").getAttribute("aria-expanded")).toBe("true");
});

test("Paths lists a tree, a folder a part", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  target.querySelectorAll<HTMLButtonElement>(".tf-oas-modes button")[1]!.click();
  flushSync();
  // Three folders or fewer at the top are open; a deeper one is shut.
  expect(railText()).toEqual(["Overview", "/pets 4", "GET /pets", "POST /pets", "/{petId} 2", "/store/inventory 1", "GET /inventory", "/health 1", "GET /health"]);
});

test("an address names a place: an operation opens there; a place the spec lacks opens the overview and says so", async () => {
  draw("api/openapi.yaml", PETS_30_YAML, { at: "get /pets/{petId}" });
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /pets/{petId}");
  unmount(view!);
  view = null;
  draw("api/openapi.yaml", PETS_30_YAML, { at: "get /gone" });
  await settle();
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Pet Store");
  expect(text(page().querySelector(".tf-oas-gone"))).toBe("This link named get /gone, which this version of the file does not have.");
});

test("a tag's place opens its fold and shows the overview", async () => {
  draw("big.yaml", bigSpec(400), { at: "tags/tag3" });
  await settle();
  expect(row("tag3").getAttribute("aria-expanded")).toBe("true");
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Big");
});

test("a ref into another file is read only when its caret opens", async () => {
  const reads: string[] = [];
  draw("api/spec.yaml", SPLIT_31_YAML, {
    read: async (p: string) => {
      reads.push(p);
      return new TextEncoder().encode(ORDER_YAML);
    },
  });
  await settle();
  row("GET /orders").click();
  await settle();
  expect(reads).toEqual([]);
});

test("a spec that does not parse says why and on which line, and offers Source", async () => {
  let source = 0;
  draw("api/openapi.yaml", BROKEN_YAML, { onsource: () => source++ });
  await settle();
  expect(text(target.querySelector(".tf-oas-bad"))).toContain("This spec could not be read.");
  button("Show source")!.click();
  expect(source).toBe(1);
});

test("an example says where it comes from: the spec's own, or made from the schema for bodies, answers and parameters", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  row("POST /pets").click();
  await settle();
  const body = [...page().querySelectorAll("section")].find((s) => text(s.querySelector("h3")).startsWith("Request body"))!;
  expect(text(body.querySelector(".tf-oas-exfrom"))).toBe("From the spec");
  row("GET /pets").click();
  await settle();
  const answers = [...page().querySelectorAll("section")].find((s) => text(s.querySelector("h3")).startsWith("Responses"))!;
  expect(text(answers.querySelector(".tf-oas-exfrom"))).toBe("Made from the schema");
  const made = JSON.parse(text(answers.querySelector(".tf-oas-ex"))) as Record<string, unknown>[];
  // Realistic and in its schema's shape: an integer id, a person's name, the first of the enum, the cycle closed.
  expect(Object.keys(made[0]!)).toEqual(["id", "name", "status", "parent", "owner"]);
  expect(Number.isInteger(made[0]!.id)).toBe(true);
  expect(made[0]!.name).toMatch(/^\S+ \S+/);
  expect(made[0]!.status).toBe("available");
  expect(made[0]!.parent).toBeNull();
  const limit = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "limit")!;
  expect(text(limit)).toContain("Example 1 made from the schema");
});

test("a line an address names (a search hit's) opens the operation that holds it, and the address says so", async () => {
  const places: string[] = [];
  const line = PETS_30_YAML.split("\n").findIndex((l) => l.includes("How many to return")) + 1;
  draw("api/openapi.yaml", PETS_30_YAML, { line, onplace: (p: string, how: string) => places.push(`${how} ${p}`) });
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("GET /pets");
  expect(places).toEqual(["replace get /pets"]);
});

test("a schema's rows: the keys open, shut and move; Alt-click opens everything below; Expand all flips to Collapse all", async () => {
  draw("p.json", DEEP_30_JSON, { at: "schemas/Charge" });
  await settle();
  const rowOf = (name: string, within: ParentNode = page()) => [...within.querySelectorAll<HTMLElement>(".tf-oas-fr")].find((r) => text(r.querySelector(".tf-oas-fname")) === name)!;
  const source = rowOf("source");
  source.focus();
  key(source, "ArrowRight");
  await settle();
  expect(source.getAttribute("aria-expanded")).toBe("true");
  key(source, "ArrowRight");
  await settle();
  expect(text(document.activeElement?.querySelector(".tf-oas-fname"))).toBe("id");
  key(document.activeElement!, "ArrowLeft");
  expect(document.activeElement).toBe(source);
  key(source, "ArrowLeft");
  await settle();
  expect(source.getAttribute("aria-expanded")).toBe("false");
  key(source, "ArrowDown");
  expect(text(document.activeElement?.querySelector(".tf-oas-fname"))).toBe("parent");
  // Alt-click opens what the row holds and everything below it; again, shuts it all.
  const customer = rowOf("customer");
  customer.dispatchEvent(new MouseEvent("click", { bubbles: true, altKey: true }));
  await settle();
  expect(customer.getAttribute("aria-expanded")).toBe("true");
  // With anything open the Properties label's button says Collapse all, and shuts it all; then Expand all opens every row that opens.
  button("Collapse all", page())!.click();
  await settle();
  expect(page().querySelectorAll('.tf-oas-fr[aria-expanded="true"]').length).toBe(0);
  button("Expand all", page())!.click();
  await settle();
  expect(rowOf("source").getAttribute("aria-expanded")).toBe("true");
  expect(rowOf("customer").getAttribute("aria-expanded")).toBe("true");
  expect(button("Collapse all", page())).toBeDefined();
});

test("a caret stands only where the opened block draws a row", async () => {
  draw("p.json", DEEP_30_JSON, { at: "schemas/Charge" });
  await settle();
  const description = [...page().querySelectorAll<HTMLElement>(".tf-oas-fr")].find((r) => text(r.querySelector(".tf-oas-fname")) === "description")!;
  expect(description.classList.contains("tf-oas-can")).toBe(false);
});
