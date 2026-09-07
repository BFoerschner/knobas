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
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createAlerts, type Alerts } from "../assets/alerts.svelte";
import { createInbox, type Inbox } from "../inbox/inbox.svelte";
import type { AssetDetail, AssetRow, OpenAlert } from "../ipc/assets";
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
    ackAlert: () => Promise.resolve(),
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

/**
 * An open-alert store holding `count` alerts and no bridge behind it (#444).
 *
 * The alerts are *made*, and the store's own count is their number: this
 * component may not compute the badge from anything but the store, and the
 * store may not compute it from anything but its list.
 */
function alertsOf(count: number): Alerts {
  const open: OpenAlert[] = Array.from({ length: count }, (_, index) => ({
    id: index + 1,
    monitor_id: `kuma:${index + 1}`,
    monitor_name: `monitor ${index + 1}`,
    state: "down" as const,
    opened_at: "2026-09-07T08:00:00Z",
    acked_at: null,
    assets: [],
  }));
  return createAlerts({
    openAlerts: () => Promise.resolve(open),
    listen: () => Promise.resolve(() => {}),
  });
}

function render(
  states: CredentialHealth[],
  inbox: Inbox = inboxOf(0),
  timer: Timer = timerOn(null),
  ontimer?: () => void,
  asset?: (assetId: string) => Promise<AssetDetail>,
  alerts: Alerts = alertsOf(0),
) {
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: () => Promise.resolve(() => {}),
  });
  for (const entry of states) health.patch(entry);
  const router = createRouter();
  app = mount(TopStrip, {
    target,
    props: { router, onsearch: () => {}, health, inbox, timer, ontimer, asset, alerts },
  });
  flushSync();
  return { health, router, timer, alerts };
}

/** One asset, as `get_asset` answers for it (#437). */
function assetNamed(id: string, name: string, monogram: string): AssetDetail {
  const asset: AssetRow = {
    id,
    parent_id: null,
    type_id: "vm",
    type_label: "VM",
    monogram,
    name,
    status: "none",
    environment: null,
    owner: null,
    has_children: false,
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
  };
  return {
    asset,
    properties: [],
    held_by: [],
    holds: [],
    // #432: this fixture is about the timer's target, and a target exposes
    // nothing and is reached by nothing in it.
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    effective_environment: null,
    effective_owner: null,
    monitors: [],
    monitoring: [],
  };
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

/** The Assets button, which is drawn whether or not anything is wrong. */
function assetsButton(): HTMLButtonElement {
  const button = target.querySelector<HTMLButtonElement>('button[title^="Assets"]');
  expect(button, "the strip has no way into the estate").toBeTruthy();
  return button!;
}

/**
 * *Assets* (#428) with its open-alert count (#444): the destination is always
 * drawn and the **number** is absent at zero.
 *
 * The two halves are one test because they are one button, and the reason the
 * rules differ is the whole design: a destination nobody can reach is a view
 * nobody opens, and a badge reading `0` is a place the eye keeps checking. The
 * inbox count follows the second rule and this is the strip's second use of it.
 */
test("*Assets* is always drawn and its alert count is absent at zero", () => {
  const { router } = render([]);

  const assets = assetsButton();
  expect(assets.querySelector(".flap"), "an estate with nothing wrong draws no badge").toBeNull();
  expect(assets.getAttribute("aria-label")).toBeNull();

  assets.click();
  flushSync();
  expect(location.hash).toBe("#/assets/tree");
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: null });
});

/**
 * The count is the store's, and it **flaps** — spec §2's rule that a flap is a
 * value that changes while you watch, applied to the one thing on this button
 * that does.
 *
 * The word *Assets* beside it is plain text in the same assertion, because
 * "split-flap on the count only" is the ticket's own clause and a button where
 * everything flapped would be a button where nothing read as news.
 */
