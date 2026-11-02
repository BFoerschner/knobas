/**
 * The launcher's `Tab` sheet through the shell's own mount (#1500, #2426), over
 * the whole fake IPC (`?fake-ipc`'s handlers), so the rows the launcher offers,
 * the job they name on each page and the link the sheet writes are the
 * product's, not a test's stand-ins. It lives under `dev/` because
 * only the harness may import the harness (`house-rules.test.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";
import { currentJob } from "../../tf/job/current.svelte";
import { until } from "../test-wait";
import { chosenJob } from "../../tf/job/chosen.svelte";

/**
 * The fixture's search answers the estate's lists alone (#430). `PAY-231` is
 * answered here with the job's own ticket, as the mirror would.
 */
function withTicketSearch(handlers: Record<string, Handler>): Record<string, Handler> {
  const estate = handlers["search"]!;
  return {
    ...handlers,
    search: (args) => {
      const raw = ((args["query"] ?? {}) as { raw?: string }).raw ?? "";
      if (raw.trim() !== "PAY-231") return estate(args);
      const hit = {
        entity_id: "mock:PAY-231",
        kind: "ticket",
        source_id: "mock",
        updated_at: null,
        synced_at: "2026-08-22T11:48:00Z",
        title: "Retry failed SEPA payouts",
        path: null,
        rank: 1,
        snippet: [],
      };
      const groups = [{ kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: 1, hits: [hit] }];
      const interpreted = { text: "", prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] };
      return { interpreted, groups, total: 1, took_ms: 1, coverage: [] };
    },
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
/** Every `create_link` the sheet sent, as the backend received it. */
let linked: Record<string, unknown>[] = [];

beforeEach(() => {
  linked = [];
  const handlers = withTicketSearch(demoHandlers());
  const create = handlers["create_link"]!;
  installFakeTauri({
    ...handlers,
    create_link: (args) => {
      linked.push(args);
      return create(args);
    },
  });
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
});

/** The launcher's dialog, once it is up. */
function launcher(): HTMLElement | null {
  return target.querySelector<HTMLElement>('.tf [role="dialog"].tf-launch');
}

/** ⌘K, as the window receives it, then `text` typed into the box and its debounce waited out. */
async function search(text: string): Promise<void> {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await until(() => launcher() !== null, "⌘K never opened the launcher");
  const box = launcher()!.querySelector("input")!;
  box.value = text;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await until(() => (launcher()?.querySelectorAll(".tf-ln-res").length ?? 0) > 0, `no result for ${text}`);
}

function press(key: string): void {
  launcher()!
    .querySelector("input")!
    .dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
}

/** The `Tab` sheet's actions, in order, or `null` when no sheet is drawn. */
function chain(): string[] | null {
  const strip = target.querySelector('[aria-label="Actions on the selected result"]');
  return strip === null ? null : [...strip.querySelectorAll(".tf-ln-nm")].map((row) => row.textContent?.trim() ?? "");
}

/** A ticket's actions after the pair: the timer, the job, the key. */
const TICKET_OWN = ["Start timer", "Work on this", "Copy key"];

async function openOn(hash: string, drawn: string): Promise<void> {
  location.hash = hash;
  app = mount(App, { target, props: {} });
  await until(() => target.textContent?.includes(drawn) ?? false, `${hash} never drew ${drawn}`);
}


test("over a job, Tab on another result offers Link to the job, then the ticket's own", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("PAY-240");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-231", ...TICKET_OWN]);
});

test("on the job's own ticket there is no Link row", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("PAY-231");
  press("Tab");
  expect(chain()).toEqual(TICKET_OWN);
});

/**
 * Off the Job page, *Link to <key>* targets the chosen job (spec #2227),
 * on every page: opening `#/job/PAY-231` chose it.
 */
test.each([
  ["#/builds", "Builds"],
  ["#/time", "Time"],
  ["#/tickets", "All tickets"],
  ["#/inbox", "Inbox"],
  ["#/settings", "Settings"],
])("on %s, Link to names the chosen job", async (hash, drawn) => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  location.hash = hash;
  await until(() => currentJob.entityId === null, `${hash} never replaced the job`);
  await until(() => target.textContent?.includes(drawn) ?? false, `${hash} never drew ${drawn}`);
  await search("PAY-240");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-231", ...TICKET_OWN]);
});

test("with no chosen job, Link to is not offered on Builds: never a guessed job", async () => {
  await openOn("#/builds", "Builds");
  await search("PAY-240");
  press("Tab");
  expect(chain()).toEqual(TICKET_OWN);
});

/** The job's key the shown page draws, or `null` where no job is drawn. */
function jobOnScreen(): string | null {
  return target.querySelector(".tf-kept:not([hidden]) .tf-jobcard .tf-jobkey")?.textContent ?? null;
}

