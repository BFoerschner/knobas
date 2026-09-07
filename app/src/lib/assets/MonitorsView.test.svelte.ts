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

import type {
  MonitorRow,
  MonitorSample,
  OpenAlert,
  UnmonitoredAsset,
} from "../ipc/assets";
import { createRouter } from "../shell/router.svelte";
import { toasts } from "../shell/toasts.svelte";
import { createAlerts } from "./alerts.svelte";
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
    actions: [],
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
        // The other read this tab makes (#449). Answered here so the tests
        // above stay about the monitor roster: a port left off would send
        // them at the real bridge, which is not in a jsdom window.
        unmonitoredAssets: () => Promise.resolve([]),
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

// -- the open-alert cards and the "Not monitored" roster (#449) --------------

/** One open alert, half an hour old, watching one asset. */
function alert(over: Partial<OpenAlert> = {}): OpenAlert {
  return {
    id: 1,
    monitor_id: "kuma:knobas-teamcity",
    monitor_name: "knobas-teamcity",
    state: "down",
    opened_at: new Date(NOW.getTime() - 30 * 60_000).toISOString(),
    acked_at: null,
    assets: [{ id: "asset:hetzner-teamcity", name: "hetzner-teamcity", path: "hel1" }],
    ...over,
  };
}

/** One row of the *Not monitored* roster. */
function gap(name: string, typeId: string, label: string, path: string | null): UnmonitoredAsset {
  return {
    id: `asset:${name}`,
    type_id: typeId,
    type_label: label,
    monogram: label.slice(0, 2).toUpperCase(),
    name,
    path,
  };
}

const GAPS = [
  gap("hel1", "site", "Site", null),
  gap("vm-db-01", "vm", "VM", "hel1"),
  gap("vm-web-01", "vm", "VM", "hel1"),
];

/**
 * Mount the tab over a roster, an alert store and a *Not monitored* answer,
 * recording what an Ack asked for.
 */
function renderWith(options: {
  open?: OpenAlert[];
  gaps?: UnmonitoredAsset[];
  gapsFail?: unknown;
}) {
  const acked: string[] = [];
  let open = options.open ?? [];
  const alerts = createAlerts({
    openAlerts: () => Promise.resolve(open),
    ackAlert: (monitorId: string) => {
      acked.push(monitorId);
      // What the backend does: the alert stays open and reads acked.
      open = open.map((row) =>
        row.monitor_id === monitorId ? { ...row, acked_at: new Date(NOW).toISOString() } : row,
      );
      return Promise.resolve();
    },
    listen: () => Promise.resolve(() => {}),
  });
  location.hash = "#/assets/monitors";
  const router = createRouter();
  app = mount(MonitorsView, {
    target,
    props: {
      router,
      now: () => NOW,
      alerts,
      ports: {
        monitorRoster: () => Promise.resolve(ROSTER),
        openExternal: () => Promise.resolve(),
        unmonitoredAssets: () =>
          options.gapsFail
            ? Promise.reject(options.gapsFail)
            : Promise.resolve(options.gaps ?? GAPS),
      },
    },
  });
  // The store is seeded by the shell in the real window (`App.svelte`, on
  // `lifecycle.ready`), never by a view — so a test that mounts one view has
  // to do the seeding the shell would have done.
  void alerts.refresh();
  flushSync();
  return {
    router,
    alerts,
    acked,
    close: (monitorId: string) => {
      open = open.filter((row) => row.monitor_id !== monitorId);
    },
  };
}

