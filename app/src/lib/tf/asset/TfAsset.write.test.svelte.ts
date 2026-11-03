/**
 * An asset page's writes: the one form *Edit* opens (name, what is set in
 * force, every property and new ones), moving, deleting, and its routes
 * (issue #429, #432; spec #427 stories 3, 6, 11 and 14).
 *
 * They were the tree's pane's, a click and a *Save* per value. The page edits
 * as one gesture: *Edit*, or a click on any value, turns the name, *In force*
 * and *Properties* into one form, and *Save* sends **one** `edit_asset` with
 * only what changed (`deps/asset-edit.ts`). So the witnesses here are that
 * one write, what it carried, and what the read after it says: the history
 * line with its old and its new value, which only the backend writes.
 *
 * The fake estate is `assets/write-estate.fixture.ts`'s store, rendered
 * through `Estate.fixture.svelte`, so a test follows the page's own address
 * into the tree (a delete lands on what held it) as the shell would.
 */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { estate, pgRoute, screen, seed, type Stored } from "../../assets/write-estate.fixture";
import { waitFor } from "../../shell/test-wait";
import { layers } from "../layers.svelte";

const view = screen();
const { button, choose, click, field, press, reachable, render, text, type, walkInto } = view;

/** The page's subtitle: its type and where it sits, `Container in hel1 › vm-db-01` (the separator's spacing is CSS). */
function subtitle(): string {
  return (view.target.querySelector(".tf-as-head .tf-sub")?.textContent ?? "")
    .replace(/\s+/g, " ")
    .replace(/\s*›\s*/g, " › ")
    .trim();
}

/**
 * Let the microtasks a keystroke or a click left behind run: a binding reads
 * its value back after a tick, and *Add a property* focuses its new row after
 * one. A person never presses the next key inside that tick; a test would.
 */
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

/** The page's title: the asset's name, while the form is not open. */
function title(): string | null {
  return view.target.querySelector(".tf-as-head h1")?.textContent?.trim() ?? null;
}

/** The tree's columns, once an address has taken the test there. */
function columns(): string[][] {
  return [...view.target.querySelectorAll("ol.tf-et-col")].map((column) =>
    [...column.querySelectorAll("button.tf-et-row span.tf-et-nm")].map((name) => (name.textContent ?? "").trim()),
  );
}

function marked(): (string | null)[] {
  return [...view.target.querySelectorAll("ol.tf-et-col")].map((column) => {
    const on = column.querySelector('button.tf-et-row[aria-current="true"] span.tf-et-nm');
    return on === null ? null : (on.textContent ?? "").trim();
  });
}

/** Whether the page is showing its form: the name is a field then. */
function editing(): boolean {
  return view.target.querySelector('.tf-as-head input[aria-label="Name"]') !== null;
}

/** The write's refusal under the header, or `null`. */
function banner(): string | null {
  return view.target.querySelector('.tf-banner.tf-err[role="alert"]')?.textContent?.trim() ?? null;
}

