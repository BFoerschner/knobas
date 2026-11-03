/**
 * The asset tree's writes: creating, from a column's plus, the header's *New
 * asset* and the preview's *New asset inside* (issue #429, spec #427 stories
 * 17, 18 and 34), and the *New asset* dialog itself.
 *
 * Editing, moving, deleting and routes left with the tree's pane: they are the
 * asset page's, and `tf/asset/TfAsset.write.test.svelte.ts` holds them. What
 * stays here is what the tree is for: putting a new asset at the level it
 * belongs, with what is known about it.
 *
 * The fake estate is `write-estate.fixture.ts`'s store, so a create is visible
 * to the next `asset_tree` and every assertion is on what the second read
 * says rather than on a callback.
 */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { layers } from "../tf/layers.svelte";
import { waitFor } from "../shell/test-wait";
import { estate, screen, seed } from "./write-estate.fixture";

const view = screen();
const { button, choose, click, field, press, reachable, render, text, type } = view;

/** Each column's row names, left to right. */
function columns(): string[][] {
  return [...view.target.querySelectorAll("ol.tf-et-col")].map((column) =>
    [...column.querySelectorAll("button.tf-et-row span.tf-et-nm")].map((name) => (name.textContent ?? "").trim()),
  );
}

/** Each column's monogram chips, in order: the type each row draws as. */
function chips(): string[][] {
  return [...view.target.querySelectorAll("ol.tf-et-col")].map((column) =>
    [...column.querySelectorAll("button.tf-et-row span.tf-dp-mono")].map((chip) => (chip.textContent ?? "").trim()),
  );
}

/** The row each column marks as the reader's path, or `null`. */
function marked(): (string | null)[] {
  return [...view.target.querySelectorAll("ol.tf-et-col")].map((column) => {
    const on = column.querySelector('button.tf-et-row[aria-current="true"] span.tf-et-nm');
    return on === null ? null : (on.textContent ?? "").trim();
  });
}

/** The name the preview heads itself with: the asset the tree has picked. */
function previewed(): string | null {
  return view.target.querySelector(".tf-et-ph h2")?.textContent?.trim() ?? null;
}

/** The open dialog's subtitle: the level a create goes into. */
function level(): string {
  return ((reachable() as Element).querySelector("header")?.textContent ?? "").replace(/\s+/g, " ").trim();
}

beforeEach(() => view.setup());
afterEach(() => view.teardown());

/**
 * **The plus on the first column creates at the top of the estate, and the new
 * asset lands picked in its own column with its type's monogram.**
 *
 * Criterion 1's first half. Nothing here tells the view what was created: the
 * dialog answers with a row, the tree goes to its **pick**, and everything on
 * screen is the read that follows, which is why the assertion is on the
 * column and the preview rather than on a callback.
 *
 * At the top there is no parent, so there is no convention: the dialog says so
 * by drawing no *usual here* line at all.
 */
test("the first column's plus creates at the top of the estate", async () => {
  const store = estate(seed());
  const { nav } = render("#/assets/tree", store);
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  click("New asset in Estate");
  expect(text()).toContain("At the top of the estate");
  expect(text()).not.toContain("Usual here");

  type("Name", "fsn1");
  click("Create");

  await waitFor(() => expect(columns()).toEqual([["fsn1", "hel1"]]));
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:new-1" });
  expect(marked()).toEqual(["fsn1"]);
  // The type's monogram, resolved by the backend out of the table: the chip a
  // column row draws.
  expect(view.target.querySelector("ol.tf-et-col button.tf-et-row.tf-et-on span.tf-dp-mono")?.textContent?.trim()).toBe("SI");
  await waitFor(() => expect(previewed()).toBe("fsn1"));
  expect(reachable()).toBe(view.target);
});

/**
 * **A plus deeper in creates under that column's asset, and the conventions
 * say what usually goes there.**
 *
 * Criterion 1's second half, and story 17's *usual here*. The heading names the
 * level, so a `+` over the third column is not a bare plus over an unnamed
 * list, and the dialog opens on the first suggestion rather than on whatever
 * the table happens to start with.
 */
