/**
 * A folder renderer's page (ADR-0044): the fields, the command's check, *Add*, *Try it* on a repository with a
 * checkout here (saved first, then drawn), and a try taken back when the page is left without *Save*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FolderAsk, FolderRenderer, FolderRendererDraft } from "../../ipc/app";
import type { EntityRow } from "../../ipc/entity";
import { createFolderRenderers } from "../../render/folders.svelte";
import type { FolderRenderPorts } from "../render/TfFolderRender.svelte";
import TfFolderRendererPage from "./TfFolderRendererPage.svelte";
import TfFolderRenderersTable from "./TfFolderRenderersTable.svelte";
import { landOn, takeLanding } from "./landing";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const REPO = "gitea:repo:o/flows";

function store(start: FolderRenderer[] = []) {
  let list = start;
  const saves: FolderRendererDraft[][] = [];
  const folders = createFolderRenderers({
    read: async () => list,
    save: async (drafts) => {
      saves.push(drafts);
      if (drafts.some((d) => !d.command.includes("{path}"))) throw { code: "invalid", message: "the command needs {path}" };
      list = drafts.map((d, i) => ({ ...d, id: d.id ?? `new-${i}`, timeout_secs: d.timeout_secs ?? 30 }) as FolderRenderer);
      return list;
    },
  });
  return { folders, saves, list: () => list };
}

const reads = {
  repos: async () => [{ entity_id: REPO, title: "o/flows", source_id: "gitea" } as EntityRow],
  checkouts: async (ids: readonly string[]) => ids.map((entityId) => ({ entityId, path: "/clones/o/flows", clonesRoot: "/clones" })),
  files: async () => ["flows/orders/orders.lcproj", "flows/orders/main.flow", "README.md"],
};

function ports() {
  const asked: FolderAsk[] = [];
  const p: FolderRenderPorts = {
    render: (at) => (asked.push(at), Promise.resolve({ renderer: at.renderer, folder: at.folder, drawn: null, current: false, running: { state: "queued" }, failure: null, from: "clone", commit: null })),
    stop: () => Promise.resolve(),
    leave: () => Promise.resolve(),
    output: () => Promise.resolve(new Uint8Array()),
    frame: () => Promise.resolve("t"),
    listen: () => Promise.resolve(() => {}),
  };
  return { p, asked };
}

/** *Try it on*'s source (#2562), pressed. */
async function source(words: string) {
  [...target.querySelectorAll<HTMLButtonElement>(".tf-try .tf-seg button")].find((b) => b.textContent?.trim() === words)!.click();
  await settle();
}

async function settle() {
  for (let round = 0; round < 5; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const type = (label: string, text: string) => {
  const input = target.querySelector<HTMLInputElement>(`[aria-label="${label}"], #${label}`)!;
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
};
const button = (text: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text)!;

test("the command is checked as typed: {path} is needed, and no other placeholder is taken", async () => {
  const s = store();
  let left = 0;
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: null, folder: null }, folders: s.folders, reads, ports: ports().p, leave: () => (left += 1) } });
  await settle();
  type("Name", "Low code");
  type("Command", "lc . --out {out}");
  expect(target.textContent).toContain("Put {path} where the folder goes");
  expect(button("Add folder renderer").disabled).toBe(true);
  type("Command", "lc {path} {branch}");
  expect(target.textContent).toContain("{branch} is not something a folder renderer is given");
  type("Command", "lc {path} --png {out}.png");
  target.querySelector<HTMLInputElement>("#tf-frp-holds")!.value = "*.lcproj";
  target.querySelector<HTMLInputElement>("#tf-frp-holds")!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(button("Add folder renderer").disabled).toBe(false);
  button("Add folder renderer").click();
  await settle();
  expect(s.list()).toMatchObject([{ name: "Low code", command: "lc {path} --png {out}.png", holds: ["*.lcproj"], output: "detect" }]);
  expect(left).toBe(1);
});

