/**
 * Dependencies (#871, R-11.11 to R-11.14), mounted over a stand-in read: the
 * bands, the tiles, the rail's checkboxes and problem counts, and the line
 * naming what sits in no layer. What the read returns is
 * `crates/knobas-app/tests/it/dependencies_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { Dependencies, DepItem } from "../../ipc/assets";
import { createHiddenLayers } from "./hidden.svelte";
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
  relations: [],
};

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

function render(read: Dependencies = READ, ready = true) {
  target = document.body.appendChild(document.createElement("div"));
  const hidden = createHiddenLayers();
  app = mount(TfDeps, { target, props: { ready, hidden, ports: { dependencies: () => Promise.resolve(read) } } });
  return hidden;
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
  const tiles = [...services.querySelectorAll("a")];
  expect(tiles.map((tile) => tile.textContent!.replace(/\s+/g, " ").trim())).toEqual([
    "SV payouts-svc down, prod",
    "SV ledger-svc 2 checks",
  ]);
  expect(tiles[0]!.className).toContain("tf-dp-err");
  expect(band("Machines").querySelector("header p")!.textContent).toContain("all fine");
  const vm = band("Machines").querySelector("a")!;
  expect(vm.className).toContain("tf-dp-none");
  expect(vm.textContent).toContain("no check");
  expect(band("Work").querySelector("a[href='#/job/PAY-231']")!.textContent).toContain("In Progress");
  expect(band("Work").querySelector("a.tf-dp-info")!.textContent).toContain("open");
});

test("the rail counts each layer and its problems, and a checkbox hides a band", async () => {
  const hidden = render();
  await settle();
  const rail = target.querySelector("nav[aria-label='Layers']")!;
  const services = [...rail.querySelectorAll(".tf-dp-rl")].find((row) => row.textContent!.includes("Services"))!;
  expect(services.querySelector(".tf-dp-rc")!.textContent).toBe("2");
  expect(services.querySelector("b")!.textContent).toBe("1");

  const work = rail.querySelector<HTMLInputElement>("input[aria-label='Show Work']")!;
  work.checked = false;
  work.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(hidden.ids.has("work")).toBe(true);
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

test("above 30 items a layer draws one-line tiles with a state dot", async () => {
  const many = Array.from({ length: 31 }, (_, n) =>
    item({ id: `asset:s${n}`, kind: "service", name: `svc-${n}`, health: "ok", monitors: 1 }),
  );
  render({ ...READ, items: many });
  await settle();
  const tiles = band("Services").querySelectorAll("a.tf-dp-c");
  expect(tiles.length).toBe(31);
  expect(tiles[0]!.querySelector("[role='img']")!.getAttribute("aria-label")).toBe("1 check");
});

test("the read waits for the database", async () => {
  let asked = 0;
  target = document.body.appendChild(document.createElement("div"));
  app = mount(TfDeps, {
    target,
    props: {
      ready: false,
      hidden: createHiddenLayers(),
      ports: {
        dependencies: () => {
          asked += 1;
          return Promise.resolve(READ);
        },
      },
    },
  });
  await settle();
  expect(asked).toBe(0);
  expect(target.querySelector("a[href='#/assets']")!.textContent).toBe("Asset tree");
});
