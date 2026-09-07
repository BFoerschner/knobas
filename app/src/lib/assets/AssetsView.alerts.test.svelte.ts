/**
 * The Assets view's list of what is wrong (issue #444, spec #427 story 58).
 *
 * A file of its own, the way #439's Import is: this one is about a strip above
 * the columns that has nothing to do with what the reader has selected, and
 * the files beside it are about a surface that reads one asset at a time.
 *
 * ## What is asserted here, and what is asserted elsewhere
 *
 * Which alerts are open is the **backend's** rule and is pinned at its own
 * seam (`crates/knobas-sync/tests/alerts.rs` for the reconcile,
 * `crates/knobas-app/tests/assets_ipc.rs` for the read): a second copy of
 * *"a crossing into down or warn opens one"* over here would be the copy that
 * goes stale. What is here is the part only this view can get wrong — that the
 * strip is absent when nothing is wrong, that a monitor watching two assets
 * draws a row each, that a monitor watching nothing is still drawn, that an
 * acked alert says so rather than disappearing, that a row is a click to the
 * **asset** rather than to an alert that has no address, and — since #446 —
 * that the ack and the recovery read as sentences in the asset's history.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetDetail, AssetRow, OpenAlert } from "../ipc/assets";
import { createRouter } from "../shell/router.svelte";
import { createAlerts, type Alerts } from "./alerts.svelte";

const { default: AssetsView } = await import("./AssetsView.svelte");

const NOW = new Date(2026, 8, 7, 9, 0, 0, 0);

/** The one asset this fixture's estate holds, so a column draws at all. */
const SITE: AssetRow = {
  id: "asset:hel1",
  parent_id: null,
  type_id: "site",
  type_label: "Site",
  monogram: "SI",
  name: "hel1",
  status: "none",
  environment: null,
  owner: null,
  has_children: true,
  health: "down",
  inside: "down",
  problems_inside: 1,
  linked_work: 0,
};

function detailOf(): AssetDetail {
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
    monitors: [],
    monitoring: [],
    monitor_targets: [],
  };
}

/**
 * One open alert, with the assets its monitor watches.
 *
 * `opened_at` is half an hour before {@link NOW}, so the *ago* reading is an
 * assertion rather than a race.
 */
function alert(over: Partial<OpenAlert> = {}): OpenAlert {
  return {
    id: 1,
    monitor_id: "kuma:1",
    monitor_name: "postgres (tunnel)",
    state: "down",
    opened_at: new Date(2026, 8, 7, 8, 30, 0, 0).toISOString(),
    acked_at: null,
    assets: [{ id: "asset:postgres", name: "postgres", path: "hel1 / vm-db-01" }],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function alertsHolding(open: OpenAlert[]): Alerts {
  return createAlerts({
    openAlerts: () => Promise.resolve(open),
    listen: () => Promise.resolve(() => {}),
  });
}

function render(alerts: Alerts) {
  location.hash = "#/assets/tree";
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      alerts,
      ports: {
        assetTypes: () => Promise.resolve([]),
        assetTree: (parentId?: string | null) =>
          Promise.resolve((parentId ?? null) === null ? [SITE] : []),
        getAsset: () => Promise.resolve(detailOf()),
      },
    },
  });
  flushSync();
  return router;
}

