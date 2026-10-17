/**
 * *Import…* in Dependencies' *Add ▾* (#1066, R-11.20, R-11.23), mounted over
 * stand-in commands: the dialog previews (which writes nothing), *Import*
 * applies the same file under the same producer, and Dependencies reads the
 * estate again. An importer asks for a token, then where to land, then
 * previews what it produced. What the commands answer is `assets_ipc.rs`'.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetRow, Dependencies, DepItem, DepViews, ImportPreview, Produced } from "../../ipc/assets";
import { toasts } from "../../shell/toasts.svelte";
import { createDepViewState } from "./views.svelte";
import { settingsOf } from "./views";
import TfDeps from "./TfDeps.svelte";

const item = (id: string, kind: string, name: string): DepItem => ({
  id,
  kind,
  name,
  health: "ok",
  state: null,
  monitors: 0,
  environment: null,
  owner: null,
});
const read = (extra: DepItem[] = []): Dependencies => ({
  layers: [{ id: "machines", name: "Machines", kinds: ["vm"] }],
  kinds: [{ id: "vm", label: "VM", monogram: "VM" }],
  items: [item("asset:vm", "vm", "vm-pay-01"), ...extra],
  relations: [],
});
const VIEWS: DepViews = {
  views: [{ id: "everything", name: "Everything", settings: settingsOf({}) }],
  opens_from_ticket: "everything",
  opens_from_top: "everything",
};
const PREVIEW: ImportPreview = {
  name: "testenv",
  known: [{ id: "asset:vm", kind: "asset", name: "vm-pay-01", type_label: "VM", parent_id: null }],
  new: [{ id: "asset:db", kind: "asset", name: "db-01", type_label: "VM", parent_id: null }],
  changes: [
    {
      id: "asset:vm",
      name: "vm-pay-01",
      properties: [{ key: "size", label: "Size", from: { kind: "text", value: "cx33" }, to: { kind: "text", value: "cx23" }, plan: "kept" }],
      monitors: [],
    },
  ],
  monitor_links: [],
  unresolved: [],
};
const ROW = { id: "asset:site", name: "fsn1", type_label: "Site" } as AssetRow;

let calls: [string, ...unknown[]][] = [];
let reads = 0;
let estate: Dependencies;
let produced: Produced[] = [];
/** What the preview of `{"name":"slow"}` waits on. */
let release = () => {};
let held = new Promise<void>((resolve) => (release = resolve));
let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

function render() {
  target = document.body.appendChild(document.createElement("div"));
  const log =
    <T,>(name: string, value: () => T) =>
    (...args: unknown[]) => {
      calls.push([name, ...args]);
      return Promise.resolve(value());
    };
  app = mount(TfDeps, {
    target,
    props: {
      ready: true,
      views: createDepViewState(),
      ports: {
        dependencies: () => {
          reads += 1;
          return Promise.resolve(estate);
        },
        depViews: () => Promise.resolve(VIEWS),
        previewEstateImport: (file: string, producer: string) => {
          calls.push(["preview", file, producer]);
          const answer = { ...PREVIEW, name: file === '{"name":"slow"}' ? "slow" : PREVIEW.name };
          return file === '{"name":"slow"}' ? held.then(() => answer) : Promise.resolve(answer);
        },
        applyEstateImport: log("apply", () => {
          estate = read([item("asset:db", "vm", "db-01")]);
          return { assets_created: 1, routes_created: 0, properties_set: 0, properties_kept: 1, monitors_kept: 0, monitors_linked: 0 };
        }),
        produceEstateFile: log("produce", () => produced.shift()!),
        assetTree: log("tree", () => [ROW]),
      },
    },
  });
}

