/**
 * Dependencies (#871 to #873, R-11.11 to R-11.18), mounted over stand-in
 * reads: the bands, the tiles, the rail's checkboxes and problem counts, the
 * line naming what sits in no layer, pointing and tracing, *Find*, the view
 * menu and the layers panel. What the reads return is
 * `crates/knobas-app/tests/it/dependencies_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { Dependencies, DepItem, DepLayer, DepView, DepViews } from "../../ipc/assets";
import { toasts } from "../../shell/toasts.svelte";
import { createDepViewState } from "./views.svelte";
import { settingsOf } from "./views";
import TfDeps from "./TfDeps.svelte";

function item(over: Partial<DepItem> & Pick<DepItem, "id" | "kind" | "name">): DepItem {
  return { health: null, state: null, monitors: 0, environment: null, owner: null, ...over };
}

const READ: Dependencies = {
  layers: [
    { id: "work", name: "Work", kinds: ["ticket", "pr"] },
    { id: "services", name: "Services", kinds: ["service"] },
    { id: "machines", name: "Machines", kinds: ["vm"] },
  ],
  kinds: [
    { id: "ticket", label: "Ticket", monogram: "TK" },
    { id: "pr", label: "Pull request", monogram: "PR" },
    { id: "service", label: "Service", monogram: "SV" },
    { id: "vm", label: "VM", monogram: "VM" },
    { id: "network", label: "Network", monogram: "NW" },
  ],
  items: [
    item({ id: "jira:PAY-231", kind: "ticket", name: "PAY-231", state: "In Progress" }),
    item({ id: "gitea:p#142", kind: "pr", name: "SEPA retry", health: "info", state: "open" }),
    item({ id: "asset:payouts", kind: "service", name: "payouts-svc", health: "err", monitors: 1, environment: "prod" }),
    item({ id: "asset:ledger", kind: "service", name: "ledger-svc", health: "ok", monitors: 2 }),
    item({ id: "asset:vm", kind: "vm", name: "vm-pay-01" }),
    item({ id: "asset:net", kind: "network", name: "vlan-20" }),
  ],
  relations: [
    { from: "jira:PAY-231", to: "asset:payouts", relation: "deployed-to" },
    { from: "asset:payouts", to: "asset:vm", relation: "inside" },
    { from: "asset:ledger", to: "asset:vm", relation: "inside" },
  ],
};

function views(): DepViews {
  return {
    views: [
      { id: "everything", name: "Everything", settings: settingsOf({}) },
      { id: "ticket", name: "This ticket", settings: settingsOf({}) },
    ],
    opens_from_ticket: "ticket",
    opens_from_top: "everything",
  };
}

/** Every write the page sent, in order. */
let calls: [string, ...unknown[]][] = [];

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

