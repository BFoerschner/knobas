/**
 * The Import, from the room bar to the third group and out the other side
 * (issue #439, spec #427 stories 21–24).
 *
 * A file of its own, the way #435's linking is: this one drives an
 * `<input type="file">` and a two-step dialog, and the files beside it are
 * about a surface that reads one asset at a time.
 *
 * ## What is asserted here, and what is asserted elsewhere
 *
 * Which properties survive an import is the **backend's** rule and is pinned
 * at its own seam (`crates/knobas-app/tests/assets_ipc.rs`): a second copy of
 * *"a hand edit is any activity line by the user on that property"* in the
 * frontend would be the copy that goes stale. What is here is the part only
 * this view can get wrong — that choosing a file previews rather than
 * imports, that the three groups draw what the preview answered, that a kept
 * property says so *before* the write, that Import sends the file the reader
 * chose, and that the Tree re-reads afterwards rather than patching itself.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail, AssetRow, ImportOutcome, ImportPreview } from "../ipc/assets";
import { createRouter } from "../shell/router.svelte";

const { default: AssetsView } = await import("./AssetsView.svelte");

const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/** The one asset this fixture's estate already holds. */
const SITE: AssetRow = {
  id: "asset:knobas-estate",
  parent_id: null,
  type_id: "site",
  type_label: "Site",
  monogram: "SI",
  name: "knobas test estate",
  status: "none",
  environment: "dev",
  owner: "Björn Förschner",
  has_children: false,
  health: "none",
  inside: "none",
  problems_inside: 0,
  linked_work: 0,
};

/**
 * A preview with something in every group.
 *
 * The shape of the real answer over the real estate file, at the size a test
 * can read: one asset that is new, one already in the tree with two properties
 * changed — one the file wins and one the reader does — and one monitor to
 * attach. A preview with an empty group in it could not tell a view that draws
 * the group from one that drops it.
 */
const PREVIEW: ImportPreview = {
  name: "knobas test estate",
  known: [
    {
      id: "asset:knobas-estate",
      kind: "asset",
      name: "knobas test estate",
      type_label: "Site",
      parent_id: null,
    },
  ],
  new: [
    {
      id: "asset:hetzner-teamcity",
      kind: "asset",
      name: "knobas-teamcity",
      type_label: "VM",
      parent_id: "asset:knobas-estate",
    },
    {
      id: "route:tunnel-jira",
      kind: "route",
      name: "Jira (tunnel)",
      type_label: null,
      parent_id: "asset:notebook",
    },
  ],
  changes: [
    {
      id: "asset:knobas-estate",
      name: "knobas test estate",
      properties: [
        {
          key: "location",
          label: "Location",
          from: { kind: "text", value: "nbg1" },
          to: { kind: "text", value: "hel1" },
          plan: "set",
        },
        {
          key: "provider",
          label: "Provider",
          from: { kind: "text", value: "Hetzner, and my desk" },
          to: { kind: "text", value: "Hetzner Cloud" },
          plan: "kept",
        },
      ],
      monitors: ["knobas-jira"],
    },
  ],
  monitor_links: [
    {
      asset_id: "asset:knobas-estate",
      asset_name: "knobas test estate",
      monitor_name: "gitea",
      monitor_id: "monitor:kuma:3",
    },
  ],
};

const OUTCOME: ImportOutcome = {
  assets_created: 1,
  routes_created: 1,
  properties_set: 1,
  properties_kept: 1,
  monitors_kept: 1,
  monitors_linked: 1,
};

function detailOf(assetId: string): AssetDetail {
  if (assetId !== SITE.id) throw { code: "not_found", message: `no asset ${assetId}` };
  return {
    asset: SITE,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: [],
    holds: [],
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    // The names an import kept (#439). Populated here because the pane draws
    // them, and a fixture with none could not tell a pane that draws the list
    // from one that does not.
    monitors: ["gitea", "jira (tunnel)"],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** What the two Import commands were handed, in order. */
interface Calls {
  previewed: string[];
  applied: string[];
  reads: number;
}

function render(refusal: unknown = null, hash = "#/assets/tree") {
  const calls: Calls = { previewed: [], applied: [], reads: 0 };
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        assetTypes: () => Promise.resolve([]),
        assetTree: (parentId?: string | null) => {
          calls.reads += 1;
          return Promise.resolve((parentId ?? null) === null ? [SITE] : []);
        },
        getAsset: (assetId: string) => {
          try {
            return Promise.resolve(detailOf(assetId));
          } catch (cause) {
            return Promise.reject(cause);
          }
        },
        previewEstateImport: (file: string) => {
          calls.previewed.push(file);
          return refusal === null ? Promise.resolve(PREVIEW) : Promise.reject(refusal);
        },
        applyEstateImport: (file: string) => {
          calls.applied.push(file);
          return Promise.resolve(OUTCOME);
        },
      },
    },
  });
  flushSync();
  return calls;
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

/**
 * Wait for the reads in flight, then redraw.
 *
 * A macrotask as well as the microtask turns every other suite here uses:
 * `File.text()` is the platform's own read and does not resolve on the
 * microtask queue, so a `Promise.resolve()` ladder alone lands before the
 * dialog has the file's text.
 */
