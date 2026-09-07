/**
 * The Monitors tab as a reader meets it (#448, spec #427 story 68).
 *
 * `monitors.ts` owns the arithmetic and is asserted as arithmetic; what this
 * file asserts is the half that is only true on the surface: that the chips
 * carry the counts and narrow the list, that a bar reaches the DOM with a
 * segment per bucket coloured by what happened in it, that the readings and
 * the attached assets are drawn beside the monitor they belong to, and that a
 * monitor with nothing attached says so.
 *
 * **The automated half of criterion 4's headless-Chrome clause.** A browser can
 * photograph this tab; what it cannot say is *which* bucket is red, and that
 * is what is asserted here. The screenshot is in the PR.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { MonitorRow, MonitorSample } from "../ipc/assets";
import { createRouter } from "../shell/router.svelte";
import { BAR_BUCKETS, BAR_WINDOW_MS } from "./monitors";
import MonitorsView from "./MonitorsView.svelte";

/** The clock every fixture below is placed against. */
const NOW = new Date("2026-09-07T12:00:00Z");

/** Half an hour — one bucket of the bar. */
const BUCKET_MS = BAR_WINDOW_MS / BAR_BUCKETS;

/** A sample `minutes` before {@link NOW}. */
function at(minutes: number, state: string | null): MonitorSample {
  return { taken_at: new Date(NOW.getTime() - minutes * 60_000).toISOString(), state };
}

function monitor(over: Partial<MonitorRow> & { name: string }): MonitorRow {
  return {
    entity_id: `kuma:${over.name}`,
    source_id: "kuma",
    state: "up",
    monitor_type: "http",
    target: null,
    response_time_ms: null,
    checked_at: new Date(NOW.getTime() - 60_000).toISOString(),
    uptime: [],
    cert_days_remaining: null,
    web_url: null,
    tombstoned: false,
    assets: [],
    samples: [],
    ...over,
  };
}

/**
 * The estate's own monitors, which is the fixture spec #427 rules for assets:
 * the real test infrastructure, nothing Tidewater-shaped invented for them.
 *
 * Six rows and every chip reachable, because a roster of one cannot witness a
 * chip narrowing a list and a roster of all-up cannot witness a count.
 */
const GITEA = monitor({
  name: "gitea",
  state: "up",
  target: "http://localhost:3000/api/healthz",
  response_time_ms: 13,
  uptime: [
    { window: "1d", ratio: 1 },
    { window: "30d", ratio: 0.9962 },
  ],
  cert_days_remaining: 9,
  web_url: "http://127.0.0.1:3001/dashboard/7",
  assets: [
    { id: "asset:knobas-gitea", name: "knobas-gitea", path: "notebook / knobas-stack" },
  ],
  // Two hours of samples with one bad half hour in the middle: a bar of one
  // colour cannot witness that a bucket is coloured by what is in it.
  samples: [at(115, "up"), at(85, "down"), at(55, "up"), at(25, "up")],
});
const JIRA = monitor({
  name: "knobas-jira",
  state: "warn",
  monitor_type: "port",
  target: "hetzner-jira.invalid:8080",
  response_time_ms: 2100,
  samples: [at(10, "warn")],
  assets: [{ id: "asset:hetzner-jira", name: "hetzner-jira", path: null }],
});
const TEAMCITY = monitor({
  name: "knobas-teamcity",
  state: "down",
  monitor_type: "ping",
  target: "hetzner-teamcity.invalid",
  samples: [at(10, "down")],
});
const CONFLUENCE = monitor({ name: "knobas-confluence", state: "pending" });
const PAUSED = monitor({ name: "canary", state: "up", tombstoned: true, web_url: null });
const ODD = monitor({ name: "under-maintenance", state: "maintenance" });
const ROSTER = [GITEA, JIRA, TEAMCITY, CONFLUENCE, PAUSED, ODD];

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** Mount the tab over a roster, recording what *Open in Kuma* opened. */
function render(roster: MonitorRow[] = ROSTER, fail?: unknown) {
  const opened: string[] = [];
  location.hash = "#/assets/monitors";
  const router = createRouter();
  app = mount(MonitorsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        monitorRoster: () => (fail ? Promise.reject(fail) : Promise.resolve(roster)),
        openExternal: (url: string) => {
          opened.push(url);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { router, opened };
}

/** Every chip, as `label count`. */
function chips(): string[] {
  return [...target.querySelectorAll("button.chip")].map((chip) =>
    (chip.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
}

function chip(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll<HTMLButtonElement>("button.chip")].find((button) =>
    button.textContent?.includes(label),
  );
  if (!found) throw new Error(`no chip labelled ${label}: ${chips().join(", ")}`);
  return found;
}

/** The roster's monitor names, in the order they are drawn. */
function listed(): string[] {
  return [...target.querySelectorAll("li.mon .nm")].map((name) => name.textContent ?? "");
}

/** One monitor's row. */
function rowOf(name: string): HTMLElement {
  const found = [...target.querySelectorAll<HTMLElement>("li.mon")].find(
    (row) => row.querySelector(".nm")?.textContent === name,
  );
  if (!found) throw new Error(`${name} is not listed: ${listed().join(", ")}`);
  return found;
}

/** The bar of one monitor, as one entry per bucket (`""` for a gap). */
function barOf(name: string): string[] {
  return [...rowOf(name).querySelectorAll<HTMLElement>(".bar .seg")].map(
    (segment) => segment.dataset.state ?? "",
  );
}

function textOf(name: string): string {
  return (rowOf(name).textContent ?? "").replace(/\s+/g, " ").trim();
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
  vi.restoreAllMocks();
});

/**
 * **Criterion 1, the counts.** Every chip is drawn with the number of monitors
 * in it, and the six add up to the roster — which is what "their counts match
 * the mirror" means when the mirror holds a `maintenance` and a paused check
 * that no named chip claims.
 */
test("the chips carry a count each and the counts add up to the roster", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  expect(chips()).toEqual([
    "Up 1",
    "Warn 1",
    "Down 1",
    "Pending 1",
    "Paused 1",
    "Other 1",
  ]);
});

/**
 * **Criterion 1, the filter.** A chip narrows the list to exactly what it
 * counted, and pressing it again gives the roster back — the chips are a
 * filter and not a destination, so there is no other way out of one.
 */
test("a chip narrows the list to what it counted, and clears on a second press", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  chip("Down").click();
  flushSync();
  expect(listed()).toEqual(["knobas-teamcity"]);
  expect(chip("Down").getAttribute("aria-pressed")).toBe("true");

  chip("Paused").click();
  flushSync();
  expect(listed()).toEqual(["canary"]);

  chip("Paused").click();
  flushSync();
  expect(listed()).toHaveLength(ROSTER.length);
});

