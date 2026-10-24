/**
 * `?fake-ipc`: The Monitors tab (#448): the roster, Pause and Resume, open alerts and
 * their ack, and the unmonitored assets.
 */
// The monitors the seeded Uptime Kuma holds -- the roster's rows. See
// `fakeMonitorRoster`.
import SEEDED_MONITORS from "../../../../../../testenv/monitors.json";
import type { OpenAlert } from "../../../ipc/assets";
import type { QueuedWrite, WriteOpPayload } from "../../../ipc/sources";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { fakeRoleScope } from "./corpus";
import { FIXTURE_ESTATE, FIXTURE_ROUTES, assetPathText } from "./estate";

// -- the Monitors tab (#448) ------------------------------------------------

/** The role `monitor_roster` is read for, as `knobas_source::role::MONITORING` spells it. */
const MONITORING_ROLE = "monitoring";

/**
 * The monitors the estate file asks for, as `monitor_roster` answers them.
 *
 * **Derived from the files and not listed here.** `testenv/monitors.json` is
 * the list the seeded Uptime Kuma holds, and `estate.json` names each asset's
 * monitors by their Kuma name (story 25), so both the rows and what each one
 * watches are already written down once; a second list here would be a
 * fixture that drifts from the files the seed and the Import load. The
 * `canary` is a row like the others and watches nothing, as in the real
 * roster (`assets::monitor_roster` draws every mirrored monitor).
 *
 * What this *does* invent is the reading — a state, a response time, a day of
 * samples — because `/metrics` is a live system and there is nothing on disk
 * to read it from. The states are dealt round in a fixed order so that every
 * chip on the tab has something in it: a roster where everything is `up` can
 * be photographed without showing that the chips do anything.
 */
const FIXTURE_MONITOR_STATES = ["up", "warn", "down", "pending", "up", "maintenance", "up"];

/**
 * The paused row. Gitea's because the five other states are dealt to the
 * other five rows, so pausing this one leaves every chip with something in it.
 */
const FIXTURE_PAUSED = "gitea";

/**
 * How many minutes ago the newest sample of the fixture roster is.
 *
 * A whole number of half hours, so the fixture's bars start on a bucket edge
 * and a screenshot is not one segment shorter than the next reader's.
 */
const FIXTURE_SAMPLE_STEP_MIN = 30;

/**
 * One monitor's last day, one sample per half hour — forty-eight of them, so
 * the bar is full rather than a strip with the left three quarters missing.
 *
 * The blips are placed by index and not at random: a fixture that redrew
 * itself on every reload would make two QA screenshots of the same build
 * disagree.
 */
function fakeSamples(state: string, seed: number, now: number) {
  return Array.from({ length: 48 }, (_, index) => {
    const takenAt = new Date(now - (47 - index) * FIXTURE_SAMPLE_STEP_MIN * 60_000);
    // Two hours with no reading at all, so a gap is on the screenshot: a bar
    // that is never interrupted cannot show what an interruption looks like.
    const quiet = index >= 10 + (seed % 5) && index < 14 + (seed % 5);
    // One bad half hour per monitor, in a different place for each.
    const blip = index === 30 + (seed % 9);
    return {
      taken_at: takenAt.toISOString(),
      state: quiet ? null : blip ? "down" : state,
    };
  });
}

/**
 * `open_alerts`: one alert per monitor the fixture roster has in trouble.
 *
 * **Derived from `fakeMonitorRoster`**, so the strip and the Monitors tab are
 * one statement about one fixture rather than two lists that can disagree —
 * the rule the backend follows, where both surfaces read one table. A
 * `pending` or `maintenance` monitor is deliberately not in it: those two
 * words neither open an alert nor close one (`CONTEXT.md`, **Alert**), and a
 * fixture that drew them would teach a reader the wrong rule.
 *
 * `opened_at` is derived from the row's index so two screenshots of the same
 * build agree, and the first one is acked so the list can show what *seen, not
 * fixed* looks like without anything here having to remember a click.
 */
