/**
 * The API explorer (ux31) on this thread's engine: the pane and its columns, the rail by tag, path and schema, the
 * overview, an operation's page, a schema's page, a ref followed and *Back to*, picks that replace and links that
 * push, the filter and its keys, refs into other files read only when asked, a broken spec, places an address
 * names, and 5,000 operations drawn as a window.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfOpenApi from "./TfOpenApi.svelte";
import { columnsFor, columnsSeen, paneHeight } from "./explorer/layout";
import { localEngine, type SpecEngine } from "../../render/openapi-port";
import { installFindKey } from "../filter-keys";
import { FuzzyOrder } from "../filter/ranked.svelte";
import { columnHolders, firstRows } from "./oas.svelte";
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
/** The names of the property rows drawn (not those that only hold the columns' widths until drawn). */
const names = (within: ParentNode = page()) => [...within.querySelectorAll(":scope .tf-oas-f .tf-oas-fname")].map((el) => text(el));
/** The rows of a schema's page not drawn yet: unseen, holding the columns' widths. */
const held = (within: ParentNode = page()) => [...within.querySelectorAll(":scope .tf-oas-fheld .tf-oas-fname")].map((el) => text(el));
/** Microtasks only, no timer and no frame: what the press's own task drew. */
async function microtasks() {
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
  flushSync();
}
/** Waits, frame by frame, until a schema's page has drawn its last property. */
async function drawnWhole() {
  for (let i = 0; i < 100 && page().querySelector(".tf-oas-fheld") !== null; i += 1) {
    await new Promise((resolve) => requestAnimationFrame(resolve));
    flushSync();
  }
  expect(page().querySelector(".tf-oas-fheld")).toBeNull();
}

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
  row("tag1").click();
  flushSync();
  row("tag0").click();
  flushSync();
  expect(row("tag0").getAttribute("aria-expanded")).toBe("true");
  expect(rail().length).toBeLessThan(200);
});