/**
 * **A paused monitor is paused whatever it last read.** `canary` is
 * tombstoned with a last state of `up`; it must not be in the *Up* chip, and
 * it must have no *Open in Kuma* — a monitor Kuma no longer publishes has no
 * page left to open, so the button is absent rather than dead.
 */
test("a tombstoned monitor is counted as paused and offers no link to Kuma", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  chip("Up").click();
  flushSync();
  expect(listed()).toEqual(["gitea"]);
  expect(listed()).not.toContain("canary");
  chip("Up").click();
  flushSync();
  expect(rowOf("canary").querySelector("button.kuma")).toBeNull();
  expect(rowOf("gitea").querySelector("button.kuma")).not.toBeNull();
});

/**
 * **Criterion 2, the bar.** Forty-eight segments per monitor, coloured by the
 * state sampled in each half hour, and the half hours knobas has no sample for
 * drawn as gaps.
 *
 * The whole array is asserted rather than "there is a red one somewhere": a
 * bar drawn from the right samples in the wrong order is a bar, and the thing
 * only this can see is *which* bucket the outage is in.
 */
test("the bar is one segment per half hour, coloured by what was sampled in it", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const drawn = barOf("gitea");
  expect(drawn).toHaveLength(BAR_BUCKETS);
  // Two hours of samples, one per half hour, ending in the last bucket.
  expect(drawn.slice(-4)).toEqual(["up", "down", "up", "up"]);
  // And nothing before them: knobas was not watching, which is not the same
  // fact as everything being fine.
  expect(drawn.slice(0, BAR_BUCKETS - 4).every((segment) => segment === "")).toBe(true);
});

/**
 * **Criterion 2, a monitor younger than a day draws what exists.** One sample
 * ten minutes old fills one bucket and leaves forty-seven gaps.
 */
test("a monitor with ten minutes of history draws ten minutes of history", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const drawn = barOf("knobas-teamcity");
  expect(drawn.filter((segment) => segment !== "")).toEqual(["down"]);
  expect(drawn.at(-1)).toBe("down");
});

/** A monitor with no samples at all draws a bar of gaps rather than no bar. */
test("a monitor knobas has never sampled draws an empty bar, not a missing one", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  expect(barOf("knobas-confluence")).toHaveLength(BAR_BUCKETS);
  expect(barOf("knobas-confluence").every((segment) => segment === "")).toBe(true);
});

/**
 * **The readings beside the bar**: the last check, the response time, the
 * uptime ratios at Kuma's own windows, and the certificate days.
 *
 * One decimal on the ratio, because `100%` and `99.6%` are a different month
 * and round to the same integer.
 */