/** The delete dialog's sentence. */
function ask(): string {
  return ((reachable() as Element).querySelector(".tf-as-ask")?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** Every edit `edit_asset` was sent, call by call. */
function edits(store: ReturnType<typeof estate>) {
  return store.sent.filter((call) => call.command === "edit").map((call) => call.edits);
}

beforeEach(() => view.setup());
afterEach(() => view.teardown());

/**
 * **A typed property and a custom one are both filled in from the page, and
 * each write's history line carries its old and its new value.**
 *
 * Criterion 2, and story 11's point: *"who changed the IP"* is only
 * answerable if the old value was written down at the moment it stopped being
 * true. The typed one is filled **from empty**, a declared key nobody had set,
 * which is the case the type table has to be on the wire for: there is no
 * kind in a `null`. The empty one is a button that says so, and pressing it
 * opens the form at that field.
 */
test("a typed and a custom property are filled in on the page and the history says so", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain("Image postgres:18"));
  expect(text()).toContain("Ports Not set");

  click("Edit Ports");
  await waitFor(() => expect(document.activeElement).toBe(field("Ports")));
  type("Ports", "5432");
  click("Save");
  await waitFor(() => expect(text()).toContain("Ports 5432"));
  expect(editing()).toBe(false);
  expect(text()).toContain("ports: nothing → 5432");

  // Now one of the reader's own, of a kind the type never declared.
  click("Edit");
  click("Add a property");
  type("New property key", "backup window");
  type("New property value", "02:00");
  click("Save");
  await waitFor(() => expect(text()).toContain("backup window 02:00"));
  expect(text()).toContain("Other properties");

  // Changing it again is where old-and-new is witnessed: the first write's
  // "from" was empty, and an editor that sent the new value as both would
  // have passed that.
  click("Edit backup window");
  type("backup window", "03:30");
  click("Save");
  await waitFor(() => expect(text()).toContain("backup window: 02:00 → 03:30"));

  expect(store.rows.find((row) => row.id === "asset:postgres")?.properties).toEqual({
    image: { kind: "text", value: "postgres:18" },
    ports: { kind: "text", value: "5432" },
    "backup window": { kind: "text", value: "03:30" },
  });
});

/**
 * **One *Save* is one write, carrying only what changed.** The form holds
 * every field, so a reader who renames, fills a port and adds a key in one go
 * sends one `edit_asset` with exactly those three, and the image they left
 * alone is not rewritten (it would write a history line that says nothing).
 */
test("one Save sends every changed field as one edit, and nothing that did not change", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  type("Name", "postgres-18");
  type("Ports", "5432");
  click("Add a property");
  type("New property key", "backup window");
  type("New property value", "02:00");
  click("Save");

  await waitFor(() => expect(title()).toBe("postgres-18"));
  expect(edits(store)).toEqual([
    [
      { field: "name", value: "postgres-18" },
      { field: "property", key: "ports", value: { kind: "text", value: "5432" } },
      { field: "property", key: "backup window", value: { kind: "text", value: "02:00" } },
    ],
  ]);
});

/** A *Save* with nothing changed writes nothing, and puts the form away. */
test("a Save with nothing changed sends nothing", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  click("Save");
  expect(editing()).toBe(false);
  expect(edits(store)).toEqual([]);
});

/**
 * **An emptied field clears its property**, the only reading a blank can
 * have: `assets::PropertyValue::vet` refuses an empty text with *clear the
 * property instead*. The page then says *Not set*, and the history says what
 * it was.
 */
test("emptying a field clears the property, and the history says what it was", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain("Image postgres:18"));

  click("Edit Image");
  type("Image", "");
  click("Save");

  await waitFor(() => expect(text()).toContain("Image Not set"));
  expect(text()).toContain("image: postgres:18 → nothing");
  expect(store.rows.find((row) => row.id === "asset:postgres")?.properties).toEqual({});
});

/**
 * **`Enter` in a line saves the form; in a box it is a new line, and `⌘Enter`
 * saves.** A long value is edited in a box that wraps, and a reader writing
 * two lines in it must not have saved after the first.
 */
test("Enter in a field saves, and a long value's box saves on ⌘Enter", async () => {
  const notes = "Restored from the 02:00 dump on the first Monday of every month, by hand.";
  const store = estate(
    seed().map((row): Stored => (row.id === "asset:postgres" ? { ...row, properties: { ...row.properties, notes: { kind: "text", value: notes } } } : row)),
  );
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain(notes));

  click("Edit Ports");
  type("Ports", "5432");
  press("Ports", "Enter");
  await waitFor(() => expect(text()).toContain("Ports 5432"));
  expect(editing()).toBe(false);

  click(`Edit notes`);
  expect(field("notes").tagName).toBe("TEXTAREA");
  type("notes", `${notes}\nAsk Björn first.`);
  press("notes", "Enter");
  expect(editing()).toBe(true);
  press("notes", "Enter", { metaKey: true });
  await waitFor(() => expect(editing()).toBe(false));
  expect(store.rows.find((row) => row.id === "asset:postgres")?.properties.notes).toEqual({
    kind: "text",
    value: `${notes}\nAsk Björn first.`,
  });
});

