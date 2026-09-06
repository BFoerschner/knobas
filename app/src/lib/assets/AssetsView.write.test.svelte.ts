/**
 * The Tree's writes: creating, editing, moving and deleting from the surface
 * (issue #429, spec #427 stories 3, 6, 11, 17, 18 and 34).
 *
 * A file of its own rather than more tests in `AssetsView.test.svelte.ts`,
 * which is #428's and stays about the *read*: what a reader sees at an
 * address. This one needs a fixture that changes when it is written to, and
 * mixing the two would make every read assertion depend on what an earlier
 * write left behind.
 *
 * ## The fake is a store, not a stub
 *
 * `estate()` below answers all seven commands out of one array, so a create is
 * visible to the next `asset_tree`, a move rewrites the path the next
 * `get_asset` walks, and an edit appends the history line the pane draws. That
 * is the only way to witness the criteria as written — *the new asset appears
 * in its column selected*, *the columns redraw at the new path*, *the history
 * shows old and new value* — because every one of them is about what the
 * **second** read says.
 *
 * ## What the refusal assertions are, and are not
 *
 * The cycle and the non-empty-branch refusals are written here in the
 * sentences `assets::move_to` and `assets::delete` use. What that witnesses is
 * that the view shows **the backend's own message, in place**, rather than one
 * it composed itself — which is the criterion (*"a cycle is refused with the
 * message the command gives"*). That those are the sentences the real commands
 * answer with is `crates/knobas-app/tests/assets_ipc.rs`'s claim, over a real
 * PostgreSQL, and it is deliberately not restated here.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  AssetDetail,
  AssetEdit,
  AssetProperty,
  AssetRow,
  AssetType,
  PropertyValue,
} from "../ipc/assets";
import type { ActivityRow } from "../ipc/entity";
import { createRouter } from "../shell/router.svelte";
import AssetsView from "./AssetsView.svelte";

const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/**
 * A corner of the **real** type table, ids, monograms and suggestions
 * included — spec #427 rules that nothing Tidewater-shaped is invented for
 * assets, and the suggestions are the ones `knobas_core::asset` declares.
 */
const TYPES: AssetType[] = [
  {
    id: "site",
    label: "Site",
    monogram: "SI",
    properties: [{ key: "location", label: "Location", kind: "text" }],
    suggests: ["hypervisor", "vm"],
  },
  {
    id: "vm",
    label: "VM",
    monogram: "VM",
    properties: [
      { key: "hostname", label: "Hostname", kind: "text" },
      { key: "ip", label: "IP", kind: "text" },
    ],
    suggests: ["container_engine", "service"],
  },
  {
    id: "container_engine",
    label: "Container engine",
    monogram: "CE",
    properties: [{ key: "version", label: "Version", kind: "text" }],
    suggests: ["container"],
  },
  {
    id: "container",
    label: "Container",
    monogram: "CT",
    properties: [
      { key: "image", label: "Image", kind: "text" },
      { key: "ports", label: "Ports", kind: "text" },
    ],
    suggests: [],
  },
  {
    id: "service",
    label: "Service",
    monogram: "SV",
    properties: [{ key: "url", label: "URL", kind: "url" }],
    suggests: [],
  },
];

interface Stored {
  id: string;
  parent_id: string | null;
  type_id: string;
  name: string;
  properties: Record<string, PropertyValue>;
}

