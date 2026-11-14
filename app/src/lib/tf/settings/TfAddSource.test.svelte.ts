/**
 * *Add a source* keeps what can be kept through `Esc` and × (F-647), and no secret, in the draft or anywhere else that
 * outlives the dialog.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ConnectionReport, SourceDescriptor } from "../../ipc/sources";
import { drafts } from "../drafts.svelte";
import { createLayers } from "../layers.svelte";
import { addressToHold, newSourceDraft } from "./source-drafts.svelte";
import TfAddSource from "./TfAddSource.svelte";

const TOKEN = "s3cret-t0ken";
const PASSWORD = "acc0unt-passw0rd";
/** Split, because the house rule reads any scheme and host in a file as a network reference. */
const SCHEME = "https:" + "//";

function adapter(id: string, over: Partial<SourceDescriptor> = {}): SourceDescriptor {
  return {
    id,
    adapter_kind: id,
    name: id === "gitea" ? "Gitea" : "Jira",
    entity_kinds: [{ kind: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR" }],
    auth_methods: ["Token"],
    accepts_account: true,
    config_schema: { type: "object", properties: {} },
    ...over,
  } as unknown as SourceDescriptor;
}

const passed = { ok: true, account: "mara", discovered: {}, error: null } as unknown as ConnectionReport;

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let layers: ReturnType<typeof createLayers>;
const closed = vi.fn();
const added = vi.fn();
const testSource = vi.fn(() => Promise.resolve(passed));
const addSource = vi.fn(() => Promise.resolve({ id: "gitea", health: {} } as never));

beforeEach(() => {
  newSourceDraft.forget();
  drafts.reset();
  closed.mockClear();
  added.mockClear();
  testSource.mockClear();
  addSource.mockClear();
  layers = createLayers();
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  shut();
  target.remove();
});

function shut() {
  if (app) unmount(app);
  app = null;
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

async function open() {
  app = mount(TfAddSource, {
    target,
    props: {
      ports: {
        listAdapters: () => Promise.resolve([adapter("gitea"), adapter("jira")]),
        testSource,
        addSource,
        previewInstallAdapter: vi.fn(),
        installAdapter: vi.fn(),
      } as never,
      layers,
      onclose: closed,
      onsaved: added,
    },
  });
  await settle();
}

const field = (id: string) => target.querySelector<HTMLInputElement>(`#${id}`);
const radio = (name: string) => [...target.querySelectorAll("label")].find((label) => label.textContent?.trim().startsWith(name))?.querySelector("input");
const press = (words: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.trim() === words)!.click();

function fill(id: string, words: string) {
  const input = field(id)!;
  input.value = words;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

async function pick(name: string) {
  radio(name)!.click();
  flushSync();
  await settle();
}

async function typeAll() {
  await pick("Gitea");
  fill("tf-add-address", "https://git.example.test");
  fill("tf-add-name", "Our forge");
  fill("tf-add-secret", TOKEN);
  fill("tf-add-account-user", "mara");
  fill("tf-add-account-pass", PASSWORD);
}

function esc() {
  target.querySelector(".tf-dialog")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
}

/** Everything that outlives the dialog: the store, the words drafts, the browser's storage, and what is left in the page. */
function whatOutlives(): string {
  const stores = [globalThis.localStorage, globalThis.sessionStorage].flatMap((store) =>
    store ? Array.from({ length: store.length }, (_, i) => `${store.key(i)}=${store.getItem(store.key(i)!)}`) : [],
  );
  return JSON.stringify([newSourceDraft.held(), drafts.get("new-source"), stores, target.innerHTML]);
}

function expectNoSecret() {
  const left = whatOutlives();
  expect(left).not.toContain(TOKEN);
  expect(left).not.toContain(PASSWORD);
  expect(left).not.toContain("mara");
}

test("`Esc` keeps the kind, the address and the name, and the token goes with the dialog", async () => {
  await open();
  await typeAll();
  esc();
  expect(closed).toHaveBeenCalledOnce();
  shut();

  expect(newSourceDraft.held()).toEqual({ adapter: "gitea", address: "https://git.example.test", name: "Our forge" });
  expect(Object.keys(newSourceDraft.held()!).sort()).toEqual(["adapter", "address", "name"]);
  expectNoSecret();

  await open();
  expect(radio("Gitea")!.checked).toBe(true);
  expect(field("tf-add-address")!.value).toBe("https://git.example.test");
  expect(field("tf-add-name")!.value).toBe("Our forge");
  expect(field("tf-add-secret")!.value).toBe("");
  expect(field("tf-add-account-user")!.value).toBe("");
  expect(field("tf-add-account-pass")!.value).toBe("");
  expectNoSecret();
});

test("× keeps the same three, and no secret", async () => {
  await open();
  await typeAll();
  target.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  shut();

  expect(newSourceDraft.held()).toEqual({ adapter: "gitea", address: "https://git.example.test", name: "Our forge" });
  expectNoSecret();
});

test("a test of the connection and then `Esc` keeps no secret and no answer: reopened, it asks for the token and tests again", async () => {
  await open();
  await typeAll();
  press("Test connection");
  await settle();
  expect(testSource).toHaveBeenCalledOnce();
  expect(target.querySelector<HTMLButtonElement>(".tf-primary")!.disabled, "the test passed").toBe(false);

  esc();
  shut();
  expect(newSourceDraft.held()).toEqual({ adapter: "gitea", address: "https://git.example.test", name: "Our forge" });
  expectNoSecret();

  await open();
  expect(field("tf-add-secret")!.value).toBe("");
  expect(target.querySelector<HTMLButtonElement>(".tf-primary")!.disabled, "no passing test came back").toBe(true);
  expect(target.textContent).toContain("Still needs the token");
  expectNoSecret();
});

test("Cancel ends the draft", async () => {
  await open();
  await typeAll();
  press("Cancel");
  flushSync();
  expect(closed).toHaveBeenCalledOnce();
  shut();

  expect(newSourceDraft.held()).toBeUndefined();
  await open();
  expect(radio("Gitea")!.checked).toBe(false);
  expect(field("tf-add-address")).toBeNull();
});

test("a source added ends the draft", async () => {
  await open();
  await typeAll();
  press("Test connection");
  await settle();
  press("Add source");
  await settle();
  expect(added).toHaveBeenCalledOnce();
  expect(newSourceDraft.held()).toBeUndefined();
});

test("an address with a sign-in in it is not held, whichever way it was written", async () => {
  await open();
  await pick("Gitea");
  for (const address of [`${SCHEME}me:hunter2@git.example.test`, `${SCHEME}me:hunter2`, "me:hunter2@git.example.test", `${SCHEME}me:pa/hunter2@git.example.test`]) {
    fill("tf-add-address", address);
    expect(newSourceDraft.held()?.address, address).toBe("");
    expect(whatOutlives()).not.toContain("hunter2");
  }
  fill("tf-add-address", "https://git.example.test/api?private_token=hunter2#frag");
  expect(newSourceDraft.held()?.address).toBe("https://git.example.test/api");
  expect(JSON.stringify(newSourceDraft.held())).not.toContain("hunter2");
});

test("what an address may keep", () => {
  expect(addressToHold(" https://git.example.test:3000/x ")).toBe("https://git.example.test:3000/x");
  expect(addressToHold("git.example.test:3000")).toBe("git.example.test:3000");
  expect(addressToHold("https://")).toBe("https://");
  expect(addressToHold("https://git.example.test/@group")).toBe("");
});

test("an adapter that has gone gives nothing back", async () => {
  newSourceDraft.hold("uninstalled", "https://x.test", "X");
  await open();
  expect(radio("Gitea")!.checked).toBe(false);
  expect(field("tf-add-address")).toBeNull();
});

/** F-656: a press folds the list to the chosen row with *Change*, so the form fits. */
const kinds = () => [...target.querySelectorAll<HTMLInputElement>("input[name='tf-add-adapter']")];
const change = () => [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.trim() === "Change");

function key(on: Element, name: string, shiftKey = false) {
  const event = new KeyboardEvent("keydown", { key: name, shiftKey, bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  flushSync();
  return event;
}

test("a press on a kind folds the list to that row, and focus goes on to the address", async () => {
  await open();
  expect(kinds()).toHaveLength(2);
  expect(change()).toBeUndefined();

  await pick("Gitea");
  expect(kinds()).toHaveLength(1);
  expect(radio("Gitea")!.checked).toBe(true);
  expect(change()).toBeDefined();
  expect(document.activeElement).toBe(field("tf-add-address"));
});

test("an arrow picks and keeps the list open until Tab, which goes on to the address", async () => {
  await open();
  const list = target.querySelector("[role='radiogroup']")!;
  key(list, "ArrowDown");
  radio("Jira")!.click();
  flushSync();
  await settle();
  expect(radio("Jira")!.checked).toBe(true);
  expect(kinds()).toHaveLength(2);

  const tab = key(radio("Jira")!, "Tab");
  await settle();
  expect(tab.defaultPrevented).toBe(true);
  expect(kinds()).toHaveLength(1);
  expect(document.activeElement).toBe(field("tf-add-address"));
});

test("Change opens the list on the chosen row, a press on that row folds it, and what is typed stays", async () => {
  await open();
  await typeAll();

  change()!.click();
  flushSync();
  await settle();
  expect(kinds()).toHaveLength(2);
  expect(document.activeElement).toBe(radio("Gitea"));
  expect(field("tf-add-secret")!.value).toBe(TOKEN);

  radio("Gitea")!.click();
  flushSync();
  await settle();
  expect(kinds()).toHaveLength(1);
  expect(document.activeElement).toBe(field("tf-add-address"));
  expect(field("tf-add-secret")!.value).toBe(TOKEN);
  expect(field("tf-add-account-pass")!.value).toBe(PASSWORD);
});

test("Change holds no secret: not in the draft, and none of it left after Esc and a reopen", async () => {
  await open();
  await typeAll();
  change()!.click();
  flushSync();
  await settle();

  expect(JSON.stringify(newSourceDraft.held())).not.toContain(TOKEN);
  expect(Object.keys(newSourceDraft.held()!).sort()).toEqual(["adapter", "address", "name"]);

  // `Esc` with the list open folds it first, on the button that opened it, and the dialog stays.
  const one = key(radio("Gitea")!, "Escape");
  await settle();
  expect(one.defaultPrevented).toBe(true);
  expect(closed).not.toHaveBeenCalled();
  expect(kinds()).toHaveLength(1);
  expect(document.activeElement).toBe(change());

  esc();
  expect(closed).toHaveBeenCalledOnce();
  shut();
  expectNoSecret();

  await open();
  expect(kinds()).toHaveLength(1);
  expect(field("tf-add-secret")!.value).toBe("");
  expect(field("tf-add-account-pass")!.value).toBe("");
  expectNoSecret();
});

test("another kind picked after Change takes the token, the account and its password with it", async () => {
  await open();
  await typeAll();
  change()!.click();
  flushSync();
  await settle();

  await pick("Jira");
  expect(kinds()).toHaveLength(1);
  expect(radio("Jira")!.checked).toBe(true);
  expect(field("tf-add-secret")!.value).toBe("");
  expect(field("tf-add-account-user")!.value).toBe("");
  expect(field("tf-add-account-pass")!.value).toBe("");
  expect(target.innerHTML).not.toContain(TOKEN);
  expect(JSON.stringify(newSourceDraft.held())).not.toContain(TOKEN);
});

test("a restored draft arrives folded, with Change, an empty token, and the list a press away", async () => {
  await open();
  await typeAll();
  esc();
  shut();
  expectNoSecret();

  await open();
  expect(kinds()).toHaveLength(1);
  expect(radio("Gitea")!.checked).toBe(true);
  expect(change()).toBeDefined();
  expect(document.activeElement).toBe(field("tf-add-address"));
  expect(field("tf-add-secret")!.value).toBe("");

  change()!.click();
  flushSync();
  await settle();
  expect(kinds()).toHaveLength(2);
  expect(field("tf-add-secret")!.value).toBe("");
  expectNoSecret();
});

test("one adapter has nothing to change to", async () => {
  app = mount(TfAddSource, {
    target,
    props: {
      ports: { listAdapters: () => Promise.resolve([adapter("gitea")]), testSource, addSource, previewInstallAdapter: vi.fn(), installAdapter: vi.fn() } as never,
      layers,
      onclose: closed,
      onsaved: added,
    },
  });
  await settle();
  expect(kinds()).toHaveLength(1);
  expect(change()).toBeUndefined();
  expect(document.activeElement).toBe(field("tf-add-address"));
});