/**
 * **What is set in force is set from the same form**, and *Inherit* is the
 * way back: the field says what it would inherit and from where, and choosing
 * it sends a clear, so the value walks up to the holder again (stories 8
 * and 9). The tree's pane could not do this at all; the asset editor could,
 * and this is its rule.
 */
test("environment, owner and status are set from the form, and Inherit gives them back", async () => {
  const store = estate(
    seed().map((row): Stored => (row.id === "asset:hel1" ? { ...row, environment: "prod", owner: "Björn" } : row)),
  );
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain("Environment prod inherited from hel1"));
  expect(text()).toContain("Owner Björn inherited from hel1");

  click("Edit");
  const env = field("Environment") as HTMLSelectElement;
  expect(env.options[0]?.textContent?.trim()).toBe("Inherit (prod from hel1)");
  expect((field("Owner") as HTMLInputElement).placeholder).toBe("Inherit (Björn from hel1)");
  choose("Environment", "dev");
  type("Owner", "Mara");
  choose("Own status", "warn");
  click("Save");

  await waitFor(() => expect(text()).toContain("Environment dev set here"));
  expect(text()).toContain("Owner Mara set here");
  expect(text()).toContain("environment: nothing → dev");
  expect(edits(store)).toEqual([
    [
      { field: "status", value: "warn" },
      { field: "environment", value: "dev" },
      { field: "owner", value: "Mara" },
    ],
  ]);

  click("Edit Environment");
  choose("Environment", "");
  type("Owner", "");
  click("Save");
  await waitFor(() => expect(text()).toContain("Environment prod inherited from hel1"));
  expect(text()).toContain("Owner Björn inherited from hel1");
  expect(edits(store).at(-1)).toEqual([
    { field: "environment", value: null },
    { field: "owner", value: null },
  ]);
});

/**
 * **A rename is a history line with its old and its new name, and everything
 * that names the asset follows**: the title, the breadcrumb, and its row in
 * the tree, which reads it again when the reader goes there.
 */
test("renaming writes the old-to-new line, and the tree's column follows", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  await waitFor(() => expect(document.activeElement).toBe(field("Name")));
  type("Name", "postgres-18");
  click("Save");

  await waitFor(() => expect(text()).toContain("name: postgres → postgres-18"));
  expect(title()).toBe("postgres-18");
  expect(view.target.querySelector(".tf-crumbs strong")?.textContent).toBe("postgres-18");

  nav.go("#/assets/asset:postgres");
  await waitFor(() => expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres-18"]]));
});

/**
 * **A refused save is said under the header in the backend's words, and the
 * form stays as typed**, so the fix is one edit rather than a re-entry; once
 * the backend takes it, the same *Save* lands.
 */
test("a refused save keeps the form as typed and says why", async () => {
  const store = estate(seed());
  store.refuseEdits({ code: "invalid", message: "an asset needs a name" });
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  type("Name", "postgres-18");
  click("Save");

  await waitFor(() => expect(banner()).toBe("an asset needs a name"));
  expect(editing()).toBe(true);
  expect((field("Name") as HTMLInputElement).value).toBe("postgres-18");

  store.refuseEdits(null);
  click("Save");
  await waitFor(() => expect(title()).toBe("postgres-18"));
  expect(banner()).toBeNull();
});

/**
 * **`Esc` in the form puts it away and goes no further.** The shell reads
 * `Esc` on `window` as *go back*, so a press that closed the form *and*
 * reached it would leave the page the reader was editing: two unwinds for one
 * key. The witness is a listener on `window`, the address, and the draft that
 * was never written; once the form is away, the next `Esc` is the shell's.
 */