function render(read: Dependencies = READ, ready = true, extra: Record<string, unknown> = {}, more: Record<string, unknown> = {}) {
  target = document.body.appendChild(document.createElement("div"));
  const state = createDepViewState();
  calls = [];
  let layers: DepLayer[] = read.layers;
  const answer =
    <T,>(name: string, value: T) =>
    (...args: unknown[]) => {
      calls.push([name, ...args]);
      return Promise.resolve(value);
    };
  app = mount(TfDeps, {
    target,
    props: {
      ready,
      views: state,
      ...more,
      ports: {
        dependencies: () => Promise.resolve(read),
        depViews: () => Promise.resolve(views()),
        saveDepView: (id: string | null, name: string, settings: DepView["settings"]) => {
          calls.push(["saveDepView", id, name, settings]);
          return Promise.resolve({ id: id ?? "mine", name, settings });
        },
        renameDepLayer: (id: string, name: string) => {
          calls.push(["renameDepLayer", id, name]);
          if (name.toLowerCase() === "services")
            return Promise.reject({ code: "conflict", message: `There is already a layer called ${name}. Names are unique, so a name always means the same things.`, source_id: null });
          layers = layers.map((layer) => (layer.id === id ? { ...layer, name } : layer));
          return Promise.resolve(layers);
        },
        placeDepKind: answer("placeDepKind", read.layers),
        ...extra,
      },
    },
  });
  return state;
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

function band(name: string): HTMLElement {
  const heading = [...target.querySelectorAll("section h3")].find((h) => h.textContent === name);
  if (!heading) throw new Error(`no band ${name}`);
  return heading.closest("section")!;
}

test("each layer is a band with its summary, what is wrong and its tiles, problems first", async () => {
  render();
  await settle();
  const services = band("Services");
  expect(services.querySelector("header p")!.textContent).toContain("2 services");
  expect(services.querySelector("header p")!.textContent).toContain("1 broken");
  const tiles = [...services.querySelectorAll(".tf-dp-t")];
  const words = (el: Element) => el.querySelector("a")!.textContent!.replace(/\s+/g, " ").trim();
  expect(tiles.map(words)).toEqual([
    "SV payouts-svc down, prod",
    "SV ledger-svc 2 checks",
  ]);
  expect(tiles[0]!.className).toContain("tf-dp-err");
  expect(band("Machines").querySelector("header p")!.textContent).toContain("all fine");
  const vm = band("Machines").querySelector(".tf-dp-t")!;
  expect(vm.className).toContain("tf-dp-none");
  expect(vm.textContent).toContain("no check");
  expect(band("Work").querySelector("a[href='#/ticket/jira%3APAY-231']")!.textContent).toContain("In Progress");
  expect(band("Work").querySelector(".tf-dp-t.tf-dp-info")!.textContent).toContain("open");
});

test("the rail counts each layer and its problems, and a checkbox hides a band", async () => {
  const state = render();
  await settle();
  const rail = target.querySelector("nav[aria-label='Layers']")!;
  const services = [...rail.querySelectorAll(".tf-dp-rl")].find((row) => row.textContent!.includes("Services"))!;
  expect(services.querySelector(".tf-dp-rc")!.textContent).toBe("2");
  expect(services.querySelector("b")!.textContent).toBe("1");

  const work = rail.querySelector<HTMLInputElement>("input[aria-label='Show Work']")!;
  work.checked = false;
  work.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(state.draft!.layers["work"]!.hidden).toBe(true);
  expect(state.dirty).toBe(true);
  expect([...target.querySelectorAll("section h3")].map((h) => h.textContent)).toEqual(["Services", "Machines"]);
  const foot = target.querySelector(".tf-dp-foot")!;
  expect(foot.textContent).toContain("Hidden in this view:");
  foot.querySelector("button")!.click();
  flushSync();
  expect([...target.querySelectorAll("section h3")].map((h) => h.textContent)).toEqual(["Work", "Services", "Machines"]);
});

test("items whose kind sits in no layer are named under the bands", async () => {
  render();
  await settle();
  expect(target.querySelector(".tf-dp-foot")!.textContent).toContain(
    "1 items are in no layer: Network. Place them in Layers…",
  );
});

test("above 30 items a layer draws one-line tiles with a state dot, and folds the healthy rest", async () => {
  const many = Array.from({ length: 31 }, (_, n) =>
    item({ id: `asset:s${n}`, kind: "service", name: `svc-${n}`, health: "ok", monitors: 1 }),
  );
  render({ ...READ, items: many });
  await settle();
  const tiles = band("Services").querySelectorAll(".tf-dp-t.tf-dp-c");
  expect(tiles.length).toBe(6);
  expect(tiles[0]!.querySelector("[role='img']")!.getAttribute("aria-label")).toBe("1 check");
  const more = band("Services").querySelector<HTMLButtonElement>(".tf-dp-more")!;
  expect(more.textContent).toBe("25 more, all fine");
  more.click();
  flushSync();
  expect(band("Services").querySelectorAll(".tf-dp-t").length).toBe(31);
});

test("the read waits for the database", async () => {
  let asked = 0;
  target = document.body.appendChild(document.createElement("div"));
  app = mount(TfDeps, {
    target,
    props: {
      ready: false,
      views: createDepViewState(),
      ports: {
        dependencies: () => {
          asked += 1;
          return Promise.resolve(READ);
        },
        depViews: () => Promise.resolve(views()),
      },
    },
  });
  await settle();
  expect(asked).toBe(0);
  expect(target.querySelector("a[href='#/assets']")!.textContent).toBe("Asset tree");
});

const tile = (id: string) => target.querySelector<HTMLElement>(`.tf-dp-t[data-id="${id}"]`)!;

test("pointing at a tile lights what it touches and fades the rest (R-11.15)", async () => {
  render();
  await settle();
  tile("asset:payouts").querySelector("a")!.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
  flushSync();
  // payouts-svc is inside the VM and PAY-231 is deployed to it; ledger-svc only shares the VM.
  expect(tile("jira:PAY-231").classList.contains("tf-dp-lit")).toBe(true);
  expect(tile("asset:vm").classList.contains("tf-dp-lit")).toBe(true);
  expect(tile("asset:ledger").classList.contains("tf-dp-dim")).toBe(true);
  expect(tile("asset:payouts").classList.contains("tf-dp-dim")).toBe(false);
  target.querySelector(".tf-dp-bands")!.dispatchEvent(new MouseEvent("mouseleave"));
  flushSync();
  expect(target.querySelectorAll(".tf-dp-dim").length).toBe(0);
});

test("⌖ and T show only what a tile touches, with Show everything", async () => {
  render();
  await settle();
  tile("asset:ledger").querySelector<HTMLButtonElement>("button[aria-label='Show only what ledger-svc touches']")!.click();
  flushSync();
  expect(target.querySelector(".tf-dp-trail")!.textContent).toContain("Only what touches ledger-svc, up and down.");
  expect([...target.querySelectorAll(".tf-dp-t")].map((t) => t.getAttribute("data-id")).sort()).toEqual(["asset:ledger", "asset:vm"]);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-dp-trail button")].find((b) => b.textContent === "Show everything")!.click();
  flushSync();
  expect(target.querySelector(".tf-dp-trail")).toBeNull();

  const link = tile("asset:payouts").querySelector("a")!;
  link.dispatchEvent(new KeyboardEvent("keydown", { key: "t", bubbles: true }));
  flushSync();
  expect(target.querySelector(".tf-dp-trail")!.textContent).toContain("payouts-svc");
});

test("Find dims what does not match and counts the matches (R-11.18)", async () => {
  render();
  await settle();
  const find = target.querySelector<HTMLInputElement>("input[aria-label='Find']")!;
  expect(find.placeholder).toBe("Find among 6 items");
  find.value = "svc";
  find.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(target.querySelector(".tf-dp-qn")!.textContent).toBe("2 found, Enter for the next");
  expect(tile("asset:vm").classList.contains("tf-dp-miss")).toBe(true);
  expect(tile("asset:payouts").classList.contains("tf-dp-hit")).toBe(true);
  find.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(document.activeElement).toBe(tile("asset:payouts").querySelector("a"));
});

test("Enter steps past the drawn matches into the group's more, which opens on the next match (#2212)", async () => {
  const services = Array.from({ length: 40 }, (_, n) =>
    item({ id: `asset:s${n}`, kind: "service", name: `svc-${String(n).padStart(2, "0")}`, health: "ok" }),
  );
  render({ ...READ, items: services, relations: [] });
  await settle();
  const find = target.querySelector<HTMLInputElement>("input[aria-label='Find']")!;
  find.value = "svc";
  find.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(target.querySelector(".tf-dp-qn")!.textContent).toBe("40 found, Enter for the next");
  expect(target.querySelectorAll(".tf-dp-t")).toHaveLength(24);
  const more = target.querySelector<HTMLButtonElement>(".tf-dp-more")!;
  expect(more.textContent!.trim()).toBe("16 more, 16 found");
  expect(more.classList.contains("tf-dp-hit")).toBe(true);
  const enter = async () => {
    find.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();
  };
  for (let n = 0; n < 24; n++) await enter();
  expect(document.activeElement).toBe(tile("asset:s23").querySelector("a"));
  await enter();
  expect(target.querySelector(".tf-dp-more")).toBeNull();
  expect(document.activeElement).toBe(tile("asset:s24").querySelector("a"));
  await enter();
  expect(document.activeElement).toBe(tile("asset:s25").querySelector("a"));
});

test("the environment chips and the problems toggle change the view's draft", async () => {
  const state = render();
  await settle();
  const prod = [...target.querySelectorAll<HTMLButtonElement>("[aria-label='Environment'] button")].find((b) => b.textContent === "prod")!;
  prod.click();
  flushSync();
  expect(state.draft!.environments).toEqual(["prod"]);
  expect(tile("asset:payouts")).not.toBeNull();
  const problems = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.startsWith("Problems and what they touch"))!;
  expect(problems.textContent).toBe("Problems and what they touch 1");
  problems.click();
  flushSync();
  expect(target.querySelector(".tf-dp-t[data-id='asset:ledger']")).toBeNull();
  expect(target.querySelector("button.tf-dp-viewbtn")!.textContent).toContain("changed");

  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save view")!.click();
  await settle();
  const [, id, name, settings] = calls.find((call) => call[0] === "saveDepView")!;
  expect([id, name]).toEqual(["everything", "Everything"]);
  expect(settings).toMatchObject({ problems: true, environments: ["prod"] });
  expect(state.dirty).toBe(false);
});