/** Each open-alert card, as a reader reads it. */
function cards(): string[] {
  return [...target.querySelectorAll("li.card")].map((card) =>
    (card.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
}

/** The *Not monitored* roster's names, in the order it draws them. */
function gaps(): string[] {
  return [...target.querySelectorAll("li.gap .nm")].map((name) => name.textContent ?? "");
}

/** One type-filter option, by its label. */
function typeFilter(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll<HTMLButtonElement>("button.tf")].find((button) =>
    button.textContent?.includes(label),
  );
  if (!found) throw new Error(`no type filter labelled ${label}`);
  return found;
}

/**
 * **Criterion 1, the cards.** Every open alert is a card at the top of the
 * tab, in the order the read answers — newest first, which is `open_alerts`'
 * own `opened_at desc` — carrying the monitor that fell, the asset it watches
 * with the path that says where it sits, and how long it has been open.
 *
 * Two alerts and not one, because *in opened-at order* is not a claim a list
 * of one can witness.
 */
test("open alerts are cards at the top of the tab, newest first", async () => {
  renderWith({
    open: [
      alert(),
      alert({
        id: 2,
        monitor_id: "kuma:knobas-jira",
        monitor_name: "knobas-jira",
        state: "warn",
        opened_at: new Date(NOW.getTime() - 3 * 60 * 60_000).toISOString(),
        assets: [{ id: "asset:hetzner-jira", name: "hetzner-jira", path: null }],
      }),
    ],
  });
  await vi.waitFor(() => expect(cards()).toHaveLength(2));

  expect(cards()[0]).toContain("knobas-teamcity");
  expect(cards()[0]).toContain("hetzner-teamcity");
  expect(cards()[0]).toContain("hel1");
  expect(cards()[0]).toContain("30 min ago");
  expect(cards()[1]).toContain("knobas-jira");
  // An asset at the top of the estate has no ancestors, and "top level" is
  // what that reads as — the roster below uses the same word.
  expect(cards()[1]).toContain("top level");
  expect(cards()[1]).toContain("3 h ago");

  // The card opens the asset, which is the next step: an alert has no address
  // of its own (spec #427 story 61).
  const open = target.querySelector<HTMLAnchorElement>("li.card a.open");
  expect(open?.getAttribute("href")).toBe("#/asset/asset:hetzner-teamcity");
});

/** Nothing wrong draws no card section at all, the top strip badge's rule. */
test("an estate with nothing wrong in it draws no cards", async () => {
  renderWith({ open: [] });
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  expect(target.querySelector("section.cards")).toBeNull();
});

/**
 * **Criterion 1, the ack.** Ack from a card is the inbox's ack: it asks for
 * the alert's own **monitor** (`ack_alert`'s argument since #446), and the
 * card comes back **acked and still there** — only a return to `up` closes an
 * alert, so a card that vanished on an ack would say the estate was well.
 */
test("Ack from a card acks the alert's monitor and leaves the card acked", async () => {
  const { acked } = renderWith({ open: [alert()] });
  await vi.waitFor(() => expect(cards()).toHaveLength(1));

  target.querySelector<HTMLButtonElement>("li.card button.ack")?.click();
  // The store re-reads *after* the write, so the card is redrawn a tick later
  // than the call — waiting on the call alone would assert against the
  // pre-ack DOM.
  await vi.waitFor(() => {
    flushSync();
    expect(cards()[0]).toContain("Acked");
  });

  expect(acked, "the ack is addressed by the monitor, `ack_alert`'s argument").toEqual([
    "kuma:knobas-teamcity",
  ]);
  expect(cards(), "acked is not closed").toHaveLength(1);
  expect(
    target.querySelector("li.card button.ack"),
    "there is nothing a second ack would say",
  ).toBeNull();
});

/** An alert that arrived already acked draws the same card, with no Ack. */
test("an alert acked elsewhere is drawn as acked here", async () => {
  renderWith({ open: [alert({ acked_at: new Date(NOW.getTime() - 60_000).toISOString() })] });
  await vi.waitFor(() => expect(cards()).toHaveLength(1));

  expect(cards()[0]).toContain("Acked");
  expect(target.querySelector("li.card button.ack")).toBeNull();
});

/**
 * **Criterion 1, the recovery.** A monitor that comes back up closes its alert
 * in the sync run, and the card leaves through the store's own re-read — no
 * remount, and nothing on this tab deciding when an alert is over.
 */
test("a monitor that recovers takes its card off the tab", async () => {
  const { alerts, close } = renderWith({ open: [alert()] });
  await vi.waitFor(() => expect(cards()).toHaveLength(1));

  close("kuma:knobas-teamcity");
  await alerts.refresh();
  flushSync();
  expect(cards()).toEqual([]);
  expect(target.querySelector("section.cards")).toBeNull();
});

/** An alert on a monitor watching nothing is the one most worth saying aloud. */
test("an alert whose monitor watches nothing is a card with nowhere to go", async () => {
  renderWith({ open: [alert({ assets: [] })] });
  await vi.waitFor(() => expect(cards()).toHaveLength(1));

  expect(cards()[0]).toContain("watching nothing");
  expect(target.querySelector("li.card a.open")).toBeNull();
  // And it can still be acked: seen is a thing a reader can say about an
  // alert whether or not anybody has finished wiring the monitor up.
  expect(target.querySelector("li.card button.ack")).not.toBeNull();
});

/**
 * **Criterion 2.** The roster lists the assets with no monitor, each with the
 * path that tells two containers called `postgres` apart, and excludes the
 * ones that have one — which is what the read answers and what this asserts
 * reaches the screen.
 */
test("the Not monitored roster lists the assets nothing watches, with their path", async () => {
  renderWith({});
  await vi.waitFor(() => expect(gaps()).toHaveLength(GAPS.length));

  expect(gaps()).toEqual(["hel1", "vm-db-01", "vm-web-01"]);
  const first = target.querySelector<HTMLAnchorElement>("li.gap a");
  expect(first?.getAttribute("href")).toBe("#/asset/asset:hel1");
  const paths = [...target.querySelectorAll("li.gap .pth")].map((path) => path.textContent);
  expect(paths).toEqual(["top level", "hel1", "hel1"]);
});

/** **Criterion 2, the filter.** A type narrows the roster and clears on a second press. */
test("a type filter narrows the Not monitored roster and clears again", async () => {
  renderWith({});
  await vi.waitFor(() => expect(gaps()).toHaveLength(GAPS.length));

  // The options are the types the answer holds, counted — never the whole
  // type table, most of which nothing on this roster is.
  expect(
    [...target.querySelectorAll("button.tf")].map((button) =>
      (button.textContent ?? "").replace(/\s+/g, " ").trim(),
    ),
  ).toEqual(["Site 1", "VM 2"]);

  typeFilter("VM").click();
  flushSync();
  expect(gaps()).toEqual(["vm-db-01", "vm-web-01"]);
  expect(typeFilter("VM").getAttribute("aria-pressed")).toBe("true");

  typeFilter("VM").click();
  flushSync();
  expect(gaps()).toEqual(["hel1", "vm-db-01", "vm-web-01"]);
});

/** An estate where everything is watched says so, rather than drawing a blank. */
test("an estate with a monitor on everything says the roster is empty", async () => {
  renderWith({ gaps: [] });
  await vi.waitFor(() => expect(listed()).toHaveLength(ROSTER.length));

  expect(target.querySelector("section.unmon")?.textContent).toContain(
    "Every asset has a monitor",
  );
  expect(target.querySelector("button.tf"), "nothing to narrow").toBeNull();
});

/**
 * A failed **alert** read says so, and says it where a reader will see it even
 * though there are no cards: the store keeps its last list when a read fails,
 * so a first read that failed leaves the count at zero — and a message drawn
 * only alongside cards would be silent in exactly the case where the tab has
 * nothing to show and no idea whether that is good news.
 */
test("a failed alert read is reported although there are no cards to draw", async () => {
  location.hash = "#/assets/monitors";
  const router = createRouter();
  const alerts = createAlerts({
    openAlerts: () => Promise.reject({ code: "internal", message: "the alerts went away" }),
    listen: () => Promise.resolve(() => {}),
  });
  app = mount(MonitorsView, {
    target,
    props: {
      router,
      now: () => NOW,
      alerts,
      ports: {
        monitorRoster: () => Promise.resolve(ROSTER),
        openExternal: () => Promise.resolve(),
        unmonitoredAssets: () => Promise.resolve([]),
      },
    },
  });
  void alerts.refresh();
  flushSync();

  await vi.waitFor(() => {
    flushSync();
    expect(target.textContent).toContain("the alerts went away");
  });
  expect(target.querySelector("section.cards"), "a failed read draws no cards").toBeNull();
});

/** A failed roster read is reported rather than drawn as a well-watched estate. */
test("a failed Not monitored read is reported in the backend's own words", async () => {
  renderWith({ gapsFail: { code: "internal", message: "the database went away" } });
  await vi.waitFor(() => expect(target.querySelector("section.unmon p.fail")).not.toBeNull());

  expect(target.querySelector("section.unmon p.fail")?.textContent).toContain(
    "the database went away",
  );
});

/**
 * The Kuma write half on the tab (#452, spec #427 story 69).
 *
 * Mounts the tab over a roster whose rows carry `actions`, recording every
 * write it queues. A separate helper from {@link render} because it needs the
 * write port and because a test asserting *no buttons* must be able to mount a
 * roster with no actions at all -- which is what a Kuma configured with only an
 * API key answers.
 */
function renderWrites(roster: MonitorRow[], reject?: unknown) {
  const queued: unknown[] = [];
  location.hash = "#/assets/monitors";
  const router = createRouter();
  app = mount(MonitorsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        monitorRoster: () => Promise.resolve(roster),
        unmonitoredAssets: () => Promise.resolve([]),
        openExternal: () => Promise.resolve(),
        submitWrite: (payload: unknown) => {
          queued.push(payload);
          return reject ? Promise.reject(reject) : Promise.resolve({} as never);
        },
      },
    },
  });
  flushSync();
  return { queued };
}