test("Esc in the form cancels the edit and does not leave the page", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain("Image postgres:18"));
  const shell = vi.fn();
  window.addEventListener("keydown", shell);
  try {
    click("Edit Ports");
    type("Ports", "5432");
    await settle();
    press("Ports", "Escape");

    expect(editing()).toBe(false);
    expect(text()).toContain("Ports Not set");
    expect(edits(store)).toEqual([]);
    expect(location.hash).toBe("#/asset/asset:postgres");
    expect(shell).not.toHaveBeenCalled();

    // A new row's key field is as much a way out as any other field.
    click("Edit");
    click("Add a property");
    await settle();
    press("New property key", "Escape");
    expect(editing()).toBe(false);
    expect(shell).not.toHaveBeenCalled();

    // With nothing open the page has no answer, and the shell gets it.
    view.target.querySelector(".tf-as")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(shell).toHaveBeenCalledTimes(1);
  } finally {
    window.removeEventListener("keydown", shell);
  }
});

/** *Cancel* is the same as `Esc`: nothing is written and the values are as they were. */
test("Cancel puts the form away without writing", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  type("Name", "postgres-18");
  await settle();
  click("Cancel");

  expect(editing()).toBe(false);
  expect(title()).toBe("postgres");
  expect(store.sent).toEqual([]);
});

/**
 * **A custom property is stored as the kind the form was told, not as the kind
 * its text looks like.** `"8080"` and `8080` are different properties, and a
 * date read as a string cannot be ordered against another one, so the witness
 * is the **tag** on the stored value and not the text on the screen, which is
 * the same either way. A new row starts on *Text*: a reader adding a hostname
 * after a port should not be handed a number box.
 */
test("a custom property is stored as the kind its row was set to", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(text()).toContain("Image postgres:18"));

  click("Edit");
  click("Add a property");
  type("New property key", "metrics port");
  choose("New property kind", "number");
  type("New property value", "9187");
  click("Save");

  await waitFor(() => expect(text()).toContain("metrics port 9187"));
  expect(banner()).toBeNull();
  expect(store.rows.find((row) => row.id === "asset:postgres")?.properties).toEqual({
    image: { kind: "text", value: "postgres:18" },
    "metrics port": { kind: "number", value: 9187 },
  });

  click("Edit");
  click("Add a property");
  expect((field("New property kind") as HTMLSelectElement).value).toBe("text");
});

/** A row the reader added and then removed sends nothing. */
test("a removed new row is not written", async () => {
  const store = estate(seed());
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Edit");
  click("Add a property");
  await settle();
  type("New property key", "backup window");
  type("New property value", "02:00");
  await settle();
  click("Remove this row");
  expect(view.target.querySelector('[aria-label="New property key"]')).toBeNull();
  click("Save");

  expect(edits(store)).toEqual([]);
  expect(editing()).toBe(false);
});

/**
 * **A move says where the asset now sits**, on the page's own subtitle, and
 * in the tree it opens at the new path.
 *
 * Criterion 3's first half. The address does not change (the asset keeps its
 * id), so what redraws is the re-read.
 */
test("moving an asset says where it now sits, and the tree opens there", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:postgres", store);
  await waitFor(() => expect(subtitle()).toBe("Container in hel1 › vm-db-01"));

  click("Move…");
  // The picker opens where the asset already is, so its own path is the crumb.
  expect(text()).toContain("Move into vm-db-01");
  click("Estate");
  expect(text()).toContain("Move into the top of the estate");
  await walkInto("hel1");
  await walkInto("vm-app-02");
  click("Move into vm-app-02");

  await waitFor(() => expect(subtitle()).toBe("Container in hel1 › vm-app-02"));
  // The line the move wrote, with the parent's **name**: the backend can only
  // write down an id, and the page resolves it against the path it just read.
  expect(text()).toContain("Moved to vm-app-02");
  expect(store.rows.find((row) => row.id === "asset:postgres")?.parent_id).toBe("asset:vm-app-02");

  nav.go("#/assets/asset:postgres");
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-app-02", "postgres"]));
});