export function fakeOpenAlerts(): OpenAlert[] {
  const now = Date.now();
  return fakeMonitorRoster()
    .filter((row) => row.state === "down" || row.state === "warn")
    .map((row, index) => ({
      id: index + 1,
      monitor_id: row.entity_id,
      monitor_name: row.name,
      state: row.state as OpenAlert["state"],
      opened_at: new Date(now - (index + 1) * 37 * 60_000).toISOString(),
      acked_at:
        index === 0 || FIXTURE_ACKED.has(row.entity_id)
          ? new Date(now - 12 * 60_000).toISOString()
          : null,
      assets: row.assets,
    }));
}

/**
 * `submit_write`: the row the queue answers with, for the fixture's own
 * Pause, Resume and Create monitor (#864).
 *
 * `sent`, with no wait reason: what a fixture must not do is show a write
 * *pending* for ever, which is what a `pending` row with nothing to flush it
 * would look like.
 */
function fakeQueuedWrite(args: Record<string, unknown>): QueuedWrite {
  const row = queuedRow(args);
  if (row.op === "create_monitor") FIXTURE_CREATES.push(row);
  return row;
}

/**
 * The creates of monitors this fixture took, oldest first (#1959). Each is
 * `sent` and never arrives, since no Kuma is behind the fixture, so the row
 * that records its name draws *Monitor queued* until a reload, which is what
 * the real read says of a create the poll has not mirrored yet.
 */
const FIXTURE_CREATES: QueuedWrite[] = [];

/** The newest create this fixture took for a name in `names`: `monitor_on_its_way`. */
function onItsWayFor(names: readonly string[]): QueuedWrite | null {
  const named = FIXTURE_CREATES.filter((write) => {
    const create = (write.payload as { CreateMonitor?: { name?: string } }).CreateMonitor;
    return create?.name !== undefined && names.includes(create.name);
  });
  return named.at(-1) ?? null;
}

/** The seeded Kuma's monitor names: a name among them is one the mirror holds. */
const SEEDED_NAMES = new Set(SEEDED_MONITORS.map((monitor) => monitor.name));

function queuedRow(args: Record<string, unknown>): QueuedWrite {
  const payload = (args.payload ?? {}) as Record<string, { entity?: string; repo?: string }>;
  const [tag, body] = Object.entries(payload)[0] ?? ["PauseMonitor", {}];
  // `CommitFile` names its repository as `repo` (#822), every other op `entity`.
  const entity = body?.entity ?? body?.repo ?? "kuma:1";
  const now = new Date().toISOString();
  return {
    id: (FIXTURE_WRITE_ID += 1),
    source_id: entity.split(":")[0] ?? "kuma",
    entity_id: entity,
    // The table's own ops, else `WriteOp::identifier()`'s snake case of the
    // tag: the repository page's writes (#839, #840, #845) come through here too.
    op: FIXTURE_OPS[tag] ?? tag.replace(/[A-Z]/g, (letter, at: number) => (at === 0 ? "" : "_") + letter.toLowerCase()),
    payload: payload as WriteOpPayload,
    target_snapshot: null,
    state: "sent",
    wait_reason: null,
    detail: null,
    queued_at: now,
    attempted_at: now,
    attempts: 1,
    held_snapshot: null,
    settled_at: now,
    source_enabled: true,
  };
}

let FIXTURE_WRITE_ID = 0;

/** Each payload tag this fixture queues, by the op identifier the queue records. */
const FIXTURE_OPS: Record<string, string> = {
  PauseMonitor: "pause_monitor",
  ResumeMonitor: "resume_monitor",
  CreateMonitor: "create_monitor",
};

/**
 * The monitors a reader has acked in this session, by monitor entity id.
 *
 * Module state, like the fixture's other writes: `?fake-ipc` is a browser
 * with no backend behind it, so the only place an ack can be remembered is
 * here. Cleared by a reload, which is what a fixture is.
 */
const FIXTURE_ACKED = new Set<string>();

/**
 * `ack_alert`: mark one monitor's open alert seen, and answer with it.
 *
 * **Leaves the alert open**, which is the whole of what #446 decided: only a
 * return to `up` closes one, so the card stays and starts reading acked. A
 * monitor with nothing open is `not_found`, the refusal the real command
 * makes for an alert that recovered while a reader was looking at it.
 */