test("the alert count is drawn as a flap, and the label beside it is not", async () => {
  const alerts = alertsOf(3);
  await alerts.refresh();
  render([], inboxOf(0), timerOn(null), undefined, undefined, alerts);

  const assets = assetsButton();
  const flap = assets.querySelector(".flap");
  expect(flap, "the count is not a flap").toBeTruthy();
  expect(flap!.textContent).toContain("3");
  expect(assets.querySelector(".k")!.textContent).toBe("Assets");
  expect(assets.querySelector(".k")!.classList.contains("flap")).toBe(false);
  expect(assets.getAttribute("aria-label")).toBe("Assets: 3 open alerts");
  expect(assets.getAttribute("title")).toContain("3 open alerts");
});

/** One is not "1 open alerts". */
test("a single alert reads in the singular", async () => {
  const alerts = alertsOf(1);
  await alerts.refresh();
  render([], inboxOf(0), timerOn(null), undefined, undefined, alerts);

  expect(assetsButton().getAttribute("aria-label")).toBe("Assets: 1 open alert");
});

/**
 * The badge follows the store without a remount, which is what makes it live:
 * an alert opened by a sync run reaches the strip through the store's own
 * re-read, and nothing here re-mounts on a sync.
 */
test("the count follows the store as alerts open and close", async () => {
  let open = 0;
  const alerts = createAlerts({
    openAlerts: () =>
      Promise.resolve(
        Array.from({ length: open }, (_, index) => ({
          id: index + 1,
          monitor_id: `kuma:${index + 1}`,
          monitor_name: "canary",
                state: "warn" as const,
          opened_at: "2026-09-07T08:00:00Z",
          acked_at: null,
          assets: [],
        })),
      ),
    listen: () => Promise.resolve(() => {}),
  });
  render([], inboxOf(0), timerOn(null), undefined, undefined, alerts);
  expect(assetsButton().querySelector(".flap")).toBeNull();

  open = 2;
  await alerts.refresh();
  flushSync();
  expect(assetsButton().querySelector(".flap")!.textContent).toContain("2");

  open = 0;
  await alerts.refresh();
  flushSync();
  expect(assetsButton().querySelector(".flap"), "a healed estate loses its badge").toBeNull();
});

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

/**
 * *Standup* (#288): the way into the digest, beside *Today*.
 *
 * A view nothing opens is a view nobody reads, and the digest is the one
 * surface whose whole purpose is being glanced at once a morning. Its address
 * carries **no** date, unlike *Today*'s — a digest is defined against today,
 * and `#/standup/<date>` is left for the standup protocol.
 */
test("*Standup* opens the digest, at an address with no date in it", () => {
  const { router } = render([]);

  const standup = target.querySelector<HTMLButtonElement>('button[title^="Standup"]');
  expect(standup, "the strip has no way into the digest").toBeTruthy();

  standup!.click();
  flushSync();

  expect(location.hash).toBe("#/standup");
  expect(router.route).toEqual({ view: "standup" });
});

