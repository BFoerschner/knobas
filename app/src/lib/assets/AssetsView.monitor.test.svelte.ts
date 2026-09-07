/**
 * *Create monitor for this asset* — the pane's control and its two writes
 * (issue #453, spec #427 story 70).
 *
 * A file of its own for `AssetsView.write.test.svelte.ts`' reason: this needs
 * ports that record two different calls and a detail whose `monitor_targets`
 * moves between tests, and mixing it into the write file would make its
 * assertions depend on which of them ran.
 *
 * **What is witnessed here is the gesture, not the write's effect.** Whether a
 * `create_monitor` reaches Uptime Kuma is `just kuma-live`'s claim, and whether
 * the recorded name becomes a `monitored-by` link is `knobas-sync`'s
 * `tests/attach.rs`. What is this file's is narrower and is the half a reader
 * touches: the control is drawn only where a source could take it, the form
 * opens on what knobas already knows, and pressing *Create* records the name
 * **before** it queues the write.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  AssetDetail,
  AssetEdit,
  AssetRow,
  MonitorTarget,
  RouteRow,
} from "../ipc/assets";
import type { WriteOpPayload } from "../ipc/sources";
import { createRouter } from "../shell/router.svelte";
import AssetsView from "./AssetsView.svelte";

const NOW = new Date(2026, 8, 7, 12, 0, 0, 0);

const KUMA: MonitorTarget = {
  source_id: "kuma",
  display_name: "Uptime Kuma",
  roster: "kuma:monitors",
};
const KUMA_EU: MonitorTarget = {
  source_id: "kuma-eu",
  display_name: "Kuma EU",
  roster: "kuma-eu:monitors",
};

function row(over: Partial<AssetRow> = {}): AssetRow {
  return {
    id: "asset:knobas-gitea",
    parent_id: null,
    type_id: "container",
    type_label: "Container",
    monogram: "CT",
    name: "knobas-gitea",
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

const ROUTE: RouteRow = {
  id: "route:notebook-gitea",
  asset_id: "asset:notebook",
  asset_name: "notebook",
  target_id: "asset:knobas-gitea",
  target_name: "knobas-gitea",
  name: "Gitea",
  url: "http://127.0.0.1:3000/",
  visibility: "internal",
  properties: [],
};

/**
 * The bridge, over one asset that never changes shape but whose recorded
 * monitor names do.
 *
 * A store rather than a stub for the write file's reason: the second read is
 * where *the name is on the asset* becomes visible, and that is the assertion
 * the criterion is really about.
 */
