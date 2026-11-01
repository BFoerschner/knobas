/**
 * One page for a ticket and the job (spec #2396, #2397) through the app's own
 * mount over the whole fake IPC (`?fake-ipc`'s handlers): `#/ticket/<entity
 * id>` draws the page in its reading mood, on a sheet, and chooses nothing;
 * `#/job/<entity id>` chooses the ticket and lands on its page, on the
 * yellow; a bare `#/job` draws the chosen job's page, or the empty state. The
 * card's foot holds the one decision, and the nav has no Job entry. It lives
 * under `dev/` because only the harness may import the harness
 * (`house-rules.test.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import { chosenJob } from "../../tf/job/chosen.svelte";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";
import { until } from "../test-wait";

/** Every command the app sent, in order, with its arguments. */
let sent: { cmd: string; args: Record<string, unknown> }[] = [];

/** The fixture's handlers, each call written down first. */
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

/** The fixture's search answers the estate's lists alone (#430); `PAY-240` is answered here with its ticket, as the mirror would. */
function withTicketSearch(handlers: Record<string, Handler>): Record<string, Handler> {
  const estate = handlers["search"]!;
  return {
    ...handlers,
    search: (args) => {
      const raw = ((args["query"] ?? {}) as { raw?: string }).raw ?? "";
      if (raw.trim() !== "PAY-240") return estate(args);
      const hit = {
        entity_id: "mock:PAY-240",
        kind: "ticket",
        source_id: "mock",
        updated_at: null,
        synced_at: "2026-08-22T11:48:00Z",
        title: "Payout dashboard latency",
        path: null,
        rank: 1,
        snippet: [],
      };
      const groups = [{ kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: 1, hits: [hit] }];
      const interpreted = { prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] };
      return { interpreted, groups, total: 1, took_ms: 1, coverage: [] };
    },
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** A storage for the chosen job, which this runner's jsdom lacks (`test-no-storage.ts`), as a restart reads it. */
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
  installFakeTauri(recorded(withTicketSearch(demoHandlers())));
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
  chosenJob.reset();
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

/** The page's card, once it has read its ticket. */
function pageCard(): HTMLElement | null {
  return shown("article.tf-ticketcard");
}

async function pageOf(key: string): Promise<HTMLElement> {
  await until(() => pageCard()?.querySelector(".tf-jobkey")?.textContent === key, `the page never drew ${key}`);
  return pageCard()!;
}

/** The card's mood: the yellow once the ticket is the chosen job, the sheet while it is read. */
function mood(card: HTMLElement): "pinned" | "reading" | "both" | "neither" {
  const yellow = card.classList.contains("tf-card");
  const sheet = card.classList.contains("tf-sheetcard");
  return yellow && sheet ? "both" : yellow ? "pinned" : sheet ? "reading" : "neither";
}

/** The foot's one button, or `null` where the card offers none. */
function foot(card: HTMLElement): HTMLButtonElement | null {
  return card.querySelector<HTMLButtonElement>(".tf-go .tf-btn");
}

function navNames(): string[] {
  return [...target.querySelectorAll(".tf-nav a")].map((link) => link.firstChild?.textContent?.trim() ?? "");
}

function lit(): string | null {
  return target.querySelector(".tf-nav a[aria-current='page']")?.firstChild?.textContent?.trim() ?? null;
}

function card(entityId: string): HTMLElement | null {
  return shown(`.tf-tcard[data-entity="${entityId}"]`);
}

function selected(): string | null {
  return shown(".tf-tcard.tf-card")?.dataset.entity ?? null;
}

function key(name: string, options: KeyboardEventInit = {}): void {
  (document.activeElement ?? window).dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...options }));
  flushSync();
}

function crumbs(): (string | undefined)[] {
  return [...(shown(".tf-crumbs")?.querySelectorAll("a, strong") ?? [])].map((part) => part.textContent?.trim());
}

function hint(): string | null {
  return shown(".tf-esc")?.textContent?.replace(/\s+/g, " ").trim() ?? null;
}