/** ...and it says so while the reader is there, like the other destinations. */
test("the strip marks the digest as current while the reader is on it", () => {
  location.hash = "#/standup";
  render([]);

  const standup = target.querySelector<HTMLButtonElement>('button[title^="Standup"]');
  expect(standup!.getAttribute("aria-current")).toBe("page");
  expect(target.querySelector('button[title^="Today"]')!.getAttribute("aria-current")).toBeNull();
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

/**
 * **An asset reads as its name, with its monogram** (#437).
 *
 * The one target whose key says nothing: knobas mints `asset:<uuid>`, so the
 * half `targetReading` shows for a ticket is a uuid here. The strip looks the
 * asset up and draws the chip its type carries — `VM`, the same chip the
 * Tree's columns put on the row — beside the name.
 *
 * The uuid is asserted **absent**, not merely the name present: a slot that
 * drew both would satisfy a `toContain("vm-db-01")` and still show the reader
 * a uuid.
 */
test("an asset timer reads as the asset's name and monogram, never its uuid", async () => {
  const timer = timerOn({ kind: "entity", entity_id: "asset:9f3c11de" });
  render([], inboxOf(0), timer, undefined, (id) =>
    Promise.resolve(assetNamed(id, "vm-db-01", "VM")),
  );
  await timer.refresh();
  flushSync();
  // The look-up is a round trip of its own; the slot draws the target's key
  // until it lands.
  await vi.waitFor(() => {
    flushSync();
    if (timerSlot()?.querySelector(".ctx")?.textContent !== "vm-db-01") {
      throw new Error("the strip never named the asset");
    }
  });

  const slot = timerSlot()!;
  expect(slot.querySelector(".mg")?.textContent).toBe("VM");
  expect(slot.textContent).not.toContain("9f3c11de");
  expect(slot.getAttribute("aria-label")).toBe("Timing vm-db-01 — 45:12");
});

/**
 * The failure direction, and the reason the look-up is not allowed to matter:
 * an asset the read cannot answer for still leaves a slot that says the clock
 * is running. A strip that blanked itself would lose the reading spec §2 asks
 * to be always true — *a reader always knows what the clock is on* — over a
 * name it could not fetch.
 */
test("an asset the read cannot name still shows a running clock", async () => {
  const timer = timerOn({ kind: "entity", entity_id: "asset:9f3c11de" });
  render([], inboxOf(0), timer, undefined, () => Promise.reject(new Error("not_ready")));
  await timer.refresh();
  flushSync();

  const slot = timerSlot();
  expect(slot, "the strip dropped the timer over a name it could not read").not.toBeNull();
  expect(slot!.querySelector(".ctx")?.textContent).toBe("9f3c11de");
  expect(slot!.querySelector(".mg")).toBeNull();
});

/**
 * **A name belongs to the asset it was read for**, and to no other (#437).
 *
 * A stop and a start on another asset are two events a beat apart, and the
 * second asset's read has not landed while the first's answer is still in the
 * component. So the slot is driven through exactly that window: the second
 * read is held open, and what the strip must *not* say in the meantime is the
 * name of the machine the reader has just stopped working on.
 */
test("a second asset is never drawn under the first one's name", async () => {
  const first = { kind: "entity", entity_id: "asset:aaa" } as const;
  const second = { kind: "entity", entity_id: "asset:bbb" } as const;
  let running: RunningTimer = {
    target: first,
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T09:00:00Z",
  };
  const timer = createTimer({
    currentTimer: () => Promise.resolve(running),
    startTimer: () => Promise.resolve(running),
    stopTimer: () => Promise.resolve(null),
    timerHeartbeat: () => Promise.resolve(running),
    listen: () => Promise.resolve(() => {}),
    focused: () => true,
    now: () => new Date("2026-09-03T09:45:12Z"),
  });
  let landSecond: ((detail: AssetDetail) => void) | null = null;
  render([], inboxOf(0), timer, undefined, (id) =>
    id === first.entity_id
      ? Promise.resolve(assetNamed(id, "vm-db-01", "VM"))
      : new Promise<AssetDetail>((resolve) => {
          landSecond = resolve;
        }),
  );
  await timer.refresh();
  flushSync();
  await vi.waitFor(() => {
    flushSync();
    if (timerSlot()?.querySelector(".ctx")?.textContent !== "vm-db-01") {
      throw new Error("the strip never named the first asset");
    }
  });

  running = { ...running, target: second };
  await timer.refresh();
  flushSync();

  const slot = timerSlot()!;
  expect(slot.querySelector(".ctx")?.textContent, "the strip kept naming the asset the clock has left").toBe(
    "bbb",
  );
  expect(slot.querySelector(".mg"), "the first asset's chip outlived its name").toBeNull();

  landSecond!(assetNamed(second.entity_id, "postgres", "CT"));
  await vi.waitFor(() => {
    flushSync();
    if (timerSlot()?.querySelector(".ctx")?.textContent !== "postgres") {
      throw new Error("the second asset's name never landed");
    }
  });
  expect(timerSlot()!.querySelector(".mg")?.textContent).toBe("CT");
});

/**
 * **The name is tried again until it lands** (#437).
 *
 * `get_asset` rejects `not_ready` for the whole of bring-up, and a
 * relaunch-restored timer is drawn inside exactly that window — so a single
 * attempt would leave the reader looking at a uuid for the rest of the
 * session. The store re-reads the timer on every activity line and every
 * beat, and each of those is the next attempt.
 *
 * The refusal is the fixture: the first read fails, the strip draws the key,
 * and the second read — triggered by nothing but the timer being re-read —
 * names it.
 */
test("an asset the estate could not name yet is named on the next read", async () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "asset:9f3c11de" },
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T09:00:00Z",
  };
  const timer = createTimer({
    currentTimer: () => Promise.resolve(running),
    startTimer: () => Promise.resolve(running),
    stopTimer: () => Promise.resolve(null),
    timerHeartbeat: () => Promise.resolve(running),
    listen: () => Promise.resolve(() => {}),
    focused: () => true,
    now: () => new Date("2026-09-03T09:45:12Z"),
  });
  let attempts = 0;
  render([], inboxOf(0), timer, undefined, (id) => {
    attempts += 1;
    return attempts === 1
      ? Promise.reject(new Error("not_ready"))
      : Promise.resolve(assetNamed(id, "vm-db-01", "VM"));
  });
  await timer.refresh();
  flushSync();
  expect(timerSlot()!.querySelector(".ctx")?.textContent, "the first read did not fail").toBe(
    "9f3c11de",
  );

  // The next re-read of the timer -- an activity line, or the thirty-second
  // beat -- and nothing else.
  await timer.refresh();
  await vi.waitFor(() => {
    flushSync();
    if (timerSlot()?.querySelector(".ctx")?.textContent !== "vm-db-01") {
      throw new Error("the strip never tried the name again");
    }
  });
  expect(timerSlot()!.querySelector(".mg")?.textContent).toBe("VM");
});

