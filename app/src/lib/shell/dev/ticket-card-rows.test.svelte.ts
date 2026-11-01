/**
 * The ticket page card's two lists (spec #2396, #2401) through the app's own
 * mount over the whole fake IPC: the sections table, one row a section with
 * its count and its worst state, each row opening that section's view
 * narrowed to the ticket without choosing it; the Sub-tasks row opening
 * Tickets on `under:<key>`; *Nothing connected yet.* for a ticket that
 * reaches nothing; and the two action rows, *Open in <source>* and *Link
 * something to it*. Refresh has no button; ⌘R still reads the ticket again.
 * Under `dev/` because only the harness may import the harness.
 */
import { mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import App from "../../../App.svelte";
import { chosenJob } from "../../tf/job/chosen.svelte";
import { forgetFilters } from "../../tf/filter/filter-state.svelte";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";
import { until } from "../test-wait";

const opened = vi.hoisted(() => [] as string[]);
vi.mock("../open-external", () => ({
  openExternal: (url: string) => {
    opened.push(url);
    return Promise.resolve();
  },
}));

/** Every command the app sent, in order, with its arguments. */
let sent: { cmd: string; args: Record<string, unknown> }[] = [];

function recorded(handlers: Record<string, Handler>): Record<string, Handler> {
  return Object.fromEntries(
    Object.entries(handlers).map(([cmd, handler]) => [
      cmd,
      (args: Record<string, unknown>) => {
        sent.push({ cmd, args });
        return handler(args);
      },
    ]),
  );
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** A storage for the chosen job, which this runner's jsdom lacks (`test-no-storage.ts`). */
function memoryStorage(): Storage {
  const held = new Map<string, string>();
  return {
    get length() {
      return held.size;
    },
    clear: () => held.clear(),
    getItem: (key) => held.get(key) ?? null,
    key: (at) => [...held.keys()][at] ?? null,
    removeItem: (key) => void held.delete(key),
    setItem: (key, value) => void held.set(key, value),
  };
}

beforeEach(() => {
  Object.defineProperty(window, "localStorage", { value: memoryStorage(), configurable: true, writable: true });
  sent = [];
  opened.length = 0;
  installFakeTauri(recorded(demoHandlers()));
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
  chosenJob.reset();
  forgetFilters();
  Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
});

async function openOn(hash: string, drawn: string): Promise<void> {
  location.hash = hash;
  app = mount(App, { target, props: {} });
  await until(() => target.textContent?.includes(drawn) ?? false, `${hash} never drew ${drawn}`);
}

/** What the shown page draws: none of what keep-alive holds hidden (#2247). */
function shown(selector: string): HTMLElement | null {
  return target.querySelector<HTMLElement>(`.tf-kept:not([hidden]) ${selector}`);
}

async function pageOf(key: string): Promise<HTMLElement> {
  const card = () => shown("article.tf-ticketcard");
  await until(() => card()?.querySelector(".tf-jobkey")?.textContent === key, `the page never drew ${key}`);
  return card()!;
}

const squash = (el: Element) => el.textContent?.replace(/\s+/g, " ").trim() ?? "";

/** The sections table's rows: the name, the count and the state words, as drawn. */
function sections(card: HTMLElement): [string, string, string][] {
  return [...card.querySelectorAll(".tf-tksecs a")].map((row) => [
    squash(row.querySelector(".tf-tkname")!),
    squash(row.querySelector(".tf-n")!),
    squash(row.querySelector(".tf-st") ?? document.createElement("span")),
  ]);
}

/** A sections row by its name. */
function sectionRow(card: HTMLElement, name: string): HTMLAnchorElement {
  const row = [...card.querySelectorAll<HTMLAnchorElement>(".tf-tksecs a")].find((a) => squash(a.querySelector(".tf-tkname")!) === name);
  if (!row) throw new Error(`no ${name} row`);
  return row;
}

/** The card's action rows by their words. */
function actionRow(card: HTMLElement, words: string): HTMLElement {
  const row = [...card.querySelectorAll<HTMLElement>(".tf-tkacts > *")].find((el) => squash(el).replace(/\s*›$/, "") === words);
  if (!row) throw new Error(`no "${words}" row`);
  return row;
}

/** Waits until every section has drawn its row, so counts are the whole answer. */
async function allSections(card: HTMLElement, names: string[]): Promise<void> {
  await until(() => sections(card).map(([name]) => name).join() === names.join(), `the sections never read ${names.join(", ")}`);
}

test("PAY-231's sections table: Sub-tasks first, then each section with its count and its worst state", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  const rows = sections(card);
  expect(rows[0]).toEqual(["Sub-tasks", "2", "2 open"]);
  expect(rows.find(([name]) => name === "Builds")?.[2]).toBe("1 failing");
  expect(rows.find(([name]) => name === "Monitors")?.[2]).toBe("1 down");
  expect(rows.map(([name, n]) => `${name} ${n}`)).toEqual(["Sub-tasks 2", "Code 5", "Builds 7", "Docs 1", "Monitors 3", "Assets 5"]);
  // A state that asks something of you is in full ink; the open sub-tasks are not.
  expect(sectionRow(card, "Builds").querySelector(".tf-st")?.classList.contains("tf-asks")).toBe(true);
  expect(sectionRow(card, "Sub-tasks").querySelector(".tf-st")?.classList.contains("tf-asks")).toBe(false);
  expect(sectionRow(card, "Builds").querySelector(".tf-st .tf-dot.tf-err")).not.toBeNull();
  expect(chosenJob.id, "reading chose nothing").toBeNull();
});

test("pressing Builds on a ticket only read lands on Builds with This job pressed, narrowed to that ticket, and chooses nothing", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  sectionRow(card, "Builds").click();
  await until(() => location.hash === "#/builds", "Builds never opened");
  const thisJob = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-kept:not([hidden]) button")].find((b) => squash(b).startsWith("This job"));
  await until(() => thisJob() !== undefined, "Builds never offered This job");
  expect(thisJob()!.getAttribute("aria-pressed")).toBe("true");
  expect(squash(thisJob()!)).toBe("This job PAY-231");
  expect(chosenJob.id).toBeNull();
});

