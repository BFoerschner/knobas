/**
 * The launcher's `Tab` chain through the shell's own mount (#1500), over the
 * whole fake IPC (`?fake-ipc`'s handlers), so the row the launcher offers, the
 * job's link dialog it opens and the Connections drawer that shows the write
 * are the product's, not a test's stand-ins. It lives under `dev/` because
 * only the harness may import the harness (`house-rules.test.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";
import { currentJob, linkRequest } from "../../tf/job/current.svelte";
import { until } from "../test-wait";

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
      const interpreted = { prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] };
      return { interpreted, groups, total: 1, took_ms: 1, coverage: [] };
    },
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

beforeEach(() => {
  installFakeTauri(withTicketSearch(demoHandlers()));
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

/** The `Tab` chain's rows, in order, or `null` when no chain is drawn. */
function chain(): string[] | null {
  const strip = target.querySelector('[aria-label="Actions on the selected result"]');
  return strip === null ? null : [...strip.querySelectorAll(".tf-ln-chain-a")].map((row) => row.textContent?.trim() ?? "");
}

async function openOn(hash: string, drawn: string): Promise<void> {
  location.hash = hash;
  app = mount(App, { target, props: {} });
  await until(() => target.textContent?.includes(drawn) ?? false, `${hash} never drew ${drawn}`);
}

/** The first row of `list:not-monitored` in the fixture: the estate's root, which asks for no monitor. */
const FIRST_UNMONITORED = "knobas test estate";

test("over a job, Tab on another result offers Link to the job, then Start timer", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("list:not-monitored");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-231", `Start timer on ${FIRST_UNMONITORED}`]);
});

test("on the job's own ticket only the timer row is offered", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("PAY-231");
  press("Tab");
  expect(chain()).toEqual(["Start timer on Retry failed SEPA payouts"]);
});

/**
 * Tickets, Inbox and Settings are no job's (#1416's ruling), though the last
 * two start their breadcrumb at the job last opened, so each is reached after
 * opening one: the row is missing because of the page, not for want of a job.
 */
test.each([
  ["#/tickets", "All tickets"],
  ["#/inbox", "Inbox"],
  ["#/settings", "Settings"],
])("on %s only the timer row is offered, with a job opened before", async (hash, drawn) => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  location.hash = hash;
  await until(() => currentJob.entityId === null, `${hash} never replaced the job`);
  await until(() => target.textContent?.includes(drawn) ?? false, `${hash} never drew ${drawn}`);
  await search("list:not-monitored");
  press("Tab");
  expect(chain()).toEqual([`Start timer on ${FIRST_UNMONITORED}`]);
});

test("over a view opened from a job, Link to names that job", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  location.hash = "#/time";
  await until(() => target.querySelector(".tf-crumbs")?.textContent?.includes("PAY-231") ?? false, "Time's breadcrumb never named PAY-231");
  await search("list:not-monitored");
  press("Tab");
  expect(chain()).toEqual(["Link to PAY-231", `Start timer on ${FIRST_UNMONITORED}`]);
});

/** The job's link dialog, once it is up. */
function linkDialog(): HTMLElement | null {
  return [...target.querySelectorAll<HTMLElement>(".tf-dialog")].find((dialog) => dialog.querySelector("h2")?.textContent === "Link PAY-231 to…") ?? null;
}

test("Link to closes the launcher and opens the job's link dialog, whose write the Connections drawer shows", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  await search("list:not-monitored");
  press("Tab");
  press("Enter");
  await until(() => linkDialog() !== null, "the job's link dialog never opened");
  expect(launcher(), "the launcher stayed open over the dialog").toBeNull();

  const dialog = linkDialog()!;
  expect(dialog.querySelector(".tf-picked")?.textContent).toContain(FIRST_UNMONITORED);
  // The relation is still the reader's to pick.
  const relation = dialog.querySelector<HTMLSelectElement>('select[aria-label="Relation"]')!;
  expect(relation.disabled).toBe(false);
  expect([...relation.options].map((option) => option.value)).toContain("runs-on");
  relation.value = "runs-on";
  relation.dispatchEvent(new Event("change", { bubbles: true }));
  const link = [...dialog.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Link")!;
  expect(link.disabled).toBe(false);
  link.click();

  await until(() => linkDialog() === null, "the dialog never closed behind the write");
  const drawer = target.querySelector<HTMLElement>('aside[aria-label="Connections"]');
  await until(
    () => drawer?.querySelector(".tf-dbody")?.textContent?.includes(FIRST_UNMONITORED) ?? false,
    "the Connections drawer never showed the new link",
  );
});

test("from a view opened from the job, Link to opens the job with its link dialog", async () => {
  await openOn("#/job/PAY-231", "Retry failed SEPA payouts");
  location.hash = "#/time";
  await until(() => target.querySelector(".tf-crumbs")?.textContent?.includes("PAY-231") ?? false, "Time's breadcrumb never named PAY-231");
  await search("list:not-monitored");
  press("Tab");
  press("Enter");
  await until(() => linkDialog() !== null, "the job's link dialog never opened");
  expect(location.hash).toBe("#/job/mock%3APAY-231");
  expect(linkDialog()!.querySelector(".tf-picked")?.textContent).toContain(FIRST_UNMONITORED);
});

/**
 * A request the job page never took (the reader went elsewhere before the job
 * drew) is dropped, so a later visit to that job does not open the dialog
 * unasked.
 */
test("a link asked for and left behind does not open on a later visit to the job", async () => {
  await openOn("#/time", "Time");
  linkRequest.ask("PAY-231", { entity_id: "asset:knobas-estate", title: FIRST_UNMONITORED, kind: "asset" });
  location.hash = "#/tickets";
  await until(() => target.querySelector(".tf-crumbs") === null, "Tickets never replaced Time");
  location.hash = "#/job/PAY-231";
  await until(() => currentJob.entityId !== null, "the job never drew");
  flushSync();
  expect(linkDialog(), "the dialog opened unasked").toBeNull();
});

/**
 * #1422: the launcher is a ticket-first dialog. It mounts inside `.tf` through
 * the shell's `dialogs`, on `TfDialog`, and nothing it draws reads the
 * unported sheet's `.scrim` or `.dlg`.
 */
test("the launcher opens inside .tf on the shell's dialog, with no .scrim or .dlg", async () => {
  await openOn("#/tickets", "Tickets");
  await search("PAY-231");
  const dialog = launcher()!;
  expect(dialog.classList.contains("tf-dialog")).toBe(true);
  expect(dialog.closest(".tf-modal")?.parentElement?.closest(".tf")).not.toBeNull();
  expect(dialog.closest(".scrim, .dlg")).toBeNull();
  expect(dialog.querySelector(".scrim, .dlg")).toBeNull();
  expect(target.querySelector(".scrim, .dlg")).toBeNull();
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