/** One in-memory estate, answering the seven commands the view calls. */
function estate(seed: Stored[]) {
  const rows = [...seed];
  const lines: ActivityRow[] = [];
  let minted = 0;
  let at = 0;

  function record(entityId: string, verb: string, detail: unknown) {
    at += 1;
    lines.push({
      id: at,
      at: new Date(NOW.getTime() - 1000 * (100 - at)).toISOString(),
      actor: "user",
      verb,
      entity_id: entityId,
      detail,
    } as ActivityRow);
  }

  function schema(typeId: string): AssetType | undefined {
    return TYPES.find((type) => type.id === typeId);
  }

  function row(stored: Stored): AssetRow {
    const type = schema(stored.type_id);
    return {
      id: stored.id,
      parent_id: stored.parent_id,
      type_id: stored.type_id,
      type_label: type?.label ?? stored.type_id,
      monogram: type?.monogram ?? "??",
      name: stored.name,
      status: "none",
      environment: null,
      owner: null,
      has_children: rows.some((other) => other.parent_id === stored.id),
      // #431's rollup, which nothing in this fixture can move: `Stored`
      // carries no status, so every asset here is unrated and every rollup
      // over unrated assets is `none` with nothing to count. Writing the
      // three out rather than computing them keeps this fake about the
      // writes it exists to witness -- `AssetsView.test.svelte.ts` is where
      // the badge is exercised.
      health: "none",
      inside: "none",
      problems_inside: 0,
    };
  }

  /** `assets::CHILDREN`'s `order by a.name asc, a.id asc`. */
  function children(parentId: string | null): AssetRow[] {
    return rows
      .filter((candidate) => candidate.parent_id === parentId)
      .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id))
      .map(row);
  }

  function find(id: string): Stored {
    const found = rows.find((candidate) => candidate.id === id);
    if (found === undefined) throw { code: "not_found", message: `no asset ${id}` };
    return found;
  }

  /** `assets::properties_of`: declared keys in schema order, custom after. */
  function properties(stored: Stored): AssetProperty[] {
    const declared = schema(stored.type_id)?.properties ?? [];
    const claimed = declared.map((property) => property.key);
    return [
      ...declared.map((property) => ({
        key: property.key,
        label: property.label,
        value: stored.properties[property.key] ?? null,
        custom: false,
      })),
      ...Object.keys(stored.properties)
        .filter((key) => !claimed.includes(key))
        .sort()
        .map((key) => ({
          key,
          label: key,
          value: stored.properties[key]!,
          custom: true,
        })),
    ];
  }

  function ancestors(stored: Stored): AssetRow[] {
    const walk: AssetRow[] = [];
    for (let parent = stored.parent_id; parent !== null;) {
      const held = rows.find((candidate) => candidate.id === parent);
      if (held === undefined) break;
      walk.unshift(row(held));
      parent = held.parent_id;
    }
    return walk;
  }

  return {
    /** What is in the estate right now, for an assertion that is not on markup. */
    rows,
    assetTypes: () => Promise.resolve(TYPES),
    assetTree: (parentId?: string | null) => Promise.resolve(children(parentId ?? null)),
    getAsset: (assetId: string): Promise<AssetDetail> => {
      try {
        const stored = find(assetId);
        return Promise.resolve({
          asset: row(stored),
          properties: properties(stored),
          // Nothing in this fixture sets either, so nothing is in force --
          // the same reason the rollup above is flat.
          effective_environment: null,
          effective_owner: null,
          held_by: ancestors(stored),
          holds: children(stored.id),
          history: lines.filter((line) => line.entity_id === assetId).reverse(),
        });
      } catch (cause) {
        return Promise.reject(cause);
      }
    },
    createAsset: (typeId: string, name: string, parentId?: string | null) => {
      if (name.trim() === "") {
        return Promise.reject({
          code: "invalid",
          message: "an asset needs a name",
        });
      }
      minted += 1;
      const stored: Stored = {
        id: `asset:new-${minted}`,
        parent_id: parentId ?? null,
        type_id: typeId,
        name: name.trim(),
        properties: {},
      };
      rows.push(stored);
      record(stored.id, "created", {
        asset: { type: typeId, name: stored.name },
      });
      return Promise.resolve(row(stored));
    },
    editAsset: (assetId: string, edits: AssetEdit[]) => {
      const stored = find(assetId);
      for (const edit of edits) {
        if (edit.field === "name") {
          const from = stored.name;
          stored.name = edit.value.trim();
          record(assetId, "renamed", { field: "name", from, to: stored.name });
          continue;
        }
        if (edit.field !== "property") continue;
        const from = stored.properties[edit.key] ?? null;
        if (edit.value === null) delete stored.properties[edit.key];
        else stored.properties[edit.key] = edit.value;
        record(assetId, "edited", {
          field: "property",
          key: edit.key,
          from,
          to: edit.value,
        });
      }
      return Promise.resolve(row(stored));
    },
    moveAsset: (assetId: string, newParentId: string | null) => {
      const stored = find(assetId);
      if (newParentId !== null) {
        // `assets::cycle_through`: the walk up from the proposed parent, and
        // the name one step *below* the moved asset on it.
        let below: string | null = null;
        for (let walk: string | null = newParentId; walk !== null;) {
          const step = rows.find((candidate) => candidate.id === walk);
          if (step === undefined) break;
          if (step.id === assetId) {
            return Promise.reject({
              code: "invalid",
              message:
                `moving "${stored.name}" under "${find(newParentId).name}" would make a ` +
                `cycle: "${below ?? step.name}" is already held by "${stored.name}"`,
            });
          }
          below = step.name;
          walk = step.parent_id;
        }
      }
      stored.parent_id = newParentId;
      record(assetId, "moved", { field: "parent", to: newParentId });
      return Promise.resolve(row(stored));
    },
    deleteAsset: (assetId: string) => {
      const stored = find(assetId);
      const held = rows.filter((candidate) => candidate.parent_id === assetId).length;
      if (held > 0) {
        return Promise.reject({
          code: "conflict",
          message: `"${stored.name}" still holds ${held} asset(s) -- move or delete them first`,
        });
      }
      rows.splice(rows.indexOf(stored), 1);
      return Promise.resolve();
    },
  };
}