test("a deeper column's plus creates inside that column's asset, with its conventions", async () => {
  const store = estate(seed());
  render("#/assets/asset:vm-db-01", store);
  await waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  expect(text()).toContain("In vm-db-01");
  // The VM's own suggestions, drawn from the type table and not from a copy.
  expect(text()).toContain("Usual here: Container engine, Service");

  type("Name", "redis");
  // *And any type allowed*: the dialog opens on the first suggestion
  // (`container_engine`) and this picks a different one, so what lands is the
  // type the reader chose rather than the one the conventions defaulted to.
  choose("Type", "service");
  click("Create");

  await waitFor(() => expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres", "redis"]]));
  expect(marked()).toEqual(["hel1", "vm-db-01", "redis"]);
  expect(store.rows.find((row) => row.name === "redis")?.parent_id).toBe("asset:vm-db-01");
  expect(store.rows.find((row) => row.name === "redis")?.type_id).toBe("service");
  // Asserted here and not on the root create: there the new asset, its only
  // sibling and the type the dialog defaults to are all `site`. Here the
  // created row is a `service` beside a `container`, and the two chips differ.
  expect(chips()).toEqual([["SI"], ["VM", "VM"], ["CT", "SV"]]);
});

/**
 * **The header's *New asset* adds to the rightmost open column**: the level in
 * front of the reader. Inside the picked asset when it holds things (its
 * column is open), beside it when it is a leaf, and at the top of the estate
 * when nothing is picked.
 */
test("the header's New asset creates in the rightmost open column", async () => {
  const store = estate(seed());
  const { nav } = render("#/assets/tree", store);
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
  click("New asset");
  expect(level()).toContain("At the top of the estate");
  click("Cancel");

  nav.go("#/assets/asset:vm-db-01");
  await waitFor(() => expect(columns()).toHaveLength(3));
  click("New asset");
  expect(level()).toContain("In vm-db-01");
  click("Cancel");

  // A leaf opens no column of its own, so the rightmost is the one it sits in.
  nav.go("#/assets/asset:postgres");
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
  click("New asset");
  expect(level()).toContain("In vm-db-01");
});

/**
 * **The preview's *New asset inside* creates under the picked asset**, leaf or
 * not: it is the one way to start a level that has no column yet.
 */
test("the preview's New asset inside creates under the picked asset, opening its column", async () => {
  const store = estate(seed());
  const { nav } = render("#/assets/asset:postgres", store);
  await waitFor(() => expect(previewed()).toBe("postgres"));

  click("New asset inside");
  expect(level()).toContain("In postgres");
  type("Name", "pg-exporter");
  click("Create");

  await waitFor(() => expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"], ["pg-exporter"]]));
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:new-1" });
  expect(store.rows.find((row) => row.name === "pg-exporter")?.parent_id).toBe("asset:postgres");
});

/**
 * **The dialog asks for the chosen type's own properties, and what is filled
 * in is on the asset the moment it exists.**
 *
 * The point of asking here: a VM is born with its hostname and IP rather than
 * as a name with empty fields to chase one at a time. They are a second write
 * on the row `create_asset` answered with, so the order is the witness:
 * create, then one edit carrying exactly the filled-in fields. An empty field
 * sends nothing, so nothing is cleared that was never set.
 */
test("the type's properties are asked for at create and written onto the new asset", async () => {
  const store = estate(seed());
  render("#/assets/asset:hel1", store);
  await waitFor(() => expect(columns()).toHaveLength(2));

  click("New asset in hel1");
  // The site suggests a hypervisor and a VM; the table here carries only the VM.
  expect((field("Type") as HTMLSelectElement).value).toBe("vm");
  expect(text()).toContain("VM properties optional");
  type("Name", "vm-db-02");
  type("Hostname", "vm-db-02.hel1");
  type("IP", "10.0.0.7");
  click("Create");

  await waitFor(() => expect(previewed()).toBe("vm-db-02"));
  expect(store.rows.find((row) => row.name === "vm-db-02")?.properties).toEqual({
    hostname: { kind: "text", value: "vm-db-02.hel1" },
    ip: { kind: "text", value: "10.0.0.7" },
  });
  expect(store.sent.map(({ command, id }) => `${command} ${id}`)).toEqual(["create asset:new-1", "edit asset:new-1"]);
  expect(store.sent[1]?.edits).toEqual([
    { field: "property", key: "hostname", value: { kind: "text", value: "vm-db-02.hel1" } },
    { field: "property", key: "ip", value: { kind: "text", value: "10.0.0.7" } },
  ]);
  // The preview draws what was filled in, so the reader sees it landed.
  expect(text()).toContain("Hostname vm-db-02.hel1");
});

/** A name and nothing else is still one write: no empty edit follows the create. */
test("a create with no property filled in sends no edit", async () => {
  const store = estate(seed());
  render("#/assets/asset:hel1", store);
  await waitFor(() => expect(columns()).toHaveLength(2));

  click("New asset in hel1");
  type("Name", "vm-bare");
  click("Create");

  await waitFor(() => expect(previewed()).toBe("vm-bare"));
  expect(store.sent.map(({ command }) => command)).toEqual(["create"]);
});

/**
 * **A property is written as the kind its type declares**, not as the kind
 * its text looks like: a service's port is a number field and lands as a
 * number, its URL as a URL. The witness is the tag on the stored value,
 * which the screen draws the same either way.
 */
test("a property is created as the kind its type declares", async () => {
  const store = estate(seed());
  render("#/assets/asset:vm-db-01", store);
  await waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  choose("Type", "service");
  expect((field("Port") as HTMLInputElement).type).toBe("number");
  type("Name", "grafana");
  type("URL", "https://grafana.hel1.example/");
  type("Port", "3000");
  click("Create");

  await waitFor(() => expect(previewed()).toBe("grafana"));
  expect(store.rows.find((row) => row.name === "grafana")?.properties).toEqual({
    url: { kind: "url", value: "https://grafana.hel1.example/" },
    port: { kind: "number", value: 3000 },
  });
});

/**
 * ***Create and add another* keeps the dialog at the same level with the same
 * type, empty and with the name focused again**, so a Docker engine's
 * containers are a name and a keypress each. The columns behind read again
 * after each, so the reader watches the list grow; `⌘Enter` is the same
 * gesture from the keyboard, and a plain `Enter` is *Create*, which closes the
 * dialog on the last one and picks it.
 */
test("Create and add another stays open for the next one, and Enter finishes", async () => {
  const store = estate(seed());
  const { nav } = render("#/assets/asset:vm-db-01", store);
  await waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  expect((field("Type") as HTMLSelectElement).value).toBe("container_engine");
  type("Name", "docker");
  type("Version", "29.1");
  click("Create and add another");

  await waitFor(() => expect(text()).toContain("Created docker. Name the next one."));
  expect(level()).toContain("In vm-db-01");
  expect((field("Name") as HTMLInputElement).value).toBe("");
  expect((field("Version") as HTMLInputElement).value).toBe("");
  expect((field("Type") as HTMLSelectElement).value).toBe("container_engine");
  await waitFor(() => expect(document.activeElement).toBe(field("Name")));
  await waitFor(() => expect(columns()[2]).toEqual(["docker", "postgres"]));
  // The tree has not moved: the next one goes in the same place.
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:vm-db-01" });

  type("Name", "podman");
  press("Name", "Enter", { metaKey: true });
  await waitFor(() => expect(text()).toContain("Created podman. Name the next one."));
  await waitFor(() => expect(columns()[2]).toEqual(["docker", "podman", "postgres"]));

  type("Name", "containerd");
  press("Name", "Enter");
  await waitFor(() => expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:new-3" }));
  expect(reachable()).toBe(view.target);
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "containerd"]));
  expect(store.rows.filter((row) => row.parent_id === "asset:vm-db-01" && row.type_id === "container_engine").map((row) => row.name)).toEqual([
    "docker",
    "podman",
    "containerd",
  ]);
  expect(store.rows.find((row) => row.name === "docker")?.properties).toEqual({ version: { kind: "text", value: "29.1" } });
});

