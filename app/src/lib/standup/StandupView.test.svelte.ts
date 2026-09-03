/**
 * The standup view: what a reader sees, and where a line takes them (issue
 * #288, spec #272 stories 58-63).
 *
 * The seam is **a rendered view in, user-visible text and addresses out** —
 * spec #272's testing decisions, and the reason nothing here reaches into the
 * component's state. The bridge is injected, so no Tauri and no database: what
 * the backend puts on the three lists is
 * `crates/knobas-app/tests/standup_ipc.rs`'s, and what is asserted here is
 * that the view asks for the right days and draws the answer as something a
 * reader can act on.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DigestLine, StandupDigest } from "../ipc/entity";
import type { DayWindow } from "../ipc/time";
import { createRouter } from "../shell/router.svelte";
import StandupView from "./StandupView.svelte";

/** The clock the view is given: a Monday afternoon. */
const NOW = new Date(2026, 7, 31, 9, 15, 0, 0);

function line(over: Partial<DigestLine> = {}): DigestLine {
  return {
    entity_id: "jira:PAY-231",
    kind: "ticket",
    title: "Retry failed SEPA payouts",
    source: "jira",
    verb: "log_work",
    reason: "you logged work on it in jira",
    at: new Date(2026, 7, 28, 14, 0, 0, 0).toISOString(),
    ...over,
  };
}

function digest(over: Partial<StandupDigest> = {}): StandupDigest {
  return { yesterday_day: null, yesterday: [], today: [], blockers: [], ...over };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(answer: StandupDigest) {
  const asked: Array<{ today: DayWindow; earlier: DayWindow[] }> = [];
  location.hash = "#/standup";
  const router = createRouter();
  app = mount(StandupView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        standupDigest: (today: DayWindow, earlier: DayWindow[]) => {
          asked.push({ today, earlier });
          return Promise.resolve(answer);
        },
      },
    },
  });
  flushSync();
  return { router, asked };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** One list's rows, by the heading above it. */
function rowsUnder(heading: string): HTMLLIElement[] {
  const section = [...target.querySelectorAll("section.dg")].find((candidate) =>
    candidate.querySelector("h2")?.textContent?.trim().startsWith(heading),
  );
  return [...(section?.querySelectorAll<HTMLLIElement>("li.dg-i") ?? [])];
}

