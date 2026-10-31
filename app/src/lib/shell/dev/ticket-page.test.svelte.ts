/**
 * The ticket page (spec #2227, #2231) through the app's own mount over the
 * whole fake IPC (`?fake-ipc`'s handlers): a card on Tickets opens
 * `#/ticket/<entity id>`, which reads the ticket without choosing it or
 * promoting its context; its one button opens the job; `⌘K` opens it too;
 * it is the foreground a capture names; *Link to* targets it; and `Esc` takes
 * the reader back to the Tickets that opened it, that ticket selected. It
 * lives under `dev/` because only the harness may import the harness
 * (`house-rules.test.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
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

beforeEach(() => {
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

/** The ticket page's card, once it has read its ticket. */
function ticketCard(): HTMLElement | null {
  return shown('article[aria-label="Ticket"]');
}

async function ticketPageOf(key: string): Promise<HTMLElement> {
  await until(() => ticketCard()?.querySelector(".tf-k")?.textContent === key, `the ticket page never drew ${key}`);
  return ticketCard()!;
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

function facts(page: HTMLElement): Record<string, string> {
  const terms = [...page.querySelectorAll(".tf-facts dt")].map((dt) => dt.textContent?.trim() ?? "");
  const values = [...page.querySelectorAll(".tf-facts dd")].map((dd) => dd.textContent?.trim() ?? "");
  return Object.fromEntries(terms.map((term, at) => [term, values[at]!]));
}

test("a card on Tickets opens its ticket page: key, title, facts, epic, description and Around this ticket as counts, under Tickets", async () => {
  await openOn("#/tickets/payout-reliability", "Payout reliability");
  await until(() => card("mock:PAY-231") !== null, "the board never drew PAY-231");
  card("mock:PAY-231")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the card opened ${location.hash}`);
  const page = await ticketPageOf("PAY-231");

  expect(lit(), "the ticket page lights Tickets").toBe("Tickets");
  expect(page.classList.contains("tf-card"), "the card is the ticket-yellow").toBe(true);
  expect(page.querySelector("h1")?.textContent).toBe("Retry failed SEPA payouts");
  expect(facts(page)).toMatchObject({ Status: "In Progress", Assignee: "mara", Priority: "High", Epic: "PAY-200" });
  expect(page.querySelector(".tf-desc")?.textContent).toBeTruthy();
  await until(() => (page.querySelectorAll(".tf-rollup > div").length ?? 0) > 0, "Around this ticket never counted");
  const lines = [...page.querySelectorAll(".tf-rollup > div")].map((line) => line.textContent ?? "");
  expect(lines.some((line) => /Builds\d+ builds?/.test(line)), lines.join(" | ")).toBe(true);
  expect(page.querySelector(".tf-rollup a"), "the counts are not links").toBeNull();
  const crumbs = [...(shown(".tf-crumbs")?.querySelectorAll("a, strong") ?? [])].map((part) => part.textContent?.trim());
  expect(crumbs).toEqual(["Tickets", "PAY-231"]);
  // Reading a ticket chose no job.
  expect(target.querySelector(".tf-nav a:first-child")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Job");
});

test.each([
  ["mock:PAY-240", "PAY-240", "Start work"],
  ["mock:PAY-231", "PAY-231", "Resume work"],
  ["mock:PAY-219", "PAY-219", "Open job"],
  ["mock:PAY-236", "PAY-236", "Open job"],
])("the ticket page of %s offers %s's button the preview offers: %s", async (id, ticket, label) => {
  await openOn("#/tickets", "All tickets");
  location.hash = `#/ticket/${encodeURIComponent(id)}`;
  const page = await ticketPageOf(ticket);
  await until(() => page.querySelector(".tf-go .tf-primary")?.textContent === label, `the button read ${page.querySelector(".tf-go .tf-primary")?.textContent}`);
  expect(page.querySelector(".tf-go .tf-primary")?.getAttribute("href")).toBe(`#/job/${encodeURIComponent(id)}`);
});

test("the ticket page's button opens the job and makes it the chosen job", async () => {
  await openOn("#/ticket/mock%3APAY-240", "Payout dashboard latency");
  const page = await ticketPageOf("PAY-240");
  await until(() => page.querySelector(".tf-go .tf-primary")?.textContent === "Start work", "Start work never offered");
  page.querySelector<HTMLAnchorElement>(".tf-go .tf-primary")!.click();
  await until(() => shown(".tf-jobcard .tf-jobkey")?.textContent === "PAY-240", "the job never drew");
  expect(location.hash).toBe("#/job/mock%3APAY-240");
  await until(() => target.querySelector(".tf-nav a:first-child")?.textContent?.replace(/\s+/g, " ").trim() === "Job PAY-240", "the job was never chosen");
});

test("the Tickets preview's button opens the job too", async () => {
  await openOn("#/tickets/payout-reliability", "Payout reliability");
  await until(() => card("mock:PAY-231") !== null, "the board never drew PAY-231");
  card("mock:PAY-231")!.click();
  flushSync();
  const button = shown('aside[aria-label="Selected ticket"] .tf-go .tf-primary');
  expect(button?.getAttribute("href")).toBe("#/job/mock%3APAY-231");
});

test("#/ticket/<id> draws the ticket page in place, a bare #/ticket lands on Tickets, and an unreadable id says it was not found", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  await ticketPageOf("PAY-231");
  expect(location.hash, "no redirect to the job").toBe("#/ticket/mock%3APAY-231");

  location.hash = "#/ticket";
  await until(() => shown(".tf-tickets") !== null, "a bare #/ticket never drew Tickets");
  expect(lit()).toBe("Tickets");

  location.hash = "#/ticket/mock%3ANOPE-1";
  await until(() => shown(".tf-ticketpage")?.textContent?.includes("No ticket NOPE-1 in the mirror") ?? false, "an unreadable ticket never said so");
});

