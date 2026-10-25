/**
 * What each store's reset clears (#1743).
 *
 * `test-resets.test.ts` checks that every store has a reset and that the
 * eight from #1674 work. This file covers the rest: one case per store that
 * dirties it through its own door, resets it, and reads it back, plus one case
 * that the registry reaches every singleton's reset.
 */
import { afterEach, expect, test, vi } from "vitest";

import { createAlerts } from "../assets/alerts.svelte";
import { createInbox } from "../inbox/inbox.svelte";
import { createNotifications } from "../inbox/notify.svelte";
import { createBadges } from "../tf/badges.svelte";
import { createDepViewState } from "../tf/deps/views.svelte";
import { breadcrumbJob, currentJob, linkRequest } from "../tf/job/current.svelte";
import { createLayers } from "../tf/layers.svelte";
import { createApps } from "../tf/openin/apps.svelte";
import { createClones } from "../tf/openin/clones.svelte";
import { askAgain, forgetEchoes, echoWrite, echoesOn, takeAgain } from "../tf/queued.svelte";
import {
  forgetEchoes as forgetRepoEchoes,
  isDeleteQueued,
  madeNotListed,
  markCommit,
  markDeleted,
  markMade,
  queuedCommit,
} from "../tf/repo/queued.svelte";
import { createDecisions } from "../tf/review/decisions.svelte";
import { createTheme } from "../tf/theme.svelte";
import { createFollower, writeFollower } from "./follow-write";
import { resetStores } from "./test-resets";
import { createTimer } from "./timer.svelte";
import { waitFor } from "./test-wait";

afterEach(() => vi.useRealTimers());

// A page's session memory lives in its component's `<script module>`, out of a
// test's reach without mounting it; these stand in so the case below can see
// that the reset reaches each one's own door.
const pages = vi.hoisted(() => ({ forgetFolds: vi.fn(), forgetBranches: vi.fn(), forgetTicketsSession: vi.fn() }));
vi.mock("../tf/builds/TfBuilds.svelte", () => ({ forgetFolds: pages.forgetFolds }));
vi.mock("../tf/repo/TfRepo.svelte", () => ({ forgetBranches: pages.forgetBranches }));
vi.mock("../tf/tickets/TicketsPage.svelte", () => ({ forgetTicketsSession: pages.forgetTicketsSession }));