/**
 * **When the properties do not save, the asset still stands, and the dialog
 * says which half landed.** A retry of *Create* would make a second asset, so
 * the dialog stays open on the sentence rather than closing as though all of
 * it had worked; the new row is already in its column behind it.
 */
test("a property write that fails after the create says the asset stands without them", async () => {
  const store = estate(seed());
  store.refuseEdits({ code: "internal", message: "the database went away" });
  render("#/assets/asset:hel1", store);
  await waitFor(() => expect(columns()).toHaveLength(2));

  click("New asset in hel1");
  type("Name", "vm-db-02");
  type("Hostname", "vm-db-02.hel1");
  click("Create");

  await waitFor(() =>
    expect(text()).toContain("Created vm-db-02, but its properties did not save: the database went away"),
  );
  expect(reachable()).not.toBe(view.target);
  expect(text()).not.toContain("Name the next one");
  expect(store.rows.find((row) => row.name === "vm-db-02")?.properties).toEqual({});
  await waitFor(() => expect(columns()[1]).toEqual(["vm-app-02", "vm-db-01", "vm-db-02"]));
});

/**
 * **A type table that could not be read says so where it can be seen, and the
 * plus says which of the two reasons it is.**
 *
 * The address this matters at is the bare `#/assets/tree`, where nothing is
 * picked and there is no preview, so the line is under the header. The
 * tooltip is the other half: *"has not been read yet"* is true while the read
 * is out and a plain lie afterwards. The header's *New asset* is held the same
 * way, saying the same thing.
 */