/** hel1 › vm-db-01 › postgres, plus a sibling VM so a column has two rows. */
function seed(): Stored[] {
  return [
    {
      id: "asset:hel1",
      parent_id: null,
      type_id: "site",
      name: "hel1",
      properties: {},
    },
    {
      id: "asset:vm-db-01",
      parent_id: "asset:hel1",
      type_id: "vm",
      name: "vm-db-01",
      properties: { hostname: { kind: "text", value: "vm-db-01" } },
    },
    {
      id: "asset:vm-app-02",
      parent_id: "asset:hel1",
      type_id: "vm",
      name: "vm-app-02",
      properties: {},
    },
    {
      id: "asset:postgres",
      parent_id: "asset:vm-db-01",
      type_id: "container",
      name: "postgres",
      properties: { image: { kind: "text", value: "postgres:18" } },
    },
  ];
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(hash: string, ports: ReturnType<typeof estate>) {
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, { target, props: { router, now: () => NOW, ports } });
  flushSync();
  return { router };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function columns(): string[][] {
  return [...target.querySelectorAll("ol.col")].map((column) =>
    [...column.querySelectorAll("button.row span.nm")].map((name) =>
      (name.textContent ?? "").trim(),
    ),
  );
}

function marked(): (string | null)[] {
  return [...target.querySelectorAll("ol.col")].map((column) => {
    const on = column.querySelector('button.row[aria-current="true"] span.nm');
    return on === null ? null : (on.textContent ?? "").trim();
  });
}

/**
 * Where a click lands: inside the open dialog, or in the view when there is
 * none.
 *
 * A modal traps focus and the scrim swallows everything behind it, so this is
 * what a person can actually reach. It also keeps the tests honest about
 * *which* control they mean: the pane's *Delete* and the confirmation's are
 * the same word, and a helper that took the first one in the document would
 * have confirmed nothing.
 */
function reachable(): ParentNode {
  return target.querySelector('[role="dialog"]') ?? target;
}

/** The first reachable button whose visible text, name or title is `label`. */
function button(label: string): HTMLButtonElement {
  const found = [...reachable().querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) =>
      (candidate.textContent ?? "").trim() === label ||
      candidate.getAttribute("aria-label") === label ||
      candidate.getAttribute("title") === label,
  );
  if (found === undefined) {
    throw new Error(`no button “${label}” — the buttons are: ${buttonNames().join(" | ")}`);
  }
  return found;
}

function buttonNames(): string[] {
  return [...reachable().querySelectorAll("button")].map(
    (candidate) =>
      (candidate.getAttribute("title") ??
        candidate.getAttribute("aria-label") ??
        (candidate.textContent ?? "").trim()) ||
      "?",
  );
}