/**
 * ...and once it is named, a beat costs no round trip: the read is skipped
 * while the name in hand belongs to the target in hand. A strip that re-read
 * on every beat would ask the estate about one asset 120 times an hour for an
 * answer it already had.
 */
test("a named asset is not re-read on every beat", async () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "asset:9f3c11de" },
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T09:00:00Z",
  };
  const timer = createTimer({
    currentTimer: () => Promise.resolve(running),
    startTimer: () => Promise.resolve(running),
    stopTimer: () => Promise.resolve(null),
    timerHeartbeat: () => Promise.resolve(running),
    listen: () => Promise.resolve(() => {}),
    focused: () => true,
    now: () => new Date("2026-09-03T09:45:12Z"),
  });
  let attempts = 0;
  render([], inboxOf(0), timer, undefined, (id) => {
    attempts += 1;
    return Promise.resolve(assetNamed(id, "vm-db-01", "VM"));
  });
  await timer.refresh();
  await vi.waitFor(() => {
    flushSync();
    if (timerSlot()?.querySelector(".ctx")?.textContent !== "vm-db-01") {
      throw new Error("the strip never named the asset");
    }
  });

  for (let beat = 0; beat < 3; beat += 1) {
    await timer.refresh();
    flushSync();
  }
  expect(attempts, "the strip re-read an asset it had already named").toBe(1);
  expect(timerSlot()!.querySelector(".ctx")?.textContent).toBe("vm-db-01");
});

/**
 * Nothing but an asset is looked up. A ticket's key is the reader's own
 * shorthand and already right, and a strip that asked the estate about
 * `jira:PAY-231` would be asking for a `not_found` on every ticket.
 */
test("a ticket timer asks the estate nothing", async () => {
  const timer = timerOn({ kind: "entity", entity_id: "jira:PAY-231" });
  let asked = 0;
  render([], inboxOf(0), timer, undefined, (id) => {
    asked += 1;
    return Promise.resolve(assetNamed(id, "vm-db-01", "VM"));
  });
  await timer.refresh();
  flushSync();

  expect(timerSlot()!.querySelector(".ctx")?.textContent).toBe("PAY-231");
  expect(asked, "the strip looked a ticket up in the estate").toBe(0);
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