async function settle() {
  for (let i = 0; i < 8; i++) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  calls = [];
  reads = 0;
  estate = read();
  produced = [];
  toasts.items = [];
  // jsdom has no blob URLs; the produced file's download link needs one.
  URL.createObjectURL = () => "blob:produced";
  URL.revokeObjectURL = () => undefined;
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const button = (text: string, root: ParentNode = document) =>
  [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim().startsWith(text))!;
const dialog = () => document.querySelector<HTMLElement>(".tf-dialog");
const field = (label: string) => {
  const el = [...dialog()!.querySelectorAll("label")].find((l) => l.textContent?.trim() === label)!;
  return document.getElementById(el.htmlFor) as HTMLInputElement & HTMLSelectElement;
};

async function openImport() {
  render();
  await settle();
  button("Add ▾").click();
  flushSync();
  const entry = [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].find((b) => b.textContent?.includes("Import…"))!;
  expect(entry.getAttribute("aria-disabled")).toBeNull();
  entry.click();
  await settle();
}

async function chooseFile(text: string) {
  const input = field("Estate file");
  Object.defineProperty(input, "files", { configurable: true, value: [new File([text], "estate.json", { type: "application/json" })] });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  await settle();
}

test("Import… opens the import dialog, and the preview writes nothing", async () => {
  await openImport();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("Import an estate file");
  expect(button("Import", dialog()!).disabled).toBe(true);
  await chooseFile('{"name":"testenv"}');
  expect(calls).toEqual([["preview", '{"name":"testenv"}', "estate_file"]]);
  const text = dialog()!.textContent!;
  expect(text).toContain("New — 1");
  expect(text).toContain("db-01");
  expect(text).toContain("Would change — 1");
  expect(text).toContain("stays — you edited it here");
  expect(text).toContain("Already in the tree — 1");
  expect(reads).toBe(1);
});

test("Import applies the previewed file under its producer, closes, and reads the estate again", async () => {
  await openImport();
  await chooseFile('{"name":"testenv"}');
  button("Import", dialog()!).click();
  await settle();
  await settle();
  expect(calls.at(-1)).toEqual(["apply", '{"name":"testenv"}', "estate_file"]);
  expect(dialog()).toBeNull();
  expect(reads).toBe(2);
  expect(target.querySelector(".tf-dp-t[data-id='asset:db']")).not.toBeNull();
  expect(toasts.items.at(-1)!.text).toBe("Imported testenv: 1 asset created, 1 property kept as you edited it");
});

test("a slower preview of an earlier file does not replace the newer one's", async () => {
  held = new Promise<void>((resolve) => (release = resolve));
  await openImport();
  await chooseFile('{"name":"slow"}');
  await chooseFile('{"name":"testenv"}');
  release();
  await settle();
  button("Import", dialog()!).click();
  await settle();
  await settle();
  expect(calls.at(-1)).toEqual(["apply", '{"name":"testenv"}', "estate_file"]);
  expect(toasts.items.at(-1)!.text).toMatch(/^Imported testenv:/);
});

test("Cancel closes without a write", async () => {
  await openImport();
  await chooseFile('{"name":"testenv"}');
  button("Cancel", dialog()!).click();
  await settle();
  expect(dialog()).toBeNull();
  expect(calls.map(([name]) => name)).toEqual(["preview"]);
  expect(reads).toBe(1);
});

test("an importer asks for a token, then where to land, then previews what it produced", async () => {
  produced = [
    { state: "token_needed" },
    { state: "landing_needed", servers: ["db-01"] },
    { state: "ready", file: "{produced}", new_servers: ["db-01"], skipped: [] },
  ];
  await openImport();
  const from = field("Import from");
  from.value = "hcloud";
  from.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  button("Read Hetzner Cloud", dialog()!).click();
  await settle();
  const token = field("API token");
  token.value = "secret";
  token.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Read Hetzner Cloud", dialog()!).click();
  await settle();
  expect(dialog()!.textContent).toContain("1 server is not in the tree yet: db-01.");
  button("fsn1", dialog()!).click();
  await settle();
  button("Put them in fsn1", dialog()!).click();
  await settle();
  await settle();
  expect(calls.filter(([name]) => name !== "tree")).toEqual([
    ["produce", "hcloud", null, null],
    ["produce", "hcloud", "secret", null],
    ["produce", "hcloud", null, { parent: "asset:site" }],
    ["preview", "{produced}", "hcloud"],
  ]);
  expect(dialog()!.textContent).toContain("Download hcloud-estate.json");
  expect(dialog()!.textContent).toContain("New — 1");
});
