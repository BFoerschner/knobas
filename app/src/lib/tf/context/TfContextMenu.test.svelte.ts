/**
 * The right-click menu (2026-11-16): it opens at once on the thing under the pointer or under focus, the web view's own
 * menu stays only in a field, its keys and its closing, and what each pick sends, through fake ports.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { takeAsked } from "../repo/text-search.svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutSettings, CheckoutView, EntityDetail } from "../../ipc/entity";
import { acks } from "../../shell/ack.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import { createApps, type Apps } from "../openin/apps.svelte";
import TfContextMenu from "./TfContextMenu.svelte";
import type { MenuPorts } from "./ports";
import { menuThing } from "./thing";
import { runsRead } from "../builds/runs-read.svelte";

const REPO = "gitea:tidewater/payout-service";

const SETTINGS: CheckoutSettings = {
  clones_root: "/Users/mara/code",
  git: { status: "found", path: "/usr/bin/git" },
  default_application: "vscode",
  applications: [
    { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}" },
    { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
  ],
};

const checkoutAt = (path: string | null): CheckoutView => ({
  repo_entity_id: REPO,
  repo_url: "https://git.example/tidewater/payout-service",
  path,
  found_by: path === null ? "nothing" : "scan",
  clones_root: "/Users/mara/code",
  clone_command: null,
  tree: null,
});

const head = (id: string, title: string, web: string | null): EntityDetail =>
  ({ row: { entity_id: id, kind: "ticket", source_id: "jira", title, updated_at: null, synced_at: "", path: null, state: null }, web_url: web, default_branch: "main" }) as unknown as EntityDetail;

let calls: Record<string, unknown[][]>;
let ports: MenuPorts;
let apps: Apps;
let cloned: string | null;
let searched: string[];
let timed: [string, string][];
let app: Record<string, unknown> | null = null;
let page: HTMLElement;
const layers = createLayers();

function record(name: string, answer: (...args: never[]) => unknown = () => undefined) {
  return (...args: unknown[]) => {
    (calls[name] ??= []).push(args);
    return Promise.resolve((answer as (...args: unknown[]) => unknown)(...args));
  };
}

beforeEach(() => {
  calls = {};
  cloned = "/Users/mara/code/payout-service";
  searched = [];
  timed = [];
  toasts.items = [];
  ports = {
    head: record("head", (id: string) => head(id, "Payout CSV export", `https://jira.example/browse/${id.slice(5)}`)) as MenuPorts["head"],
    checkout: record("checkout", () => checkoutAt(cloned)) as MenuPorts["checkout"],
    objectPage: record("objectPage", () => "https://git.example/tidewater/payout-service/src/main/src/sepa") as MenuPorts["objectPage"],
    sources: record("sources", () => [{ id: "jira", display_name: "Jira", base_url: "https://jira.example" }]) as MenuPorts["sources"],
    resolveUrl: record("resolveUrl", () => null) as MenuPorts["resolveUrl"],
    copyText: record("copyText") as MenuPorts["copyText"],
    copyLater: (text) => text.then((value) => void (calls["copyLater"] ??= []).push([value])),
    copySelection: () => ((calls["copySelection"] ??= []).push([]), true),
    openExternal: record("openExternal") as MenuPorts["openExternal"],
    openWeb: record("openWeb") as MenuPorts["openWeb"],
    submitWrite: record("submitWrite", () => ({ id: 1 })) as MenuPorts["submitWrite"],
    ackAlert: record("ackAlert") as MenuPorts["ackAlert"],
    unackAlert: record("unackAlert") as MenuPorts["unackAlert"],
    createLink: record("createLink") as MenuPorts["createLink"],
    inboxAck: record("inboxAck") as MenuPorts["inboxAck"],
    inboxDone: record("inboxDone") as MenuPorts["inboxDone"],
    clone: record("clone") as MenuPorts["clone"],
    go: (hash) => void (calls["go"] ??= []).push([hash]),
  };
  apps = createApps({
    checkoutSettings: () => Promise.resolve(SETTINGS),
    setCheckoutApplications: () => Promise.resolve(SETTINGS),
    openCheckout: (entityId, at) => ((calls["openCheckout"] ??= []).push([entityId, at]), Promise.resolve(["code", "/x"])),
    openFolder: (entityId, application, folder) => ((calls["openFolder"] ??= []).push([entityId, application, folder]), Promise.resolve(["open", "-a", "Terminal", "/x"])),
  });
  location.hash = "#/inbox";
});

afterEach(() => {
  if (app) void unmount(app);
  app = null;
  page?.remove();
  layers.reset();
  getSelection()?.removeAllRanges();
});

async function draw(html: string, props: Record<string, unknown> = {}) {
  page = document.createElement("main");
  page.className = "tf-main";
  page.innerHTML = html;
  document.body.append(page);
  app = mount(TfContextMenu, {
    target: document.body,
    props: { onsearch: (text: string) => searched.push(text), onstarttimer: (id: string, title: string) => timed.push([id, title]), canTime: true, apps, ports, layers, dev: false, ...props },
  });
  await apps.ensure();
  await settle();
}

async function settle() {
  flushSync();
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
}

const menus = () => [...document.querySelectorAll<HTMLElement>("[role=menu]")];
const menu = () => menus()[0] ?? null;
const rows = (at: HTMLElement | null = menu()) => [...(at?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])].map((row) => row.dataset["item"]);
const row = (id: string, at: HTMLElement | null = menu()) => at!.querySelector<HTMLButtonElement>(`[data-item="${id}"]`)!;

function rightClick(node: Element, x = 120, y = 80, init: MouseEventInit = {}) {
  node.dispatchEvent(new PointerEvent("pointerdown", { button: 2, bubbles: true, cancelable: true }));
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 2, ...init });
  node.dispatchEvent(event);
  flushSync();
  return event;
}

function key(name: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...init });
  (document.activeElement ?? document.body).dispatchEvent(event);
  flushSync();
  return event;
}

const TICKET_ROW = `<table><tr><td><a class="t" href="#/ticket/jira%3APAY-231">Payout CSV export</a></td><td><button class="act">Ack</button></td></tr></table>`;

test("a right-click on a ticket opens its menu at the pointer in the same frame, and stops the web view's", async () => {
  await draw(TICKET_ROW);
  const event = rightClick(page.querySelector(".t")!, 140, 90);
  expect(event.defaultPrevented).toBe(true);
  expect(menu()).not.toBeNull();
  expect(rows()).toEqual(["open", "work", "timer", "web", "copy-key", "copy-key-title", "copy-link"]);
  expect(menu()!.style.left).toBe("140px");
  expect(menu()!.style.top).toBe("90px");
  // Nothing was waited for: the head read started as it opened.
  expect(calls["head"]).toEqual([["jira:PAY-231"]]);
  await settle();
  expect(row("web").textContent).toBe("Open in Jira");
});

test("a button inside the row gives the row's menu; the row keeps its look while it stands", async () => {
  await draw(TICKET_ROW);
  rightClick(page.querySelector(".act")!);
  expect(rows()[0]).toBe("open");
  expect(page.querySelector("[data-ctx-on]")).toBe(page.querySelector(".t"));
  await settle();
  key("Escape");
  await settle();
  expect(page.querySelector("[data-ctx-on]")).toBeNull();
});

test("a field keeps the web view's own menu; empty space gets none at all", async () => {
  await draw(`<input class="f" aria-label="Find"><div class="empty">Nothing here</div><a class="nav" href="#/code">Code</a>`);
  expect(rightClick(page.querySelector(".f")!).defaultPrevented).toBe(false);
  expect(menu()).toBeNull();
  expect(rightClick(page.querySelector(".empty")!).defaultPrevented).toBe(true);
  expect(menu()).toBeNull();
  expect(rightClick(page.querySelector(".nav")!).defaultPrevented).toBe(true);
  expect(menu()).toBeNull();
});

test("in a development build Shift+right-click is the web view's, for Inspect; in a release it is not", async () => {
  await draw(TICKET_ROW, { dev: true });
  expect(rightClick(page.querySelector(".t")!, 10, 10, { shiftKey: true }).defaultPrevented).toBe(false);
  expect(menu()).toBeNull();
  void unmount(app!);
  app = null;
  page.remove();
  await draw(TICKET_ROW, { dev: false });
  expect(rightClick(page.querySelector(".t")!, 10, 10, { shiftKey: true }).defaultPrevented).toBe(true);
  expect(menu()).not.toBeNull();
});

test("its keys: ↓ walks from the menu, Enter picks, Esc closes and focus goes back to what had it", async () => {
  await draw(`${TICKET_ROW}<button class="before">Before</button>`);
  const before = page.querySelector<HTMLButtonElement>(".before")!;
  before.focus();
  rightClick(page.querySelector(".t")!);
  await settle();
  // A pointer's opening focuses the menu, not a row, so the press's release picks nothing.
  expect(document.activeElement).toBe(menu());
  key("ArrowDown");
  expect(document.activeElement).toBe(row("open"));
  key("ArrowDown");
  expect(document.activeElement).toBe(row("work"));
  key("Escape");
  await settle();
  expect(menu()).toBeNull();
  expect(document.activeElement).toBe(before);
});

test("Shift+F10 and the context menu key open it on the focused thing, under it, its first row focused", async () => {
  await draw(TICKET_ROW);
  const link = page.querySelector<HTMLAnchorElement>(".t")!;
  for (const press of [{ key: "F10", shiftKey: true }, { key: "ContextMenu" }]) {
    link.focus();
    const event = key(press.key, press);
    expect(event.defaultPrevented).toBe(true);
    await settle();
    expect(rows()[0]).toBe("open");
    expect(document.activeElement).toBe(row("open"));
    // The platform's own `contextmenu` for the same key press only stops its menu: the one standing stays, focused.
    expect(rightClick(link).defaultPrevented).toBe(true);
    await settle();
    expect(document.activeElement).toBe(row("open"));
    key("Escape");
    await settle();
    expect(document.activeElement).toBe(link);
    await new Promise((done) => setTimeout(done, 650));
  }
});

test("a key in a field is the field's", async () => {
  await draw(`<input class="f">`);
  page.querySelector<HTMLInputElement>(".f")!.focus();
  expect(key("F10", { shiftKey: true }).defaultPrevented).toBe(false);
  expect(menu()).toBeNull();
});

test("it closes on a press elsewhere, a scroll and the window losing focus", async () => {
  await draw(`${TICKET_ROW}<p class="else">else</p>`);
  const link = page.querySelector(".t")!;
  rightClick(link);
  page.querySelector(".else")!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(menu()).toBeNull();
  rightClick(link);
  page.dispatchEvent(new Event("scroll"));
  flushSync();
  expect(menu()).toBeNull();
  rightClick(link);
  window.dispatchEvent(new Event("blur"));
  flushSync();
  expect(menu()).toBeNull();
  // A press on the row it was opened on is a press elsewhere too, and so is one on what had focus, which focus goes back to.
  rightClick(link);
  link.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(menu()).toBeNull();
  const other = page.querySelector<HTMLElement>(".else")!;
  other.tabIndex = -1;
  other.focus();
  rightClick(link);
  other.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(menu()).toBeNull();
});

test("a right-click inside the menu draws no second menu and none of the web view's", async () => {
  await draw(TICKET_ROW);
  rightClick(page.querySelector(".t")!);
  const event = rightClick(row("open"));
  expect(event.defaultPrevented).toBe(true);
  expect(menus()).toHaveLength(1);
});

test("Copy key writes the clipboard at once; Copy link waits on the read it started, and Open in Jira opens its page where its pages open", async () => {
  await draw(TICKET_ROW);
  rightClick(page.querySelector(".t")!);
  row("copy-key").click();
  await settle();
  expect(calls["copyText"]).toEqual([["PAY-231"]]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Copied PAY-231");
  rightClick(page.querySelector(".t")!);
  row("copy-link").click();
  await settle();
  expect([...(calls["copyText"] ?? []), ...(calls["copyLater"] ?? [])]).toContainEqual(["https://jira.example/browse/PAY-231"]);
  rightClick(page.querySelector(".t")!);
  await settle();
  row("web").click();
  await settle();
  expect(calls["openWeb"]).toEqual([["https://jira.example/browse/PAY-231", { source: "jira" }]]);
});

test("Open goes to the thing; Start timer takes the title read as the menu opened", async () => {
  await draw(TICKET_ROW);
  rightClick(page.querySelector(".t")!);
  row("open").click();
  expect(calls["go"]).toEqual([["#/ticket/jira%3APAY-231"]]);
  rightClick(page.querySelector(".t")!);
  await settle();
  row("timer").click();
  await settle();
  expect(timed).toEqual([["jira:PAY-231", "Payout CSV export"]]);
});

const TREE = `<button class="tf-fdir" data-path="src/sepa">sepa</button><a class="file" href="#/repo/${encodeURIComponent(REPO)}/files/src/sepa/retry.rs">retry.rs</a>`;

test("a folder's Open in…: → opens the applications, each opens the folder as on a repository, ← goes back", async () => {
  location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
  await draw(TREE);
  rightClick(page.querySelector(".tf-fdir")!);
  await settle();
  expect(rows()).toEqual(["open-default", "open-in", "forge", "copy-path", "copy-full", "copy-link"]);
  key("ArrowDown");
  key("ArrowDown");
  expect(document.activeElement).toBe(row("open-in"));
  key("ArrowRight");
  await settle();
  expect(menus()).toHaveLength(2);
  const sub = menus()[1]!;
  expect(rows(sub)).toEqual(["app-vscode", "app-terminal", "add-app", "manage-apps"]);
  expect(document.activeElement).toBe(row("app-vscode", sub));
  key("ArrowLeft");
  await settle();
  expect(menus()).toHaveLength(1);
  expect(document.activeElement).toBe(row("open-in"));
  // Resting the pointer on it opens it too, and the pick runs the folder's: a press in the submenu is no press elsewhere.
  row("open-in").dispatchEvent(new PointerEvent("pointerenter"));
  await settle();
  row("app-terminal", menus()[1]!).dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(menus()).toHaveLength(2);
  row("app-terminal", menus()[1]!).click();
  await settle();
  expect(calls["openFolder"]).toEqual([[REPO, "terminal", "src/sepa"]]);
  expect(menus()).toHaveLength(0);
  expect(acks.item?.text).toBe("Opening sepa in Terminal");
  expect(toasts.items).toEqual([]);
});

test("a file opens with the file command; an application with none opens the file's folder", async () => {
  location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
  await draw(TREE);
  rightClick(page.querySelector(".file")!);
  await settle();
  row("open-default").click();
  await settle();
  expect(calls["openCheckout"]).toEqual([[REPO, { application: "vscode", file: "src/sepa/retry.rs", line: null }]]);
  rightClick(page.querySelector(".file")!);
  await settle();
  row("open-in").click();
  await settle();
  row("app-terminal", menus()[1]!).click();
  await settle();
  expect(calls["openFolder"]).toEqual([[REPO, "terminal", "src/sepa"]]);
});

test("a file's full path and its page on the forge come from the reads started as the menu opened", async () => {
  location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
  await draw(TREE);
  rightClick(page.querySelector(".file")!);
  await settle();
  row("copy-full").click();
  await settle();
  expect(calls["copyText"]).toEqual([["/Users/mara/code/payout-service/src/sepa/retry.rs"]]);
  expect(calls["objectPage"]).toEqual([[REPO, { kind: "file", path: "src/sepa/retry.rs", line: null, at: "main" }]]);
});

test("not cloned: Clone in the default's slot, Open in… greyed and inert", async () => {
  cloned = null;
  location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
  await draw(TREE);
  rightClick(page.querySelector(".tf-fdir")!);
  await settle();
  expect(rows()).toEqual(["clone", "open-in", "forge", "copy-path", "copy-full", "copy-link"]);
  expect(row("open-in").getAttribute("aria-disabled")).toBe("true");
  row("open-in").click();
  await settle();
  expect(menus()).toHaveLength(1);
  row("clone").click();
  await settle();
  expect(calls["clone"]).toEqual([[{ entityId: REPO, title: "tidewater/payout-service" }]]);
});

test("selected words: Copy is the document's own copy, Search opens the launcher with them", async () => {
  await draw(`<p class="words">retry the payout</p>`);
  const words = page.querySelector(".words")!;
  const range = document.createRange();
  range.selectNodeContents(words);
  getSelection()!.removeAllRanges();
  getSelection()!.addRange(range);
  rightClick(words);
  expect(rows()).toEqual(["copy-sel", "search-sel"]);
  row("copy-sel").click();
  expect(calls["copySelection"]).toHaveLength(1);
  rightClick(words);
  row("search-sel").click();
  expect(searched).toEqual(["retry the payout"]);
});

test("Search in repository on a diff's selection hands the words to the repository's Files tab and goes there", async () => {
  location.hash = "#/pr/gitea%3Aacme%2Fpay%2312/files";
  await draw(`<div class="tf-diff" data-search-repo="gitea:acme/pay"><span class="words">retry_on</span></div>`);
  const words = page.querySelector(".words")!;
  const range = document.createRange();
  range.selectNodeContents(words);
  getSelection()!.removeAllRanges();
  getSelection()!.addRange(range);
  rightClick(words);
  expect(rows()).toEqual(["copy-sel", "search-sel", "search-repo"]);
  row("search-repo").click();
  expect(calls["go"]).toEqual([["#/repo/gitea%3Aacme%2Fpay/files"]]);
  expect(takeAsked("gitea:acme/pay")).toBe("retry_on");
});

test("Search in repository on the repository's own file stays on the file", async () => {
  location.hash = "#/repo/gitea%3Aacme%2Fpay/files/src/a.rs";
  await draw(`<div class="tf-fileview"><div class="tf-l" data-n="1"><span class="words">retry_on</span></div></div>`);
  const words = page.querySelector(".words")!;
  const range = document.createRange();
  range.selectNodeContents(words);
  getSelection()!.removeAllRanges();
  getSelection()!.addRange(range);
  rightClick(words);
  row("search-repo").click();
  expect(calls["go"]).toBeUndefined();
  expect(takeAsked("gitea:acme/pay")).toBe("retry_on");
});

test("a tagged row: a monitor with an open alert offers Acknowledge, with Undo on its toast", async () => {
  await draw(`<table><tr class="m"><td>payout-api</td></tr></table>`);
  menuThing(() => ({ kind: "monitor", id: "kuma:7", hash: "#/monitors/at/kuma%3A7", alerting: true }))(page.querySelector(".m")!);
  rightClick(page.querySelector(".m td")!);
  row("ack").click();
  await settle();
  expect(calls["ackAlert"]).toEqual([["kuma:7"]]);
  const toast = toasts.items.find((one) => one.action?.label === "Undo")!;
  toast.action!.run();
  await settle();
  expect(calls["unackAlert"]).toEqual([["kuma:7"]]);
});

test("a monitor row's Acknowledge is the row's own Ack, so its Acked and its toast follow", async () => {
  await draw(`<table><tr class="m"><td>payout-api</td></tr></table>`);
  const pressed: string[] = [];
  menuThing(() => ({ kind: "monitor", id: "kuma:7", hash: "#/monitors/at/kuma%3A7", alerting: true, ack: () => void pressed.push("kuma:7") }))(page.querySelector(".m")!);
  rightClick(page.querySelector(".m td")!);
  row("ack").click();
  await settle();
  expect(pressed).toEqual(["kuma:7"]);
  expect(calls["ackAlert"]).toBeUndefined();
});

test("a web link on a mirrored source opens in knobas when it resolves, else says so and offers the browser", async () => {
  await draw(`<a class="w" href="https://jira.example/browse/PAY-1">PAY-1</a>`);
  await settle();
  rightClick(page.querySelector(".w")!);
  expect(rows()).toEqual(["in-knobas", "browser", "copy-link"]);
  row("in-knobas").click();
  await settle();
  const toast = toasts.items.find((one) => one.action?.label === "Open in browser")!;
  expect(toast.text).toBe("knobas holds nothing at that address yet.");
  ports.resolveUrl = () => Promise.resolve({ entity_id: "jira:PAY-1", kind: "ticket" });
  rightClick(page.querySelector(".w")!);
  row("in-knobas").click();
  await settle();
  expect(calls["go"]).toEqual([["#/ticket/jira%3APAY-1"]]);
});


// The stuck menu (ux27, 2026-11-21; Björn: "When I spam right click … the context menu stays open indefinitely"). Drawn in
// the shell's float host, as in the app: the menu that closed took its *Open in…* with it only from where it was opened, and
// the submenu's node stayed in the host with nothing left to close it.
async function inHost(run: () => Promise<void>) {
  const host = document.createElement("div");
  host.className = "tf-floats";
  document.body.append(host);
  try {
    await run();
    expect(host.querySelectorAll(".tf-menu").length).toBe(menus().length);
  } finally {
    host.remove();
  }
}

async function folderWithOpenIn() {
  location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
  await draw(`${TREE}<p class="else">else</p>`);
  rightClick(page.querySelector(".tf-fdir")!);
  await settle();
  row("open-in").dispatchEvent(new PointerEvent("pointerenter"));
  await settle();
  expect(menus()).toHaveLength(2);
}

test("in the float host, Open in… goes with its menu when the window loses focus", async () => {
  await inHost(async () => {
    await folderWithOpenIn();
    window.dispatchEvent(new Event("blur"));
    await settle();
    await settle();
    expect(menus()).toHaveLength(0);
  });
});

test("in the float host, a right press that went down on the menu and came up on another thing draws that thing's menu alone", async () => {
  await inHost(async () => {
    await folderWithOpenIn();
    // WebView2 sends `contextmenu` as the button comes up: the hand moved off the menu between the two.
    row("open-in").dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 2 }));
    rightClick(page.querySelector(".file")!, 300, 200);
    await settle();
    await settle();
    expect(menus()).toHaveLength(1);
    expect(menu()!.style.left).toBe("300px");
    expect(rows()[0]).toBe("open");
    key("Escape");
    await settle();
    expect(menus()).toHaveLength(0);
  });
});

test("in the float host, Esc walks back a level at a time and leaves nothing", async () => {
  await inHost(async () => {
    await folderWithOpenIn();
    key("Escape");
    await settle();
    expect(menus()).toHaveLength(1);
    key("Escape");
    await settle();
    expect(menus()).toHaveLength(0);
  });
});

/** A small seeded random, so a failing seed fails the same way every run. */
function seeded(seed: number) {
  let at = seed;
  return () => (at = (at * 1103515245 + 12345) % 2 ** 31) / 2 ** 31;
}