async function settle() {
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
  await new Promise((done) => setTimeout(done, 0));
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
  flushSync();
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

/** Choose `text` as the estate file, the way a reader chooses one. */
async function choose(text: string) {
  const input = target.querySelector<HTMLInputElement>('input[type="file"]');
  if (!input) throw new Error("the dialog draws no file input");
  Object.defineProperty(input, "files", {
    configurable: true,
    value: [new File([text], "estate.json", { type: "application/json" })],
  });
  // `bubbles`, and not decoration: Svelte 5 delegates `change` to the mount
  // root, so an event that does not bubble reaches no listener at all and the
  // dialog sits there having been handed a file it never heard about.
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

/** The headings of the preview's groups, in the order they are drawn. */
function groups(): string[] {
  return [...target.querySelectorAll(".grp h3")].map(
    (heading) => heading.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
}

/**
 * The monitor names an import kept are on the asset's pane (#439's fourth
 * criterion, at the surface a reader can see it on).
 *
 * The section is drawn only when there are names: *Exposes* and *Reachable
 * via* answer a question about every asset, and this one is the estate file's
 * own note. So the assertion is both — the names on an asset that has them,
 * and no heading at all on an estate nobody imported.
 */
test("the pane lists the monitor names an import kept", async () => {
  render(null, `#/asset/${SITE.id}`);
  await settle();

  const section = [...target.querySelectorAll(".grp")].find(
    (group) => group.querySelector("h3")?.textContent?.includes("Monitors") ?? false,
  );
  expect(
    [...(section?.querySelectorAll("li") ?? [])].map((row) => row.textContent?.trim()),
  ).toEqual(["gitea", "jira (tunnel)"]);
});

/**
 * The acceptance criterion, at the dialog: choosing a file **previews** — it
 * does not import — and the answer is drawn in its three groups.
 *
 * The count in every heading is the preview's own, so a view that drew the
 * headings and dropped the lists would fail here, and so would one that put
 * a group's entries under another group's name.
 */
test("choosing a file previews it and draws the three groups", async () => {
  const calls = render();
  await settle();

  button("Import")?.click();
  flushSync();
  expect(target.querySelector('input[type="file"]')).not.toBeNull();
  expect(calls.previewed).toEqual([]);

  await choose('{"name":"knobas test estate"}');

  expect(calls.previewed).toEqual(['{"name":"knobas test estate"}']);
  expect(calls.applied).toEqual([]);
  expect(groups()).toEqual([
    "New — 2",
    "Would change — 1",
    "Monitors to attach — 1",
    "Already in the tree — 1",
  ]);

  const listed = [...target.querySelectorAll(".grp .lst .nm")].map((cell) =>
    cell.textContent?.trim(),
  );
  expect(listed).toEqual([
    "knobas-teamcity",
    "Jira (tunnel)",
    "knobas test estate",
    "knobas test estate",
  ]);
});

/**
 * Story 24, said **before** the write: the property the reader edited is drawn
 * as staying, and the one nobody touched is drawn as changing.
 *
 * The two lines are asserted together because a dialog that drew every change
 * the same way would satisfy either one alone — and the whole promise is that
 * a reader can tell, from the preview, which of their own values a file is
 * about to leave alone.
 */
test("the preview says which value wins on each changed property", async () => {
  render();
  await settle();
  button("Import")?.click();
  flushSync();
  await choose("{}");

  const lines = [...target.querySelectorAll(".props > li")].map(
    (row) => row.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
  expect(lines).toEqual([
    "Location nbg1 → hel1",
    "Provider Hetzner, and my desk stays — you edited it here Hetzner Cloud",
    "monitors + knobas-jira",
  ]);
});

/**
 * Import sends the file the reader chose, and the Tree re-reads afterwards.
 *
 * The re-read is the assertion that matters: an import creates assets under
 * an address that has not changed, so a view that patched its own columns
 * would draw an estate nobody answered with. The count is *more* reads after
 * the import than before it, rather than an exact number, because how many
 * columns a walk opens is `AssetsView.test.svelte.ts`' subject.
 */
test("Import applies the chosen file and the Tree re-reads", async () => {
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();
  await choose('{"assets":[]}');

  const before = calls.reads;
  const apply = [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === "Import" && candidate.closest(".dlg") !== null,
  );
  expect(apply?.disabled).toBe(false);
  apply?.click();
  await settle();

  expect(calls.applied).toEqual(['{"assets":[]}']);
  expect(target.querySelector('input[type="file"]')).toBeNull();
  expect(calls.reads).toBeGreaterThan(before);
});

/**
 * A file the backend refuses is refused **in the dialog**, in the backend's
 * own words, and the Import button stays out of reach.
 *
 * The refusal is the one the backend actually sends — a file that is not an
 * estate file — because the frontend has no rule of its own about what an
 * estate file is and must not grow one.
 */
test("a file the backend refuses says so in the dialog and cannot be applied", async () => {
  const calls = render({
    code: "invalid",
    message: "this is not an estate file: expected value at line 1 column 1",
  });
  await settle();
  button("Import")?.click();
  flushSync();
  await choose("{ this is not json");

  expect(target.querySelector('[role="alert"]')?.textContent).toContain("not an estate file");
  expect(groups()).toEqual([]);
  const apply = [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === "Import" && candidate.closest(".dlg") !== null,
  );
  expect(apply?.disabled).toBe(true);
  expect(calls.applied).toEqual([]);
});
