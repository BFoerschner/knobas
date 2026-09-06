/**
 * Linking from the Tree's pane, and the count a column row carries for it
 * (issue #435, spec #427 stories 32, 33 and 36).
 *
 * A file of its own, the way #429's writes are: this one needs `../ipc/search`
 * mocked — *Link to…* is the launcher's own picker, and mounting it in jsdom
 * would otherwise reach for Tauri — and the two files beside it are about a
 * surface that never opens a dialog of somebody else's.
 *
 * ## What is asserted here, and what is asserted elsewhere
 *
 * The **wording** of a relation is `relations.test.ts`'; the **dialog's** own
 * behaviour is `LinkDialog.test.svelte.ts`'; the **panel's** rows, its
 * withdrawn marker and its empty state are `LinksPanel.test.svelte.ts`'. What
 * is here is the part only this view can get wrong: that the pane is handed
 * *its own asset's* links and its own id, so one row reads `runs on` from the
 * container and `hosts` from the VM; that the action opens the dialog over the
 * asset the reader is looking at; that unlinking re-reads rather than patching;
 * and that the column's badge draws what the backend counted.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetDetail, AssetRow } from "../ipc/assets";
import type { LinkEntry } from "../ipc/entity";
import { createRouter } from "../shell/router.svelte";

/**
 * The picker's engine, stubbed at the module boundary.
 *
 * `LinkDialog` imports it directly rather than taking it as a prop — it is the
 * launcher's `Session`, and the whole point of that is that linking is as fast
 * as finding — so a dialog mounted here would otherwise call the real
 * `invoke`. `launcherHome` rejects because the dialog never draws the
 * launcher's home list, which is `LinkDialog.test.svelte.ts`' own reading of
 * it.
 */
vi.mock("../ipc/search", () => ({
  search: () =>
    Promise.resolve({
      hits: [],
      total: 0,
      truncated: false,
      took_ms: 0,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
    }),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the launcher's home list")),
  noFilters: () => ({
    sources: [],
    kinds: [],
    updated_within_days: null,
    mine: false,
    authors: [],
  }),
}));

const { default: AssetsView } = await import("./AssetsView.svelte");

const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

function row(over: Partial<AssetRow> & Pick<AssetRow, "id" | "name">): AssetRow {
  return {
    parent_id: null,
    type_id: "custom",
    type_label: "Custom",
    monogram: "??",
    status: "none",
    environment: null,
    owner: null,
    has_children: true,
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
    ...over,
  } as AssetRow;
}

/** The corner of the real estate the acceptance criterion is written over. */
const SITE = row({ id: "asset:hel1", type_id: "site", monogram: "SI", name: "hel1" });
const VM = row({
  id: "asset:vm-db-01",
  parent_id: SITE.id,
  type_id: "vm",
  type_label: "VM",
  monogram: "VM",
  name: "vm-db-01",
  // One ticket links to the VM, which is what the badge on its row counts.
  linked_work: 1,
});
const CONTAINER = row({
  id: "asset:postgres",
  parent_id: VM.id,
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "postgres",
  has_children: false,
  linked_work: 2,
});
const ESTATE = [SITE, VM, CONTAINER];

const TICKET = "jira:PAY-231";

/** One link entry, as `get_asset` hands it over. */
function entry(over: {
  id: string;
  from: string;
  to: string;
  relation: string;
  otherKind?: string;
  otherTitle?: string;
}): LinkEntry {
  return {
    link: {
      id: over.id,
      from_id: over.from,
      to_id: over.to,
      relation: over.relation,
      origin: "manual",
      note: null,
      created_by: "user",
      created_at: "2026-09-06T09:30:00Z",
      confirmed_at: "2026-09-06T09:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: {
      entity_id: over.from === CONTAINER.id ? over.to : over.from,
      kind: over.otherKind ?? "asset",
      title: over.otherTitle ?? "the other end",
      deleted_at: null,
    },
  };
}

/**
 * The container's two links, which are the acceptance criterion's own:
 * `deployed-from` to a ticket, `runs-on` to the VM it sits on.
 */
const DEPLOYED_FROM = entry({
  id: "link-deploy",
  from: CONTAINER.id,
  to: TICKET,
  relation: "deployed-from",
  otherKind: "ticket",
  otherTitle: "Retry storm on payouts",
});
const RUNS_ON = entry({
  id: "link-runs",
  from: CONTAINER.id,
  to: VM.id,
  relation: "runs-on",
  otherTitle: "vm-db-01",
});

/**
 * Every link in the fixture, resolved per asset the way the backend does: the
 * *other* end is whichever end is not the asset being read.
 *
 * Written as a rule rather than as a per-asset table, because the claim under
 * test is that one row reads two ways — a table would let the fixture state
 * both readings itself and the view could not disagree with it.
 */
function linksFor(assetId: string): LinkEntry[] {
  return [DEPLOYED_FROM, RUNS_ON]
    .filter((item) => item.link.from_id === assetId || item.link.to_id === assetId)
    .map((item) => ({
      link: item.link,
      other: {
        ...item.other,
        entity_id: item.link.from_id === assetId ? item.link.to_id : item.link.from_id,
        kind: item.link.from_id === assetId ? item.other.kind : "asset",
        title: item.link.from_id === assetId ? item.other.title : "postgres",
      },
    }));
}