/**
 * **A move that would make a cycle is refused in the dialog, in the command's
 * own words, and nothing moves.**
 *
 * Criterion 3's second half. The picker does **not** hide the asset's own
 * subtree: hiding it would be a second copy of `assets::cycle_through`, and
 * the refusal is the only place the loop is explained. Two hops down, so the
 * one-step case cannot be what makes this pass: the offender named is
 * `vm-db-01`, neither end of the move.
 */
test("a cycle is refused with the message the command gives, and nothing moves", async () => {
  const store = estate(seed());
  render("#/asset/asset:hel1", store);
  await waitFor(() => expect(title()).toBe("hel1"));

  click("Move…");
  await walkInto("hel1");
  await walkInto("vm-db-01");
  await walkInto("postgres");
  click("Move into postgres");

  await waitFor(() =>
    expect(text()).toContain('moving "hel1" under "postgres" would make a cycle: "vm-db-01" is already held by "hel1"'),
  );
  expect(text()).toContain("Move into postgres");
  expect(store.rows.find((row) => row.id === "asset:hel1")?.parent_id).toBeNull();
});

/**
 * **A leaf is deleted after a confirmation, and the reader lands in the tree
 * on what held it.**
 *
 * Criterion 4's first half. Landing on the parent rather than re-reading the id
 * that has just gone is the difference between a deletion that worked and a
 * deep-link failure drawn over one; the tree is where a reader sees it gone.
 */
test("deleting a leaf asks first, then lands in the tree on what held it", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Delete…");
  expect(text()).toContain("Delete postgres?");
  click("Cancel");
  expect(text()).not.toContain("Delete postgres?");
  expect(store.rows).toHaveLength(4);

  click("Delete…");
  click("Delete");

  await waitFor(() => expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:vm-db-01" }));
  await waitFor(() => expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"]]));
  expect(store.rows.map((row) => row.id)).not.toContain("asset:postgres");
});

/** An asset at the top of the estate leaves to the tree's top. */
test("deleting a top-level asset lands on the bare tree", async () => {
  const store = estate([...seed(), { id: "asset:fsn1", parent_id: null, type_id: "site", name: "fsn1", properties: {} }]);
  const { nav } = render("#/asset/asset:fsn1", store);
  await waitFor(() => expect(subtitle()).toBe("Site at the top of the estate"));

  click("Delete…");
  click("Delete");

  await waitFor(() => expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: null }));
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
});

/**
 * **The delete confirm says what the backend does** (#1463, R-11.19a): links
 * stay, marked deleted, and the routes landing on the asset are counted. It is
 * the Dependencies editor's `deleteConfirm`, so the history, which the backend
 * keeps, is not mentioned.
 */
test("the delete confirm counts the routes landing on the asset and says nothing about history", async () => {
  const store = estate(seed(), [pgRoute({ name: "pg" }), pgRoute({ id: "route:pgadmin", name: "pgadmin" })]);
  render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Delete…");
  expect(ask()).toBe("Delete postgres? 2 routes landing on it will land on nothing. Links to it stay, marked deleted.");
  expect(ask()).not.toMatch(/history/i);
});

test("with no route landing on it, the delete confirm has no routes clause", async () => {
  render("#/asset/asset:postgres", estate(seed()));
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Delete…");
  expect(ask()).toBe("Delete postgres? Links to it stay, marked deleted.");
});

/**
 * **Deleting a branch is refused by name, and the branch is still there.**
 *
 * Criterion 4's second half. The page counts nothing itself: `assets::delete`
 * refuses anything but a leaf with a `conflict` naming what it still holds, in
 * the dialog the reader confirmed in.
 */
test("deleting a branch is refused by name and nothing is removed", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(title()).toBe("vm-db-01"));

  click("Delete…");
  click("Delete");

  await waitFor(() => expect(text()).toContain('"vm-db-01" still holds 1 asset(s) -- move or delete them first'));
  expect(reachable()).not.toBe(view.target);
  expect(store.rows).toHaveLength(4);
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: "asset:vm-db-01" });
});

/**
 * **The Holds panel's *New asset inside* creates under this asset and opens
 * the new one's page**: from a page, the next thing is to read it, and what
 * the dialog asked for is already on it.
 */