test("#/ticket/<id> draws the page on a sheet and leaves the chosen job as it was", async () => {
  chosenJob.choose("mock:PAY-240");
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");

  expect(mood(page)).toBe("reading");
  expect(page.getAttribute("aria-label")).toBe("Ticket");
  expect(location.hash, "no redirect").toBe("#/ticket/mock%3APAY-231");
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(chosenJob.id, "reading chose nothing").toBe("mock:PAY-240");
});

test("#/job/<id> chooses the ticket and lands on #/ticket/<id>, on the yellow", async () => {
  await openOn("#/job/mock%3APAY-231", "Retry failed SEPA payouts");
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the job address stayed at ${location.hash}`);
  const page = await pageOf("PAY-231");

  expect(chosenJob.id).toBe("mock:PAY-231");
  expect(mood(page)).toBe("pinned");
  expect(page.getAttribute("aria-label")).toBe("Job card");
});

test("#/job/<tab> keeps the tab on the ticket's address", async () => {
  await openOn("#/job/mock%3APAY-231/conversation", "Retry failed SEPA payouts");
  await until(() => location.hash === "#/ticket/mock%3APAY-231/conversation", `the job address went to ${location.hash}`);
  expect(chosenJob.id).toBe("mock:PAY-231");
});

test("a bare #/job draws the chosen job's page, and says how to choose one when none is", async () => {
  await openOn("#/job", "No job is chosen yet");
  expect(pageCard()).toBeNull();
  unmount(app!);
  app = undefined;

  chosenJob.choose("mock:PAY-231");
  await openOn("#/job", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");
  expect(mood(page)).toBe("pinned");
  expect(location.hash, "home keeps its address").toBe("#/job");
});

test("the nav has no Job entry, and Tickets is current on a ticket page in both moods", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  expect(navNames()).not.toContain("Job");
  expect(navNames()).toContain("Tickets");
  expect(lit()).toBe("Tickets");

  location.hash = "#/job/mock%3APAY-240";
  await until(() => location.hash === "#/ticket/mock%3APAY-240", `the job address stayed at ${location.hash}`);
  const pinned = await pageOf("PAY-240");
  expect(mood(pinned)).toBe("pinned");
  expect(lit()).toBe("Tickets");

  location.hash = "#/job";
  await until(() => pageCard()?.querySelector(".tf-jobkey")?.textContent === "PAY-240", "home never drew the chosen job");
  expect(lit()).toBe("Tickets");
});

test("the breadcrumb is Tickets › KEY; Esc on #/job does nothing and shows no hint", async () => {
  chosenJob.choose("mock:PAY-231");
  await openOn("#/job", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  expect(crumbs()).toEqual(["Tickets", "PAY-231"]);
  expect(hint()).toBeNull();

  key("Escape");
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(location.hash).toBe("#/job");
});

test("Esc on a ticket page Tickets opened returns there with that ticket selected", async () => {
  await openOn("#/tickets/payout-reliability", "Payout reliability");
  await until(() => card("mock:PAY-231") !== null, "the board never drew PAY-231");
  card("mock:PAY-231")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the card opened ${location.hash}`);
  await pageOf("PAY-231");
  expect(crumbs()).toEqual(["Tickets", "PAY-231"]);
  expect(hint()).toBe("Back to Tickets Esc");

  key("Escape");
  await until(() => location.hash === "#/tickets/payout-reliability", `Esc went to ${location.hash}`);
  await until(() => selected() === "mock:PAY-231", `the board selected ${selected()}`);
});

test.each([
  ["mock:PAY-240", "PAY-240", "Start work"],
  ["mock:PAY-231", "PAY-231", "Resume work"],
  ["mock:PAY-236", "PAY-236", "Work on this"],
])("reading %s, the foot offers %s's decision: %s, with no line under it", async (id, ticket, label) => {
  await openOn("#/tickets", "All tickets");
  location.hash = `#/ticket/${encodeURIComponent(id)}`;
  const page = await pageOf(ticket);
  await until(() => foot(page)?.textContent?.trim() === label, `the foot read ${foot(page)?.textContent}`);
  expect(page.querySelectorAll(".tf-go .tf-btn")).toHaveLength(1);
  expect(page.querySelector(".tf-go small")).toBeNull();
});