test("opening the ticket page promotes no context; opening the job does", async () => {
  await openOn("#/ticket/mock%3APAY-231", "Retry failed SEPA payouts");
  const page = await ticketPageOf("PAY-231");
  await until(() => (page.querySelectorAll(".tf-rollup > div").length ?? 0) > 0, "Around this ticket never counted");
  expect(sent.map((call) => call.cmd)).not.toContain("promote_context");
  location.hash = "#/job/mock%3APAY-231";
  await until(() => sent.some((call) => call.cmd === "promote_context"), "the job never promoted its context");
});

test("the ticket page is the foreground a capture names, not the chosen job", async () => {
  await openOn("#/job/mock%3APAY-231", "Retry failed SEPA payouts");
  location.hash = "#/ticket/mock%3APAY-240";
  await ticketPageOf("PAY-240");
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
  await ticketPageOf("PAY-240");
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
  await ticketPageOf("PAY-240");
  await until(() => launcher() === null, "the launcher stayed open");
  key("Escape");
  await until(() => location.hash === "#/tickets", `Esc went to ${location.hash}`);
});

test("Esc on a ticket page Tickets opened returns to that ticket view with that ticket selected, even after the selection moved", async () => {
  await openOn("#/tickets/payout-reliability", "Payout reliability");
  await until(() => card("mock:PAY-231") !== null && card("mock:PAY-240") !== null, "the board never drew its cards");
  card("mock:PAY-231")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await ticketPageOf("PAY-231");
  expect(shown(".tf-esc")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Back to Tickets Esc");

  // Back on the board, another card is picked, and the reader comes forward to the ticket page again.
  key("[", { metaKey: true });
  await until(() => card("mock:PAY-240") !== null, "⌘[ never showed the board");
  card("mock:PAY-240")!.click();
  flushSync();
  expect(selected()).toBe("mock:PAY-240");
  key("]", { metaKey: true });
  await ticketPageOf("PAY-231");

  key("Escape");
  await until(() => location.hash === "#/tickets/payout-reliability", `Esc went to ${location.hash}`);
  await until(() => selected() === "mock:PAY-231", `the board selected ${selected()}`);
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

function hint(): string | null {
  return shown(".tf-esc")?.textContent?.replace(/\s+/g, " ").trim() ?? null;
}

test("a run's ticket link on Builds opens the ticket page, and Esc there returns to the run", async () => {
  installFakeTauri(recorded(withRunTicket(withTicketSearch(demoHandlers()))));
  await openOn(RUN_HASH, "#1187");
  const link = `a[href="#/ticket/${encodeURIComponent("mock:PAY-231")}"]`;
  await until(() => shown(link) !== null, "the run never drew its ticket");
  shown(link)!.click();
  await until(() => location.hash === "#/ticket/mock%3APAY-231", `the run's ticket opened ${location.hash}`);
  await ticketPageOf("PAY-231");
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