function estate(targets: MonitorTarget[], reachable: RouteRow[] = [ROUTE]) {
  const monitors: string[] = [];
  const edits: { assetId: string; edits: AssetEdit[] }[] = [];
  const queued: WriteOpPayload[] = [];
  /** Set to reject the next call of that name, in the backend's own words. */
  let refuseRecord: { code: string; message: string } | null = null;
  let refuseQueue: { code: string; message: string } | null = null;

  return {
    monitors,
    edits,
    queued,
    refuse(
      which: "record" | "queue",
      refusal: { code: string; message: string },
    ) {
      if (which === "record") refuseRecord = refusal;
      else refuseQueue = refusal;
    },
    assetTypes: () => Promise.resolve([]),
    assetTree: (parentId?: string | null) =>
      Promise.resolve(
        parentId === null || parentId === undefined ? [row()] : [],
      ),
    getAsset: (): Promise<AssetDetail> =>
      Promise.resolve({
        asset: row(),
        properties: [],
        effective_environment: null,
        effective_owner: null,
        held_by: [],
        holds: [],
        exposes: [],
        reachable_via: reachable,
        history: [],
        links: [],
        // Read back out of the store, so the pane's *not in Kuma yet* list is
        // this fixture's second read rather than a constant.
        monitors: [...monitors],
        monitoring: [],
        monitor_targets: targets,
      }),
    editAsset: (assetId: string, list: AssetEdit[]) => {
      if (refuseRecord !== null) return Promise.reject(refuseRecord);
      edits.push({ assetId, edits: list });
      for (const edit of list) {
        if (edit.field !== "monitors") continue;
        for (const name of edit.added)
          if (!monitors.includes(name)) monitors.push(name);
      }
      return Promise.resolve(row());
    },
    submitWrite: (payload: WriteOpPayload) => {
      if (refuseQueue !== null) return Promise.reject(refuseQueue);
      queued.push(payload);
      // The queue's own answer is a row, and nothing in this view reads it:
      // the pane re-reads instead. `undefined as never` would be a lie the
      // types allow; this is the honest minimum the caller ignores.
      return Promise.resolve({}) as never;
    },
    openExternal: () => Promise.resolve(),
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(ports: ReturnType<typeof estate>) {
  location.hash = "#/asset/asset:knobas-gitea";
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: { router, now: () => NOW, ports: ports as never },
  });
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function reachable(): ParentNode {
  return target.querySelector('[role="dialog"]') ?? target;
}

function names(): string[] {
  return [...reachable().querySelectorAll("button")].map(
    (candidate) =>
      (candidate.getAttribute("title") ??
        candidate.getAttribute("aria-label") ??
        (candidate.textContent ?? "").trim()) ||
      "?",
  );
}

function button(label: string): HTMLButtonElement {
  const found = [
    ...reachable().querySelectorAll<HTMLButtonElement>("button"),
  ].find((candidate) => (candidate.textContent ?? "").trim() === label);
  if (found === undefined) {
    throw new Error(
      `no button “${label}” — the buttons are: ${names().join(" | ")}`,
    );
  }
  return found;
}

function click(label: string) {
  button(label).click();
  flushSync();
}

function field(label: string): HTMLInputElement | HTMLSelectElement {
  const where = reachable();
  const tag = [...where.querySelectorAll("label")].find(
    (candidate) => (candidate.textContent ?? "").trim() === label,
  );
  const found =
    tag === undefined
      ? null
      : where.querySelector<HTMLInputElement | HTMLSelectElement>(
          `#${CSS.escape(tag.htmlFor)}`,
        );
  if (found === null) throw new Error(`no field “${label}”`);
  return found;
}

function type(label: string, value: string) {
  const control = field(label);
  control.value = value;
  control.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function choose(label: string, value: string) {
  const control = field(label);
  control.value = value;
  control.dispatchEvent(new Event("input", { bubbles: true }));
  control.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * **The criterion's negative: without the account the action is absent.**
 *
 * An Uptime Kuma configured with only its API key declares no write ops, so
 * `monitor_targets` comes back empty and the pane draws nothing — not a
 * disabled button, not an explanatory line. The other assertion is what makes
 * this a statement about the *targets* rather than about the fixture: the pane
 * is fully drawn, it just has no create in it.
 */
test("a pane with no monitor target draws no create", async () => {
  const store = estate([]);
  render(store);
  await vi.waitFor(() => expect(text()).toContain("knobas-gitea"));

  expect(names()).not.toContain("Create monitor for this asset");
  expect(text()).not.toContain("Create monitor");
});

/**
 * **The headline**: the form opens prefilled, and *Create* records the name
 * and then queues the write — in that order.
 *
 * The order is the design (`MonitorDialog`'s header): the name is what
 * attaches the monitor when the next poll mirrors it, so a create whose write
 * landed with no name recorded would be a monitor watching this asset that
 * knobas joined to nothing. It is asserted as an order and not as two facts,
 * because two facts hold whichever way round they happened.
 */
test("create records the name first and then queues the write", async () => {
  const store = estate([KUMA]);
  const order: string[] = [];
  const recording = store.editAsset;
  const queueing = store.submitWrite;
  store.editAsset = (assetId, list) => {
    order.push("record");
    return recording(assetId, list);
  };
  store.submitWrite = ((payload: WriteOpPayload) => {
    order.push("queue");
    return queueing(payload);
  }) as typeof store.submitWrite;

  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  // Prefilled from what knobas already knows: the asset's name, and the URL of
  // the route that reaches it.
  expect((field("Name") as HTMLInputElement).value).toBe("knobas-gitea");
  expect((field("URL to check") as HTMLInputElement).value).toBe(
    "http://127.0.0.1:3000/",
  );
  // One target, so no picker — a `<select>` over one option is a question with
  // one answer.
  expect(text()).toContain("In Uptime Kuma");

  click("Create");
  await vi.waitFor(() => expect(store.queued).toHaveLength(1));

  expect(order).toEqual(["record", "queue"]);
  expect(store.edits).toEqual([
    {
      assetId: "asset:knobas-gitea",
      edits: [{ field: "monitors", added: ["knobas-gitea"] }],
    },
  ]);
  expect(store.queued).toEqual([
    {
      CreateMonitor: {
        entity: "kuma:monitors",
        name: "knobas-gitea",
        url: "http://127.0.0.1:3000/",
      },
    },
  ]);
});

/**
 * The name and the URL are the reader's to change, and the **roster** is not.
 *
 * The entity a create targets is composed by the backend
 * (`assets::MonitorTarget.roster`); a form that built `` `${id}:monitors` ``
 * would be a per-adapter table in a component. So this presses the second of
 * two sources and asserts the entity is *that source's* roster rather than the
 * first's — which a hardcoded string could not get right.
 */
test("a second source is picked, and its own roster is what the write targets", async () => {
  const store = estate([KUMA, KUMA_EU]);
  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  type("Name", "gitea (local)");
  type("URL to check", "http://127.0.0.1:3000/api/healthz");
  choose("Create it in", "kuma-eu");
  click("Create");

  await vi.waitFor(() => expect(store.queued).toHaveLength(1));
  expect(store.queued[0]).toEqual({
    CreateMonitor: {
      entity: "kuma-eu:monitors",
      name: "gitea (local)",
      url: "http://127.0.0.1:3000/api/healthz",
    },
  });
  expect(store.monitors).toEqual(["gitea (local)"]);
});

/**
 * After the create, the pane says what is true **now**: the name is on the
 * asset and no monitor answers to it yet.
 *
 * That is the state #439 already draws and a reader already has a word for, so
 * the create leaves them somewhere the app can describe rather than in a
 * moment of silence. The monitor itself is up to a poll away, which is why
 * nothing here claims it exists.
 */
test("the recorded name lands in the pane's waiting list", async () => {
  const store = estate([KUMA]);
  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  type("Name", "gitea");
  click("Create");

  await vi.waitFor(() =>
    expect(text()).toContain("Named by the import, not in Kuma yet"),
  );
  expect(text()).toContain("gitea");
});

/**
 * A refused **queue** leaves the dialog open with the backend's own words in
 * it, and does not report the create as done.
 *
 * The recorded name stays, deliberately: it is what a retry needs, and the
 * pane says it is waiting. What must not happen is the dialog closing on a
 * write that never went — the reader would have no way to tell that from one
 * that did.
 */
test("a refused write keeps the dialog open and says why", async () => {
  const store = estate([KUMA]);
  store.refuse("queue", {
    code: "invalid",
    message: 'source "kuma" does not offer "create_monitor"',
  });
  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  click("Create");

  await vi.waitFor(() =>
    expect(text()).toContain('does not offer "create_monitor"'),
  );
  expect(store.queued).toEqual([]);
  // Still open: the *Create* button is still reachable, which it would not be
  // if the dialog had closed over the failure.
  expect(names()).toContain("Create");
});

/**
 * A refused **record** never reaches the queue at all.
 *
 * The order's other half: if the name could not be written, queueing the
 * create would make the monitor knobas cannot attach — which is the exact
 * state the order exists to prevent, arrived at by a different door.
 */
test("a name that could not be recorded queues nothing", async () => {
  const store = estate([KUMA]);
  store.refuse("record", { code: "invalid", message: "an asset needs a name" });
  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  click("Create");

  await vi.waitFor(() => expect(text()).toContain("an asset needs a name"));
  expect(store.queued).toEqual([]);
});

/**
 * An asset with nothing to guess a URL from opens the form empty, and *Create*
 * is not pressable until the reader has typed one.
 *
 * The blank guard is here and not only in the adapter because the two refuse
 * different things: the adapter refuses a URL it cannot deliver, and this
 * refuses a form that has not been filled in. A button that queued an empty
 * URL would be a write that exists only to come back refused.
 */
test("a form with no URL cannot be submitted", async () => {
  const store = estate([KUMA], []);
  render(store);
  await vi.waitFor(() =>
    expect(names()).toContain("Create monitor for this asset"),
  );

  click("Create monitor for this asset");
  expect((field("URL to check") as HTMLInputElement).value).toBe("");
  expect(button("Create").disabled).toBe(true);

  type("URL to check", "http://127.0.0.1:3000/");
  expect(button("Create").disabled).toBe(false);
});