test("leaving Builds forgets the row's ticket: back from the nav, with no job chosen, Builds offers no This job", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  sectionRow(card, "Builds").click();
  const thisJob = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-kept:not([hidden]) button")].find((b) => squash(b).startsWith("This job"));
  await until(() => location.hash === "#/builds" && thisJob() !== undefined, "Builds never offered This job");
  location.hash = "#/inbox";
  await until(() => target.querySelector(".tf-nav a[aria-current='page']")?.textContent?.includes("Inbox") === true, "Inbox never opened");
  location.hash = "#/builds";
  await until(() => target.querySelector(".tf-nav a[aria-current='page']")?.textContent?.includes("Builds") === true, "Builds never opened again");
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(thisJob()).toBeUndefined();
});

test("pressing Monitors on a ticket only read opens Monitors on This job for that ticket", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  sectionRow(card, "Monitors").click();
  await until(() => location.hash === "#/monitors", "Monitors never opened");
  const thisJob = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-kept:not([hidden]) button")].find((b) => squash(b).startsWith("This job"));
  await until(() => thisJob()?.getAttribute("aria-pressed") === "true", "Monitors never pressed This job");
  expect(squash(thisJob()!)).toContain("PAY-231");
});

test("Assets opens the ticket's own Dependencies opening", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  expect(sectionRow(card, "Assets").getAttribute("href")).toBe("#/deps/job/PAY-231");
});

test("pressing Sub-tasks lands on Tickets with under:PAY-231 in the search line, and the board holds its sub-tasks", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  await allSections(card, ["Sub-tasks", "Code", "Builds", "Docs", "Monitors", "Assets"]);
  sectionRow(card, "Sub-tasks").click();
  await until(() => location.hash === "#/tickets", "Tickets never opened");
  const line = () => shown("input[aria-label='Filter by key, summary, epic or person']") as HTMLInputElement | null;
  await until(() => line()?.value === "under:PAY-231", "the search line never read under:PAY-231");
  const cards = () => [...target.querySelectorAll(".tf-kept:not([hidden]) .tf-tcard[data-entity]")].map((c) => c.getAttribute("data-entity")).sort();
  await until(() => cards().join() === "mock:PAY-232,mock:PAY-233", `the board drew ${cards().join()}`);
});

test("OPS-77 reaches nothing and has no sub-tasks: Nothing connected yet.", async () => {
  await openOn("#/ticket/mock%3AOPS-77", "Rotate the staging database credentials");
  const card = await pageOf("OPS-77");
  await until(() => card.textContent?.includes("Nothing connected yet.") ?? false, "OPS-77 never said Nothing connected yet.");
  expect(card.querySelector(".tf-tksecs")).toBeNull();
});

test("Open in <source> hands the ticket's web address to the external opener", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  actionRow(card, "Open in Tidewater (mock)").click();
  await until(() => opened.length > 0, "nothing was opened");
  expect(opened).toEqual(["http://127.0.0.1:5399/browse/PAY-231"]);
  expect(location.hash).toBe("#/ticket/mock%3APAY-231");
});

test("Link something to it opens the Connections drawer, on a ticket only read too", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const card = await pageOf("PAY-231");
  expect(target.querySelector("aside.tf-drawer")).toBeNull();
  actionRow(card, "Link something to it").click();
  await until(() => target.querySelector("aside.tf-drawer[aria-label='Connections']") !== null, "the drawer never opened");
  expect(chosenJob.id).toBeNull();
});

test("the page has no Refresh button, and ⌘R still reads the ticket again", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  expect(shown(".tf-refresh")).toBeNull();
  expect([...target.querySelectorAll(".tf-kept:not([hidden]) button")].some((b) => /^(Refresh|Sync now)$/.test(squash(b)))).toBe(false);
  const before = sent.filter((call) => call.cmd === "refresh_entity" || call.cmd === "sync_source").length;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "r", metaKey: true, bubbles: true, cancelable: true }));
  await until(
    () => sent.filter((call) => call.cmd === "refresh_entity" || call.cmd === "sync_source").length > before,
    `⌘R sent nothing: ${sent.map((call) => call.cmd).slice(-5).join(", ")}`,
  );
});