/** Click, and let the effects the click set off run. */
function click(label: string) {
  button(label).click();
  flushSync();
}

/** A reachable field, by its `aria-label` or by the `<label>` pointing at it. */
function field(label: string): HTMLInputElement | HTMLSelectElement {
  const where = reachable();
  const named = where.querySelector<HTMLInputElement | HTMLSelectElement>(
    `[aria-label="${label}"]`,
  );
  if (named !== null) return named;
  const tag = [...where.querySelectorAll("label")].find(
    (candidate) => (candidate.textContent ?? "").trim() === label,
  );
  const found =
    tag === undefined
      ? null
      : where.querySelector<HTMLInputElement | HTMLSelectElement>(`#${CSS.escape(tag.htmlFor)}`);
  if (found === null) throw new Error(`no field “${label}”`);
  return found;
}

/**
 * Walk the move picker one level down, once that level has come back.
 *
 * Each step is a read, so the row cannot be clicked until it is there — which
 * is the picker's own claim: it walks the estate one `asset_tree` at a time
 * rather than holding a copy of it.
 */
async function walkInto(name: string) {
  await vi.waitFor(() => button(`Open ${name}`));
  click(`Open ${name}`);
}

/** Type into a field the way a person does — the view listens to `input`. */
function type(label: string, value: string) {
  const control = field(label);
  control.value = value;
  control.dispatchEvent(new Event("input", { bubbles: true }));
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
 * **The plus on the first column creates at the top of the estate, and the new
 * asset lands selected in its own column with its type's monogram.**
 *
 * Criterion 1's first half. Nothing here tells the view what was created: the
 * dialog answers with a row, the view goes to its **address**, and everything
 * on screen is the read that follows — which is why the assertion is on the
 * column and the pane rather than on a callback.
 *
 * At the top there is no parent, so there is no convention: the dialog says so
 * by drawing no *usual here* line at all.
 */
test("the first column's plus creates at the top of the estate", async () => {
  const store = estate(seed());
  const { router } = render("#/assets/tree", store);
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  click("New asset in Estate");
  expect(text()).toContain("At the top of the estate");
  expect(text()).not.toContain("Usual here");

  type("Name", "fsn1");
  click("Create");

  await vi.waitFor(() => expect(columns()).toEqual([["fsn1", "hel1"]]));
  expect(router.route).toEqual({
    view: "assets",
    tab: "tree",
    assetId: "asset:new-1",
  });
  expect(marked()).toEqual(["fsn1"]);
  // The type's monogram, resolved by the backend out of the table — the chip a
  // column row draws.
  expect(target.querySelector("ol.col button.row.on span.mg")?.textContent?.trim()).toBe("SI");
  expect(text()).toContain("Created");
});

/**
 * **A plus deeper in creates under that column's asset, and the conventions
 * say what usually goes there.**
 *
 * Criterion 1's second half, and story 17's *usual here*. The heading names the
 * level, so a `+` over the third column is not a bare plus over an unnamed
 * list — and the dialog opens on the first suggestion rather than on whatever
 * the table happens to start with.
 */
test("a deeper column's plus creates inside that column's asset, with its conventions", async () => {
  const store = estate(seed());
  render("#/asset/asset:vm-db-01", store);
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  expect(text()).toContain("In vm-db-01");
  // The VM's own suggestions, drawn from the type table and not from a copy.
  expect(text()).toContain("Usual here: Container engine, Service");

  type("Name", "redis");
  click("Create");

  await vi.waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres", "redis"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-db-01", "redis"]);
  expect(store.rows.find((row) => row.name === "redis")?.parent_id).toBe("asset:vm-db-01");
});

/**
 * **A typed property and a custom property are both edited in the pane, and
 * each edit's history line carries its old and its new value.**
 *
 * Criterion 2, and story 11's whole point: *"who changed the IP"* is only
 * answerable if the old value was written down at the moment it stopped being
 * true. The typed one is edited **from empty** — a declared key nobody has
 * filled in — because that is the case the type table has to be on the wire
 * for: there is no kind in a `null`, so without the schema the editor would be
 * guessing what to send.
 */