function detailOf(assetId: string): AssetDetail {
  const asset = ESTATE.find((candidate) => candidate.id === assetId);
  if (asset === undefined) throw { code: "not_found", message: `no asset ${assetId}` };
  const heldBy: AssetRow[] = [];
  for (let at = asset; at.parent_id !== null; ) {
    const parent = ESTATE.find((candidate) => candidate.id === at.parent_id);
    if (parent === undefined) break;
    heldBy.unshift(parent);
    at = parent;
  }
  return {
    asset,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: heldBy,
    holds: ESTATE.filter((candidate) => candidate.parent_id === asset.id),
    // Nothing here exposes a route; #432's surface is
    // `AssetsView.write.test.svelte.ts`'.
    exposes: [],
    reachable_via: [],
    history: [],
    links: linksFor(assetId),
    monitors: [],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(hash: string) {
  const reads: string[] = [];
  const unlinked: string[] = [];
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        assetTypes: () => Promise.resolve([]),
        assetTree: (parentId?: string | null) =>
          Promise.resolve(ESTATE.filter((candidate) => candidate.parent_id === (parentId ?? null))),
        getAsset: (assetId: string) => {
          reads.push(assetId);
          try {
            return Promise.resolve(detailOf(assetId));
          } catch (cause) {
            return Promise.reject(cause);
          }
        },
        unlink: (linkId: string) => {
          unlinked.push(linkId);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { router, reads, unlinked };
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/** Wait for the reads the mount issued, then redraw. */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

/** The headings the links panel drew, in order. */
function headings(): string[] {
  return [...target.querySelectorAll(".sec .row.hd span")]
    .map((cell) => cell.textContent?.trim() ?? "")
    .filter((text) => text !== "");
}

/** The other end of each link row, as the reader reads it. */
function linkRows(): string[] {
  return [...target.querySelectorAll(".sec .row.lrow:not(.hd) .lopen")].map(
    (cell) => cell.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

/**
 * The acceptance criterion, at the pane: one `runs-on` row, drawn from the
 * container's side.
 *
 * The ticket is under `deployed from` and the VM under `runs on`, which are
 * the two relations the criterion names.
 */
test("the container's pane reads its links from the container's side", async () => {
  render("#/asset/asset:postgres");
  await settle();

  expect(headings()).toEqual(["deployed from", "runs on"]);
  expect(linkRows()).toEqual(["Retry storm on payouts", "vm-db-01"]);
});

/**
 * The same row, from the other end — the half of the criterion that a panel
 * handed the wrong entity id would fail while still looking right.
 *
 * `hosts` and not `runs on`: the VM is the end the link points *at*, and the
 * reading is computed against the id this pane was given.
 */
test("the VM's pane reads the same link as hosts", async () => {
  render("#/asset/asset:vm-db-01");
  await settle();

  expect(headings()).toEqual(["hosts"]);
  expect(linkRows()).toEqual(["postgres"]);
});

/** Story 36: the action is on the asset, and it opens over the asset. */
test("Link to… opens the dialog over the open asset", async () => {
  render("#/asset/asset:postgres");
  await settle();

  expect(target.textContent).not.toContain("What to link to");
  button("Link to…")?.click();
  flushSync();

  expect(target.textContent, "the dialog's own first field").toContain("What to link to");
  expect(
    [...target.querySelectorAll("h2, .sub, .subtitle")].map((node) => node.textContent?.trim()),
    "and it names the asset it is linking from",
  ).toContain("postgres");
});

/**
 * Unlinking is a mutation like any other here: it goes to the backend and the
 * pane **re-reads**.
 *
 * The second read is the assertion. A view that spliced the row out of its own
 * copy would look identical and would leave the column's linked-work badge
 * saying what it said before.
 */
test("unlinking withdraws the link and re-reads the asset", async () => {
  const { reads, unlinked } = render("#/asset/asset:postgres");
  await settle();
  const before = reads.length;

  target.querySelectorAll<HTMLButtonElement>(".sec .row.lrow .btn")[0]?.click();
  await settle();

  expect(unlinked).toEqual(["link-deploy"]);
  expect(reads.length, "the pane read the asset again").toBeGreaterThan(before);
});

/**
 * Story 32's first badge, on the column rows: the count the backend gave, and
 * **no badge at all** where there is nothing to count.
 *
 * The site is the negative and it is in the same column as nothing else, so
 * the two assertions are about the same read: `hel1` links to no work and
 * draws no badge, while `vm-db-01` draws one and `postgres` draws two.
 */
test("a column row carries its linked-work count and nothing when it is zero", async () => {
  render("#/asset/asset:postgres");
  await settle();

  const badges = [...target.querySelectorAll(".cols .row")].map((rowEl) => [
    rowEl.querySelector(".nm")?.textContent ?? "",
    rowEl.querySelector(".badge.work")?.textContent ?? "",
  ]);
  expect(badges).toEqual([
    ["hel1", ""],
    ["vm-db-01", "1"],
    ["postgres", "2"],
  ]);
});

/** The badge says what it counts, so a number in a circle is readable. */
test("the linked-work badge names what it counts", async () => {
  render("#/asset/asset:postgres");
  await settle();

  const titles = [...target.querySelectorAll(".cols .row .badge.work")].map((node) =>
    node.getAttribute("title"),
  );
  expect(titles).toEqual(["1 linked work item", "2 linked work items"]);
});
