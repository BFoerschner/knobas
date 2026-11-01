/**
 * The asset editor (#875, R-11.19, R-11.19a) over stand-ins for the IPC
 * mirror's commands: the save order (one `edit_asset` with only the changed
 * fields, then `move_asset`, then the `depends-on` links), a failure that keeps
 * the dialog open naming the part that did not save, and *Delete…*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { AssetDetail, AssetRow, AssetType, Dependencies, DependsOnThis, RouteRow } from "../../ipc/assets";
import type { LinkEntry } from "../../ipc/entity";
import type { EditorPorts } from "./ports";
import TfAssetEditor from "./TfAssetEditor.svelte";

function row(over: Partial<AssetRow> & Pick<AssetRow, "id" | "name">): AssetRow {
  return {
    parent_id: null,
    type_id: "vm",
    type_label: "VM",
    monogram: "VM",
    status: "none",
    environment: null,
    owner: null,
    has_children: false,
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
    ...over,
  };
}

const HV = row({ id: "asset:hv", name: "hv-01", type_id: "hypervisor", environment: "prod", owner: "platform" });
const VM = row({ id: "asset:vm", name: "vm-pay-01", parent_id: "asset:hv" });

function detail(over: Partial<AssetDetail> = {}): AssetDetail {
  return {
    asset: VM,
    properties: [
      { key: "ip", label: "IP address", value: { kind: "text", value: "10.0.0.4" }, custom: false },
      { key: "cores", label: "Cores", value: null, custom: false },
      { key: "rack", label: "rack", value: { kind: "text", value: "B2" }, custom: true },
    ],
    effective_environment: { value: "prod", source_id: "asset:hv", source_name: "hv-01" },
    effective_owner: { value: "platform", source_id: "asset:hv", source_name: "hv-01" },
    held_by: [HV],
    holds: [],
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    monitors: [],
    monitoring: [],
    monitor_targets: [],
    ...over,
  };
}

const TYPES: AssetType[] = [
  {
    id: "vm",
    label: "VM",
    monogram: "VM",
    properties: [
      { key: "ip", label: "IP address", kind: "text" },
      { key: "cores", label: "Cores", kind: "number" },
    ],
    suggests: [],
  } as unknown as AssetType,
];

const READ: Dependencies = {
  layers: [],
  kinds: [{ id: "vm", label: "VM", monogram: "VM" }],
  items: [
    { id: "asset:hv", kind: "hypervisor", name: "hv-01", health: null, state: null, monitors: 0, environment: "prod", owner: "platform" },
    { id: "asset:hv2", kind: "hypervisor", name: "hv-02", health: null, state: null, monitors: 0, environment: "prod", owner: null },
    { id: "asset:vm", kind: "vm", name: "vm-pay-01", health: null, state: null, monitors: 0, environment: "prod", owner: null },
    { id: "asset:db", kind: "database", name: "payments", health: null, state: null, monitors: 0, environment: null, owner: null },
  ],
  relations: [{ from: "asset:vm", to: "asset:hv", relation: "inside" }],
};

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let calls: unknown[][];
let closed: { saved?: { name: string; moved: boolean }; deleted?: string };

function render(over: Partial<EditorPorts> = {}, read: AssetDetail = detail()) {
  target = document.body.appendChild(document.createElement("div"));
  calls = [];
  closed = {};
  const ports: EditorPorts = {
    getAsset: () => Promise.resolve(read),
    assetTypes: () => Promise.resolve(TYPES),
    editAsset: (id, edits) => (calls.push(["editAsset", id, edits]), Promise.resolve(VM)),
    moveAsset: (id, to) => (calls.push(["moveAsset", id, to]), Promise.resolve(VM)),
    createLink: (from, to, relation) => (calls.push(["createLink", from, to, relation]), Promise.resolve({})),
    deleteAsset: (id) => (calls.push(["deleteAsset", id]), Promise.resolve()),
    dependsOnThis: () => Promise.resolve({ assets: [], routes: [] }),
    ...over,
  };
  app = mount(TfAssetEditor, {
    target,
    props: {
      assetId: "asset:vm",
      read: READ,
      ports,
      onclose: () => {},
      onsaved: (what: { name: string; moved: boolean }) => (closed.saved = what),
      ondeleted: (name: string) => (closed.deleted = name),
      oncreatemonitor: () => {},
    },
  });
}

async function settle() {
  for (let i = 0; i < 6; i++) await Promise.resolve();
  flushSync();
}

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const field = (label: string) => {
  const at = [...target.querySelectorAll("label")].find((l) => l.textContent!.trim() === label)!;
  return document.getElementById(at.getAttribute("for")!) as HTMLInputElement | HTMLSelectElement;
};
function set(el: HTMLInputElement | HTMLSelectElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event(el instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  flushSync();
}
const press = (text: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === text)!.click();

test("it opens on the asset: inherit names where the value comes from", async () => {
  render();
  await settle();
  expect((field("Name") as HTMLInputElement).value).toBe("vm-pay-01");
  const env = field("Environment") as HTMLSelectElement;
  expect(env.options[0]!.textContent).toBe("Inherit (prod from hv-01)");
  expect((field("Owner") as HTMLInputElement).placeholder).toBe("Inherit (platform)");
  expect([...target.querySelectorAll(".tf-dp-ecols label")].map((l) => l.textContent!.trim())).toContain("rack");
});

test("Save sends only the changed fields in one edit, then the move, then the links", async () => {
  render();
  await settle();
  set(field("Name") as HTMLInputElement, "vm-pay-02");
  set(field("Own status"), "down");
  set(field("Cores") as HTMLInputElement, "8");
  set(field("Inside"), "asset:hv2");
  set(target.querySelector<HTMLSelectElement>("select[aria-label='Add something it depends on']")!, "asset:db");
  press("Save");
  await settle();
  expect(calls).toEqual([
    [
      "editAsset",
      "asset:vm",
      [
        { field: "name", value: "vm-pay-02" },
        { field: "status", value: "down" },
        { field: "property", key: "cores", value: { kind: "number", value: 8 } },
      ],
    ],
    ["moveAsset", "asset:vm", "asset:hv2"],
    ["createLink", "asset:vm", "asset:db", "depends-on"],
  ]);
  expect(closed.saved).toEqual({ name: "vm-pay-02", moved: true });
});

test("nothing changed is no edit at all", async () => {
  render();
  await settle();
  press("Save");
  await settle();
  expect(calls).toEqual([]);
  expect(closed.saved).toEqual({ name: "vm-pay-01", moved: false });
});

test("a failed move keeps the dialog open, says what did not save, and a retry sends only the rest", async () => {
  let refuse = true;
  render({
    moveAsset: (id, to) => {
      calls.push(["moveAsset", id, to]);
      return refuse ? Promise.reject({ code: "invalid", message: "hv-02 is inside vm-pay-01", source_id: null }) : Promise.resolve(VM);
    },
  });
  await settle();
  set(field("Name") as HTMLInputElement, "vm-pay-02");
  set(field("Inside"), "asset:hv2");
  press("Save");
  await settle();
  expect(closed.saved).toBeUndefined();
  expect(target.querySelector("[role='alert']")!.textContent).toBe("Saved the fields, but not the move: hv-02 is inside vm-pay-01");
  expect(calls.map((call) => call[0])).toEqual(["editAsset", "moveAsset"]);

  refuse = false;
  calls = [];
  press("Save");
  await settle();
  // The fields were kept: the form is the re-read asset's, so only the move goes again.
  expect(calls).toEqual([["moveAsset", "asset:vm", "asset:hv2"]]);
  expect(closed.saved).toMatchObject({ moved: true });
});

test("a failed edit saves nothing and moves nothing", async () => {
  render({
    editAsset: (id, edits) => (calls.push(["editAsset", id, edits]), Promise.reject({ code: "invalid", message: "cores is not a number", source_id: null })),
  });
  await settle();
  set(field("Cores") as HTMLInputElement, "8");
  set(field("Inside"), "asset:hv2");
  press("Save");
  await settle();
  expect(calls.map((call) => call[0])).toEqual(["editAsset"]);
  expect(target.querySelector("[role='alert']")!.textContent).toBe("Did not save the fields: cores is not a number");
});

test("Delete… is disabled with the reason while it holds assets or exposes routes", async () => {
  render({}, detail({ holds: [row({ id: "asset:ct", name: "ct" })] }));
  await settle();
  const del = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Delete…")!;
  expect(del.disabled).toBe(true);
  expect(del.title).toBe("It holds 1 asset: move or delete them first.");
});

test("the delete confirm counts the routes landing on it, and a conflict shows in place", async () => {
  const route = { id: "route:r", asset_id: "asset:px", asset_name: "px", target_id: "asset:vm", target_name: "vm-pay-01", name: "pay", url: "https://pay.example", visibility: "public" as const, properties: [] };
  render({ deleteAsset: () => Promise.reject({ code: "conflict", message: "vm-pay-01 still holds something", source_id: null }) }, detail({ reachable_via: [route] }));
  await settle();
  press("Delete…");
  flushSync();
  // The sentence is a line of its own above the footer's buttons, not the button's label (#1458).
  expect(target.querySelector(".tf-dp-sure")!.textContent).toBe(
    "Delete vm-pay-01? 1 route landing on it will land on nothing. Links to it stay, marked deleted.",
  );
  const confirm = target.querySelector<HTMLButtonElement>(".tf-dp-del")!;
  expect(confirm.textContent!.trim()).toBe("Delete vm-pay-01");
  confirm.click();
  await settle();
  expect(target.querySelector("[role='alert']")!.textContent).toBe("vm-pay-01 still holds something");
  expect(closed.deleted).toBeUndefined();
});

/** A `depends-on` link drawn from `from` at the asset in the editor. */
function dependsOnVm(id: string, from: string, title: string): LinkEntry {
  return {
    link: {
      id,
      from_id: from,
      to_id: "asset:vm",
      relation: "depends-on",
      origin: "manual",
      note: null,
      created_by: "user",
      created_at: "2026-10-21T09:00:00Z",
      confirmed_at: "2026-10-21T09:00:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: { entity_id: from, kind: "asset", title, deleted_at: null },
  };
}

const ROUTE: RouteRow = {
  id: "route:pay",
  asset_id: "asset:px",
  asset_name: "traefik",
  target_id: "asset:vm",
  target_name: "vm-pay-01",
  name: "pay.example",
  url: "https://pay.example",
  visibility: "public",
  properties: [],
};

/** The *Needed by* column: its heading through to *Watched by*. */
function neededBy(): Element[] {
  const heads = [...target.querySelectorAll(".tf-dp-ecols h4")];
  const from = heads.find((h) => h.textContent!.trim().startsWith("Needed by"))!;
  const out: Element[] = [];
  for (let at = from.nextElementSibling; at !== null && at.tagName !== "H4"; at = at.nextElementSibling) out.push(at);
  return out;
}
/** An element's text with its whitespace collapsed. */
const flat = (el: Element) => el.textContent!.replace(/\s+/g, " ").trim();
const neededText = () => neededBy().map(flat).join(" | ");

test("Needed by lists what breaks if it goes down: the closure nearest first, then the routes, under the direct links", async () => {
  const answer: DependsOnThis = {
    assets: [
      { asset: row({ id: "asset:api", name: "payments-api" }), path: "hv-02", relation: "depends-on" },
      { asset: row({ id: "asset:ct", name: "ct-worker", parent_id: "asset:vm" }), path: "hv-01 / vm-pay-01", relation: null },
    ],
    routes: [ROUTE],
  };
  const asked: string[] = [];
  render(
    { dependsOnThis: (id) => (asked.push(id), Promise.resolve(answer)) },
    detail({ links: [dependsOnVm("link:1", "asset:api", "payments-api")] }),
  );
  await settle();
  expect(asked).toEqual(["asset:vm"]);
  // The direct link stays a chip, the part *Depends on* edits from the other end.
  const chips = neededBy().flatMap((el) => [...el.querySelectorAll(".tf-chip")].map((c) => c.textContent!.trim()));
  expect(chips).toEqual(["payments-api"]);
  // Beneath it, read-only and in the answer's order: the assets, counted, then the route, not counted.
  const breaks = [...target.querySelectorAll(".tf-dp-breaks li")].map(flat);
  expect(breaks).toEqual(["payments-api depends on", "ct-worker inside", "pay.example https://pay.example"]);
  expect(target.querySelector(".tf-dp-breaks-n")!.textContent).toBe("2");
  expect(neededText()).not.toContain("Nothing recorded");
});

test("an asset nothing depends on and no route lands on reads Nothing recorded", async () => {
  render();
  await settle();
  expect(neededText()).toBe("Nothing recorded");
  expect(target.querySelector(".tf-dp-breaks")).toBeNull();
});

test("the holds note says thing for one", async () => {
  render({}, detail({ holds: [row({ id: "asset:ct", name: "ct" })] }));
  await settle();
  expect(neededText()).toContain("and the 1 thing inside it");
  expect(neededText()).not.toContain("things");
});

test("the holds note says things for two", async () => {
  render({}, detail({ holds: [row({ id: "asset:ct", name: "ct" }), row({ id: "asset:ct2", name: "ct2" })] }));
  await settle();
  expect(neededText()).toContain("and the 2 things inside it");
});

test("Delete… asks with the sentence above the footer, and the second click deletes", async () => {
  render();
  await settle();
  expect(target.querySelector(".tf-dp-sure")).toBeNull();
  press("Delete…");
  flushSync();
  expect(target.querySelector(".tf-dp-sure")!.textContent).toBe("Delete vm-pay-01? Links to it stay, marked deleted.");
  expect(calls).toEqual([]);
  press("Delete vm-pay-01");
  await settle();
  expect(calls).toEqual([["deleteAsset", "asset:vm"]]);
  expect(closed.deleted).toBe("vm-pay-01");
});

test("a depends_on_this that fails says so under Needed by and leaves the rest of the editor working", async () => {
  render({ dependsOnThis: () => Promise.reject({ code: "not_found", message: "no asset asset:vm", source_id: null }) });
  await settle();
  expect(neededText()).toBe("What breaks could not be read: no asset asset:vm");
  expect((field("Name") as HTMLInputElement).value).toBe("vm-pay-01");
});

test("a linked ticket opens its ticket page, which chooses no job (spec #2396)", async () => {
  const ticketLink = { ...dependsOnVm("link:t", "jira:PAY-231", "PAY-231 Retry failed SEPA payouts"), other: { entity_id: "jira:PAY-231", kind: "ticket", title: "PAY-231 Retry failed SEPA payouts", deleted_at: null } };
  render({}, detail({ links: [ticketLink] }));
  await settle();
  const chip = [...target.querySelectorAll<HTMLAnchorElement>(".tf-dp-rels a")].find((a) => a.textContent === "PAY-231 Retry failed SEPA payouts");
  expect(chip?.getAttribute("href")).toBe("#/ticket/jira%3APAY-231");
});