/** Every action button on the roster, as `monitor name → button label`. */
function actionButtons(): string[] {
  return [...target.querySelectorAll("ol.roster li.mon")].flatMap((row) => {
    const name = row.querySelector(".nm")?.textContent?.trim() ?? "?";
    return [...row.querySelectorAll<HTMLButtonElement>("button.act")].map(
      (button) => `${name} → ${(button.textContent ?? "").trim()}`,
    );
  });
}

/**
 * The whole of criterion 1's second half: **a source with only the key shows
 * no buttons.**
 *
 * The roster is the ordinary one, whose rows carry an empty `actions` -- which
 * is what `monitor_roster` answers for a Kuma with no account in its keychain
 * item. Nothing else about the tab changes, which is the point: the reader of a
 * read-only Kuma sees the roster they always saw.
 */
test("a monitor whose source offers nothing gets no buttons", async () => {
  renderWrites(ROSTER);
  await vi.waitFor(() => expect(target.querySelectorAll("li.mon").length).toBe(ROSTER.length));
  expect(actionButtons()).toEqual([]);
  // *Open in Kuma* is not a write and is unaffected.
  expect(target.querySelectorAll("button.kuma").length).toBeGreaterThan(0);
});

/**
 * The other direction, and the per-row rule with it: a live monitor is offered
 * *Pause*, a tombstoned one *Resume*, and neither is offered both.
 *
 * The row's own `actions` decide it -- the backend has already filtered -- so
 * what this asserts is that the tab renders what it is handed and invents
 * nothing. A tab that read `tombstoned` itself would pass a fixture where the
 * two agree and fail the day the backend narrows differently.
 */