test("an address naming an operation in a large spec opens the fold that holds it, the pill on its row", async () => {
  draw("big.yaml", bigSpec(5000), { at: "get /resource3/{id}" });
  await settle();
  await vi.waitFor(() => expect(target.querySelector('.tf-oas-row[aria-current="true"]')?.getAttribute("title")).toMatch(/^GET \/resource3\/\{id\}/));
  expect(row("tag3").getAttribute("aria-expanded")).toBe("true");
  expect(row("tag0").getAttribute("aria-expanded")).toBe("false");
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
  draw("api/openapi.yaml", PETS_30_YAML, { onplace: (p: string, how: string, label: string) => places.push(`${how} ${p}${how === "push" ? ` from ${label}` : ""}`) });
  await settle();
  row("GET /pets/{petId}").click();
  await settle();
  page().scrollTop = 120;
  button("Pet", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Pet");
  // The address names the place left: the crumb's Esc says Back to it.
  expect(places.at(-1)).toBe("push schemas/Pet from GET /pets/{petId}");
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
  await drawnWhole();
  expect(names().length).toBe(80);
  button("Show the other 1 properties", page())!.click();
  flushSync();
  expect(names().length).toBe(81);
});

/** A schema as TeamCity's `build` is: 86 properties, each with its words, and an operation that names it. */
function wideSpec(): string {
  const properties: Record<string, unknown> = {};
  for (let i = 0; i < 86; i++) properties[i === 40 ? "aVeryLongPropertyNameThatSetsTheColumn" : `p${i}`] = i === 85 ? { type: "string", readOnly: true, description: "The last." } : { type: "string", description: `What property ${i} holds, in a sentence or two of the spec's own words.` };
  return JSON.stringify({
    openapi: "3.0.3",
    info: { title: "W", version: "1" },
    paths: { "/builds/{id}": { get: { responses: { "200": { description: "OK", content: { "application/json": { schema: { $ref: "#/components/schemas/Build" } } } } } } } },
    components: { schemas: { Build: { type: "object", required: ["p85"], properties }, Other: { type: "object", properties: { build: { $ref: "#/components/schemas/Build" } } } } },
  });
}

test("a followed ref's schema draws a screenful in the press's own frame, the rest after it, its columns held from the first", async () => {
  draw("w.json", wideSpec(), { at: "get /builds/{id}" });
  await settle();
  button("Build", page())!.click();
  await microtasks();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Build");
  expect(page().querySelector(".tf-oas-back")).not.toBeNull();
  // The ux30 end gate: 86 rows drawn whole in the press's frame took 82 ms at 4x. A screenful is drawn, not all.
  const first = names();
  expect(first.length).toBeGreaterThanOrEqual(8);
  expect(first.length).toBeLessThan(40);
  // Of the rows not drawn, those that can widen a column stand already, unseen: the longest name, the one row with
  // notes and *required* (p85 lies past the 80 drawn, so it is not one).
  expect(held()).toEqual(["aVeryLongPropertyNameThatSetsTheColumn"]);
  // What stands below the properties waits for the last of them.
  expect(text(page())).not.toContain("Used by");
  expect(text(page())).not.toContain("Show the other");
  await drawnWhole();
  expect(names().length).toBe(80);
  expect(names().slice(0, first.length)).toEqual(first);
  expect(text(page())).toContain("Used by 1");
  expect(text(page())).toContain("Show the other 6 properties");
});

test("the rows that can widen a list's columns: the longest name, each set of notes, a required, a name past ASCII", () => {
  const p = (name: string, node: Record<string, unknown> = {}, required = false) => ({ name, node, required, words: "" });
  const props = [p("a"), p("longest-name"), p("b", { readOnly: true }), p("c", { readOnly: true }), p("d", {}, true), p("e", {}, true), p("f", { readOnly: true, deprecated: true }), p("naïve"), p("g")];
  expect(columnHolders(props).map((x) => x.name)).toEqual(["longest-name", "b", "d", "f", "naïve"]);
  expect(columnHolders([])).toEqual([]);
});

test("a schema's first frame draws the rows a window's height holds, by a guess at each, never under 8", () => {
  const p = (words: string) => ({ name: "x", node: { type: "string" }, required: false, words });
  expect(firstRows(Array.from({ length: 200 }, () => p("")), 868)).toBe(28);
  expect(firstRows(Array.from({ length: 200 }, () => p("w".repeat(250))), 868)).toBe(10);
  expect(firstRows(Array.from({ length: 200 }, () => p("w".repeat(250))), 300)).toBe(8);
  expect(firstRows([p(""), p("")], 868)).toBe(2);
});

test("a schema's page gone back to at a scroll draws whole at once, so the scroll is there to go back to", async () => {
  draw("w.json", wideSpec(), { at: "schemas/Other" });
  await settle();
  button("Build", page())!.click();
  await settle();
  await drawnWhole();
  page().scrollTop = 900;
  button("Other", page())!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Other");
  button("Back to Build", page())!.click();
  await microtasks();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Build");
  expect(page().querySelector(".tf-oas-fheld")).toBeNull();
  expect(names().length).toBe(80);
  expect(page().scrollTop).toBe(900);
});

test("a spec's open builds no list for the filter: the 5,000 rows are read once words are typed", async () => {
  const asked: number[] = [];
  const ask = vi.spyOn(FuzzyOrder.prototype, "ask").mockImplementation(function (_text: string, rows: readonly unknown[]) {
    asked.push(rows.length);
  });
  draw("api/big.yaml", bigSpec(5000));
  await settle();
  expect(rail().length).toBeGreaterThan(0);
  expect(asked.length).toBeGreaterThan(0);
  expect(Math.max(...asked)).toBe(0);
  type("resource12");
  expect(asked.at(-1)).toBe(5000 + 1250);
  ask.mockRestore();
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
  // The path marks the matched letters at their place in it, past the method the ranking also read (#2520, #2532).
  expect([...rail()[0]!.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["resource7"]);
  type("");
  await settle();
  expect(list.scrollTop).toBe(84);
  // The rail is windowed: back at its top, the fold opened before the filter is open still.
  list.scrollTop = 0;
  list.dispatchEvent(new Event("scroll"));
  // The window draws its rows over a frame or two; wait for them, not for a count of ticks.
  await vi.waitFor(() => {
    flushSync();
    expect(row("tag3")?.getAttribute("aria-expanded")).toBe("true");
  });
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
  // Realistic and in its schema's shape: an integer id, a pet's name (an identifier: a person's only under a user or contact), the first of the enum, the cycle closed.
  expect(Object.keys(made[0]!)).toEqual(["id", "name", "status", "parent", "owner"]);
  expect(Number.isInteger(made[0]!.id)).toBe(true);
  expect(made[0]!.name).toMatch(/^[a-z0-9]+(-[a-z0-9]+)+$/);
  expect(made[0]!.status).toBe("available");
  expect(made[0]!.parent).toBeNull();
  const limit = [...page().querySelectorAll<HTMLElement>(".tf-oas-f")].find((f) => text(f.querySelector(".tf-oas-fname")) === "limit")!;
  // A parameter's example reads its name too: `limit` is a count.
  expect(text(limit)).toMatch(/Example \d+ made from the schema/);
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

test("a file view with no width, a page kept hidden, says nothing of its columns", () => {
  expect(columnsSeen(0)).toBeNull();
  expect(columnsSeen(1)).toBe(2);
  expect(columnsSeen(2560)).toBe(4);
});

test("Part of renders distinct parent schema chips with a Show all / Show fewer toggle", async () => {
  const schemas: Record<string, unknown> = {
    Shared: { type: "object", properties: { id: { type: "string" } } },
  };
  for (let i = 0; i < 16; i++) {
    schemas[`Holder${i}`] = { type: "object", properties: { shared: { $ref: "#/components/schemas/Shared" } } };
  }
  const spec = JSON.stringify({ openapi: "3.0.3", info: { title: "Holders", version: "1" }, paths: {}, components: { schemas } });
  draw("h.json", spec, { at: "schemas/Shared" });
  await settle();
  await drawnWhole();

  const chips = () => [...page().querySelectorAll<HTMLButtonElement>(".tf-oas-chips .tf-oas-part .tf-chipbtn")];
  expect(text(page())).toContain("Part of 16");
  expect(chips().map((b) => text(b))).toHaveLength(12);
  expect(chips()[0]!.textContent?.trim()).toBe("Holder0");
  expect(text(page())).not.toContain("and 4 more");

  button("Show all", page())!.click();
  flushSync();
  expect(chips()).toHaveLength(16);
  expect(button("Show fewer", page())).toBeDefined();

  button("Show fewer", page())!.click();
  flushSync();
  expect(chips()).toHaveLength(12);

  chips()[0]!.click();
  await settle();
  expect(text(page().querySelector(".tf-oas-h"))).toBe("Holder0");
});

test("schema markdown tables expand into available width without mid-word breaks or oversized typography", () => {
  const css = readFileSync(join(process.cwd(), "src/lib/tf/css/openapi.css"), "utf8");
  expect(css).toMatch(/\.tf-oas-desc:has\(table\),\s*\.tf-oas-fd:has\(table\)\s*\{\s*max-width:\s*none;/);
  expect(css).toMatch(/\.tf-oas-prose \.tf-md table\s*\{[^}]*font-size:\s*calc\(var\(--t-sm\) \* var\(--fs\)\)/);
  expect(css).toMatch(/\.tf-oas-prose \.tf-md table\s*\{[^}]*overflow-wrap:\s*break-word/);
  expect(css).toMatch(/\.tf-oas-fd \.tf-md :is\(th, td\) code\s*\{[^}]*overflow-wrap:\s*break-word/);
});

test("Alt+click on an OpenAPI Paths rail folder recursively expands and collapses nested path folders", async () => {
  const nestedSpec = JSON.stringify({
    openapi: "3.0.3",
    info: { title: "Nested", version: "1.0" },
    paths: {
      "/v1/items": { get: { responses: { "200": { description: "ok" } } } },
      "/v1/items/{id}/sub": { get: { responses: { "200": { description: "ok" } } } },
      "/v1/items/{id}/sub/leaf": { get: { responses: { "200": { description: "ok" } } } },
      "/v1/orders": { get: { responses: { "200": { description: "ok" } } } },
    },
  });
  draw("nested.json", nestedSpec);
  await settle();
  button("Paths", target.querySelector(".tf-oas-rail")!)!.click();
  flushSync();
  const dirRow = (label: string) => rail().find((b) => text(b).startsWith(label))!;
  // Ensure /v1 starts collapsed, then Alt+click expands /v1 and all nested path folders at once.
  if (dirRow("/v1").getAttribute("aria-expanded") === "true") {
    dirRow("/v1").click();
    flushSync();
  }
  dirRow("/v1").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirRow("/v1").getAttribute("aria-expanded")).toBe("true");
  expect(dirRow("/items").getAttribute("aria-expanded")).toBe("true");
  expect(dirRow("/{id}/sub").getAttribute("aria-expanded")).toBe("true");

  // Alt+click again collapses /v1 and every nested folder under it.
  dirRow("/v1").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirRow("/v1").getAttribute("aria-expanded")).toBe("false");
  dirRow("/v1").click();
  flushSync();
  expect(dirRow("/items").getAttribute("aria-expanded")).toBe("false");
});