test("the view menu opens another view and saves as a new one (R-11.16)", async () => {
  const state = render();
  await settle();
  const button = target.querySelector<HTMLButtonElement>("button.tf-dp-viewbtn")!;
  expect(button.textContent).toContain("View: Everything");
  button.click();
  await settle();
  const items = [...target.querySelectorAll<HTMLButtonElement>("[role='menuitem']")].map((b) => b.textContent!.trim());
  expect(items).toEqual([
    "Everything (on)opens from the top bar",
    "This ticketopens from a ticket",
    "Save as a new view…",
    "Rename this view…",
    "Delete this view",
    "Which view opens first…",
  ]);
  [...target.querySelectorAll<HTMLButtonElement>("[role='menuitem']")].find((b) => b.textContent === "Save as a new view…")!.click();
  flushSync();
  const name = target.querySelector<HTMLInputElement>(".tf-dialog input[type='text']")!;
  expect(name.value).toBe("My view");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((b) => b.textContent === "Save view")!.click();
  await settle();
  expect(calls.find((call) => call[0] === "saveDepView")!.slice(1, 3)).toEqual([null, "My view"]);
  expect(state.current!.name).toBe("My view");
});

test("the layers panel refuses a taken name in place and says every view uses a change (R-11.13)", async () => {
  render();
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Layers…")!.click();
  flushSync();
  const panel = target.querySelector("aside[aria-label='Layers']")!;
  const names = panel.querySelectorAll<HTMLInputElement>("input[aria-label='Name of the layer']");
  const machines = names[2]!;
  machines.value = "services";
  machines.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(panel.querySelector("[role='alert']")!.textContent).toBe(
    "There is already a layer called services. Names are unique, so a name always means the same things.",
  );
  machines.value = "Hosts";
  machines.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(panel.querySelector("[role='alert']")).toBeNull();
  expect([...target.querySelectorAll("section h3")].map((h) => h.textContent)).toContain("Hosts");
  expect(toasts.items.map((t) => t.text)).toContain("Renamed Machines to Hosts. Every view uses it.");

  const network = panel.querySelector<HTMLSelectElement>("select[aria-label='Layer for Network']")!;
  network.value = "machines";
  network.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(calls.find((call) => call[0] === "placeDepKind")!.slice(1)).toEqual(["network", "machines"]);
});

