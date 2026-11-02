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
import { afterEach, beforeEach, expect, onTestFinished, test } from "vitest";

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
  return shown('.tf-tcard[aria-current="true"]')?.dataset.entity ?? null;
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

test("#/job/<tab>, an old tab's address, lands on the ticket's own address: the page has no tabs (#2399)", async () => {
  await openOn("#/job/mock%3APAY-231/conversation", "Retry failed SEPA payouts");
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the job address went to ${location.hash}`);
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

// ---- the reading column (#2399) ----

/** The page's reading column, beside the card. */
function reading(): HTMLElement | null {
  return shown(".tf-work .tf-tkread");
}

/** The thread's comments, as the reading column draws them. */
function comments(): HTMLElement[] {
  return [...(reading()?.querySelectorAll<HTMLElement>(".tf-convo .tf-msg:not(.tf-queued)") ?? [])];
}

test("the reading column is the title, the description and the whole thread, with the new-since line before the first new comment", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  await until(() => comments().length > 0, "the thread never drew");

  const column = reading()!;
  expect(column.querySelector("h1")?.textContent?.trim()).toBe("Retry failed SEPA payouts");
  expect(column.querySelector(".tf-tkdesc")?.textContent).toContain("Payouts to two SEPA banks fail with a");
  expect(comments()).toHaveLength(20);
  expect(column.querySelector(".tf-fold"), "no fold").toBeNull();
  expect(comments()[0]!.textContent).toContain("Finance keeps chasing failed SEPA payouts by hand");

  const line = column.querySelector(".tf-newline")!;
  expect(line.textContent).toContain("New since your last visit");
  const firstNew = comments().find((comment) => comment.textContent?.includes("Left a comment on the PR about the jitter width"))!;
  expect(line.nextElementSibling).toBe(firstNew);
  expect(comments().filter((comment) => comment.querySelector(".tf-chip-mention")).length).toBe(2);
  expect(column.querySelector(".tf-compose textarea"), "the compose box ends the thread").not.toBeNull();
});

test("opening the page records a visit", async () => {
  await openOn("#/ticket/mock%3AOPS-77", "Rotate the staging database credentials");
  await pageOf("OPS-77");
  await until(() => sent.some((call) => call.cmd === "visit_entity" && call.args["entityId"] === "mock:OPS-77"), "no visit was recorded");
});

test("a comment on a ticket that is not the chosen job is queued", async () => {
  chosenJob.choose("mock:PAY-231");
  await openOn("#/ticket/mock%3AOPS-77", "Rotate the staging database credentials");
  const page = await pageOf("OPS-77");
  expect(mood(page)).toBe("reading");
  await until(() => reading()?.querySelector(".tf-compose textarea") !== null, "no compose box");

  const box = reading()!.querySelector<HTMLTextAreaElement>(".tf-compose textarea")!;
  box.value = "The rotation is scheduled for Tuesday.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const button = reading()!.querySelector<HTMLButtonElement>(".tf-compose .tf-btn.tf-primary")!;
  await until(() => !button.disabled, "Comment stayed disabled");
  button.click();
  await until(() => sent.some((call) => call.cmd === "submit_write"), "nothing was queued");
  const payload = sent.find((call) => call.cmd === "submit_write")!.args["payload"] as { Comment?: { entity: string; body: string } };
  expect(payload.Comment).toEqual({ entity: "mock:OPS-77", body: "The rotation is scheduled for Tuesday." });
  expect(chosenJob.id, "commenting chose nothing").toBe("mock:PAY-231");
});

test.each([
  ["#/ticket/mock%3APAY-231", "pinned"],
  ["#/ticket/mock%3APAY-231/map", "reading"],
  ["#/ticket/mock%3APAY-231/conversation", "reading"],
])("%s draws none of the job page's retired extras", async (hash, wanted) => {
  if (wanted === "pinned") chosenJob.choose("mock:PAY-231");
  await openOn(hash, "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  await until(() => comments().length === 20, "the thread never drew whole");
  const page = shown(".tf-ticketpage")!;

  expect(page.querySelector('[role="tablist"], [role="tab"]'), "no tab strip").toBeNull();
  expect(page.querySelector(".tf-deps, .tf-mapmode"), "the Map draws nothing").toBeNull();
  expect(page.textContent).not.toContain("Also on this job");
  expect(page.querySelector('[data-testid="job-connections"]'), "no connected count").toBeNull();
  expect(page.textContent).not.toMatch(/\d+ connected/);
  expect(page.querySelector(".tf-grid, .tf-sec, .tf-secfoot"), "no section panels").toBeNull();
  expect(page.querySelector(".tf-filter, .tf-convo-bar"), "no filter boxes").toBeNull();
  expect(page.querySelector(".tf-week"), "no week chart").toBeNull();
  expect(page.textContent).not.toContain("Time this week");
  expect(page.querySelector("select"), "no status select").toBeNull();
  expect(page.querySelector(".tf-clock"), "no clock").toBeNull();
});

test("Start work draws the stepper above the description", async () => {
  await openOn("#/ticket/mock%3APAY-240", "Payout dashboard latency");
  const page = await pageOf("PAY-240");
  await until(() => foot(page)?.textContent?.trim() === "Start work", "Start work never offered");
  foot(page)!.click();
  await until(() => reading()?.querySelector(".tf-steps") !== null, "the stepper never ran in the reading column");
  const steps = reading()!.querySelector(".tf-steps")!;
  const description = reading()!.querySelector(".tf-tkdesc")!;
  expect(steps.compareDocumentPosition(description) & Node.DOCUMENT_POSITION_FOLLOWING, "the steps come first").toBeTruthy();
});

// ---- the card: key with copy, status chip, facts list (#2400) ----

/** The card's facts list as label and value pairs, in order; a value of parts (an avatar and a name, chips) reads them space-separated. */
function facts(card: HTMLElement): [string, string][] {
  const terms = [...card.querySelectorAll("dl.tf-facts > dt")];
  const value = (dd: Element | null) => {
    if (dd === null) return "";
    const parts = [...dd.children].map((part) => part.textContent?.trim() ?? "");
    return (parts.length > 0 ? parts.join(" ") : (dd.textContent ?? "")).replace(/\s+/g, " ").trim();
  };
  return terms.map((term) => [term.textContent?.trim() ?? "", value(term.nextElementSibling)]);
}

test("PAY-231's card: the key with its copy button, the status chip, then Epic, Reporter, Assignee, Type, Priority, Labels and Logged", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");
  await until(() => facts(page).length === 7, `the facts read ${JSON.stringify(facts(page))}`);

  expect(page.querySelector(".tf-tkhead .tf-jobkey")?.textContent).toBe("PAY-231");
  expect(page.querySelector(".tf-tkhead button.tf-tkcopy")?.getAttribute("aria-label")).toBe("Copy the key and the title");
  expect(page.querySelector(".tf-jobrow .tf-chip")?.textContent).toBe("In Progress");
  expect(facts(page).map(([label]) => label)).toEqual(["Epic", "Reporter", "Assignee", "Type", "Priority", "Labels", "Logged"]);
  expect(facts(page)[1]).toEqual(["Reporter", "LH Lena Hoffmann"]);
  expect(avatar(page, "Reporter")?.classList.contains("tf-me"), "somebody else's report is not in full ink").toBe(false);
  expect(facts(page)[2]![1]).toBe("MA mara (you)");
  expect(avatar(page, "Assignee")?.classList.contains("tf-me"), "my own ticket's avatar is in full ink").toBe(true);
  expect(facts(page).slice(3)).toEqual([
    ["Type", "Story"],
    ["Priority", "High"],
    ["Labels", "sepa psp"],
    ["Logged", "2h 10m of 6h, 1h 40m yours"],
  ]);
});

// ---- the card: Reporter and Logged (#2403) ----

/** The avatar on the facts row labelled `label`, or `null`. */
function avatar(card: HTMLElement, label: string): HTMLElement | null {
  const term = [...card.querySelectorAll("dl.tf-facts > dt")].find((dt) => dt.textContent?.trim() === label);
  return term?.nextElementSibling?.querySelector<HTMLElement>(".tf-av") ?? null;
}

/** The Logged row's meter: its segments' classes and widths, whether it is over, or `null` for none. */
function meter(card: HTMLElement): { segments: [string, string][]; over: boolean } | null {
  const bar = card.querySelector<HTMLElement>("dl.tf-facts .tf-tkmeter");
  if (bar === null) return null;
  return {
    segments: [...bar.querySelectorAll<HTMLElement>("i")].map((segment) => [segment.className, segment.style.width]),
    over: bar.classList.contains("tf-tkover"),
  };
}

test("PAY-231's meter splits the logged time against the estimate: mine in full ink, the others' after it", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");
  await until(() => meter(page) !== null, "no meter drawn");
  // 6h is the track; 1h 40m of it mine, 30m somebody else's.
  expect(meter(page)).toEqual({
    segments: [
      ["tf-tkme", "27.8%"],
      ["tf-tkothers", "8.3%"],
    ],
    over: false,
  });
  expect(page.querySelector("dl.tf-facts .tf-tkmeter")?.getAttribute("aria-label")).toBe("2h 10m of 6h logged, 1h 40m yours");
});

test("a ticket with no estimate draws the time alone and no meter; my own report is in full ink", async () => {
  await openOn("#/ticket/mock%3APAY-228", "Idempotency key on the payout endpoint");
  const page = await pageOf("PAY-228");
  await until(() => facts(page).some(([label]) => label === "Logged"), `the facts read ${JSON.stringify(facts(page))}`);
  expect(facts(page).find(([label]) => label === "Logged")).toEqual(["Logged", "3h 40m"]);
  expect(meter(page)).toBeNull();
  expect(facts(page).find(([label]) => label === "Reporter")).toEqual(["Reporter", "MA mara (you)"]);
  expect(avatar(page, "Reporter")?.classList.contains("tf-me")).toBe(true);
});

test("a ticket with an estimate and nothing logged draws 0m of it over an empty track", async () => {
  await openOn("#/ticket/mock%3APAY-240", "Payout dashboard latency");
  const page = await pageOf("PAY-240");
  await until(() => facts(page).some(([label]) => label === "Logged"), `the facts read ${JSON.stringify(facts(page))}`);
  expect(facts(page).find(([label]) => label === "Logged")).toEqual(["Logged", "0m of 2h"]);
  expect(meter(page)).toEqual({ segments: [], over: false });
});

test("the epic is a chip that opens the epic's page, cut with its whole name as its title", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await pageOf("PAY-231");
  await until(() => page.querySelector("a.tf-tkepicchip")?.textContent?.includes("Payout reliability") ?? false, "the epic chip never named its epic");
  const chip = page.querySelector<HTMLAnchorElement>("a.tf-tkepicchip")!;
  expect(chip.getAttribute("href")).toBe("#/ticket/mock%3APAY-200");
  expect(chip.classList.contains("tf-chip")).toBe(true);
  expect(chip.textContent?.trim()).toBe("PAY-200 Payout reliability");
  expect(chip.title).toBe("PAY-200 Payout reliability");
});

test("OPS-77's card draws no row its source leaves empty: no Epic, no Reporter, no Type and no Logged; somebody else's assignee is not mine", async () => {
  await openOn("#/ticket/mock%3AOPS-77", "Rotate the staging database credentials");
  const page = await pageOf("OPS-77");
  await until(() => facts(page).length > 0, "no facts drawn");
  expect(facts(page)).toEqual([
    ["Assignee", "PR priya"],
    ["Priority", "Low"],
    ["Labels", "on-call"],
  ]);
});

test("the copy button puts the key and the title on the clipboard and toasts Copied", async () => {
  const written: string[] = [];
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: (text: string) => (written.push(text), Promise.resolve()) },
    configurable: true,
  });
  try {
    await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
    const page = await pageOf("PAY-231");
    const button = page.querySelector<HTMLButtonElement>("button.tf-tkcopy")!;
    button.click();
    await until(() => written.length === 1, "nothing was copied");
    expect(written).toEqual(["PAY-231 Retry failed SEPA payouts"]);
    await until(() => target.querySelector(".tf-toasts")?.textContent?.includes("Copied") ?? false, "no Copied toast");
    expect(button.classList.contains("tf-tkcopied"), "the button takes the ok tone for a moment").toBe(true);
  } finally {
    Reflect.deleteProperty(navigator, "clipboard");
  }
});

test("both moods draw the same card markup", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const reading = await pageOf("PAY-231");
  await until(() => facts(reading).length === 7, "the facts never drew");
  const shape = (card: HTMLElement) => [...card.querySelectorAll("*")].map((node) => node.tagName).join(" ");
  const before = shape(reading);
  location.hash = "#/job/mock%3APAY-231";
  await until(() => mood(pageCard()!) === "pinned", "the card never turned yellow");
  const pinned = pageCard()!;
  await until(() => facts(pinned).length === 7, "the pinned facts never drew");
  expect(shape(pinned)).toBe(before);
});

// ---- the attachments (#2404) ----

/** The reading column's Attachments section, if it draws one. */
function attachments(): HTMLDetailsElement | null {
  return reading()?.querySelector<HTMLDetailsElement>("section[aria-labelledby='tf-tkatt'] details.tf-tkfold") ?? null;
}

test("PAY-231 folds its four attachments to one line between the description and the conversation", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  await until(() => attachments() !== null, "no Attachments section");

  const fold = attachments()!;
  expect(fold.open, "closed by default").toBe(false);
  const summary = fold.querySelector("summary")!;
  expect(summary.querySelector("h2")?.textContent?.trim()).toBe("Attachments");
  expect(summary.querySelector(".tf-count")?.textContent?.trim()).toBe("4");
  expect(summary.querySelector("svg.tf-tkmark"), "the chevron").not.toBeNull();

  const sections = [...reading()!.querySelectorAll(":scope > section")];
  const at = sections.indexOf(fold.closest("section")!);
  expect(sections[at - 1]?.getAttribute("aria-label")).toBe("Description");
  expect(sections[at + 1]?.getAttribute("aria-labelledby")).toBe("tf-tktalk");
});

test("open, PAY-231's attachments are four rows, each with its kind, its name, who, when and how big, and Open on its content address", async () => {
  sent = [];
  installFakeTauri(recorded({ ...withTicketSearch(demoHandlers()), "plugin:opener|open_url": () => null }));
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  await until(() => attachments() !== null, "no Attachments section");
  const fold = attachments()!;
  fold.querySelector("summary")!.click();
  flushSync();
  expect(fold.open).toBe(true);

  const rows = [...fold.querySelectorAll<HTMLElement>(".tf-tkatt .tf-r")];
  const read = rows.map((row) => [
    row.querySelector(".tf-tkkind")?.textContent?.trim(),
    row.querySelector(".tf-t")?.firstChild?.textContent?.trim(),
    row.querySelector(".tf-rsub")?.textContent?.trim(),
    row.querySelector<HTMLAnchorElement>("a.tf-btn")?.textContent?.trim(),
    row.querySelector<HTMLAnchorElement>("a.tf-btn")?.getAttribute("href"),
  ]);
  const at = "http://127.0.0.1:5399/secure/attachment";
  expect(read).toEqual([
    ["PNG", "psp-409-trace.png", "Lena Hoffmann, 2026-08-12, 182 KB", "Open", `${at}/50000/psp-409-trace.png`],
    ["PNG", "retry-timeline.png", "Mara Lindqvist, 2026-08-19, 96 KB", "Open", `${at}/50001/retry-timeline.png`],
    ["PDF", "sepa-error-codes.pdf", "Priya Nair, 2026-08-12, 1.2 MB", "Open", `${at}/50002/sepa-error-codes.pdf`],
    ["CSV", "batch-13-failures.csv", "Lena Hoffmann, 2026-08-14, 48 KB", "Open", `${at}/50003/batch-13-failures.csv`],
  ]);

  rows[2]!.querySelector<HTMLAnchorElement>("a.tf-btn")!.click();
  await until(() => sent.some((call) => call.cmd === "plugin:opener|open_url"), "Open handed nothing to the browser");
  expect(sent.find((call) => call.cmd === "plugin:opener|open_url")!.args["url"]).toBe(`${at}/50002/sepa-error-codes.pdf`);
});

test("OPS-77 has no attachments and draws no Attachments section", async () => {
  await openOn("#/ticket/mock%3AOPS-77", "Rotate the staging database credentials");
  await pageOf("OPS-77");
  await until(() => reading()?.querySelector("section[aria-labelledby='tf-tktalk']") !== null, "the reading column never drew");
  expect(attachments()).toBeNull();
  expect(reading()!.textContent).not.toContain("Attachments");
});

// ---- the description's inline images and task boxes (#2406) ----

/**
 * An observer that sees every node it is given (#2405): the suite's own never
 * reports an intersection (`test-setup.ts`), and the page fetches an inline
 * image only once it is in view. Answers the undo.
 */
function seeEverything(): () => void {
  const held = globalThis.IntersectionObserver;
  class Seeing {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe(target: Element): void {
      queueMicrotask(() => this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as unknown as IntersectionObserver));
    }
    unobserve(): void {}
    disconnect(): void {}
    takeRecords(): [] {
      return [];
    }
  }
  Object.defineProperty(globalThis, "IntersectionObserver", { writable: true, configurable: true, value: Seeing });
  return () => Object.defineProperty(globalThis, "IntersectionObserver", { writable: true, configurable: true, value: held });
}

test("PAY-231's description draws its inline screenshot where the wiki put it, and its checklist as task boxes", async () => {
  onTestFinished(seeEverything());
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  const desc = () => reading()?.querySelector(".tf-tkdesc .tf-md");
  await until(() => desc()?.querySelector("img.tf-md-att")?.getAttribute("src")?.startsWith("data:image/") ?? false, "the inline image never drew");
  const img = desc()!.querySelector("img.tf-md-att")!;
  expect(img.getAttribute("alt")).toBe("psp-409-trace.png");
  const blocks = [...desc()!.children];
  expect(blocks.findIndex((block) => block.contains(img)), "after the first paragraph, before the list").toBe(1);
  expect(desc()!.textContent).not.toContain("!psp-409-trace.png");
  const tasks = [...desc()!.querySelectorAll("li.tf-task")].map((item) => [item.querySelector(".tf-box")?.getAttribute("aria-label"), item.textContent?.trim()]);
  expect(tasks).toEqual([
    ["Done", "Retry only the transient codes (AC04 is final)"],
    ["Not done", "Cap the retry budget"],
  ]);
});

test("with the fake's attachment read failing, PAY-231's inline screenshot is the placeholder with a link to the ticket in the source", async () => {
  onTestFinished(seeEverything());
  sent = [];
  installFakeTauri(recorded({ ...withTicketSearch(demoHandlers(new URLSearchParams("fake-attachments=unreachable"))), "plugin:opener|open_url": () => null }));
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await pageOf("PAY-231");
  const missing = () => reading()?.querySelector<HTMLElement>(".tf-tkdesc .tf-md-noimg");
  await until(() => missing() != null, "the placeholder never drew");
  expect(reading()!.querySelector(".tf-tkdesc img")).toBeNull();
  expect(missing()!.textContent).toMatch(/^psp-409-trace\.png, not reachable in .+ right now\. Open in /);
  const link = missing()!.querySelector<HTMLAnchorElement>("a.tf-ext")!;
  expect(link.getAttribute("href")).toBe("http://127.0.0.1:5399/browse/PAY-231");
  link.click();
  await until(() => sent.some((call) => call.cmd === "plugin:opener|open_url"), "the link never reached the opener");
  expect(sent.find((call) => call.cmd === "plugin:opener|open_url")!.args["url"]).toBe("http://127.0.0.1:5399/browse/PAY-231");
});