test("Try it saves the entry and draws the folder picked; leaving without Save takes the try back", async () => {
  const s = store();
  const fp = ports();
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: REPO, folder: "flows/orders" }, folders: s.folders, reads, ports: fp.p, leave: () => {} } });
  await settle();
  expect(target.querySelector<HTMLSelectElement>("#tf-frp-repo")!.value).toBe(REPO);
  expect(target.querySelector<HTMLInputElement>("#tf-frp-folder")!.value).toBe("flows/orders");
  const offered = [...target.querySelectorAll("#tf-frp-folders-list option")].map((o) => o.getAttribute("value"));
  expect(offered).toEqual(["", "flows", "flows/orders"]);
  type("Name", "Lister");
  type("Command", "tree {path}");
  button("Try it").click();
  await settle();
  expect(s.list().map((r) => r.name)).toEqual(["Lister"]);
  expect(fp.asked[0]).toMatchObject({ entityId: REPO, renderer: "new-0", folder: "flows/orders", ask: "open" });
  expect(target.querySelector(".tf-frender header .tf-p")?.textContent).toBe("flows/orders");

  unmount(app);
  app = null;
  await settle();
  expect(s.list()).toEqual([]);
});

test("a refused try says why and saves nothing", async () => {
  const s = store([{ id: "a", name: "A", command: "a {path}", holds: [], folders: [], output: "text", timeout_secs: 30 }]);
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: "a", repo: null, folder: null }, folders: s.folders, reads, ports: ports().p, leave: () => {} } });
  await settle();
  expect(target.querySelector<HTMLInputElement>("#tf-frp-name")!.value).toBe("A");
  expect(button("Remove")).toBeDefined();
});

test("while later batches of checkouts come in, the picked repository stays and its folders are read once (2026-11-17)", async () => {
  // The application page's flicker (Björn, v3.2.9-ux17): each batch of checkouts drew the picked repository anew and read its folders again.
  const others = Array.from({ length: 120 }, (_, n) => ({ entity_id: `gitea:repo:o/forge-${n}`, title: `o/forge-${n}`, source_id: "gitea" }) as EntityRow);
  const batches: (() => void)[] = [];
  let asked = 0;
  const late = {
    repos: async () => [...(await reads.repos()), ...others],
    checkouts: (ids: readonly string[]) =>
      new Promise<{ entityId: string; path: string | null; clonesRoot: string | null }[]>((resolve) =>
        batches.push(() => resolve(ids.map((entityId) => ({ entityId, path: entityId === REPO ? "/clones/o/flows" : null, clonesRoot: "/clones" })))),
      ),
    // Asked again, it never answers: the folders would stand empty, as they did between batches.
    files: () => (asked++ === 0 ? reads.files() : new Promise<string[]>(() => {})),
  };
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: null, folder: null }, folders: store().folders, reads: late, ports: ports().p, leave: () => {} } });
  await settle();
  await source("A repository");
  batches.shift()!();
  await settle();
  const folders = () => [...target.querySelectorAll("#tf-frp-folders-list option")].map((o) => o.getAttribute("value"));
  const first = folders();
  expect(first).toContain("flows/orders");
  while (batches.length > 0) {
    batches.shift()!();
    await settle();
    expect(target.querySelector<HTMLSelectElement>("#tf-frp-repo")!.value).toBe(REPO);
    expect(folders()).toEqual(first);
  }
  expect(asked).toBe(1);
});

test("with no checkout on this disk the rows drawn while reading stay and say so (2026-11-17)", async () => {
  // The saved scale database, where no repository is cloned: the two rows gave way to a paragraph once the checkouts answered.
  let answer: () => void = () => {};
  const none = {
    ...reads,
    checkouts: (ids: readonly string[]) =>
      new Promise<{ entityId: string; path: string | null; clonesRoot: string | null }[]>((resolve) => (answer = () => resolve(ids.map((entityId) => ({ entityId, path: null, clonesRoot: "/clones" }))))),
  };
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: null, folder: null }, folders: store().folders, reads: none, ports: ports().p, leave: () => {} } });
  await settle();
  await source("A repository");
  const rows = () => [...target.querySelectorAll(".tf-stg-row :is(input, select, button)")].map((el) => el.id || el.textContent?.trim());
  const before = rows();
  expect(before).toContain("tf-frp-repo");
  expect(before).toContain("tf-frp-folder");
  answer();
  await settle();
  expect(rows()).toEqual(before);
  const pick = target.querySelector<HTMLSelectElement>("#tf-frp-repo")!;
  expect(pick.disabled).toBe(true);
  expect(pick.textContent?.trim()).toBe("No repository is cloned on this computer");
  expect(target.querySelector(".tf-stg-more a[href='#/code']")!.parentElement!.classList.contains("tf-stg-done")).toBe(false);
  expect(button("Try it").disabled).toBe(true);
});