test("New asset inside creates under the page's asset and opens its page", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Holds 1"));

  click("New asset inside");
  expect(text()).toContain("In vm-db-01");
  type("Name", "redis");
  choose("Type", "service");
  type("Port", "6379");
  click("Create");

  await waitFor(() => expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: "asset:new-1" }));
  await waitFor(() => expect(title()).toBe("redis"));
  expect(subtitle()).toBe("Service in hel1 › vm-db-01");
  expect(text()).toContain("Port 6379");
});

/**
 * The page's dialogs are `TfDialog`s with a header Close, and each sits on the
 * shell's layer stack while open (#1527): the stack's close, which the
 * window's `Esc` calls when focus is outside the dialog, shuts it, and nothing
 * is left on the stack for history to wait on (#1086).
 */
test.each([
  ["Move…", "Move into hel1"],
  ["Delete…", "Delete"],
  ["Add a route", "Expose"],
  ["Edit Postgres UI", "Save"],
  ["New asset inside", "Create"],
])("%s opens a dialog on the layer stack, and its close shuts it (#1527)", async (opener, inside) => {
  render("#/asset/asset:vm-db-01", estate(seed(), [pgRoute()]));
  await waitFor(() => expect(text()).toContain("Postgres UI"));

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
 * How a footer button is drawn, in the words of the design system's Controls
 * section (#2291): the default size is the only one a footer takes, *Cancel* is
 * a ghost, the confirm is the primary (filled red when it destroys), and an
 * edit dialog's extra action sits apart on the far left as red text.
 */
function drawnAs(control: Element): string {
  const has = (name: string) => control.classList.contains(name);
  if (!has("tf-btn") || has("tf-sm") || has("tf-lg")) return "not a default .tf-btn";
  if (has("tf-primary")) return has("tf-danger") ? "confirm, red" : "primary";
  if (has("tf-first")) return has("tf-danger") ? "far left, red" : "far left";
  return has("tf-ghost") ? "ghost" : "plain";
}

/**
 * **Every dialog the page opens has the one footer: ghost *Cancel*, then the
 * primary; an edit dialog's *Delete* on the far left** (#2291, spec #2238
 * stories 26-28 and 32).
 */
test.each([
  ["Move…", ["Cancel: ghost", "Move into hel1: primary"]],
  ["Delete…", ["Cancel: ghost", "Delete: confirm, red"]],
  ["Add a route", ["Cancel: ghost", "Expose: primary"]],
  ["Edit Postgres UI", ["Delete: far left, red", "Cancel: ghost", "Save: primary"]],
  ["New asset inside", ["Cancel: ghost", "Create and add another: plain", "Create: primary"]],
])("%s opens a dialog with the canonical footer", async (opener, footer) => {
  render("#/asset/asset:vm-db-01", estate(seed(), [pgRoute()]));
  await waitFor(() => expect(text()).toContain("Postgres UI"));

  click(opener);
  const dialog = view.target.querySelector('[role="dialog"]')!;
  const foot = [...dialog.querySelectorAll("footer button")];
  expect(foot.map((control) => `${(control.textContent ?? "").trim()}: ${drawnAs(control)}`)).toEqual(footer);
});

/**
 * **A dialog does not outlive the asset it was opened on.**
 *
 * *Move…* and the delete confirmation both name the page's asset and act on
 * it, so one still standing after the address moved would offer to move or
 * delete something other than the thing it names. The address can move while
 * a dialog is up without anybody clicking past the scrim (a launcher hit, a
 * notification's deep link), which is what `nav.go` stands in for. The open
 * form goes the same way: a half-typed value must not save onto another asset.
 */
test("an open dialog or form closes when the address names another asset", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:postgres", store);
  await waitFor(() => expect(title()).toBe("postgres"));

  click("Move…");
  expect(text()).toContain("Move into vm-db-01");
  nav.go("#/asset/asset:vm-app-02");
  flushSync();
  expect(view.target.querySelector('[role="dialog"]')).toBeNull();
  await waitFor(() => expect(subtitle()).toBe("VM in hel1"));
  expect(store.rows.find((row) => row.id === "asset:postgres")?.parent_id).toBe("asset:vm-db-01");

  click("Edit");
  type("Name", "vm-app-03");
  await settle();
  nav.go("#/asset/asset:vm-db-01");
  flushSync();
  expect(editing()).toBe(false);
  await waitFor(() => expect(title()).toBe("vm-db-01"));
  expect(store.sent).toEqual([]);
});

/**
 * **A route is exposed from the page, lands on an asset picked by walking, and
 * is picked at its own address** (#432, criterion 3).
 *
 * The dialog answers with a row, the page goes to the **route's** address, and
 * what is on screen is the read that followed. The target is picked by walking
 * the estate, so the click on `postgres` is also the assertion that the picker
 * reads a level at a time.
 */
test("a route is exposed from the page and lands where the picker was walked", async () => {
  const store = estate(seed());
  const { nav } = render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Exposes No routes."));

  click("Add a route");
  expect(text()).toContain("On vm-db-01");
  type("Name", "Postgres UI");
  type("URL or endpoint", "https://pg.hel1.example/");
  choose("Visibility", "public");
  await waitFor(() => button("Land on postgres"));
  click("Land on postgres");
  click("Expose");

  await waitFor(() => expect(text()).toContain("Postgres UI"));
  const written = store.routes[0];
  expect(written).toMatchObject({
    asset_id: "asset:vm-db-01",
    target_id: "asset:postgres",
    name: "Postgres UI",
    url: "https://pg.hel1.example/",
    visibility: "public",
  });
  // The route's own address, kept: a reader who copies this link gets the
  // route rather than the asset.
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: null, routeId: written?.id });
  await waitFor(() => expect(title()).toBe("vm-db-01"));
  const page = text();
  expect(page).toContain("https://pg.hel1.example/");
  expect(page).toContain("lands on postgres");
  // Exposed here, and reaching the container this VM holds: both ends, on the
  // one page that is on both.
  expect(page).toContain("inside, on postgres");
  expect(view.target.querySelector('.tf-as-route[aria-current="true"] .tf-as-rname')?.textContent).toBe("Postgres UI");
});

