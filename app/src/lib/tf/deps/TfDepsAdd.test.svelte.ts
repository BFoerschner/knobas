/**
 * Adding in Dependencies (#876, R-11.20, R-11.21), mounted over stand-in
 * commands: *Add ▾* and *+ Add* open *New asset*, which sends `create_asset`
 * and then, from a ticket, the link; the new tile is read again, drawn and
 * flashed, and the toast offers *Edit it*. The route dialog creates, edits and
 * deletes a route. What the commands answer is `assets_ipc.rs`'.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetRow, AssetType, Dependencies, DepItem, DepViews, RouteRow } from "../../ipc/assets";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import { createDepViewState } from "./views.svelte";
import { settingsOf } from "./views";
import TfDeps from "./TfDeps.svelte";

function item(id: string, kind: string, name: string): DepItem {
  return { id, kind, name, health: "ok", state: null, monitors: 1, environment: null, owner: null };
}

const KINDS = [
  { id: "ticket", label: "Ticket", monogram: "TK" },
  { id: "route", label: "Route", monogram: "RO" },
  { id: "reverse_proxy", label: "Reverse proxy", monogram: "RP" },
  { id: "service", label: "Service", monogram: "SV" },
  { id: "vm", label: "VM", monogram: "VM" },
];
const LAYERS = [
  { id: "work", name: "Work", kinds: ["ticket"] },
  { id: "entry", name: "Entry points", kinds: ["route"] },
  { id: "services", name: "Services", kinds: ["service", "reverse_proxy"] },
  { id: "machines", name: "Machines", kinds: ["vm"] },
];
const BASE: DepItem[] = [
  item("jira:PAY-231", "ticket", "PAY-231"),
  item("asset:vm", "vm", "vm-pay-01"),
  item("asset:proxy", "reverse_proxy", "traefik"),
  item("route:pay", "route", "payouts.example"),
];
const read = (extra: DepItem[] = []): Dependencies => ({
  layers: LAYERS,
  kinds: KINDS,
  items: [...BASE, ...extra],
  relations: [
    { from: "asset:proxy", to: "asset:vm", relation: "inside" },
    { from: "asset:proxy", to: "route:pay", relation: "exposes" },
  ],
});
const type = (id: string, label: string, suggests: string[] = []): AssetType => ({ id, label, monogram: "??", properties: [], suggests });
const TYPES = [type("vm", "VM", ["service", "reverse_proxy"]), type("service", "Service"), type("reverse_proxy", "Reverse proxy")];
const VIEWS: DepViews = {
  views: [{ id: "everything", name: "Everything", settings: settingsOf({}) }],
  opens_from_ticket: "everything",
  opens_from_top: "everything",
};
const ROUTE: RouteRow = {
  id: "route:pay",
  asset_id: "asset:proxy",
  asset_name: "traefik",
  target_id: "asset:vm",
  target_name: "vm-pay-01",
  name: "payouts.example",
  url: "https://payouts.example",
  visibility: "internal",
  properties: [],
};

let calls: [string, ...unknown[]][] = [];
let estate: Dependencies;
let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

function render(props: Record<string, unknown> = {}, ports: Record<string, unknown> = {}) {
  target = document.body.appendChild(document.createElement("div"));
  const log =
    <T,>(name: string, value: () => T) =>
    (...args: unknown[]) => {
      calls.push([name, ...args]);
      return Promise.resolve(value());
    };
  app = mount(TfDeps, {
    target,
    props: {
      ready: true,
      views: createDepViewState(),
      ...props,
      ports: {
        dependencies: () => Promise.resolve(estate),
        depViews: () => Promise.resolve(VIEWS),
        assetTypes: () => Promise.resolve(TYPES),
        createAsset: log("createAsset", () => {
          estate = read([item("asset:new", "service", "payments-api")]);
          return { id: "asset:new" } as AssetRow;
        }),
        createLink: log("createLink", () => ({})),
        getRoute: (id: string) => Promise.resolve({ route: { ...ROUTE, id }, history: [], monitor_targets: [] }),
        createRoute: log("createRoute", () => ({ ...ROUTE, id: "route:new", name: "pay.example" })),
        editRoute: log("editRoute", () => ROUTE),
        deleteRoute: log("deleteRoute", () => undefined),
        ticketId: () => Promise.resolve("jira:PAY-231"),
        jobReach: () => Promise.resolve([{ entity_id: "asset:new", kind: "asset", step: "asset:new", origin: { kind: "own" } }]),
        jobPipelines: () => Promise.resolve([]),
        ...ports,
      },
    },
  });
}

async function settle() {
  for (let i = 0; i < 6; i++) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  calls = [];
  estate = read();
  toasts.items = [];
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const button = (text: string, root: ParentNode = document) =>
  [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim().startsWith(text))!;
const dialog = () => document.querySelector<HTMLElement>(".tf-dialog")!;
const field = (label: string) => {
  const el = [...dialog().querySelectorAll("label")].find((l) => l.textContent?.trim() === label)!;
  return document.getElementById(el.htmlFor) as HTMLInputElement & HTMLSelectElement;
};
function type_(el: HTMLInputElement | HTMLSelectElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event(el instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  flushSync();
}

test("Add ▾ holds Asset, Route and Import…, and Link to the ticket only from a ticket", async () => {
  render();
  await settle();
  button("Add ▾").click();
  flushSync();
  const items = () => [...target.querySelectorAll("[role=menuitem] b")].map((b) => b.textContent);
  expect(items()).toEqual(["Asset", "Route", "Import…"]);
  app && unmount(app);
  app = null;
  target.remove();
  render({ ticket: "PAY-231" });
  await settle();
  await settle();
  button("Add ▾").click();
  flushSync();
  expect(items()).toEqual(["Asset", "Route", "Import…", "Link to PAY-231…"]);
});

test("+ Add on a layer offers its types first and names what usually goes inside", async () => {
  render();
  await settle();
  target.querySelector<HTMLButtonElement>("[aria-label='Add to Services']")!.click();
  await settle();
  expect(dialog().querySelector("h2")!.textContent).toBe("New asset in Services");
  const groups = [...field("Type").querySelectorAll("optgroup")].map((g) => [g.label, [...g.querySelectorAll("option")].map((o) => o.text)]);
  expect(groups).toEqual([
    ["In this layer", ["Service", "Reverse proxy"]],
    ["Any type", ["VM"]],
  ]);
  expect(dialog().textContent).toContain("At the top of the estate: usually a site or a hypervisor.");
  type_(field("Inside"), "asset:vm");
  expect(dialog().textContent).toContain("Usual inside vm-pay-01: Service, Reverse proxy. Any type is allowed.");
  expect(target.querySelector("[aria-label='Add to Work']")).toBeNull();
});

test("Create sends create_asset with environment and owner unset, then the new tile flashes and the toast offers Edit it", async () => {
  render();
  await settle();
  target.querySelector<HTMLButtonElement>("[aria-label='Add to Services']")!.click();
  await settle();
  button("Create", dialog()).click();
  flushSync();
  expect(dialog().textContent).toContain("A name is needed.");
  type_(field("Name"), "payments-api");
  type_(field("Inside"), "asset:vm");
  button("Create", dialog()).click();
  await settle();
  await settle();
  expect(calls).toEqual([["createAsset", "service", "payments-api", "asset:vm"]]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  const tile = target.querySelector<HTMLElement>(".tf-dp-t[data-id='asset:new']")!;
  expect(tile.classList.contains("tf-dp-flash")).toBe(true);
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Created payments-api");
  expect(toast.action?.label).toBe("Edit it");
});

test("the flash timer is cleared when the page unmounts within 1.6 s (PAGE-14)", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    render();
    await settle();
    target.querySelector<HTMLButtonElement>("[aria-label='Add to Services']")!.click();
    await settle();
    type_(field("Name"), "payments-api");
    type_(field("Inside"), "asset:vm");
    button("Create", dialog()).click();
    await settle();
    await settle();
    expect(target.querySelector(".tf-dp-flash")).not.toBeNull();
    const pending = vi.getTimerCount();
    unmount(app!);
    app = null;
    expect(vi.getTimerCount()).toBe(pending - 1);
  } finally {
    vi.useRealTimers();
  }
});

test("a second flash within 1.6 s leaves no timer of the first behind the unmount (PAGE-14)", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    let made = 0;
    const createAsset = (_type: string, name: string) => {
      made += 1;
      const id = `asset:new${made}`;
      estate = read([...estate.items.filter((i) => i.id.startsWith("asset:new")), item(id, "service", name)]);
      return Promise.resolve({ id } as AssetRow);
    };
    render({}, { createAsset });
    await settle();
    for (const name of ["payments-api", "ledger-api"]) {
      target.querySelector<HTMLButtonElement>("[aria-label='Add to Services']")!.click();
      await settle();
      type_(field("Name"), name);
      button("Create", dialog()).click();
      await settle();
      await settle();
    }
    expect(target.querySelector(".tf-dp-t[data-id='asset:new2']")!.classList.contains("tf-dp-flash")).toBe(true);
    unmount(app!);
    app = null;
    // What is left besides the flash: jsdom's zero-delay selection timers,
    // run here, and the two toasts' dismiss timers, which are the shell's.
    vi.advanceTimersByTime(1);
    resetToasts();
    expect(vi.getTimerCount(), "a flash timer outlived the page").toBe(0);
  } finally {
    vi.useRealTimers();
  }
});

test("after a create, the layer it landed in still folds by hand", async () => {
  render();
  await settle();
  target.querySelector<HTMLButtonElement>("[aria-label='Add to Services']")!.click();
  await settle();
  type_(field("Name"), "payments-api");
  button("Create", dialog()).click();
  await settle();
  await settle();
  expect(target.querySelector(".tf-dp-t[data-id='asset:new']")).not.toBeNull();
  target.querySelector<HTMLButtonElement>("[aria-label='Fold Services']")!.click();
  flushSync();
  expect(target.querySelector("[aria-label='Unfold Services']")).not.toBeNull();
  expect(target.querySelector(".tf-dp-t[data-id='asset:new']")).toBeNull();
});

test("from a ticket, Link it to the ticket is ticked and links after the asset exists", async () => {
  render({ ticket: "PAY-231" });
  await settle();
  await settle();
  button("Add ▾").click();
  flushSync();
  button("Asset", target.querySelector("[role=menu]")!).click();
  await settle();
  const box = dialog().querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(box.checked).toBe(true);
  expect(box.parentElement!.textContent).toContain("Link it to PAY-231, so it is on this ticket's map");
  expect(dialog().textContent).toContain("Links stay in knobas and are never written to a source.");
  type_(field("Name"), "payments-api");
  button("Create", dialog()).click();
  await settle();
  await settle();
  expect(calls.map((c) => c[0])).toEqual(["createAsset", "createLink"]);
  expect(calls[1]).toEqual(["createLink", "jira:PAY-231", "asset:new"]);
  expect(target.querySelector(".tf-dp-t[data-id='asset:new']")).not.toBeNull();
});

test("a link that fails after the asset exists closes the dialog and says the link did not save", async () => {
  render({ ticket: "PAY-231" }, { createLink: () => Promise.reject({ code: "invalid", message: "no such entity", source_id: null }) });
  await settle();
  await settle();
  button("Add ▾").click();
  flushSync();
  button("Asset", target.querySelector("[role=menu]")!).click();
  await settle();
  type_(field("Name"), "payments-api");
  button("Create", dialog()).click();
  await settle();
  expect(document.querySelector(".tf-dialog")).toBeNull();
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toContain("Created payments-api, but it is not linked to PAY-231");
  expect(toast.tone).toBe("err");
});

test("a new route needs a scheme, takes its name from the address and may land on nothing knobas knows", async () => {
  render();
  await settle();
  button("Add ▾").click();
  flushSync();
  button("Route", target.querySelector("[role=menu]")!).click();
  await settle();
  expect(dialog().querySelector("h2")!.textContent).toBe("New route");
  expect(field("Exposed by").value).toBe("asset:proxy");
  expect(field("Lands on").options[0]!.text).toBe("Nothing knobas knows (a health path, a port)");
  type_(field("Address"), "pay.example/health");
  button("Create route", dialog()).click();
  await settle();
  expect(dialog().textContent).toContain("The address needs a scheme, like https://");
  expect(calls).toEqual([]);
  type_(field("Address"), "https://pay.example/health");
  type_(field("Who can reach it"), "public");
  button("Create route", dialog()).click();
  await settle();
  expect(calls).toEqual([["createRoute", "asset:proxy", "pay.example", "https://pay.example/health", null, "public"]]);
  expect(toasts.items.at(-1)!.text).toBe("Created pay.example");
});

test("a route tile opens the route dialog: Exposed by is fixed, Save sends only what changed, Delete… asks once more", async () => {
  render();
  await settle();
  target.querySelector<HTMLElement>(".tf-dp-t[data-id='route:pay'] .tf-dp-open")!.click();
  await settle();
  expect(dialog().querySelector("h2")!.textContent).toBe("Edit route");
  expect(dialog().querySelector("select option[value='asset:proxy']:checked")).toBeNull();
  expect(dialog().textContent).toContain("traefik");
  type_(field("Lands on"), "");
  button("Save", dialog()).click();
  await settle();
  expect(calls).toEqual([["editRoute", "route:pay", [{ field: "target", value: null }]]]);

  target.querySelector<HTMLElement>(".tf-dp-t[data-id='route:pay'] .tf-dp-open")!.click();
  await settle();
  button("Delete…", dialog()).click();
  flushSync();
  // The sentence above the footer, the button short (#1458).
  expect(dialog().querySelector(".tf-dp-sure")!.textContent).toBe("Delete payouts.example? Links to it stay, marked deleted.");
  expect(calls.at(-1)).not.toEqual(["deleteRoute", "route:pay"]);
  expect(button("Delete payouts.example", dialog()).textContent!.trim()).toBe("Delete payouts.example");
  button("Delete payouts.example", dialog()).click();
  await settle();
  expect(calls.at(-1)).toEqual(["deleteRoute", "route:pay"]);
  expect(toasts.items.at(-1)!.text).toBe("Deleted payouts.example");
});