function headingOf(prefix: string): string {
  const found = [...target.querySelectorAll("h2")].find((candidate) =>
    candidate.textContent?.trim().startsWith(prefix),
  );
  return (found?.textContent ?? "").replace(/\s+/g, " ").trim();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * **The view asks for the day it is on and the seven before it.**
 *
 * The windows are the whole of what this side decides: the backend does no
 * timezone arithmetic and cannot, so a view that sent six days would narrow
 * the digest and one that sent the wrong midnights would move everybody's
 * day. Each window is checked as a date *and* as an interval — a `day` that
 * did not match its own `from` is exactly the drift that has no other witness.
 */
test("the view asks for its own day and the seven before it", async () => {
  const { asked } = render(digest());
  await vi.waitFor(() => expect(asked.length).toBe(1));

  const { today, earlier } = asked[0]!;
  expect(today.day).toBe("2026-08-31");
  expect(earlier.map((window) => window.day)).toEqual([
    "2026-08-24",
    "2026-08-25",
    "2026-08-26",
    "2026-08-27",
    "2026-08-28",
    "2026-08-29",
    "2026-08-30",
  ]);
  // Each window is the reader's own midnights, and each ends exactly where the
  // next begins: a gap would be an hour no list claims, and an overlap would
  // put one commit on two days. `standup.test.ts` is where the arithmetic
  // itself is driven; this is the view handing it over intact.
  const all = [...earlier, today];
  for (const window of all) {
    expect(new Date(window.from).getHours()).toBe(0);
    expect(new Date(window.from).getDate()).toBe(Number(window.day.slice(8)));
  }
  for (let index = 0; index < all.length - 1; index += 1) {
    expect(all[index]!.to).toBe(all[index + 1]!.from);
  }
});

/**
 * **Yesterday's heading names the day it is really about** (story 60).
 *
 * On a Monday that is Friday, and the heading is the only place a reader can
 * check it: three commits under a bare "Yesterday" on a Monday morning is a
 * digest that looks right whether it read Sunday, Friday or last month.
 */
test("the yesterday heading names the day the backend resolved", async () => {
  render(digest({ yesterday_day: "2026-08-28", yesterday: [line()] }));
  await vi.waitFor(() => expect(rowsUnder("Yesterday").length).toBe(1));
  expect(headingOf("Yesterday")).toBe("Yesterday — Friday 28 August 2026");
});

/**
 * **Every line is a way in** (story 59, criterion 3).
 *
 * A line is a button onto its item's address, and the address is the
 * kind-agnostic alias with the key encoded: a Gitea key carries `#`, which
 * truncates a fragment at the browser level, and `/`, which reads as another
 * path segment. Both are checked here, because an address that is merely
 * *present* is not one that works.
 */
test("a line opens its item's address, with the key encoded", async () => {
  const { router } = render(
    digest({
      today: [
        line({
          entity_id: "gitea:acme/payouts#144",
          kind: "pr",
          title: "Add payout CSV export",
          source: "gitea",
          verb: "attributed",
        }),
      ],
    }),
  );
  await vi.waitFor(() => expect(rowsUnder("Today").length).toBe(1));

  const button = rowsUnder("Today")[0]!.querySelector<HTMLButtonElement>("button.dg-t")!;
  expect(button.title).toBe("#/entity/gitea:acme%2Fpayouts%23144");
  button.click();
  flushSync();
  expect(location.hash).toBe("#/entity/gitea:acme%2Fpayouts%23144");
  expect(router.route.view).toBe("room");
});

/**
 * **The reason travels with the line** (story 59).
 *
 * Rendered as the backend sent it and never re-assembled here from the verb: a
 * reason built by whichever surface draws it is one that can go missing from
 * the next surface, and a line whose provenance cannot be shown is not
 * shippable.
 */
test("each line shows the reason it came with, source and verb in it", async () => {
  render(
    digest({
      blockers: [
        line({
          verb: "blocked_status",
          reason: "jira calls this status blocked-like: Waiting for support",
        }),
      ],
    }),
  );
  await vi.waitFor(() => expect(rowsUnder("Blockers").length).toBe(1));
  expect(text()).toContain("jira calls this status blocked-like: Waiting for support");
});

/**
 * **The one line with nowhere to click is text, not a dead button** (the
 * ad-hoc timer label).
 *
 * `CONTEXT.md`'s timer target is an entity *or* a label. A label has no item,
 * so it has no address — and a disabled button would promise a door that is
 * not there while a dropped line would leave the reader's own afternoon out of
 * their standup.
 */
test("a line with no item behind it is drawn as text", async () => {
  render(
    digest({
      today: [
        line({
          entity_id: null,
          kind: null,
          title: "DB config for the migration",
          source: "knobas",
          verb: "timer",
          reason: "the timer is running on this label",
        }),
      ],
    }),
  );
  await vi.waitFor(() => expect(rowsUnder("Today").length).toBe(1));

  const row = rowsUnder("Today")[0]!;
  expect(row.textContent).toContain("DB config for the migration");
  expect(row.querySelector("button")).toBeNull();
});

/**
 * **An empty list says so, and says which silence it is** (criterion 3).
 *
 * Three different facts: nothing found in a week, nothing yet this morning,
 * nothing blocked. Only the third is good news, and one shared "Nothing here"
 * would tell a reader which list was empty and nothing else.
 */
test("each empty list says its own silence", async () => {
  render(digest());
  await vi.waitFor(() => expect(text()).toContain("Nothing of yours"));

  expect(text()).toContain("Nothing of yours in the last 7 days.");
  expect(text()).toContain("Nothing touched yet today.");
  expect(text()).toContain("Nothing of yours is blocked.");
  // And with no day resolved, the heading does not invent one -- a date over
  // an empty list would claim that day was quiet.
  expect(headingOf("Yesterday")).toBe("Yesterday");
});

/**
 * A failed read says what went wrong rather than drawing three empty lists.
 *
 * The two are indistinguishable on screen otherwise, and they are opposite
 * facts: "you did nothing" against "knobas could not find out".
 */
test("a failed read says so instead of reading as a quiet week", async () => {
  location.hash = "#/standup";
  const router = createRouter();
  app = mount(StandupView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        standupDigest: () => Promise.reject({ code: "internal", message: "the read fell over" }),
      },
    },
  });
  flushSync();

  await vi.waitFor(() => expect(text()).toContain("the read fell over"));
});