test.each(Array.from({ length: 60 }, (_, index) => index + 1))("right-click spam, seed %i: at most a menu and its Open in…, and one press away closes everything", async (seed) => {
  const random = seeded(seed);
  /** None, a microtask, Svelte's tick or a task: the gaps a real press's events arrive with. */
  const gap = async () => {
    const at = random();
    if (at < 0.3) return;
    if (at < 0.55) return void (await Promise.resolve());
    if (at < 0.8) return void (await tick());
    await new Promise((resolve) => setTimeout(resolve, 0));
  };
  const any = <T,>(list: readonly T[]): T => list[Math.floor(random() * list.length)]!;
  const steps: string[] = [];
  await inHost(async () => {
    location.hash = `#/repo/${encodeURIComponent(REPO)}/files`;
    await draw(`${TREE}${TICKET_ROW}<p class="else">else</p>`);
    const things = [page.querySelector(".tf-fdir")!, page.querySelector(".file")!, page.querySelector(".t")!];
    for (let step = 0; step < 40; step++) {
      const open = menus();
      const onMenu = open.length > 0 ? any([...open.flatMap((one) => [...one.querySelectorAll<HTMLElement>("[role=menuitem]")]), open[0]!]) : null;
      const what = any(["press", "press", "press", "drift", "drift", "press-off", "esc", "blur", "hash"] as const);
      steps.push(what);
      const at = { clientX: 20 + Math.floor(random() * 500), clientY: 20 + Math.floor(random() * 400), button: 2, bubbles: true, cancelable: true };
      if (what === "press" || what === "press-off") {
        // A real press's events, in the Mac's order (`contextmenu` on the way down) or Windows' (on the way up). `press-off`:
        // it went down on the menu and came up on a thing, the hand having moved.
        const down = what === "press-off" && onMenu !== null ? onMenu : any(things);
        const up = what === "press-off" ? any(things) : random() < 0.25 && onMenu !== null ? onMenu : down;
        const mac = random() < 0.5;
        down.dispatchEvent(new PointerEvent("pointerdown", at));
        await gap();
        down.dispatchEvent(new MouseEvent("mousedown", at));
        await gap();
        if (mac) up.dispatchEvent(new MouseEvent("contextmenu", at));
        await gap();
        up.dispatchEvent(new MouseEvent("mouseup", at));
        await gap();
        if (!mac) up.dispatchEvent(new MouseEvent("contextmenu", at));
      } else if (what === "drift") {
        // The pointer rests on a row: *Open in… ▸* opens its submenu, any other row closes it.
        if (onMenu !== null) onMenu.dispatchEvent(new PointerEvent("pointerenter"));
      } else if (what === "esc") {
        (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      } else if (what === "blur") {
        window.dispatchEvent(new Event("blur"));
      } else {
        window.dispatchEvent(new HashChangeEvent("hashchange"));
      }
      await gap();
      flushSync();
      const now = menus();
      expect(now.length, steps.join(" ")).toBeLessThanOrEqual(2);
      if (now.length === 2) expect(now.filter((one) => one.querySelector('[data-item="app-vscode"]') !== null), steps.join(" ")).toHaveLength(1);
    }
    await settle();
    const away = page.querySelector(".else")!;
    away.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    away.dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0 }));
    await settle();
    await settle();
    expect(menus(), steps.join(" ")).toHaveLength(0);
    expect(layers.open, steps.join(" ")).toHaveLength(0);
    expect(page.querySelectorAll("[data-ctx-on]"), steps.join(" ")).toHaveLength(0);
  });
});