/** A listener that records when it is torn down. */
function listener() {
  const off = vi.fn();
  return { off, listen: vi.fn(async () => off) };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

test("resetStores reaches the reset of every singleton", async () => {
  const spies = [
    vi.spyOn((await import("../assets/alerts.svelte")).alerts, "reset"),
    vi.spyOn((await import("../inbox/inbox.svelte")).inbox, "reset"),
    vi.spyOn((await import("../inbox/notify.svelte")).notifications, "reset"),
    vi.spyOn((await import("./timer.svelte")).timer, "reset"),
    vi.spyOn((await import("../tf/badges.svelte")).badges, "reset"),
    vi.spyOn((await import("../tf/deps/views.svelte")).depViewState, "reset"),
    vi.spyOn((await import("../tf/layers.svelte")).layers, "reset"),
    vi.spyOn((await import("../tf/openin/apps.svelte")).apps, "reset"),
    vi.spyOn((await import("../tf/openin/clones.svelte")).clones, "reset"),
    vi.spyOn((await import("../tf/review/decisions.svelte")).decisions, "reset"),
    vi.spyOn((await import("../tf/theme.svelte")).theme, "reset"),
  ];
  const follower = writeFollower();
  await resetStores();
  for (const spy of spies) expect(spy).toHaveBeenCalledTimes(1);
  expect(writeFollower(), "the shared write follower survived").not.toBe(follower);
  for (const spy of spies) spy.mockRestore();
});

test("resetStores reaches the session memory of each page that keeps one", async () => {
  for (const forget of Object.values(pages)) forget.mockClear();
  await resetStores();
  for (const forget of Object.values(pages)) expect(forget).toHaveBeenCalledTimes(1);
});

test("resetStores offers again every hint put away", async () => {
  const { firstHint, notNow } = await import("../tf/sets/hints.svelte");
  const hint = { set_id: "s", member: { entity_id: "e" } } as Parameters<typeof firstHint>[0][number];
  notNow(hint);
  expect(firstHint([hint])).toBeNull();
  await resetStores();
  expect(firstHint([hint])).toBe(hint);
});

test("viewMemory.clear drops a read ahead still in flight, so the next one reads again", async () => {
  const { createViewMemory, readAhead } = await import("../detail/view-read");
  const memory = createViewMemory();
  const read = vi.fn(() => new Promise<never>(() => {}));
  const readView = vi.fn(async () => null);
  void readAhead("e", read, readView, memory);
  void readAhead("e", read, readView, memory);
  expect(read).toHaveBeenCalledTimes(1);
  memory.clear();
  void readAhead("e", read, readView, memory);
  expect(read).toHaveBeenCalledTimes(2);
});

test("alerts.reset drops the list and unsubscribes", async () => {
  const { off, listen } = listener();
  const alerts = createAlerts({ openAlerts: async () => [{ monitor_id: "m" } as never], listen });
  alerts.start();
  await alerts.refresh();
  await flush();
  expect(alerts.count).toBe(1);
  alerts.reset();
  expect(alerts.open).toEqual([]);
  expect(off).toHaveBeenCalledTimes(2);
  // A new start subscribes again rather than being told it is already live.
  alerts.start();
  expect(listen).toHaveBeenCalledTimes(4);
});

test("alerts.reset clears an error", async () => {
  const alerts = createAlerts({
    openAlerts: async () => {
      throw new Error("down");
    },
  });
  await alerts.refresh();
  expect(alerts.error).not.toBeNull();
  alerts.reset();
  expect(alerts.error).toBeNull();
});

function inboxPorts(overrides: Partial<Parameters<typeof createInbox>[0] & object> = {}) {
  const entry = { item: { key: "alert:m" }, actions: [] } as never;
  return {
    inboxItems: async () => [entry],
    inboxCount: async () => 3,
    snoozeInboxItem: async () => {},
    completeInboxItem: async () => {},
    ackAlert: async () => {},
    listen: listener().listen,
    ...overrides,
  };
}

test("inbox.reset empties both shelves, the count and the answered flag, and unsubscribes", async () => {
  const { off, listen } = listener();
  const inbox = createInbox(inboxPorts({ listen }));
  inbox.start();
  await inbox.refresh();
  await flush();
  expect([inbox.stream.length, inbox.snoozed.length, inbox.count, inbox.answered]).toEqual([1, 1, 3, true]);
  inbox.reset();
  expect([inbox.stream, inbox.snoozed, inbox.count, inbox.answered, inbox.error, inbox.busy]).toEqual([[], [], 0, false, null, false]);
  expect(off).toHaveBeenCalledTimes(3);
});

test("inbox.reset clears an error", async () => {
  const inbox = createInbox(
    inboxPorts({
      inboxCount: async () => {
        throw new Error("down");
      },
    }),
  );
  await inbox.refreshCount();
  expect(inbox.error).not.toBeNull();
  inbox.reset();
  expect(inbox.error).toBeNull();
});

test("inbox.reset makes a read still in flight land nowhere", async () => {
  const releases: ((rows: never[]) => void)[] = [];
  const inbox = createInbox(inboxPorts({ inboxItems: () => new Promise((resolve) => releases.push(resolve as (rows: never[]) => void)) }));
  const read = inbox.refresh();
  inbox.reset();
  for (const release of releases) release([]);
  await read;
  expect(inbox.count).toBe(0);
  expect(inbox.answered).toBe(false);
});

function noticeEntry(key: string) {
  return {
    item: { key: `failed_build:${key}`, category: "failed_build", source_id: "gitea", entity_id: "gitea:a/b#1", kind: "pr", title: key, reason: key, occurred_at: "2026-09-03T09:00:00Z", web_url: null, snoozed_until: null },
    actions: [],
  } as never;
}

test("notifications.reset forgets what was seen, the priming, the kinds and the subscription", async () => {
  const { off, listen } = listener();
  const send = vi.fn();
  const notifications = createNotifications({
    notificationKinds: async () => ["failed_build"],
    focused: () => false,
    send,
    onAction: listen,
  });
  notifications.start();
  await notifications.reseed();
  notifications.saw([noticeEntry("a")]);
  notifications.saw([noticeEntry("b")]);
  expect(send).toHaveBeenCalledTimes(1);
  await flush();

  notifications.reset();
  expect([notifications.kinds, notifications.loaded, notifications.permission, notifications.error, notifications.busy]).toEqual([[], false, null, null, false]);
  expect(off).toHaveBeenCalledTimes(1);

  await notifications.reseed();
  notifications.saw([noticeEntry("a")]); // primes again: no send
  expect(send).toHaveBeenCalledTimes(1);
  notifications.saw([noticeEntry("b")]); // `seen` was cleared: sends
  expect(send).toHaveBeenCalledTimes(2);
});

test("timer.reset drops the running timer and what the reader has in front, and stops the clock", async () => {
  vi.useFakeTimers();
  const timerOff = vi.fn();
  const timer = createTimer({
    currentTimer: async () => ({ started_at: "2026-09-03T09:00:00Z" }) as never,
    listen: (async () => timerOff) as never,
    now: () => new Date("2026-09-03T09:10:00Z"),
  });
  timer.begin();
  await timer.refresh();
  timer.foreground = { kind: "ad_hoc", label: "x" } as never;
  timer.breadcrumbJob = { kind: "ad_hoc", label: "y" } as never;
  timer.roomContext = "work";
  expect(timer.current).not.toBeNull();
  timer.reset();
  expect([timer.current, timer.foreground, timer.breadcrumbJob, timer.roomContext]).toEqual([null, null, null, null]);
  expect(vi.getTimerCount(), "the tick and the beat outlived the reset").toBe(0);
  await vi.advanceTimersByTimeAsync(0);
  expect(timerOff).toHaveBeenCalledTimes(1);
});

test("badges.reset forgets the three counts and unsubscribes", async () => {
  const off = vi.fn();
  const badges = createBadges({
    failingBuilds: async () => 2,
    monitorsDown: async () => 3,
    proposals: async () => 4,
    listen: async () => off,
  });
  badges.start();
  await badges.refresh();
  await flush();
  expect([badges.builds, badges.monitors, badges.proposals]).toEqual([2, 3, 4]);
  badges.reset();
  expect([badges.builds, badges.monitors, badges.proposals]).toEqual([null, null, null]);
  expect(off).toHaveBeenCalledTimes(2);
});

test("depViewState.reset forgets the views, the draft and the opening last drawn", () => {
  const settings = { layers: {}, environments: [] } as never;
  const views = {
    views: [
      { id: "top", settings },
      { id: "ticket", settings },
    ],
    opens_from_top: "top",
    opens_from_ticket: "ticket",
  } as never;
  const state = createDepViewState();
  state.take(views);
  state.opening("T-1");
  expect(state.current?.id).toBe("ticket");
  state.reset();
  expect([state.saved, state.current, state.draft]).toEqual([null, null, null]);
  state.take(views);
  expect(state.current?.id).toBe("top");
  state.opening("T-1");
  expect(state.current?.id, "the opening survived the reset").toBe("ticket");
});

test("the job id, the breadcrumb and the link request reset through their own doors", async () => {
  currentJob.set("job-1");
  breadcrumbJob.set({ key: "PAY-1", entityId: "e" });
  linkRequest.ask("k", { entity_id: "e", title: "t", kind: "ticket" });
  await resetStores();
  expect(currentJob.entityId).toBeNull();
  expect(breadcrumbJob.job).toBeNull();
  expect(linkRequest.take("k")).toBeNull();
});

test("layers.reset forgets every open layer", () => {
  const layers = createLayers();
  layers.add({ kind: "dialog", close: () => {} });
  layers.reset();
  expect(layers.open).toEqual([]);
});

test("apps.reset forgets the settings, so the next ensure reads again", async () => {
  const checkoutSettings = vi.fn(async () => ({ applications: [], default_application: null }) as never);
  const apps = createApps({ checkoutSettings, setCheckoutApplications: vi.fn(), openCheckout: vi.fn() });
  await apps.ensure();
  expect(apps.settings).not.toBeNull();
  apps.reset();
  expect(apps.settings).toBeNull();
  await apps.ensure();
  expect(checkoutSettings).toHaveBeenCalledTimes(2);
});

test("clones.reset drops the marks and the finished count and unlistens", async () => {
  const { off, listen } = listener();
  const answers = [{ status: "adopted", path: "/q" }, { status: "cloning", path: "/p" }];
  const clones = createClones(
    { cloneRepo: async () => answers.shift() as never, entityCheckout: async () => ({}) as never, listen },
    createApps({ checkoutSettings: async () => ({ applications: [], default_application: null }) as never, setCheckoutApplications: vi.fn(), openCheckout: vi.fn() }),
  );
  await clones.start({ entityId: "done", name: "n", repoUrl: null, root: null });
  expect(clones.finished).toBe(1);
  await clones.start({ entityId: "e", name: "n", repoUrl: null, root: null });
  expect(Object.keys(clones.marks)).toEqual(["e"]);
  clones.reset();
  expect(clones.marks).toEqual({});
  expect(clones.finished).toBe(0);
  await waitFor(() => expect(off).toHaveBeenCalledTimes(1));
  await clones.start({ entityId: "e", name: "n", repoUrl: null, root: null });
  expect(listen, "the listener was not registered again").toHaveBeenCalledTimes(2);
});

test("tf echoes and asks reset through forgetEchoes", () => {
  echoWrite("e", "title", "New title");
  askAgain("e", "title", "again");
  forgetEchoes();
  expect(echoesOn("e")).toEqual([]);
  expect(takeAgain("e", "title")).toBeNull();
});

test("forgetEchoes stops the write follow an echo started", async () => {
  const unlisten = vi.fn();
  const follower = createFollower({ listen: async () => unlisten, pendingWrites: async () => [] });
  echoWrite("e", "title", "T", { write: { id: 9 }, following: { what: "x" } as never, follower });
  await flush();
  forgetEchoes();
  expect(unlisten).toHaveBeenCalledTimes(1);
});

test("the repo's queued names and commits reset through forgetEchoes", () => {
  markMade("r", "branch-a");
  markDeleted("r", "branch-b");
  markCommit("r", "main", "a.md", { message: "m", base: "sha" });
  forgetRepoEchoes();
  expect(madeNotListed("r", [])).toEqual([]);
  expect(isDeleteQueued("r", "branch-b")).toBe(false);
  expect(queuedCommit("r", "main", "a.md", "sha")).toBeNull();
});

test("decisions.reset cancels a held write and forgets the verdicts", async () => {
  vi.useFakeTimers();
  const acceptSuggestion = vi.fn(async () => {});
  const decisions = createDecisions({ acceptSuggestion, dismissSuggestion: vi.fn(), createLink: vi.fn() }, 100);
  const entry = { link: { id: "l", relation: "r", from_id: "a", to_id: "b" } } as never;
  decisions.decide(entry, "yes", "r", () => {});
  expect(decisions.verdicts).toEqual({ l: "yes" });
  decisions.reset();
  expect(decisions.verdicts).toEqual({});
  expect(decisions.entries).toEqual([]);
  await vi.advanceTimersByTimeAsync(500);
  expect(acceptSuggestion, "a write held before the reset still went out").not.toHaveBeenCalled();
});

test("theme.reset goes back to following the system", () => {
  const theme = createTheme();
  theme.set("dark");
  theme.reset();
  expect(theme.choice).toBe("system");
});

test("a write follower's reset drops its subscribers and its listener", async () => {
  const unlisten = vi.fn();
  const hear = vi.fn();
  let queue: never[] = [];
  const follower = createFollower({ listen: async () => unlisten, pendingWrites: async () => queue });
  follower.follow(1, hear);
  await flush();
  follower.reset();
  expect(unlisten).toHaveBeenCalledTimes(1);
  // A subscriber from before the reset hears nothing from a read after it.
  hear.mockClear();
  queue = [];
  follower.follow(2, () => {});
  await flush();
  expect(hear).not.toHaveBeenCalled();
});

// A pair, in this order: the second is the first's next test, and finds the
// filter memory as a fresh session would (#1983).
test("a filter typed in one test ...", async () => {
  const { sessionFilter } = await import("../tf/filter/filter-state.svelte");
  sessionFilter("probe-view").text = "left over";
});

test("... is gone in the next", async () => {
  const { sessionFilter } = await import("../tf/filter/filter-state.svelte");
  expect(sessionFilter("probe-view").text).toBe("");
});