test("a typed and a custom property are edited in the pane and the history says so", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await vi.waitFor(() => expect(text()).toContain("Image postgres:18"));
  expect(text()).toContain("Ports —");

  // A declared key with nothing in it: the em dash is the button.
  click("Edit Ports");
  type("Ports", "5432");
  click("Save");
  await vi.waitFor(() => expect(text()).toContain("Ports 5432"));
  expect(text()).toContain("ports: nothing → 5432");

  // Now one of the reader's own, of a kind the type never declared.
  click("Add a property");
  type("New property key", "backup window");
  type("New property value", "02:00");
  click("Add");
  await vi.waitFor(() => expect(text()).toContain("backup window 02:00"));

  // Changing it again is where old-and-new is actually witnessed: the first
  // write's "from" was empty, and an editor that sent the new value as both
  // would have passed that.
  click("Edit backup window");
  type("backup window", "03:30");
  click("Save");
  await vi.waitFor(() => expect(text()).toContain("backup window: 02:00 → 03:30"));

  expect(store.rows.find((row) => row.id === "asset:postgres")?.properties).toEqual({
    image: { kind: "text", value: "postgres:18" },
    ports: { kind: "text", value: "5432" },
    "backup window": { kind: "text", value: "03:30" },
  });
});

/**
 * **A move redraws the columns at the new path.**
 *
 * Criterion 3's first half. The address does not change — the asset keeps its
 * id — so what redraws the Tree is the re-read, and the assertion is on the
 * columns and the held-by line rather than on the router.
 */
test("moving an asset redraws the columns at its new path", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01 / postgres"));

  click("Move…");
  // The picker opens where the asset already is, so its own path is the crumb.
  expect(text()).toContain("Move into vm-db-01");
  click("Estate");
  expect(text()).toContain("Move into the top of the estate");
  await walkInto("hel1");
  // A leaf, and the commonest move there is: vm-app-02 holds nothing yet.
  await walkInto("vm-app-02");
  click("Move into vm-app-02");

  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-app-02 / postgres"));
  // The line the move wrote, with the parent's **name**: the backend can only
  // write down an id, and the pane resolves it against the path it just read.
  expect(text()).toContain("Moved to vm-app-02");
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]);
  expect(marked()).toEqual(["hel1", "vm-app-02", "postgres"]);
  expect(store.rows.find((row) => row.id === "asset:postgres")?.parent_id).toBe("asset:vm-app-02");
});

/**
 * **A move that would make a cycle is refused in the dialog, in the command's
 * own words, and nothing moves.**
 *
 * Criterion 3's second half. The picker deliberately does **not** hide the
 * asset's own subtree: hiding it would be a second copy of
 * `assets::cycle_through` in the frontend, and the refusal is the only place
 * the loop is explained. A two-hop loop is used — the site under the container
 * it transitively holds — so the one-step case cannot be what makes this pass.
 */
test("a cycle is refused with the message the command gives, and nothing moves", async () => {
  const store = estate(seed());
  render("#/asset/asset:hel1", store);
  await vi.waitFor(() => expect(text()).toContain("hel1"));

  click("Move…");
  await walkInto("hel1");
  await walkInto("vm-db-01");
  click("Move into vm-db-01");

  await vi.waitFor(() =>
    expect(text()).toContain(
      'moving "hel1" under "vm-db-01" would make a cycle: "vm-db-01" is already held by "hel1"',
    ),
  );
  // The dialog stays open on the refusal, and the estate is untouched.
  expect(text()).toContain("Move into vm-db-01");
  expect(store.rows.find((row) => row.id === "asset:hel1")?.parent_id).toBeNull();
});

/**
 * **A leaf is deleted after a confirmation, and the pane lands on its parent.**
 *
 * Criterion 4's first half. Landing on the parent rather than re-reading the id
 * that has just gone is the difference between a deletion that worked and a
 * deep-link failure drawn over one.
 */