test("a type table that will not load is said out loud, not left as a dead plus", async () => {
  const store = estate(seed());
  render("#/assets/tree", {
    ...store,
    assetTypes: () => Promise.reject({ code: "internal", message: "the bridge is down" }),
  });

  await waitFor(() => expect(text()).toContain("Creating is unavailable: the bridge is down"));
  const plus = button("New asset in Estate");
  expect(plus.disabled).toBe(true);
  expect(plus.getAttribute("title")).toBe("The type table could not be read: the bridge is down");
  const header = button("New asset");
  expect(header.disabled).toBe(true);
  expect(header.getAttribute("title")).toBe("The type table could not be read: the bridge is down");
  // The columns are unaffected: only creating stopped working.
  expect(columns()).toEqual([["hel1"]]);
});

/**
 * The tree's dialogs are `TfDialog`s with a header Close, and each sits on the
 * shell's layer stack while open (#1527): the stack's close, which the
 * window's `Esc` calls when focus is outside the dialog, shuts it, and nothing
 * is left on the stack for history to wait on (#1086). Import's is
 * `AssetsView.import.test.svelte.ts`'s.
 */
test.each([
  ["New asset in vm-db-01", "Create"],
  ["New asset", "Create"],
  ["New asset inside", "Create"],
])("%s opens a dialog on the layer stack, and its close shuts it (#1527)", async (opener, inside) => {
  render("#/assets/asset:vm-db-01", estate(seed()));
  await waitFor(() => expect(previewed()).toBe("vm-db-01"));

  click(opener);
  expect(view.target.querySelector('[role="dialog"]')).not.toBeNull();
  expect(button(inside).closest('[role="dialog"]'), "the opener's dialog, not another").not.toBeNull();
  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);

  layers.open[0]!.close();
  flushSync();
  expect(view.target.querySelector('[role="dialog"]')).toBeNull();
  expect(layers.open).toEqual([]);
});

/**
 * **The *New asset* footer is the canonical one** (#2291, spec #2238 stories
 * 26-28 and 32): ghost *Cancel* first, the primary last, at the default size;
 * *Create and add another* is a plain default button between them, an
 * alternative to the primary rather than a second one.
 */
test("New asset opens a dialog with the canonical footer", async () => {
  render("#/assets/asset:vm-db-01", estate(seed()));
  await waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  const dialog = view.target.querySelector('[role="dialog"]')!;
  const foot = [...dialog.querySelectorAll("footer button")].map((control) => {
    const has = (name: string) => control.classList.contains(name);
    const size = !has("tf-btn") || has("tf-sm") || has("tf-lg") ? "not a default .tf-btn" : null;
    return `${(control.textContent ?? "").trim()}: ${size ?? (has("tf-primary") ? "primary" : has("tf-ghost") ? "ghost" : "plain")}`;
  });
  expect(foot).toEqual(["Cancel: ghost", "Create and add another: plain", "Create: primary"]);
});

/**
 * **The dialog does not outlive the pick it was opened on.** It names the
 * level it creates in; one still standing after the address moved (a launcher
 * hit, a notification's deep link) would create somewhere the reader is no
 * longer looking.
 */
test("an open dialog closes when the address picks another asset", async () => {
  const store = estate(seed());
  const { nav } = render("#/assets/asset:vm-db-01", store);
  await waitFor(() => expect(columns()).toHaveLength(3));

  click("New asset in vm-db-01");
  type("Name", "redis");
  nav.go("#/assets/asset:vm-app-02");
  flushSync();

  expect(view.target.querySelector('[role="dialog"]')).toBeNull();
  await waitFor(() => expect(previewed()).toBe("vm-app-02"));
  expect(store.rows.map((row) => row.name)).not.toContain("redis");
});