function fakeAck(args: Record<string, unknown>) {
  const monitorId = typeof args.monitorId === "string" ? args.monitorId : "";
  const alert = fakeOpenAlerts().find((row) => row.monitor_id === monitorId);
  if (!alert) {
    throw { code: "not_found", message: `no open alert on ${monitorId}`, source_id: null };
  }
  FIXTURE_ACKED.add(monitorId);
  return { ...alert, acked_at: new Date().toISOString() };
}

/**
 * `unmonitored_assets`: every asset in the file for which it names no Uptime
 * Kuma monitor.
 *
 * **Derived from the file and not listed here**, `fakeMonitorRoster`'s rule:
 * `estate.json` says which monitors each asset asks for, so *which assets ask
 * for none* is already written down once and a second list would drift from
 * it.
 */
export function fakeUnmonitored() {
  // A name the seeded Kuma holds attaches its monitor; one a create recorded
  // does not, so the asset stays listed with its create on its way (#1959).
  const assets = FIXTURE_ESTATE.filter((asset) => !asset.monitors.some((name) => SEEDED_NAMES.has(name)))
    .map((asset) => ({
      id: asset.id,
      type_id: asset.type_id,
      type_label: asset.type_label,
      monogram: asset.monogram,
      name: asset.name,
      path: assetPathText(asset),
      // In force, the backend's walk (#864): the asset's own, else the
      // nearest ancestor's.
      environment: inForce(asset, (row) => row.environment),
      owner: inForce(asset, (row) => row.owner),
      url: null as string | null,
      monitor_on_its_way: onItsWayFor(asset.monitors),
    }))
    .sort(
      (left, right) =>
        (left.path ?? "").localeCompare(right.path ?? "") ||
        left.name.localeCompare(right.name),
    );
  // The routes follow the assets (#976). The estate file names no monitor for
  // a route, so every fixture route is a gap; its path is where the exposing
  // asset sits, that asset's own name last, and its env and owner are that
  // asset's, in force.
  const routes = FIXTURE_ROUTES.flatMap((route) => {
    const exposer = FIXTURE_ESTATE.find((asset) => asset.id === route.asset_id);
    if (!exposer) return [];
    const above = assetPathText(exposer);
    return [
      {
        id: route.id,
        type_id: "route",
        type_label: "Route",
        monogram: "RT",
        name: route.name,
        path: above === null ? exposer.name : `${above} / ${exposer.name}`,
        environment: inForce(exposer, (row) => row.environment),
        owner: inForce(exposer, (row) => row.owner),
        url: route.url as string | null,
        monitor_on_its_way: onItsWayFor(route.monitors ?? []),
      },
    ];
  }).sort(
    (left, right) =>
      left.path.localeCompare(right.path) || left.name.localeCompare(right.name),
  );
  return [...assets, ...routes];
}

/** A field in force on `asset`: its own value, else the nearest ancestor's (#431). */
function inForce(
  asset: (typeof FIXTURE_ESTATE)[number],
  of: (row: (typeof FIXTURE_ESTATE)[number]) => string | null,
): string | null {
  for (let at: (typeof FIXTURE_ESTATE)[number] | undefined = asset; at; ) {
    const value = of(at);
    if (value !== null) return value;
    const parent: string | null = at.parent_id;
    at = parent === null ? undefined : FIXTURE_ESTATE.find((row) => row.id === parent);
  }
  return null;
}

/**
 * The roster rows drawn as **port** checks, so the tab shows more than one
 * monitor type: the three on-demand products'. A fixture choice and not a
 * claim -- the seeded Kuma checks them over HTTP (`testenv/monitors.json`).
 */
const FIXTURE_PORT_CHECKS = new Set(["teamcity", "jira", "confluence"]);

/**
 * `monitor_roster`: every monitor the seed creates, with a reading, a day of
 * samples and the assets the estate file says it watches.
 *
 * The **last** sample is what the row's `state` says, which is the rule the
 * backend follows (`assets::monitor_roster`): the chip and the bar's
 * right-hand end are one statement, and a fixture that let them disagree
 * would make the tab look right while hiding the one thing that could be
 * wrong.
 */
