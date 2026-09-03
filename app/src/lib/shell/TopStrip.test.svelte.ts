/**
 * The top strip's sync cluster — spec §2's *"sync monograms with 401
 * highlighted"*.
 *
 * 44 px is not enough room for a list of sources and their states, so the
 * cluster is a row of monograms whose dots carry the state and whose `title`
 * carries the words. What is pinned here is that the dot means what the rest
 * of the app means by it, that a 401 is called out rather than being one red
 * dot among several, and that clicking the cluster lands where the fix is.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { createInbox, type Inbox } from "../inbox/inbox.svelte";
import type { AuthState, CredentialHealth } from "../ipc/sources";
import type { RunningTimer, TimerTarget } from "../ipc/time";
import TopStrip from "./TopStrip.svelte";
import { createHealth } from "./health.svelte";
import { createRouter } from "./router.svelte";
import { createTimer, type Timer } from "./timer.svelte";

function row(source_id: string, state: AuthState): CredentialHealth {
  return {
    source_id,
    state,
    checked_at: "2026-08-25T11:50:00Z",
    detail: null,
    secret_expires_at: null,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/**
 * An inbox store with a fixed count and no bridge behind it.
 *
 * The count is *given*, never derived from a list, which is the point the
 * strip's own tests can make and the view's cannot: nothing in this component
 * may compute the badge from anything it is holding.
 */
function inboxOf(count: number): Inbox {
  const inbox = createInbox({
    inboxItems: () => Promise.resolve([]),
    inboxCount: () => Promise.resolve(count),
    snoozeInboxItem: () => Promise.resolve(),
    completeInboxItem: () => Promise.resolve(),
    listen: () => Promise.resolve(() => {}),
  });
  return inbox;
}

/**
 * A timer store with no bridge behind it, holding what the test dictates.
 *
 * `now` is fixed, so the elapsed reading is an assertion rather than a race,
 * and `begin()` is never called: the strip reads the store, it does not drive
 * it, and a ticking clock in these tests would only make them flaky.
 */
function timerOn(target_: TimerTarget | null, startedAt = "2026-09-03T09:00:00Z"): Timer {
  const running: RunningTimer | null = target_
    ? { target: target_, started_at: startedAt, last_heartbeat: startedAt }
    : null;
  return createTimer({
    currentTimer: () => Promise.resolve(running),
    startTimer: () => Promise.resolve(running!),
    stopTimer: () => Promise.resolve(null),
    timerHeartbeat: () => Promise.resolve(running),
    listen: () => Promise.resolve(() => {}),
    focused: () => true,
    now: () => new Date("2026-09-03T09:45:12Z"),
  });
}

function render(
  states: CredentialHealth[],
  inbox: Inbox = inboxOf(0),
  timer: Timer = timerOn(null),
  ontimer?: () => void,
) {
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: () => Promise.resolve(() => {}),
  });
  for (const entry of states) health.patch(entry);
  const router = createRouter();
  app = mount(TopStrip, {
    target,
    props: { router, onsearch: () => {}, health, inbox, timer, ontimer },
  });
  flushSync();
  return { health, router, timer };
}

/** The timer slot, if the strip is drawing one. */
function timerSlot(): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>("button.timer");
}

/** The inbox button, if the strip is drawing one. */
function inboxButton(): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
    (button.getAttribute("aria-label") ?? "").startsWith("Inbox"),
  );
}

function monograms() {
  return [...target.querySelectorAll<HTMLElement>(".sync .mg")];
}