test("a layer row's move and remove buttons are icon-only, square at the small size (#2288)", async () => {
  render();
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Layers…")!.click();
  flushSync();
  const panel = target.querySelector("aside[aria-label='Layers']")!;
  const label = (name: string) => panel.querySelector<HTMLButtonElement>(`button[aria-label='${name}']`)!.className;
  expect([label("Move Machines up"), label("Move Machines down"), label("Remove Machines")]).toEqual([
    "tf-iconbtn tf-sm",
    "tf-iconbtn tf-sm",
    "tf-iconbtn tf-sm",
  ]);
});

test("clicking an asset opens its editor; a ticket follows its link", async () => {
  render(READ, true, {
    getAsset: () => new Promise(() => {}),
    assetTypes: () => Promise.resolve([]),
  });
  await settle();
  const ticket = tile("jira:PAY-231").querySelector("a")!;
  const followed = ticket.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  expect(followed).toBe(true);
  const asset = tile("asset:payouts").querySelector("a")!;
  expect(asset.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))).toBe(false);
  flushSync();
  expect(document.querySelector(".tf-dialog")).not.toBeNull();
});

// -- #874: the opening from a ticket ----------------------------------------

const REACHES = {
  ticketId: (key: string) => Promise.resolve(key === "PAY-231" ? "jira:PAY-231" : null),
  jobReach: (id: string) =>
    Promise.resolve(id === "jira:PAY-231" ? [{ entity_id: "asset:payouts", kind: "asset", step: "asset:payouts", origin: { kind: "own" } }] : []),
  jobPipelines: () => Promise.resolve([]),
};