test("pause is offered on a live monitor and resume on a paused one", async () => {
  renderWrites([
    { ...GITEA, actions: ["pause_monitor"] },
    { ...PAUSED, actions: ["resume_monitor"] },
    // An op the tab has no form for is skipped rather than drawn nameless.
    { ...JIRA, actions: ["create_monitor"] },
  ]);
  await vi.waitFor(() => expect(target.querySelectorAll("li.mon").length).toBe(3));
  expect(actionButtons()).toEqual(["gitea → Pause", "canary → Resume"]);
});

/** The payload each button queues, and that it is the row's own entity. */
test("pressing pause queues a PauseMonitor for that monitor", async () => {
  const { queued } = renderWrites([
    { ...GITEA, actions: ["pause_monitor"] },
    { ...PAUSED, actions: ["resume_monitor"] },
  ]);
  await vi.waitFor(() => expect(target.querySelectorAll("button.act").length).toBe(2));
  const [pause, resume] = [...target.querySelectorAll<HTMLButtonElement>("button.act")];
  pause!.click();
  await vi.waitFor(() => expect(queued.length).toBe(1));
  expect(queued[0]).toEqual({ PauseMonitor: { entity: GITEA.entity_id } });

  resume!.click();
  await vi.waitFor(() => expect(queued.length).toBe(2));
  expect(queued[1]).toEqual({ ResumeMonitor: { entity: PAUSED.entity_id } });
  // What the reader is told is that knobas *queued* it -- not that the monitor
  // is paused, which is only true once the next poll reads it back.
  await vi.waitFor(() =>
    expect(toasts.items.map((item) => item.text)).toContain("Resume queued for canary"),
  );
});

/**
 * A refused write says so and leaves the roster alone.
 *
 * The direction that matters: a tab that dropped the rejection would leave a
 * reader believing a monitor was paused -- and a paused monitor looks exactly
 * like a monitor nobody touched until the next poll, so nothing on screen would
 * ever correct them.
 */
test("a refused pause is reported and the row stays as it was", async () => {
  renderWrites([{ ...GITEA, actions: ["pause_monitor"] }], {
    code: "invalid",
    message: "source \"kuma\" does not offer \"pause_monitor\"",
  });
  await vi.waitFor(() => expect(target.querySelectorAll("button.act").length).toBe(1));
  toasts.items = [];
  target.querySelector<HTMLButtonElement>("button.act")!.click();
  // The toast store rather than the DOM: the toaster is the shell's and is not
  // mounted inside this component's tree.
  await vi.waitFor(() => expect(toasts.items.length).toBe(1));
  expect(toasts.items[0]!.text).toContain("Could not queue that for gitea");
  expect(toasts.items[0]!.tone).toBe("err");
  expect(actionButtons()).toEqual(["gitea → Pause"]);
});