/**
 * **A URL with no scheme is refused in the dialog, in the backend's own
 * words**, and nothing is written: a second copy of "what is a URL" here would
 * be the copy that goes stale the day a route may be an `ssh://` endpoint.
 */
test("a route with no scheme is refused in place and nothing is written", async () => {
  const store = estate(seed());
  render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Exposes No routes."));

  click("Add a route");
  type("Name", "Postgres UI");
  type("URL or endpoint", "pg.hel1.example");
  click("Expose");

  await waitFor(() => expect(text()).toContain("carries no scheme"));
  expect(store.routes).toHaveLength(0);
  expect((field("Name") as HTMLInputElement).value).toBe("Postgres UI");
});

/**
 * **An exposed route is edited and deleted from the same dialog**, and each
 * lands the reader where the thing they were looking at still exists: the
 * delete goes back to the asset's page, since the route's own address would
 * answer `not_found` over a deletion that worked.
 */
test("a route is edited and then deleted from the page", async () => {
  const store = estate(seed(), [pgRoute()]);
  const { nav } = render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Postgres UI"));

  click("Edit Postgres UI");
  expect((field("URL or endpoint") as HTMLInputElement).value).toBe("https://pg.hel1.example/");
  type("Name", "Postgres console");
  // Clearing the target is a first-class choice: an endpoint that lands on
  // nothing knobas knows is a route the model holds on purpose.
  click("Clear");
  click("Save");

  await waitFor(() => expect(text()).toContain("Postgres console"));
  expect(store.routes[0]).toMatchObject({ name: "Postgres console", target_id: null });
  expect(text()).toContain("lands on nothing knobas knows");
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: null, routeId: "route:pg" });

  click("Edit Postgres console");
  click("Delete");
  await waitFor(() => expect(text()).toContain("Exposes No routes."));
  expect(store.routes).toHaveLength(0);
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: "asset:vm-db-01" });
});