test("deleting a leaf asks first, then lands on what held it", async () => {
  const store = estate(seed());
  const { router } = render("#/asset/asset:postgres", store);
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01 / postgres"));

  click("Delete");
  expect(text()).toContain("Delete postgres?");
  click("Cancel");
  expect(text()).not.toContain("Delete postgres?");
  expect(store.rows).toHaveLength(4);

  click("Delete");
  click("Delete");

  await vi.waitFor(() =>
    expect(router.route).toEqual({
      view: "assets",
      tab: "tree",
      assetId: "asset:vm-db-01",
    }),
  );
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"]]));
  expect(store.rows.map((row) => row.id)).not.toContain("asset:postgres");
});

/**
 * **Deleting a branch is refused by name, and the branch is still there.**
 *
 * Criterion 4's second half. The view counts nothing itself: `assets::delete`
 * refuses anything but a leaf with a `conflict` naming what it still holds, and
 * a count in the frontend would be a second copy of that rule — the copy that
 * goes stale.
 */
test("deleting a branch is refused by name and nothing is removed", async () => {
  const store = estate(seed());
  render("#/asset/asset:vm-db-01", store);
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01"));

  click("Delete");
  click("Delete");

  await vi.waitFor(() =>
    expect(text()).toContain('"vm-db-01" still holds 1 asset(s) -- move or delete them first'),
  );
  expect(store.rows).toHaveLength(4);
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]);
});

/**
 * **A type table that could not be read says so where it can be seen, and the
 * plus says which of the two reasons it is.**
 *
 * The address this matters at is the bare `#/assets/tree`, where nothing is
 * selected and there is therefore **no pane** — so a message drawn inside the
 * pane, as every other failure here is, would not exist. The tooltip is the
 * other half: *"has not been read yet"* is true while the read is out and a
 * plain lie afterwards, and a disabled control that explains itself wrongly is
 * worse than one that says nothing.
 */
test("a type table that will not load is said out loud, not left as a dead plus", async () => {
  const store = estate(seed());
  render("#/assets/tree", {
    ...store,
    assetTypes: () => Promise.reject({ code: "internal", message: "the bridge is down" }),
  });

  await vi.waitFor(() => expect(text()).toContain("Creating is unavailable: the bridge is down"));
  const plus = button("New asset in Estate");
  expect(plus.disabled).toBe(true);
  expect(plus.getAttribute("title")).toBe("The type table could not be read: the bridge is down");
  // The columns are unaffected: only creating stopped working.
  expect(columns()).toEqual([["hel1"]]);
});

/**
 * **A dialog does not outlive the selection it was opened on.**
 *
 * *Move to…* and the delete confirmation both name the selected asset and act
 * on it, so one still standing after the address moved would offer to move or
 * delete something other than the thing it names. The address can move while a
 * dialog is up without anybody clicking past the scrim — a launcher hit, a
 * notification's deep link — which is what `router.go` stands in for here, and
 * a headless walk of this branch is where it was found.
 */
test("an open dialog closes when the address names another asset", async () => {
  const store = estate(seed());
  const { router } = render("#/asset/asset:postgres", store);
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01 / postgres"));

  click("Move…");
  expect(text()).toContain("Move into vm-db-01");

  router.go("#/asset/asset:vm-app-02");
  flushSync();

  expect(target.querySelector('[role="dialog"]')).toBeNull();
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-app-02"));
  expect(store.rows.find((row) => row.id === "asset:postgres")?.parent_id).toBe("asset:vm-db-01");
});

/**
 * **A rename is a history line with its old and its new name, and the columns
 * follow.**
 *
 * The third mutation story 11 names, beside the property edit and the move.
 * The column redraw is the part that could silently not happen: the name lives
 * in the column as well as in the pane.
 */
test("renaming writes the old-to-new line and redraws the column", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  click("Rename");
  type("Name", "postgres-18");
  click("Save");

  await vi.waitFor(() => expect(text()).toContain("name: postgres → postgres-18"));
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres-18"]]);
  expect(text()).toContain("hel1 / vm-db-01 / postgres-18");
});