test("a done ticket's card has no button", async () => {
  await openOn("#/ticket/mock%3APAY-219", "Rotate PSP credentials");
  const page = await pageOf("PAY-219");
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(foot(page)).toBeNull();
});

test("Work on this chooses the ticket and starts nothing", async () => {
  await openOn("#/ticket/mock%3APAY-236", "Payout CSV export for finance");
  const page = await pageOf("PAY-236");
  await until(() => foot(page)?.textContent?.trim() === "Work on this", "Work on this never offered");
  foot(page)!.click();
  await until(() => chosenJob.id === "mock:PAY-236", `the chosen job is ${chosenJob.id}`);
  await until(() => mood(pageCard()!) === "pinned", "the card never turned yellow");
  expect(location.hash).toBe("#/ticket/mock%3APAY-236");
  expect(sent.map((call) => call.cmd)).not.toContain("start_timer");
  expect(shown(".tf-steps")).toBeNull();
  await until(() => foot(pageCard()!)?.textContent?.trim() === "Start timer", `the pinned foot read ${foot(pageCard()!)?.textContent}`);
});

test("Resume work chooses the ticket and starts the timer on it; the foot then stops and logs", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");
  await until(() => foot(page)?.textContent?.trim() === "Resume work", "Resume work never offered");
  foot(page)!.click();
  await until(() => chosenJob.id === "mock:PAY-231", `the chosen job is ${chosenJob.id}`);
  await until(() => sent.some((call) => call.cmd === "start_timer"), "the timer never started");
  const started = sent.find((call) => call.cmd === "start_timer")!.args["target"] as { entity_id?: string };
  expect(started.entity_id).toBe("mock:PAY-231");
  await until(() => foot(pageCard()!)?.textContent?.trim() === "Stop and log", `the pinned foot read ${foot(pageCard()!)?.textContent}`);
  expect(pageCard()!.querySelector(".tf-clock"), "the running time is the chip's alone").toBeNull();
});

test("Start work chooses the ticket and runs the stepper", async () => {
  await openOn("#/ticket/mock%3APAY-240", "Payout dashboard latency");
  const page = await pageOf("PAY-240");
  expect(shown(".tf-steps"), "reading runs no stepper").toBeNull();
  await until(() => foot(page)?.textContent?.trim() === "Start work", "Start work never offered");
  foot(page)!.click();
  await until(() => chosenJob.id === "mock:PAY-240", `the chosen job is ${chosenJob.id}`);
  await until(() => shown(".tf-steps") !== null, "the stepper never ran");
  expect(mood(pageCard()!)).toBe("pinned");
});

test("the card keeps no status select, no clock and no hole in either mood", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const reading = await pageOf("PAY-231");
  expect(reading.querySelector("select")).toBeNull();
  expect(reading.querySelector(".tf-clock")).toBeNull();
  location.hash = "#/job/mock%3APAY-231";
  await until(() => mood(pageCard()!) === "pinned", "the card never turned yellow");
  expect(pageCard()!.querySelector("select")).toBeNull();
  expect(pageCard()!.querySelector(".tf-clock")).toBeNull();
});

test("the ticket page is the foreground a capture names, not the chosen job", async () => {
  await openOn("#/job/mock%3APAY-231", "Retry failed SEPA payouts");
  location.hash = "#/ticket/mock%3APAY-240";
  await pageOf("PAY-240");
  await until(
    () => sent.filter((call) => call.cmd === "record_capture_context").at(-1)?.args["foreground"] === "mock:PAY-240",
    "the ticket page was never the foreground",
  );
});

/** The launcher's dialog, once it is up. */
function launcher(): HTMLElement | null {
  return target.querySelector<HTMLElement>('.tf [role="dialog"].tf-launch');
}

async function search(text: string): Promise<void> {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await until(() => launcher() !== null, "⌘K never opened the launcher");
  const box = launcher()!.querySelector("input")!;
  box.value = text;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await until(() => (launcher()?.querySelectorAll(".tf-ln-res").length ?? 0) > 0, `no result for ${text}`);
}

function inLauncher(name: string): void {
  launcher()!
    .querySelector("input")!
    .dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  flushSync();
}