/**
 * **A property of the reader's own is written on a route, and cleared from
 * it**: spec #427's *"the certificate expiry as a property"* (#432). The date
 * kind is the one that would silently arrive as a string if the picker were
 * ignored, so the assertion is on the stored shape too.
 */
test("a certificate expiry is written on a route as a property and cleared again", async () => {
  const store = estate(seed(), [pgRoute()]);
  render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Postgres UI"));

  click("Edit Postgres UI");
  click("Add a property");
  type("Property key", "cert_expires");
  choose("Property kind", "date");
  type("Property value", "2026-12-01");
  click("Save");

  await waitFor(() => expect(text()).toContain("cert_expires 2026-12-01"));
  expect(store.routes[0]?.properties).toEqual({ cert_expires: { kind: "date", value: "2026-12-01" } });

  // Emptying the value clears the key: a blank text is refused by the backend
  // with *clear the property instead*, so that is what a blank means.
  click("Edit Postgres UI");
  type("Property value", "");
  click("Save");

  // The route's history (it is picked, at its own address) says where it went:
  // the difference between a cleared property and one that was never there.
  await waitFor(() => expect(text()).not.toContain("cert_expires 2026-12-01"));
  expect(text()).toContain("cert_expires: 2026-12-01 → nothing");
  expect(store.routes[0]?.properties).toEqual({});
});

/**
 * **A route's URL is handed to the OS browser**, spec §12.2's *open URL*, and
 * a scheme no browser takes is said rather than swallowed: a route is *a URL
 * or an endpoint*, so `postgres://…` is a route the model holds on purpose.
 */
test("a route's URL opens in the browser, and a scheme no browser takes is said out loud", async () => {
  const store = estate(seed(), [
    pgRoute(),
    pgRoute({ id: "route:sql", name: "Postgres wire", url: "postgres://pg.hel1.example:5432/" }),
  ]);
  render("#/asset/asset:vm-db-01", store);
  await waitFor(() => expect(text()).toContain("Postgres UI"));

  click("Open https://pg.hel1.example/");
  await waitFor(() => expect(store.opened).toEqual(["https://pg.hel1.example/"]));
  expect(banner()).toBeNull();

  store.refuseToOpen({
    code: "invalid",
    message: "refused to open postgres://pg.hel1.example:5432/ — only http and https",
  });
  click("Open postgres://pg.hel1.example:5432/");
  await waitFor(() => expect(banner()).toContain("Could not open the link: refused to open"));
  expect(store.opened).toHaveLength(2);
  expect(store.routes).toHaveLength(2);
});

/**
 * **The route the address names shows its own history**: spec #427, routes
 * *"have `#/route/<id>` addresses, and their own history"*. A route's line is
 * written on the **route's** entity, so the asset's history beside it says
 * nothing about the rename. A target change carries asset **ids** on the
 * wire, and the page names them, the way it names a move's new parent.
 */
test("a route's own history is drawn when its address names it", async () => {
  const store = estate(seed(), [pgRoute()]);
  render("#/route/route:pg", store);
  await waitFor(() => expect(text()).toContain("Postgres UI history"));
  expect(text()).toContain("Nothing recorded.");

  click("Edit Postgres UI");
  type("Name", "Postgres console");
  click("Save");

  await waitFor(() => expect(text()).toContain("Postgres console history"));
  // One line, old and new: the fields the dialog also sent changed nothing,
  // and an edit that changes nothing writes no line.
  expect(text()).toContain("name: Postgres UI → Postgres console");
  expect(text()).not.toContain("url:");
  expect(text()).toContain("History Nothing recorded.");

  click("Edit Postgres console");
  click("Clear");
  click("Save");
  await waitFor(() => expect(text()).toContain("target: postgres → nothing"));
  expect(text()).not.toContain("asset:postgres");
});