const shownIds = () => [...target.querySelectorAll<HTMLElement>(".tf-dp-t")].map((el) => el.dataset["id"]);
const viewButton = () => [...target.querySelectorAll("button")].find((b) => b.textContent?.startsWith("View:"))!;
const scopeButtons = () => [...target.querySelectorAll<HTMLButtonElement>("[aria-label='Scope'] button")];

test("from a ticket it draws only what the ticket reaches, in the view that opens from a ticket", async () => {
  render(READ, true, REACHES, { ticket: "PAY-231" });
  await settle();
  await settle();
  expect(shownIds().sort()).toEqual(["asset:payouts", "jira:PAY-231"]);
  expect(viewButton().textContent).toContain("This ticket");
  expect(target.querySelector(".tf-sub")!.textContent).toContain("What PAY-231 reaches");
  const [mine, all] = scopeButtons();
  expect(mine!.textContent).toBe("This job PAY-231");
  expect(mine!.getAttribute("aria-pressed")).toBe("true");
  expect(all!.textContent).toBe("Everything");
  all!.click();
  expect(location.hash).toBe("#/deps");
});

test("the top bar draws everything in its own view, and This job goes back to the ticket", async () => {
  render(READ, true, REACHES, { job: "PAY-231" });
  await settle();
  expect(shownIds()).toHaveLength(READ.items.length - 1); // the network sits in no layer
  expect(viewButton().textContent).toContain("Everything");
  const [mine, all] = scopeButtons();
  expect(all!.getAttribute("aria-pressed")).toBe("true");
  mine!.click();
  expect(location.hash).toBe("#/deps/job/PAY-231");
});

test("on the job's Map tab there is no page header, and This job stays on the tab", async () => {
  render(READ, true, REACHES, { ticket: "PAY-231", ticketId: "jira:PAY-231", host: "job" });
  await settle();
  await settle();
  expect(target.querySelector("h1")).toBeNull();
  scopeButtons()[0]!.click();
  expect(location.hash).toBe("#/job/jira%3APAY-231/map");
});

test("with no job open there is no scope switch on the top bar", async () => {
  render(READ, true, REACHES);
  await settle();
  expect(scopeButtons()).toHaveLength(0);
});

test("a ticket nothing in the mirror answers to says so", async () => {
  render(READ, true, REACHES, { ticket: "PAY-999" });
  await settle();
  await settle();
  expect(target.querySelector("[role='alert']")!.textContent).toContain("nothing in the mirror answers to PAY-999");
});

const slash = (on: EventTarget = window) => {
  const event = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  return event;
};

test("/ focuses Find, and Find is the page's filter for the shell's Esc ladder (#1419)", async () => {
  render();
  await settle();
  const find = target.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  expect(find.hasAttribute("data-filter")).toBe(true);
  expect(document.activeElement).not.toBe(find);
  expect(slash().defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(find);
});

test("/ typed in another field does not steal focus (#1419)", async () => {
  render();
  await settle();
  const other = document.body.appendChild(document.createElement("input"));
  other.focus();
  slash(other);
  expect(document.activeElement).toBe(other);
  other.remove();
});

test("the / listener is removed when the page unmounts (#1419)", async () => {
  render();
  await settle();
  const find = target.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  unmount(app!);
  app = null;
  const event = slash();
  expect(event.defaultPrevented).toBe(false);
  expect(document.activeElement).not.toBe(find);
});