test("each way back to the list names the entry it lands on, and the list focuses it (2026-11-17)", async () => {
  const one = { id: "a", name: "A", command: "a {path}", holds: [], folders: [], output: "text", timeout_secs: 30 } as FolderRenderer;
  const two = { ...one, id: "b", name: "B", command: "b {path}" } as FolderRenderer;
  takeLanding("folder-renderers");
  const mountPage = async (s: ReturnType<typeof store>, renderer: string | null) => {
    app = mount(TfFolderRendererPage, { target, props: { at: { renderer, repo: null, folder: null }, folders: s.folders, reads, ports: ports().p, leave: () => {} } });
    await settle();
  };
  const done = () => {
    unmount(app!);
    app = null;
  };

  let s = store([one]);
  await mountPage(s, null);
  type("Name", "Tree");
  type("Command", "tree {path}");
  button("Add folder renderer").click();
  await settle();
  expect(takeLanding("folder-renderers"), "Add lands on the new entry").toBe(s.list().at(-1)!.id);
  done();

  s = store([one, two]);
  await mountPage(s, "a");
  button("Save").click();
  await settle();
  expect(takeLanding("folder-renderers"), "Save lands on the entry edited").toBe("a");
  done();

  s = store([one, two]);
  await mountPage(s, "a");
  button("Remove").click();
  await settle();
  expect(takeLanding("folder-renderers"), "Remove lands on the entry that took its place").toBe("b");
  done();

  s = store([one]);
  await mountPage(s, "a");
  button("Cancel").click();
  await settle();
  expect(takeLanding("folder-renderers"), "Cancel lands on the entry it was opened on").toBe("a");
  done();

  // The list's end: it focuses what it was told, once it has read the list.
  s = store([one, two]);
  landOn("folder-renderers", "b");
  app = mount(TfFolderRenderersTable, { target, props: { folders: s.folders } });
  await settle();
  expect(document.activeElement?.tagName).toBe("TR");
  expect(document.activeElement?.querySelector("a")?.getAttribute("aria-label")).toBe("Edit B");
  expect(takeLanding("folder-renderers")).toBeNull();
});

test("typing {outdir} makes it a folder of files: the kind is set and the others are disabled (ADR-0046)", async () => {
  const s = store();
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: null, folder: null }, folders: s.folders, reads, ports: ports().p, leave: () => {} } });
  await settle();
  expect([...target.querySelectorAll(".tf-phs .tf-mono")].map((e) => e.textContent)).toEqual(["{path}", "{out}", "{outdir}"]);
  type("Name", "Docs site");
  type("Command", "lc site {path} --out {outdir}");
  await settle();
  const select = target.querySelector<HTMLSelectElement>("#tf-frp-out")!;
  expect(select.value).toBe("folder");
  const others = [...select.options].filter((o) => o.value !== "folder");
  expect(others.every((o) => o.disabled && o.title === "The command holds {outdir}, so it makes a folder")).toBe(true);
  // Chosen without {outdir}, the command field says what it needs.
  type("Command", "lc site {path}");
  await settle();
  select.value = "folder";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(target.textContent).toContain("Put {outdir} where the tool takes its output folder.");
});

test("Try it on pasted text saves the draft, then runs it in the tries' own registry, outside any repository (#2562)", async () => {
  const s = store();
  const fp = ports();
  const tried: [string, unknown, string][] = [];
  const tryRender = (renderer: string, on: unknown, ask: string) => (
    tried.push([renderer, on, ask]),
    Promise.resolve({ renderer, folder: "", drawn: null, current: false, running: { state: "queued" as const }, failure: null, from: "working_tree" as const, commit: null })
  );
  app = mount(TfFolderRendererPage, { target, props: { at: { renderer: null, repo: null, folder: null }, folders: s.folders, reads, ports: fp.p, tryRender, leave: () => {} } });
  await settle();
  type("Name", "Low code");
  type("Command", "lc {path} {out}.png");
  await source("Pasted text");
  type("tf-frp-try-name", "orders.lcproj");
  const text = target.querySelector<HTMLTextAreaElement>("#tf-frp-try-text")!;
  text.value = "<project/>";
  text.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-try button")].find((b) => b.textContent?.trim() === "Try it")!.click();
  await settle();
  expect(s.saves.length, "saved first: only a saved command runs").toBe(1);
  expect(tried).toEqual([["new-0", { kind: "text", name: "orders.lcproj", text: "<project/>" }, "open"]]);
  expect(fp.asked, "no repository's render was asked").toEqual([]);
  expect(target.querySelector(".tf-try .tf-stg-gap"), "drawn under the panel").not.toBeNull();
});