/** The strip's rows, as a reader reads them. */
function rows(): string[] {
  return [...target.querySelectorAll(".alerts li")].map((row) =>
    (row.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

/**
 * Nothing wrong, nothing drawn — the top strip badge's rule, and for its
 * reason: a permanently empty box is a place the eye keeps checking.
 */
test("an estate with nothing wrong in it draws no strip at all", async () => {
  const alerts = alertsHolding([]);
  await alerts.refresh();
  render(alerts);

  expect(target.querySelector(".alerts")).toBeNull();
});

/**
 * The headline: the monitor, the asset, where it lives and how long it has
 * been like this.
 *
 * The path is on the row because two containers called `postgres` are told
 * apart by it and by nothing else — the rule the launcher's own offers follow
 * in this view.
 */
test("an open alert names the monitor, the asset, its path and how long", async () => {
  const alerts = alertsHolding([alert()]);
  await alerts.refresh();
  render(alerts);

  expect(rows()).toEqual(["postgres (tunnel) postgres hel1 / vm-db-01 30 min ago"]);
  expect(target.querySelector(".alerts .st")!.classList.contains("down")).toBe(true);
});

/**
 * A monitor watching two assets draws a row each.
 *
 * *Which* of the two is the question the row exists to answer, and one row
 * naming both would answer neither — a VM and the container on it can honestly
 * both be watched by one HTTP monitor on the product, and the reader's next step
 * is different for each.
 */
test("one alert on a monitor watching two assets draws a row each", async () => {
  const alerts = alertsHolding([
    alert({
      assets: [
        { id: "asset:postgres", name: "postgres", path: "hel1 / vm-db-01" },
        { id: "asset:vm-db-01", name: "vm-db-01", path: "hel1" },
      ],
    }),
  ]);
  await alerts.refresh();
  render(alerts);

  expect(rows()).toEqual([
    "postgres (tunnel) postgres hel1 / vm-db-01 30 min ago",
    "postgres (tunnel) vm-db-01 hel1 30 min ago",
  ]);
});

/**
 * A monitor attached to nothing is still drawn.
 *
 * The direction that fails safely: an alert nobody can act on is the one most
 * worth saying out loud, and a strip that hid it would be quietest about
 * exactly the monitor somebody has not finished wiring up.
 */
test("an alert whose monitor watches nothing is drawn, with nowhere to go", async () => {
  const alerts = alertsHolding([alert({ monitor_name: "canary", assets: [] })]);
  await alerts.refresh();
  render(alerts);

  expect(rows()).toEqual(["canary watching nothing 30 min ago"]);
  expect(target.querySelector(".alerts .go"), "there is no asset to open").toBeNull();
});

/**
 * An acked alert says *acked* and stays.
 *
 * #446's ack clears the **inbox** item and leaves the alert open, so a strip
 * that dropped it would tell the reader the estate was well while it was not.
 */
test("an acked alert is still listed, and marked", async () => {
  const alerts = alertsHolding([
    alert({ acked_at: new Date(2026, 8, 7, 8, 45, 0, 0).toISOString() }),
  ]);
  await alerts.refresh();
  render(alerts);

  expect(rows()[0]).toContain("acked");
});

/** A warn alert is amber, not red — down and warn are two readings. */
test("a warn alert is drawn amber", async () => {
  const alerts = alertsHolding([alert({ state: "warn" })]);
  await alerts.refresh();
  render(alerts);

  const dot = target.querySelector(".alerts .st")!;
  expect(dot.classList.contains("warn")).toBe(true);
  expect(dot.classList.contains("down")).toBe(false);
});

/**
 * The row opens the **asset**, at its address in this view.
 *
 * Spec #427 story 61: the next action is one step away. An alert has no
 * address of its own — the row navigates to the machine, which is where the
 * pane, the monitors and the history are.
 */
test("clicking a row opens the asset it is about", async () => {
  const alerts = alertsHolding([alert()]);
  await alerts.refresh();
  const router = render(alerts);

  target.querySelector<HTMLButtonElement>(".alerts .go")!.click();
  flushSync();

  expect(location.hash).toBe("#/asset/asset:postgres");
  expect(router.route).toEqual({
    view: "assets",
    tab: "tree",
    assetId: "asset:postgres",
  });
});

/**
 * The strip follows the store without a remount, which is what makes it live:
 * an alert closed by a sync run leaves through the store's own re-read.
 */
test("the strip follows the store as alerts open and close", async () => {
  let open: OpenAlert[] = [];
  const alerts = createAlerts({
    openAlerts: () => Promise.resolve(open),
    listen: () => Promise.resolve(() => {}),
  });
  render(alerts);
  expect(target.querySelector(".alerts")).toBeNull();

  open = [alert()];
  await alerts.refresh();
  flushSync();
  expect(rows().length).toBe(1);

  open = [];
  await alerts.refresh();
  flushSync();
  expect(target.querySelector(".alerts"), "a healed estate loses the strip").toBeNull();
});

/**
 * The two lines an alert leaves behind on the asset (#446), as the pane draws
 * them: the ack a person made and the recovery the estate made.
 *
 * Every other verb this pane has no arm for renders as
 * `nothing: nothing → nothing`, which is what these two would do without one
 * — the failure #439's import lines already had once, on every asset in the
 * demo profile.
 */
test("the ack and the recovery are sentences in the asset's history", async () => {
  const alerts = alertsHolding([]);
  await alerts.refresh();
  // The pane, not the strip: the history is what the *selected* asset draws.
  location.hash = "#/asset/asset:hel1";
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      alerts,
      ports: {
        assetTypes: () => Promise.resolve([]),
        assetTree: (parentId?: string | null) =>
          Promise.resolve((parentId ?? null) === null ? [SITE] : []),
        getAsset: () =>
          Promise.resolve({
            ...detailOf(),
            history: [
              {
                id: 2,
                at: new Date(2026, 8, 7, 8, 45, 0, 0).toISOString(),
                actor: "sync:kuma",
                verb: "recovered",
                entity_id: SITE.id,
                detail: { monitor: "kuma:1", monitor_name: "postgres (tunnel)" },
              },
              {
                id: 1,
                at: new Date(2026, 8, 7, 8, 40, 0, 0).toISOString(),
                actor: "user",
                verb: "acked",
                entity_id: SITE.id,
                detail: {
                  item_key: "alert:kuma:1",
                  monitor: "kuma:1",
                  monitor_name: "postgres (tunnel)",
                  state: "down",
                },
              },
            ],
          }),
      },
    },
  });
  flushSync();
  await vi.waitFor(() => {
    flushSync();
    expect(target.textContent ?? "").toContain("Alert closed");
  });

  const text = (target.textContent ?? "").replace(/\s+/g, " ");
  expect(text).toContain("Alert acked: postgres (tunnel) is down");
  expect(text).toContain("Alert closed: postgres (tunnel) recovered");
  expect(text, "neither falls through to the old-to-new composition").not.toContain(
    "nothing → nothing",
  );
});