test("each monitor draws its type, target, last check, uptime and certificate days", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const gitea = textOf("gitea");
  expect(gitea).toContain("http");
  expect(gitea).toContain("http://localhost:3000/api/healthz");
  expect(gitea).toContain("Last check 1 min ago");
  expect(gitea).toContain("13 ms");
  expect(gitea).toContain("1d 100.0%");
  expect(gitea).toContain("30d 99.6%");
  expect(gitea).toContain("Cert 9 d");
  // A check with no certificate says nothing about certificates, rather than
  // "Cert — d".
  expect(textOf("knobas-teamcity")).not.toContain("Cert");
});

/**
 * **Criterion 3.** A monitor with attached assets links to them with the path
 * that tells two containers called `postgres` apart; one without says so.
 */
test("a monitor links to the assets it watches, and one without says so", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const linked = rowOf("gitea").querySelector<HTMLAnchorElement>("a.ast");
  expect(linked?.getAttribute("href")).toBe("#/asset/asset:knobas-gitea");
  // The name and the path are two elements laid out side by side, so they are
  // read as two: a `textContent` of the anchor would run them together and the
  // assertion would be about the absence of a space rather than about either.
  expect(linked?.firstChild?.textContent?.trim()).toBe("knobas-gitea");
  expect(linked?.querySelector(".pth")?.textContent).toBe("notebook / knobas-stack");
  // An asset at the top of the estate has no ancestors, and "top level" is
  // what that reads as — the same word the Tree's search box uses.
  expect(
    rowOf("knobas-jira").querySelector("a.ast .pth")?.textContent,
  ).toBe("top level");
  expect(rowOf("knobas-teamcity").querySelector("a.ast")).toBeNull();
  expect(textOf("knobas-teamcity")).toContain("Attached to no asset");
});

/** Story 71: one click from a monitor to its own page in Uptime Kuma. */
test("Open in Kuma hands the monitor's own page to the OS browser", async () => {
  const { opened } = render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  rowOf("gitea").querySelector<HTMLButtonElement>("button.kuma")?.click();
  flushSync();
  expect(opened).toEqual(["http://127.0.0.1:3001/dashboard/7"]);
});

/**
 * **A mirror with no monitors in it is a sentence, not a blank tab.** Every
 * profile has one before a Kuma source is configured, and it is the state a
 * reader is most likely to meet first.
 */
test("an empty mirror says where monitors come from", async () => {
  render([]);
  await vi.waitFor(() => expect(target.querySelector("li.none")).not.toBeNull());

  expect(target.textContent).toContain("Nothing is mirrored yet");
  // The chips are still there, all reading zero: a roster that hid them would
  // make the empty state a different surface from the full one.
  expect(chips()).toEqual(["Up 0", "Warn 0", "Down 0", "Pending 0", "Paused 0", "Other 0"]);
});

/** A read that failed says so, and says it in the backend's own words. */
test("a failed read is reported rather than drawn as an empty roster", async () => {
  render([], { code: "internal", message: "the database went away" });
  await vi.waitFor(() => expect(target.querySelector("p.fail")).not.toBeNull());

  expect(target.querySelector("p.fail")?.textContent).toContain("the database went away");
});

/**
 * The tab strip is the same strip the Tree draws, and *Tree* is a destination
 * from here: the two tabs are two components and one view, so the way back is
 * the address.
 */
test("the tab strip marks Monitors and navigates back to the Tree", async () => {
  const { router } = render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const tabs = [...target.querySelectorAll<HTMLButtonElement>("nav.tabs button.tab")];
  expect(tabs.map((tab) => tab.textContent?.trim())).toEqual(["Tree", "Monitors"]);
  expect(tabs[1]?.getAttribute("aria-current")).toBe("page");
  expect(tabs[0]?.getAttribute("aria-current")).toBeNull();

  tabs[0]?.click();
  flushSync();
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: null });
});

/**
 * The bar is placed against the reader's clock, so the newest bucket ends now.
 *
 * Asserted through the segment titles rather than through the arithmetic:
 * `monitors.ts` already proves the buckets, and what this proves is that the
 * component hands it the clock it was given rather than `new Date()` — an
 * error that would be invisible in every other assertion here, because a
 * fixture and a real clock are minutes apart, not hours.
 */
test("the bar's newest bucket ends at the clock the view was given", async () => {
  render();
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  const segments = [...rowOf("gitea").querySelectorAll<HTMLElement>(".bar .seg")];
  const last = segments.at(-1)?.getAttribute("title") ?? "";
  expect(last).toContain(new Date(NOW.getTime() - BUCKET_MS).toLocaleString());
});
