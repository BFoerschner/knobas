/**
 * The fake estate the write tests run against (#429), and the screen they
 * read: the tree's creating (`AssetsView.write.test.svelte.ts`) and the asset
 * page's edits, moves, deletes and routes (`tf/asset/TfAsset.write.test.svelte.ts`).
 *
 * ## The fake is a store, not a stub
 *
 * `estate()` answers every command either view calls out of one array, so a
 * create is visible to the next `asset_tree`, a move rewrites the path the
 * next `get_asset` walks, and an edit appends the history line the page draws.
 * That is the only way to witness *the new asset appears in its column
 * picked*, *the page says where it now sits*, *the history shows old and new
 * value*: every one of them is about what the **second** read says.
 *
 * ## What the refusal sentences are, and are not
 *
 * The cycle, the branch-delete and the no-scheme refusals are written here in
 * the sentences `assets::move_to`, `assets::delete` and `assets::vet_url` use.
 * What that witnesses is that a view shows **the backend's own message, in
 * place**; that they are the sentences the real commands answer with is
 * `crates/knobas-app/tests/it/assets_ipc.rs`'s claim.
 *
 * The screen renders `Estate.fixture.svelte`, the two views behind one test
 * navigation, so a test can follow an address from the page to the tree and
 * back as the shell does.
 */
import { flushSync, mount, unmount } from "svelte";

import type {
  AssetDetail,
  AssetEdit,
  AssetProperty,
  AssetRow,
  AssetStatus,
  AssetType,
  Environment,
  Inherited,
  PropertyValue,
  RouteEdit,
  RouteRow,
  Visibility,
} from "../ipc/assets";
import type { ActivityRow } from "../ipc/entity";
import { waitFor } from "../shell/test-wait";
import Estate from "./Estate.fixture.svelte";
import { createTestNav } from "./test-nav.svelte";

/** The clock the history's *ago* readings are relative to. */
export const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/**
 * A corner of the **real** type table, ids, monograms and suggestions
 * included: spec #427 rules that nothing Tidewater-shaped is invented for
 * assets, and the suggestions are the ones `knobas_core::asset` declares.
 */
export const TYPES: AssetType[] = [
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
    properties: [
      { key: "url", label: "URL", kind: "url" },
      { key: "port", label: "Port", kind: "number" },
    ],
    suggests: [],
  },
];

export interface Stored {
  id: string;
  parent_id: string | null;
  type_id: string;
  name: string;
  properties: Record<string, PropertyValue>;
  /** Set **on** this asset; `undefined` and `null` inherit. */
  environment?: Environment | null;
  owner?: string | null;
  status?: AssetStatus;
}

/**
 * One route as the store holds it: the wire row minus the two names, which
 * are resolved on the way out the way the backend's join resolves them.
 */
export interface StoredRoute {
  id: string;
  asset_id: string;
  target_id: string | null;
  name: string;
  url: string;
  visibility: Visibility;
  properties: Record<string, PropertyValue>;
}

