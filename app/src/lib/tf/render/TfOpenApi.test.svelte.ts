/**
 * The OpenAPI view (ux18) on this thread's engine: the head, the endpoints by tag, the filter, an operation opened,
 * a ref followed and *Back to*, a ref cycle that does not open again, refs into other files read only when asked
 * and refs to URLs never, a broken spec, a place an address names, and 5,000 operations drawn as a window.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfOpenApi from "./TfOpenApi.svelte";
import { localEngine } from "../../render/openapi-port";
import { BROKEN_YAML, ORDER_YAML, PETS_30_YAML, SPLIT_31_YAML, STORE_20_JSON, bigSpec } from "../../render/openapi-fixture";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const scrolled: string[] = [];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  scrolled.length = 0;
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push((this as HTMLElement).dataset["k"] ?? this.tagName);
  };
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
const opRows = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-opbtn")].map((b) => text(b));
const opButton = (label: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-opbtn")].find((b) => text(b).startsWith(label))!;
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);
const detail = () => target.querySelector(".tf-oas-detail");
const props = (root: Element | null) => [...(root?.querySelectorAll(":scope .tf-oas-pname") ?? [])].map((el) => text(el));

function type(value: string) {
  const box = target.querySelector<HTMLInputElement>(".tf-oas-band input")!;
  box.value = value;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("a spec opens on its head and its endpoints grouped by tag, every row its method, path and summary", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Pet Store");
  expect(text(target.querySelector(".tf-oas-vers"))).toBe("1.4.0 OpenAPI 3.0.3");
  expect(text(target.querySelector(".tf-oas-servers dd"))).toBe("https://api.example.com/v1Production");
  expect(text(target.querySelector(".tf-oas-count"))).toBe("6 endpoints");
  expect([...target.querySelectorAll(".tf-oas-group h3")].map((h) => text(h))).toEqual(["pets", "store", "/health", "Schemas"]);
  expect(opRows().slice(0, 4)).toEqual(["GET /pets List all pets", "POST /pets Create a pet", "GET /pets/{petId} Info for a specific pet", "DELETE /pets/{petId} Remove a pet Deprecated"]);
  // A path's parameters are dim; nothing here can call the API.
  expect(text(opButton("GET /pets/{petId}").querySelector(".tf-oas-pp"))).toBe("{petId}");
  expect(target.textContent).not.toMatch(/Try it out|Execute/);
});

test("an endpoint opens in place: its parameters, and its response's schema by ref, read whole the first frame", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  opButton("GET /pets/{petId}").click();
  await settle();
  expect(opButton("GET /pets/{petId}").getAttribute("aria-expanded")).toBe("true");
  const open = detail()!;
  expect(text(open.querySelector(".tf-oas-opid"))).toBe("showPetById");
  expect(text(open.querySelector(".tf-oas-param"))).toBe("petId required path string");
  expect(text(open.querySelector(".tf-oas-status"))).toBe("200");
  expect(text(open.querySelector(".tf-oas-mtype"))).toBe("application/json");
  // The response's schema is a ref to Pet: its name links, its properties are drawn under it.
  expect(text(open.querySelector(".tf-oas-resp .tf-oas-top"))).toBe("Pet");
  expect(props(open.querySelector(".tf-oas-resp"))).toEqual(["id", "name", "status", "parent", "owner"]);
  expect(text(open.querySelector(".tf-oas-resp .tf-oas-enum"))).toBe("One of available sold");
  opButton("GET /pets/{petId}").click();
  flushSync();
  expect(detail()).toBeNull();
});

test("a ref's name follows to its schema, and Back to returns to the endpoint it was followed from", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  opButton("GET /pets/{petId}").click();
  await settle();
  const pet = detail()!.querySelector<HTMLButtonElement>(".tf-oas-top .tf-oas-ref")!;
  expect(pet.title).toBe("Go to Pet in Schemas");
  pet.click();
  await settle();
  const schema = [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-sbtn")].find((b) => text(b) === "Pet")!;
  expect(schema.getAttribute("aria-expanded")).toBe("true");
  expect(scrolled).toContain("s:Pet");
  expect(document.activeElement).toBe(schema);
  const back = button("Back to GET /pets/{petId}");
  expect(back).toBeDefined();
  back!.click();
  flushSync();
  expect(button("Back to GET /pets/{petId}")).toBeUndefined();
  expect(document.activeElement?.closest("[data-k]")?.getAttribute("data-k")).toMatch(/^do0:2$/);
});

test("a ref cycle opens one level and no further: the schema that holds itself is a link, not a caret", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-sbtn")].find((b) => text(b) === "Pet")!.click();
  await settle();
  const body = target.querySelector(".tf-oas-detail")!;
  const line = (name: string) => [...body.querySelectorAll(".tf-oas-pline")].find((l) => text(l.querySelector(".tf-oas-pname")) === name)!;
  // `parent` is Pet itself: no caret, its name follows.
  expect(line("parent").querySelector("button.tf-oas-caret")).toBeNull();
  expect(text(line("parent").querySelector(".tf-oas-ref"))).toBe("Pet");
  // `owner` opens Owner; Owner's `pets` are Pet again, which it sits inside: no caret there either.
  line("owner").querySelector<HTMLButtonElement>("button.tf-oas-caret")!.click();
  await settle();
  const pets = [...body.querySelectorAll(".tf-oas-pline")].find((l) => text(l.querySelector(".tf-oas-pname")) === "pets")!;
  expect(text(pets.querySelector(".tf-oas-type"))).toBe("array of Pet");
  expect(pets.querySelector("button.tf-oas-caret")).toBeNull();
});

test("the filter keeps the endpoints its words find and says how many of all; Esc clears it", async () => {
  draw("api/openapi.yaml", PETS_30_YAML);
  await settle();
  type("delete");
  expect(opRows()).toEqual(["DELETE /pets/{petId} Remove a pet Deprecated"]);
  expect(text(target.querySelector(".tf-oas-count"))).toBe("1 of 6");
  type("nothing like this");
  expect(text(target.querySelector(".tf-oas-none"))).toBe("No endpoint or schema matches the filter.");
  const box = target.querySelector<HTMLInputElement>(".tf-oas-band input")!;
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  expect(box.value).toBe("");
  expect(opRows()).toHaveLength(9);
});

test("Swagger 2.0 in JSON: the body parameter is drawn as the request body, its schema's properties under it", async () => {
  draw("swagger.json", STORE_20_JSON);
  await settle();
  expect(text(target.querySelector(".tf-oas-vers"))).toBe("0.9 Swagger 2.0");
  opButton("POST /orders").click();
  await settle();
  const body = [...detail()!.querySelectorAll(".tf-oas-sec")].find((s) => text(s.querySelector("h4")).startsWith("Request body"))!;
  expect(text(body.querySelector("h4"))).toBe("Request bodyrequired");
  expect(props(body)).toEqual(["id", "qty"]);
  expect(text(detail()!.querySelector(".tf-oas-param"))).toBe("X-Trace header string");
});

test("a ref into another file is read only when asked, at the spec's commit through the read it was given; a URL never", async () => {
  const reads: string[] = [];
  const read = vi.fn(async (path: string) => {
    reads.push(path);
    return new TextEncoder().encode(ORDER_YAML);
  });
  draw("api/spec.yaml", SPLIT_31_YAML, { read, fileHref: (file: string, place: string | null) => `#file/${file}/${place ?? ""}` });
  await settle();
  opButton("GET /orders").click();
  await settle();
  expect(reads).toEqual([]);
  // The name links to the file on the Code page (a file of schemas, not a spec: no place in it).
  expect(detail()!.querySelector<HTMLAnchorElement>(".tf-oas-top a.tf-oas-ref")!.getAttribute("href")).toBe("#file/api/schemas/order.yaml/");
  button("Show it here")!.click();
  await settle();
  expect(reads).toEqual(["api/schemas/order.yaml"]);
  expect(props(detail())).toEqual(["id", "lines"]);
  expect(text([...detail()!.querySelectorAll(".tf-oas-type")].find((t) => text(t).startsWith("string")))).toBe("string, or null");

  opButton("GET /orders").click();
  opButton("GET /far").click();
  await settle();
  const far = detail()!.querySelector<HTMLElement>(".tf-oas-far")!;
  expect(text(far)).toBe("far.json");
  expect(far.title).toBe("https://example.com/schemas/far.json: outside the repository, not fetched");
  expect(button("Show it here")).toBeUndefined();
  expect(reads).toEqual(["api/schemas/order.yaml"]);
});

test("a path item in another file is listed by its path and read when opened", async () => {
  const read = async () => new TextEncoder().encode("get:\n  summary: List items\n  responses:\n    '200':\n      description: Items\n");
  draw("api/spec.yaml", SPLIT_31_YAML, { read });
  await settle();
  opButton("… /items In ./paths/items.yaml").click();
  await settle();
  expect(text(detail()!.querySelector(".tf-oas-subhead"))).toBe("GET /items List items");
  expect(text(detail()!.querySelector(".tf-oas-rdesc"))).toBe("Items");
});

test("a spec that does not parse says why and on which line, and offers its source", async () => {
  const onsource = vi.fn();
  draw("broken.yaml", BROKEN_YAML, { onsource });
  await settle();
  const bad = target.querySelector('[role="alert"]')!;
  expect(text(bad.querySelector("b"))).toBe("This spec could not be read.");
  expect(text(bad)).toContain("Line 7.");
  button("Show source")!.click();
  expect(onsource).toHaveBeenCalledOnce();
});

test("an address that names an operation opens the spec there; one that names a schema opens that", async () => {
  draw("api/openapi.yaml", PETS_30_YAML, { at: "delete /pets/{petId}" });
  await settle();
  expect(opButton("DELETE /pets/{petId}").getAttribute("aria-expanded")).toBe("true");
  expect(scrolled).toContain("o0:3");
  unmount(view!);
  view = null;
  draw("api/openapi.yaml", PETS_30_YAML, { at: "schemas/Owner" });
  await settle();
  expect([...target.querySelectorAll<HTMLButtonElement>(".tf-oas-sbtn")].find((b) => text(b) === "Owner")!.getAttribute("aria-expanded")).toBe("true");
});

test("Copy link names the operation's place", async () => {
  const copied: string[] = [];
  draw("api/openapi.yaml", PETS_30_YAML, { oncopylink: (place: string, label: string) => copied.push(`${place}|${label}`) });
  await settle();
  opButton("POST /pets").click();
  await settle();
  button("Copy link")!.click();
  expect(copied).toEqual(["post /pets|POST /pets"]);
});

test("5,000 operations draw a window of rows, and the filter's count reads every one", async () => {
  draw("big.yaml", bigSpec(5000));
  await settle();
  expect(text(target.querySelector(".tf-oas-count"))).toBe("5000 endpoints");
  const drawn = target.querySelectorAll(".tf-oas-opbtn").length;
  expect(drawn).toBeGreaterThan(0);
  expect(drawn).toBeLessThan(200);
  type("delete resource4999");
  expect(text(target.querySelector(".tf-oas-count"))).toBe("0 of 5000");
  type("delete resource1249");
  expect(opRows()).toEqual(["DELETE /resource1249/{id} delete resource 1249"]);
});