test("starting the timer on a ticket makes it the chosen job: #/job draws it, and Link to names it", async () => {
  await openOn("#/time", "Time");
  location.hash = "#/job";
  await until(() => target.textContent?.includes("No job is chosen yet") ?? false, "#/job never drew its empty state");
  await search("PAY-231");
  press("Tab");
  expect(chain()).toEqual(TICKET_OWN);
  press("Enter");
  await until(() => jobOnScreen() === "PAY-231", "#/job never drew the ticket the timer started on");
  expect(location.hash, "#/job stays the home address").toBe("#/job");
  expect(launcher(), "Start timer closes the launcher").toBeNull();
  location.hash = "#/builds";
  await until(() => target.querySelector(".tf-nav a[aria-current='page']")?.textContent?.trim() === "Builds", "Builds never drew");
  await search("PAY-240");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-231", ...TICKET_OWN]);
});

test("running Start work on a ticket makes it the chosen job", async () => {
  await openOn("#/job/PAY-240", "Payout dashboard latency");
  const run = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-kept:not([hidden]) button")].find((b) => b.textContent === "Run all steps");
  await until(() => run() !== undefined && !run()!.disabled, "Start work never offered Run all steps");
  // Another ticket chosen meanwhile, by a timer started on it from the launcher.
  await search("PAY-231");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-240", ...TICKET_OWN]);
  press("ArrowDown");
  press("Enter");
  await until(() => chosenJob.id === "mock:PAY-231", "the timer's ticket never became the chosen job");
  // The timer step is skipped, so the choice is Start work's own.
  const timerRow = [...target.querySelectorAll<HTMLElement>(".tf-kept:not([hidden]) .tf-steps > *")].find((row) => row.textContent?.includes("Start timer"));
  [...(timerRow?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find((b) => b.textContent === "Skip")?.click();
  run()!.click();
  await until(() => chosenJob.id === "mock:PAY-240", "Start work never made its ticket the chosen job");
});

/**
 * The pair writes itself (#2426): the relation picked by the arrows, the link
 * drawn by the job's own mapping (from the job to the row), and the toast names
 * both ends. No dialog: the 2026-10-21 hand-off is reversed.
 */
test("Link to writes the link with the relation picked inline, and the job's Connections count it", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("PAY-240");
  press("Tab");
  // related → implements → documents → runs on
  press("ArrowRight");
  press("ArrowRight");
  press("ArrowRight");
  press("Enter");
  await until(() => linked.length === 1, "the sheet never wrote the link");
  expect(linked[0]).toMatchObject({ fromId: "mock:PAY-231", toId: "mock:PAY-240", relation: "runs-on" });
  await until(() => launcher() === null, "the launcher stayed open after the write");
  expect(target.textContent).toContain(`Linked PAY-240 to PAY-231 as runs on`);
});

test("from Time, Link to writes against the chosen job and stays on the page", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  location.hash = "#/time";
  await until(() => currentJob.entityId === null, "Time never replaced the job");
  await search("PAY-240");
  press("Tab");
  press("Enter");
  await until(() => linked.length === 1, "the sheet never wrote the link");
  expect(linked[0]).toMatchObject({ fromId: "mock:PAY-231", toId: "mock:PAY-240", relation: "related" });
  expect(location.hash).toBe("#/time");
});

/**
 * #1422: the launcher is a ticket-first dialog. It mounts inside `.tf` through
 * the shell's `dialogs`, on `TfDialog`.
 */
test("the launcher opens inside .tf on the shell's dialog", async () => {
  await openOn("#/tickets", "Tickets");
  await search("PAY-231");
  const dialog = launcher()!;
  expect(dialog.classList.contains("tf-dialog")).toBe(true);
  expect(dialog.closest(".tf-modal")?.parentElement?.closest(".tf")).not.toBeNull();
  expect(dialog.getAttribute("aria-labelledby")).toBeTruthy();
  expect(document.getElementById(dialog.getAttribute("aria-labelledby")!)?.textContent).toBe("Search and actions");
});

/**
 * #1417 end to end under the fixture: `>` reaches the fake's `search`, which
 * answers it as the backend does, so the palette draws the navigation menu,
 * and a row goes to its address and closes the launcher.
 */
test("> under the fixture draws the navigation menu, and Enter on Inbox goes there", async () => {
  await openOn("#/tickets", "Tickets");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await until(() => launcher() !== null, "⌘K never opened the launcher");
  const box = launcher()!.querySelector("input")!;
  box.value = ">";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  const rows = () => [...(launcher()?.querySelectorAll('[aria-label="Results"] .tf-ln-opt .tf-ln-nm') ?? [])].map((row) => row.textContent);
  await until(() => rows().length > 0, "the palette never drew");
  expect(rows().slice(0, 3)).toEqual(["Tickets", "Inbox", "Code"]);
  expect(rows()).toContain("Settings › Keyboard");
  press("ArrowDown");
  press("Enter");
  await until(() => launcher() === null, "the launcher stayed open");
  expect(location.hash).toBe("#/inbox");
});