test("a Builds folder's Mark N runs as read marks exactly those runs, with Undo on its toast (#2527)", async () => {
  const marked: string[][] = [];
  const unmarked: string[][] = [];
  runsRead.ports = {
    read: async () => [],
    mark: async (runs, pipelines) => (marked.push([...runs, ...pipelines]), [...runs]),
    unmark: async (runs) => (unmarked.push([...runs]), [...runs]),
  };
  try {
    await runsRead.load();
    await draw(`<details><summary class="f">Payout</summary></details>`);
    menuThing(() => ({ kind: "pipeline-folder", name: "Payout", unread: ["tc:9", "tc:3"] }))(page.querySelector(".f")!);
    rightClick(page.querySelector(".f")!);
    expect(rows()).toEqual(["read-runs", "copy-name"]);
    expect(row("read-runs").textContent).toContain("Mark 2 runs as read");
    row("read-runs").click();
    await settle();
    expect(marked).toEqual([["tc:9", "tc:3"]]);
    const toast = toasts.items.find((one) => one.action?.label === "Undo")!;
    expect(toast.text).toBe("Marked 2 runs as read");
    toast.action!.run();
    await settle();
    expect(unmarked).toEqual([["tc:9", "tc:3"]]);
  } finally {
    runsRead.forget();
  }
});