export function fakeMonitorRoster() {
  const now = Date.now();
  const named = SEEDED_MONITORS.map(({ name, type }) => ({
    name,
    type,
    watching: FIXTURE_ESTATE.filter((asset) => asset.monitors.includes(name)),
  }));
  return named.map((entry, index) => {
    const state = FIXTURE_MONITOR_STATES[index % FIXTURE_MONITOR_STATES.length] ?? "up";
    const samples = fakeSamples(state, index, now);
    // The last one, for the reason above.
    const last = samples.at(-1);
    // One paused monitor, because a roster with none cannot show what a
    // silenced check looks like and it is the state the tab most needs to be
    // able to say out loud.
    const tombstoned = entry.name === FIXTURE_PAUSED;
    return {
      entity_id: `kuma:${index + 1}`,
      source_id: "kuma",
      name: entry.name,
      state: last?.state ?? null,
      monitor_type: FIXTURE_PORT_CHECKS.has(entry.name) ? "port" : entry.type,
      target: FIXTURE_PORT_CHECKS.has(entry.name)
        ? `${entry.name}.invalid:8080`
        : `http://localhost:3001/${entry.name.replace(/\W+/g, "-")}`,
      response_time_ms: state === "warn" ? 2_140 : 13 + index * 7,
      checked_at: new Date(now - 60_000).toISOString(),
      uptime: [
        { window: "1d", ratio: state === "down" ? 0.62 : 0.998 },
        { window: "30d", ratio: 0.9962 },
      ],
      // One certificate check, since most monitors watch no certificate and a
      // roster where every row had one would hide that the column is optional.
      cert_days_remaining: index === 0 ? 9 : null,
      web_url: tombstoned ? null : `http://localhost:3001/dashboard/${index + 1}`,
      tombstoned,
      assets: entry.watching.map((asset) => ({
        id: asset.id,
        name: asset.name,
        path: assetPathText(asset),
        type_label: asset.type_label,
      })),
      // The fixture's Kuma has an account, so the tab draws its buttons
      // (issue #452) -- and the per-row half of the rule is the fixture's
      // too: the paused monitor is the one offered *Resume*. A fixture that
      // offered both on every row would photograph a tab that cannot exist.
      actions: [tombstoned ? "resume_monitor" : "pause_monitor"],
      samples,
    };
  });
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The Monitors tab (#448). Derived from the seed's monitor list and the
    // estate file's names -- see `fakeMonitorRoster` -- so the roster a
    // browser draws is the set of monitors the seed creates, with a reading
    // invented on top.
    // Monitoring's read by definition, so it narrows itself to the role's
    // fillers by source, tombstones kept, as the server's does (#1297).
    monitor_roster: () => {
      const fillers = new Set(fakeRoleScope(MONITORING_ROLE).sources);
      return fakeMonitorRoster().filter((row) => fillers.has(row.source_id));
    },
    // The Monitors tab's Pause / Resume (#452). Answers the queued row the
    // real command answers, and remembers nothing: `?fake-ipc` has no Kuma
    // behind it, so the honest fixture is "knobas took the write" and a
    // roster that still says what it said. A handler is here at all because
    // the button beside it is -- a control that answers "command not found"
    // in red is worse than no control.
    submit_write: (args: Record<string, unknown>) => fakeQueuedWrite(args),
    // The estate's open alerts (#444), derived from the same roster so the
    // strip a browser draws agrees with the tab beside it. Added with #446
    // because #444 left it out and the two surfaces it feeds -- the Assets
    // view's strip and the top strip's badge -- were the only ones under
    // `?fake-ipc` answering "command not found" in red. The inbox has its own
    // module since #921 (`inbox.ts`), which carries an `alert` row.
    open_alerts: () => fakeOpenAlerts(),
    // The ack a card offers (#446's write, #449's button). Kept in a set here
    // rather than derived, because *acked* is the one fact on this surface a
    // reader makes themselves — a fixture that answered the same list after
    // the click would make the button look broken.
    ack_alert: (args) => fakeAck(args),
    // The Monitors tab's *Not monitored* roster (#449), derived from the same
    // file: an asset the estate names no Uptime Kuma monitor for is an asset
    // nothing watches, which is exactly what the backend's read answers.
    unmonitored_assets: () => fakeUnmonitored(),
  };
}

keepSeed(FIXTURE_CREATES, FIXTURE_ACKED);