beforeEach(() => {
  location.hash = "#/ctx/all";
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("one monogram per configured source, in a stable order", () => {
  render([row("teamcity", "ok"), row("gitea", "ok"), row("jira", "ok")]);
  // By source id, so the cluster does not reshuffle itself every time a
  // health event arrives.
  expect(monograms().map((m) => m.textContent)).toEqual(["GI", "JI", "TE"]);
});

test("an unauthorized source paints its monogram as failed and shows the code", () => {
  render([row("jira", "unauthorized"), row("gitea", "ok")]);
  const jira = monograms().find((m) => m.getAttribute("aria-label")?.includes("jira"))!;
  expect(jira.className).toContain("err");
  expect(monograms().find((m) => m.getAttribute("aria-label")?.includes("gitea"))!.className)
    .not.toContain("err");
  // The mockup's `.err-txt`: a 401 is the one state a person has to act on
  // themselves, and one red dot among five is not a call to action.
  expect(target.querySelector(".sync .err-txt")?.textContent).toContain("401");
});

test("an unreachable source is marked, but is not called a 401", () => {
  render([row("jira", "unreachable")]);
  expect(monograms()[0]!.className).toContain("err");
  // A network fault is not a rejected credential, and telling a reader to go
  // rotate a token that was never the problem wastes their afternoon.
  expect(target.querySelector(".sync .err-txt")).toBeNull();
});

test("unknown is not a fault — it is what every source reads before its first check", () => {
  render([row("jira", "unknown"), row("gitea", "ok")]);
  expect(monograms().every((m) => !m.className.includes("err"))).toBe(true);
  expect(target.querySelector(".sync .err-txt")).toBeNull();
});

test("the cluster's tooltip is where the full list fits", () => {
  render([row("jira", "unauthorized"), row("gitea", "ok")]);
  const title = target.querySelector<HTMLElement>(".sync")!.getAttribute("title") ?? "";
  expect(title).toContain("jira");
  expect(title).toContain("gitea");
  expect(title).toMatch(/rejected|unauthorized/i);
});

test("clicking the cluster goes to the sources view, where the fix is", () => {
  const { router } = render([row("jira", "unauthorized")]);
  target.querySelector<HTMLButtonElement>(".sync")!.click();
  flushSync();
  expect(router.route.view).toBe("sources");
});

test("no sources means no cluster at all, not an empty box", () => {
  render([]);
  expect(target.querySelector(".sync")).toBeNull();
});

test("a health event repaints the cluster without a remount", () => {
  const { health } = render([row("jira", "ok")]);
  expect(monograms()[0]!.className).not.toContain("err");
  health.patch(row("jira", "unauthorized"));
  flushSync();
  expect(monograms()[0]!.className).toContain("err");
  expect(target.querySelector(".sync .err-txt")).toBeTruthy();
});

/** A strip button by its accessible name. */
function tool(label: string) {
  return target.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
}

/**
 * Settings is reached the way sources is: a labelled button on the strip.
 *
 * §14's backup surface (#69) had nowhere to be reached from — the strip's only
 * destination was `#/sources`. This is the whole of the navigation that ticket
 * adds; there is no settings router and no tab strip behind it.
 */
test("the strip has a way into settings, and it goes to #/settings", () => {
  const { router } = render([]);

  const settings = tool("Settings")!;
  expect(settings).toBeTruthy();
  // Two destinations, not one relabelled: sources has not moved.
  expect(tool("Sources")).toBeTruthy();

  settings.click();
  flushSync();
  expect(location.hash).toBe("#/settings");
  expect(router.route).toEqual({ view: "settings" });
});

/**
 * *Today* (#279): spec §2's day button, and the one control on this strip that
 * is drawn unconditionally rather than when it has something to say.
 *
 * The address it produces carries **the date**, not the bare `#/time`, and
 * that is the assertion worth making: an address that meant "whenever this was
 * opened" would be a bookmark, a note link and a back button that all showed a
 * different day.
 */
test("*Today* opens the day review on the day it is pressed", () => {
  const { router } = render([]);

  const today = target.querySelector<HTMLButtonElement>('button[title^="Today"]');
  expect(today, "the strip has no way into the day review").toBeTruthy();

  today!.click();
  flushSync();

  const now = new Date();
  const day = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
    now.getDate(),
  ).padStart(2, "0")}`;
  expect(location.hash).toBe(`#/time/${day}`);
  expect(router.route).toEqual({ view: "time", day });
});

/** ...and it says so while the reader is there, like the other destinations. */
test("the strip marks the day review as current while the reader is on it", () => {
  location.hash = "#/time/2026-09-03";
  render([]);

  const today = target.querySelector<HTMLButtonElement>('button[title^="Today"]');
  expect(today!.getAttribute("aria-current")).toBe("page");
  expect(tool("Settings")!.getAttribute("aria-current")).toBeNull();
});