test("on a ticket page, Link to targets that ticket, not the chosen job", async () => {
  await openOn("#/job/mock%3APAY-231", "Retry failed SEPA payouts");
  location.hash = "#/ticket/mock%3APAY-240";
  await pageOf("PAY-240");
  await search("list:not-monitored");
  inLauncher("Tab");
  const rows = [...target.querySelectorAll('[aria-label="Actions on the selected result"] .tf-ln-chain-a')].map((row) => row.textContent?.trim());
  expect(rows[0]).toBe("Link to PAY-240");
});

test("a ticket result in ⌘K opens the ticket page, and Esc there goes to Tickets", async () => {
  await openOn("#/builds", "Builds");
  await search("PAY-240");
  inLauncher("Enter");
  await until(() => location.hash === "#/ticket/mock%3APAY-240", `the result opened ${location.hash}`);
  await pageOf("PAY-240");
  await until(() => launcher() === null, "the launcher stayed open");
  key("Escape");
  await until(() => location.hash === "#/tickets", `Esc went to ${location.hash}`);
});

test("a bare #/ticket lands on Tickets, and an unreadable id says it was not found", async () => {
  await openOn("#/ticket", "All tickets");
  await until(() => shown(".tf-tickets") !== null, "a bare #/ticket never drew Tickets");
  expect(lit()).toBe("Tickets");

  location.hash = "#/ticket/mock%3ANOPE-1";
  await until(() => shown(".tf-empty")?.textContent?.includes("No ticket NOPE-1 in the mirror") ?? false, "an unreadable ticket never said so");
});

/** A failed run of the fixture, and its address (`runHash`). */
const RUN = "teamcity:build:Payout_IntegrationTests-1187";
const RUN_HASH = `#/build/${encodeURIComponent("teamcity:buildType:Payout_IntegrationTests")}/${encodeURIComponent(RUN)}`;

/** The run is linked to PAY-231, as a ticket filed from it is once its source syncs (#1451). */
function withRunTicket(handlers: Record<string, Handler>): Record<string, Handler> {
  const read = handlers["get_entity"]!;
  return {
    ...handlers,
    get_entity: (args) => {
      const detail = read(args) as { links: unknown[] };
      if (args["entityId"] !== RUN) return detail;
      const link = {
        id: "run-ticket",
        from_id: "mock:PAY-231",
        to_id: RUN,
        relation: "related",
        origin: "manual",
        note: null,
        created_by: "mara",
        created_at: "2026-08-22T10:30:00Z",
        confirmed_at: "2026-08-22T10:30:00Z",
        rule: null,
        rule_class: null,
        reason: null,
      };
      const other = { entity_id: "mock:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", deleted_at: null };
      return { ...detail, links: [{ link, other }, ...detail.links] };
    },
  };
}

test("a run's ticket link on Builds opens the ticket page, and Esc there returns to the run", async () => {
  installFakeTauri(recorded(withRunTicket(withTicketSearch(demoHandlers()))));
  await openOn(RUN_HASH, "#1187");
  const link = `a[href="#/ticket/${encodeURIComponent("mock:PAY-231")}"]`;
  await until(() => shown(link) !== null, "the run never drew its ticket");
  shown(link)!.click();
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the run's ticket opened ${location.hash}`);
  await pageOf("PAY-231");
  expect(lit()).toBe("Tickets");
  expect(hint()).toBe("Back to Run #1187 Esc");

  key("Escape");
  await until(() => location.hash === RUN_HASH, `Esc went to ${location.hash}`);
});

test("a run opened from a doc page names the doc page in its hint, as Esc goes there (spec #2227, story 35)", async () => {
  await openOn("#/page/confluence%3Asepa", "SEPA payout retry design");
  await until(() => shown(".tf-rd-title")?.textContent?.includes("SEPA payout retry design") ?? false, "the doc page never drew");
  location.hash = RUN_HASH;
  await until(() => shown(".tf-crumbs strong")?.textContent === "#1187", "the run never drew");
  expect(hint()).toBe("Back to SEPA payout retry design Esc");

  key("Escape");
  await until(() => location.hash === "#/page/confluence%3Asepa", `Esc went to ${location.hash}`);
});