/** One in-memory estate, answering every command the tree and the page call. */
export function estate(seed: Stored[], seedRoutes: StoredRoute[] = []) {
  const rows = [...seed];
  const routes = [...seedRoutes];
  const lines: ActivityRow[] = [];
  const opened: string[] = [];
  /** Every command a write sent, in order: `create`, `edit`, `move`, `delete`. */
  const sent: { command: string; id: string; edits?: AssetEdit[] }[] = [];
  let openRefusal: { code: string; message: string } | null = null;
  let editRefusal: { code: string; message: string } | null = null;
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
      status: stored.status ?? "none",
      environment: stored.environment ?? null,
      owner: stored.owner ?? null,
      has_children: rows.some((other) => other.parent_id === stored.id),
      // #431's rollup, which nothing in this fixture moves: these tests are
      // about writes, and `AssetsView.test.svelte.ts` exercises the badge.
      health: stored.status ?? "none",
      inside: "none",
      problems_inside: 0,
      linked_work: 0,
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
        .map((key) => ({ key, label: key, value: stored.properties[key]!, custom: true })),
    ];
  }

  /** Outermost first, excluding the asset itself. */
  function ancestorsOf(stored: Stored): Stored[] {
    const walk: Stored[] = [];
    for (let parent = stored.parent_id; parent !== null; ) {
      const held = rows.find((candidate) => candidate.id === parent);
      if (held === undefined) break;
      walk.unshift(held);
      parent = held.parent_id;
    }
    return walk;
  }

  /** `assets::inherited`: the nearest of the asset and its holders that sets the field. */
  function inForce<K extends "environment" | "owner">(stored: Stored, key: K): Inherited<NonNullable<Stored[K]>> | null {
    for (const step of [stored, ...ancestorsOf(stored).reverse()]) {
      const value = step[key];
      if (value !== undefined && value !== null) {
        return { value: value as NonNullable<Stored[K]>, source_id: step.id, source_name: step.name };
      }
    }
    return null;
  }

  function routeRow(stored: StoredRoute): RouteRow {
    const target = stored.target_id === null ? undefined : rows.find((r) => r.id === stored.target_id);
    return {
      id: stored.id,
      asset_id: stored.asset_id,
      asset_name: find(stored.asset_id).name,
      target_id: stored.target_id,
      target_name: target?.name ?? null,
      name: stored.name,
      url: stored.url,
      visibility: stored.visibility,
      // `assets::custom_properties`: a route declares nothing, so every key is
      // the reader's own and the list is by key.
      properties: Object.keys(stored.properties)
        .sort()
        .map((key) => ({ key, label: key, value: stored.properties[key]!, custom: true })),
    };
  }

  function findRoute(id: string): StoredRoute {
    const found = routes.find((candidate) => candidate.id === id);
    if (found === undefined) throw { code: "not_found", message: `no route ${id}` };
    return found;
  }

  /** `assets::ROUTES_REACHABLE`: the target is on this asset's own path. */
  function reachable(stored: Stored): RouteRow[] {
    const above = new Set([stored.id, ...ancestorsOf(stored).map((held) => held.id)]);
    const under = (id: string): boolean => {
      for (let step = rows.find((candidate) => candidate.id === id); step !== undefined; ) {
        if (step.id === stored.id) return true;
        step = rows.find((candidate) => candidate.id === step?.parent_id);
      }
      return false;
    };
    return routes
      .filter((route) => route.target_id !== null && (above.has(route.target_id) || under(route.target_id)))
      .map(routeRow);
  }

  return {
    /** What is in the estate right now, for an assertion that is not on markup. */
    rows,
    /** And what it exposes: the same, for the routes. */
    routes,
    /** Every URL a view handed to the OS browser, in order. */
    opened,
    /** Every write command, in order, with an edit's whole list. */
    sent,
    /** Make the next open fail, the way `shell/open-external` fails a scheme it will not hand over. */
    refuseToOpen(refusal: { code: string; message: string }) {
      openRefusal = refusal;
    },
    /** Make every `edit_asset` fail with this, until it is called with `null`. */
    refuseEdits(refusal: { code: string; message: string } | null) {
      editRefusal = refusal;
    },
    /**
     * The OS browser, faked: a port left out falls through to the real
     * `openExternal`, and the first test to press a URL would hand it to
     * `@tauri-apps/plugin-opener` for real.
     */
    openExternal: (url: string) => {
      opened.push(url);
      return openRefusal === null ? Promise.resolve() : Promise.reject(openRefusal);
    },
    assetTypes: () => Promise.resolve(TYPES),
    // #505's panel, read on every asset the page draws. Empty is honest: no
    // write test is about it, and a port left out would reach the real `invoke`.
    dependsOnThis: () => Promise.resolve({ assets: [], routes: [] }),
    assetTree: (parentId?: string | null) => Promise.resolve(children(parentId ?? null)),
    getAsset: (assetId: string): Promise<AssetDetail> => {
      try {
        const stored = find(assetId);
        return Promise.resolve({
          asset: row(stored),
          properties: properties(stored),
          effective_environment: inForce(stored, "environment"),
          effective_owner: inForce(stored, "owner"),
          held_by: ancestorsOf(stored).map(row),
          holds: children(stored.id),
          exposes: routes
            .filter((route) => route.asset_id === assetId)
            .sort((left, right) => left.name.localeCompare(right.name))
            .map(routeRow),
          reachable_via: reachable(stored),
          // Nothing here draws a link; #435's writes are the links tests'.
          links: [],
          history: lines.filter((line) => line.entity_id === assetId).reverse(),
          // Nothing in this fixture was imported or is watched.
          monitors: [],
          monitoring: [],
          monitor_targets: [],
        });
      } catch (cause) {
        return Promise.reject(cause);
      }
    },
    createAsset: (typeId: string, name: string, parentId?: string | null) => {
      if (name.trim() === "") {
        return Promise.reject({ code: "invalid", message: "an asset needs a name" });
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
      sent.push({ command: "create", id: stored.id });
      record(stored.id, "created", { asset: { type: typeId, name: stored.name } });
      return Promise.resolve(row(stored));
    },
    editAsset: (assetId: string, edits: AssetEdit[]) => {
      sent.push({ command: "edit", id: assetId, edits });
      if (editRefusal !== null) return Promise.reject(editRefusal);
      const stored = find(assetId);
      for (const edit of edits) {
        if (edit.field === "name") {
          const from = stored.name;
          stored.name = edit.value.trim();
          record(assetId, "renamed", { field: "name", from, to: stored.name });
          continue;
        }
        if (edit.field === "environment" || edit.field === "owner" || edit.field === "status") {
          const from = stored[edit.field] ?? null;
          if (edit.field === "status") stored.status = edit.value;
          else if (edit.field === "environment") stored.environment = edit.value;
          else stored.owner = edit.value;
          record(assetId, "edited", { field: edit.field, from, to: edit.value });
          continue;
        }
        if (edit.field !== "property") continue;
        const from = stored.properties[edit.key] ?? null;
        if (edit.value === null) delete stored.properties[edit.key];
        else stored.properties[edit.key] = edit.value;
        record(assetId, "edited", { field: "property", key: edit.key, from, to: edit.value });
      }
      return Promise.resolve(row(stored));
    },
    moveAsset: (assetId: string, newParentId: string | null) => {
      const stored = find(assetId);
      if (newParentId !== null) {
        // `assets::cycle_through`: the walk up from the proposed parent, and
        // the name one step *below* the moved asset on it.
        let below: string | null = null;
        for (let walk: string | null = newParentId; walk !== null; ) {
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
      sent.push({ command: "move", id: assetId });
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
      sent.push({ command: "delete", id: assetId });
      return Promise.resolve();
    },
    getRoute: (routeId: string) => {
      try {
        const stored = findRoute(routeId);
        return Promise.resolve({
          route: routeRow(stored),
          history: lines.filter((line) => line.entity_id === routeId).reverse(),
          monitor_targets: [],
        });
      } catch (cause) {
        return Promise.reject(cause);
      }
    },
    createRoute: (
      assetId: string,
      name: string,
      url: string,
      targetId?: string | null,
      visibility?: Visibility,
      properties?: [string, PropertyValue][],
    ) => {
      // `assets::vet_url`'s rule, in the sentence it uses.
      if (!url.includes("://")) {
        return Promise.reject({
          code: "invalid",
          message: `"${url}" carries no scheme -- a route is a URL or an endpoint`,
        });
      }
      minted += 1;
      const stored: StoredRoute = {
        id: `route:new-${minted}`,
        asset_id: assetId,
        target_id: targetId ?? null,
        name: name.trim(),
        url: url.trim(),
        visibility: visibility ?? "internal",
        properties: Object.fromEntries(properties ?? []),
      };
      routes.push(stored);
      record(stored.id, "created", { route: { name: stored.name, url: stored.url } });
      return Promise.resolve(routeRow(stored));
    },
    editRoute: (routeId: string, edits: RouteEdit[]) => {
      const stored = findRoute(routeId);
      for (const edit of edits) {
        // The page writes no monitor name on a route (#976's dialog does).
        if (edit.field === "monitors") continue;
        // `assets::edit_route`'s two rules: an edit that changes nothing writes
        // no line, and one that does writes its **old and new** value.
        const was = {
          name: stored.name,
          url: stored.url,
          target: stored.target_id,
          visibility: stored.visibility,
          property: edit.field === "property" ? (stored.properties[edit.key] ?? null) : null,
        }[edit.field];
        if (edit.field === "name") stored.name = edit.value.trim();
        if (edit.field === "url") stored.url = edit.value.trim();
        if (edit.field === "target") stored.target_id = edit.value;
        if (edit.field === "visibility") stored.visibility = edit.value;
        if (edit.field === "property") {
          if (edit.value === null) delete stored.properties[edit.key];
          else stored.properties[edit.key] = edit.value;
        }
        const now = {
          name: stored.name,
          url: stored.url,
          target: stored.target_id,
          visibility: stored.visibility,
          property: edit.field === "property" ? (stored.properties[edit.key] ?? null) : null,
        }[edit.field];
        if (JSON.stringify(was) === JSON.stringify(now)) continue;
        record(routeId, edit.field === "name" ? "renamed" : "edited", {
          field: edit.field,
          ...(edit.field === "property" ? { key: edit.key } : {}),
          from: was,
          to: now,
        });
      }
      return Promise.resolve(routeRow(stored));
    },
    deleteRoute: (routeId: string) => {
      const stored = findRoute(routeId);
      routes.splice(routes.indexOf(stored), 1);
      return Promise.resolve();
    },
  };
}

export type FakeEstate = ReturnType<typeof estate>;

/** hel1 › vm-db-01 › postgres, plus a sibling VM so a column has two rows. */
export function seed(): Stored[] {
  return [
    { id: "asset:hel1", parent_id: null, type_id: "site", name: "hel1", properties: {} },
    {
      id: "asset:vm-db-01",
      parent_id: "asset:hel1",
      type_id: "vm",
      name: "vm-db-01",
      properties: { hostname: { kind: "text", value: "vm-db-01" } },
    },
    { id: "asset:vm-app-02", parent_id: "asset:hel1", type_id: "vm", name: "vm-app-02", properties: {} },
    {
      id: "asset:postgres",
      parent_id: "asset:vm-db-01",
      type_id: "container",
      name: "postgres",
      properties: { image: { kind: "text", value: "postgres:18" } },
    },
  ];
}

/** A route on vm-db-01 landing on postgres: the one the route tests edit. */
export function pgRoute(overrides: Partial<StoredRoute> = {}): StoredRoute {
  return {
    id: "route:pg",
    asset_id: "asset:vm-db-01",
    target_id: "asset:postgres",
    name: "Postgres UI",
    url: "https://pg.hel1.example/",
    visibility: "internal",
    properties: {},
    ...overrides,
  };
}

/**
 * What a reader can see and reach, over one mounted estate.
 *
 * A click lands inside the open dialog, or in the view when there is none: a
 * modal traps focus and the scrim swallows everything behind it, so this is
 * what a person can actually reach. It also keeps a test honest about *which*
 * control it means: the page's *Delete…* opens a dialog whose confirm is
 * *Delete*, and a helper that took the first one in the document would have
 * confirmed nothing.
 */
export function screen() {
  let target: HTMLDivElement | undefined;
  let app: Record<string, unknown> | undefined;

  const root = (): HTMLDivElement => {
    if (target === undefined) throw new Error("nothing is mounted");
    return target;
  };
  const reachable = (): ParentNode => root().querySelector('[role="dialog"]') ?? root();

  function buttonNames(): string[] {
    return [...reachable().querySelectorAll("button")].map(
      (candidate) =>
        (candidate.getAttribute("title") ?? candidate.getAttribute("aria-label") ?? (candidate.textContent ?? "").trim()) || "?",
    );
  }

  /** The first reachable button whose visible text, name or title is `label`. */
  function button(label: string): HTMLButtonElement {
    const found = [...reachable().querySelectorAll<HTMLButtonElement>("button")].find(
      (candidate) =>
        (candidate.textContent ?? "").trim() === label ||
        candidate.getAttribute("aria-label") === label ||
        candidate.getAttribute("title") === label,
    );
    if (found === undefined) throw new Error(`no button “${label}” — the buttons are: ${buttonNames().join(" | ")}`);
    return found;
  }

  /** A reachable field, by its `aria-label` or by the `<label>` pointing at it. */
  function field(label: string): HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement {
    const where = reachable();
    const named = where.querySelector<HTMLInputElement | HTMLSelectElement>(`[aria-label="${label}"]`);
    if (named !== null) return named;
    const tag = [...where.querySelectorAll("label")].find((candidate) => (candidate.textContent ?? "").trim() === label);
    const found =
      tag === undefined
        ? null
        : where.querySelector<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(`#${CSS.escape(tag.htmlFor)}`);
    if (found === null) throw new Error(`no field “${label}”`);
    return found;
  }

  return {
    setup() {
      target = document.createElement("div");
      document.body.append(target);
      location.hash = "";
    },
    teardown() {
      if (app) unmount(app);
      app = undefined;
      target?.remove();
      target = undefined;
      location.hash = "";
    },
    /** Mount both views at `hash` over the fake, with the test's clock. */
    render(hash: string, ports: object) {
      location.hash = hash;
      const nav = createTestNav();
      app = mount(Estate, { target: root(), props: { nav, now: () => NOW, ports } });
      flushSync();
      return { nav };
    },
    get target() {
      return root();
    },
    reachable,
    button,
    buttonNames,
    field,
    text: (): string => (root().textContent ?? "").replace(/\s+/g, " ").trim(),
    /** Click, and let the effects the click set off run. */
    click(label: string) {
      button(label).click();
      flushSync();
    },
    /** Type into a field the way a person does: the views listen to `input`. */
    type(label: string, value: string) {
      const control = field(label);
      control.value = value;
      control.dispatchEvent(new Event("input", { bubbles: true }));
      flushSync();
    },
    /** Pick an option the way a person does: `bind:value` on a select reads `change`. */
    choose(label: string, value: string) {
      const control = field(label);
      control.value = value;
      control.dispatchEvent(new Event("input", { bubbles: true }));
      control.dispatchEvent(new Event("change", { bubbles: true }));
      flushSync();
    },
    /** A key pressed in a field, as the keyboard sends it. */
    press(label: string, key: string, modifiers: KeyboardEventInit = {}) {
      field(label).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...modifiers }));
      flushSync();
    },
    /**
     * Walk the move picker one level down, once that level has come back:
     * each step is a read, so the row cannot be clicked until it is there.
     */
    async walkInto(name: string) {
      await waitFor(() => button(`Open ${name}`));
      button(`Open ${name}`).click();
      flushSync();
    },
  };
}