/** Which of the two is current, so the strip says where the reader is. */
test("the strip marks the surface the reader is actually on", () => {
  location.hash = "#/settings";
  render([]);

  expect(tool("Settings")!.getAttribute("aria-current")).toBe("page");
  expect(tool("Sources")!.getAttribute("aria-current")).toBeNull();
});

/**
 * Story 18: the number is how a reader knows there is something without
 * opening it, and pressing it is how they get there.
 */
test("the inbox count is in the strip, and it opens the inbox", async () => {
  const inbox = inboxOf(3);
  await inbox.refreshCount();
  const { router } = render([], inbox);

  const button = inboxButton();
  expect(button, "no inbox button on a strip with three items waiting").toBeTruthy();
  expect(button!.textContent).toContain("3");

  button!.click();
  flushSync();
  expect(location.hash).toBe("#/inbox");
  expect(router.route.view).toBe("inbox");
});

/**
 * Absent at zero, not drawn as `0`.
 *
 * An empty inbox is the state a person should be able to stop thinking about,
 * and a permanent zero in the strip is a slot the eye keeps checking. The rest
 * of this file's cluster follows the same rule — "no sources means no cluster
 * at all, not an empty box".
 */
test("an empty inbox draws no button at all", async () => {
  const inbox = inboxOf(0);
  await inbox.refreshCount();
  render([], inbox);
  expect(inboxButton()).toBeFalsy();
});


// -- the timer slot (#278) --------------------------------------------------

/**
 * Absent when nothing is running — the inbox count's rule, and for the same
 * reason: ⌘T starts a timer from anywhere, so an empty slot would buy nothing
 * and cost a permanent place for the eye to check.
 */
test("the timer slot is absent while nothing is running", () => {
  render([]);
  expect(timerSlot()).toBeNull();
});

test("a running timer shows what it is on and how long it has been", async () => {
  const timer = timerOn({ kind: "entity", entity_id: "jira:PAY-231" });
  render([], inboxOf(0), timer);
  await timer.refresh();
  flushSync();

  const slot = timerSlot();
  expect(slot, "the strip drew no timer for a running one").not.toBeNull();
  // The key, not the whole id: the same half the detail header and the
  // launcher's *Link to…* row use, so the surfaces say one word per ticket.
  expect(slot!.querySelector(".ctx")?.textContent).toBe("PAY-231");
  // 09:00:00 to 09:45:12, and drawn **through a `Flap`** (story 16): the
  // elapsed reading is spec §2's archetypal value that changes while you
  // watch, and `Flap` is also where `prefers-reduced-motion` is honoured, so
  // a hand-rolled span here would silently drop that promise.
  const flap = slot!.querySelector(".flap");
  expect(flap, "the elapsed reading is not a flap").not.toBeNull();
  expect(flap!.textContent).toContain("45:12");
  expect(slot!.getAttribute("aria-label")).toBe("Timing PAY-231 — 45:12");
});

/** An ad-hoc label reads as itself: it is already what a person typed. */
test("a label timer reads as its label", async () => {
  const timer = timerOn({ kind: "label", label: "DB config for the migration" });
  render([], inboxOf(0), timer);
  await timer.refresh();
  flushSync();

  expect(timerSlot()!.querySelector(".ctx")?.textContent).toBe("DB config for the migration");
});

/**
 * The slot is a control, and pressing it is the same verb ⌘T performs. The
 * strip does not stop the timer itself — #280 opens the worklog draft on a
 * stop, and the draft is the shell's.
 */
test("pressing the slot asks the shell to stop, and the strip stops nothing itself", async () => {
  const timer = timerOn({ kind: "entity", entity_id: "jira:PAY-231" });
  let asked = 0;
  render([], inboxOf(0), timer, () => {
    asked += 1;
  });
  await timer.refresh();
  flushSync();

  timerSlot()!.click();
  flushSync();
  expect(asked).toBe(1);
  expect(timer.current, "the strip stopped the timer behind the shell's back").not.toBeNull();
});
