/**
 * `?fake-ipc`: The inbox and its notification kinds (#834; Settings > Notifications, #831).
 *
 * Answers `inbox_items`, `inbox_count`, `snooze_inbox_item`,
 * `complete_inbox_item`, `notification_kinds`, `set_notification_kinds` and
 * `notify`, which the census used to list as out of the walk's reach.
 *
 * **Shape from prototype-6's `inbox` array**, in its order: a review asked of
 * you, a monitor down, a mention, a failed build, a mention on a pull
 * request, two assignments and a mention on a ticket, and (on the snoozed
 * shelf) the two credential expiries its footer names. Times are minutes
 * before the browser's clock, so "today 13:50" stays today whenever the page
 * is opened.
 *
 * **Where this diverges from prototype-6 and the mock adapter:**
 *
 * - Each `reason` is in the backend's own words (`knobas_core::inbox`: "jonas
 *   asked for your review", "you are mentioned here", "<build> failed",
 *   "assigned to you", "<asset> is down", "the <source> credential expires on
 *   …"), with `detail` the second reading (a build's status, an asset's path),
 *   not prototype-6's richer sentences, which no rule writes (#834): the
 *   screenshot shows what the app will say.
 * - The rows use entities the fixture holds (`corpus.ts`, `job-reach.ts`), so
 *   the build and the pull request are ones PAY-231 reaches.
 * - The alert is on `kuma:3`, the fixture's one unacked open alert
 *   (`monitors.ts`), and its entity is the asset the monitor watches, as the
 *   backend's is. Once `ack_alert` acks it, the stream leaves it out.
 * - `actions` are each category's candidate ops (`Category::candidate_ops`):
 *   `approve` and `comment` on a review request, `comment` on a mention,
 *   `rerun_build` on a failed build, none on an assignment or an alert.
 * - A credential expiry has `entity_id: null`, as in the backend.
 * - A failed sign-in (#1571) is one row per fixture source whose sign-in fails
 *   (`FIXTURE_SOURCES`: TeamCity's 401 and Kuma's missing token), in the
 *   backend's words, so the inbox and the source strip tell one story. Not in
 *   prototype-6, which predates the category.
 * - `notification_kinds` starts with all seven on, as prototype-6's Notifications
 *   page draws it; the backend's default is none ("every kind is off by
 *   default"), so a screenshot of the empty state needs the toggles clicked off.
 * - *Done* removes a row for the session, and a snooze moves it to the
 *   snoozed shelf until `until`. Neither survives a reload. A snooze writes
 *   the item's `snoozed_until` whichever shelf holds it, as the backend's
 *   upsert does (`knobas_core::inbox`), so one whose `until` has passed (*Wake*,
 *   *Undo*) puts the item back in the stream, from the shelf or from *Done*.
 * - `?fake-inbox-items=<n>` appends `n` generated rows after prototype-6's,
 *   a minute apart and cycling the five kinds the chips name, to measure the
 *   Inbox at size (`&fake-inbox-items=4000`).
 */
import type { Handlers } from "../fake-tauri";
import { corpusRow } from "./corpus";
import { sentenceDay } from "../../../tf/inbox/inbox";
import { signInFails } from "../../health.svelte";
import { fakeHiddenMonitors, fakeOpenAlerts } from "./monitors";
import { FIXTURE_SOURCES } from "./sources";
import type { InboxCategory, InboxEntry, InboxShelf } from "../../../ipc/entity";
import type { AuthState } from "../../../ipc/sources";

const KINDS: InboxCategory[] = [
  "review_request",
  "mention",
  "failed_build",
  "new_assignment",
  "credential_expiry",
  "failed_sign_in",
  "alert",
];

/** An RFC 3339 moment `minutes` before the browser's clock. */
function ago(minutes: number): string {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

function entry(
  key: string,
  category: InboxCategory,
  source_id: string,
  entity_id: string | null,
  kind: string | null,
  title: string,
  reason: string,
  minutes: number,
  actions: string[],
  detail?: string,
): InboxEntry {
  return {
    item: {
      key,
      category,
      source_id,
      entity_id,
      kind,
      // The mirror's own title when the entity is in it, so a row cannot
      // name a ticket differently from the board beside it.
      title: corpusRow(entity_id)?.title ?? title,
      reason,
      ...(detail === undefined ? {} : { detail }),
      occurred_at: ago(minutes),
      web_url: null,
      snoozed_until: null,
    },
    actions,
  };
}

/**
 * The fixture's failed sign-ins, as `knobas_core::inbox`'s `failed_sign_in!`
 * words them: one per source whose state is `unauthorized` or `missing_secret`.
 */
function signIns(): InboxEntry[] {
  return FIXTURE_SOURCES.flatMap((source, index) => {
    const state = source.health.state as AuthState;
    if (!signInFails(state)) return [];
    const name = source.display_name;
    const reason = state === "missing_secret" ? `no credential is stored for ${name}` : `the ${name} sign-in was rejected`;
    const minutes = 40 + index * 60;
    return [entry(`failed_sign_in:${source.id}`, "failed_sign_in", source.id, null, null, name, reason, minutes, [])];
  });
}

/** The stream, newest first (prototype-6's order, with the failed sign-ins dated in). */
function stream(): InboxEntry[] {
  return [...signIns(), ...prototype()].sort((a, b) => b.item.occurred_at.localeCompare(a.item.occurred_at));
}

/** Prototype-6's rows, newest first. */
function prototype(): InboxEntry[] {
  return [
    entry("review_request:mock:payout-service#150", "review_request", "mock", "mock:payout-service#150", "pr",
      "Payout CSV export", "jonas asked for your review", 10, ["approve", "comment"]),
    entry("alert:kuma:3", "alert", "kuma", "asset:knobas-jira", "asset",
      "jira", "knobas-jira is down", 80, [], "knobas test estate / DESKTOP-QUUAHUB / Ubuntu 26.04 (WSL 2) / Docker engine (WSL)"),
    entry("mention:confluence:sepa", "mention", "confluence", "confluence:sepa", "page",
      "SEPA retry design", "you are mentioned here", 115, ["comment"]),
    entry("failed_build:teamcity:build:Payout_IntegrationTests-1187", "failed_build", "teamcity", "teamcity:build:Payout_IntegrationTests-1187", "build",
      "Payout_IntegrationTests #1187", "Payout_IntegrationTests #1187 failed", 226, ["rerun_build"], "Tests failed: 3 (3 new), passed: 212"),
    entry("mention:mock:payout-service#142", "mention", "mock", "mock:payout-service#142", "pr",
      "Idempotent retry window", "you are mentioned here", 260, ["comment"]),
    entry("new_assignment:mock:PAY-240", "new_assignment", "mock", "mock:PAY-240", "ticket",
      "Payout dashboard latency", "assigned to you", 338, []),
    entry("new_assignment:mock:PAY-228", "new_assignment", "mock", "mock:PAY-228", "ticket",
      "Idempotency key on the payout endpoint", "assigned to you", 1_290, []),
    entry("mention:mock:PAY-231", "mention", "mock", "mock:PAY-231", "ticket",
      "Retry failed SEPA payouts", "you are mentioned here", 1_380, ["comment"]),
  ];
}

/** The snoozed shelf: prototype-6's footer, back on 2026-08-29. */
function snoozed(): InboxEntry[] {
  const daysOn = (days: number) => new Date(Date.now() + days * 86_400_000);
  const back = daysOn(3).toISOString();
  const row = (source: string, name: string, expires: Date): InboxEntry => {
    const made = entry(`credential_expiry:${source}`, "credential_expiry", source, null, null, name,
      `the ${name} credential expires on ${sentenceDay(expires)}`, 3_000, []);
    made.item.snoozed_until = back;
    return made;
  };
  return [row("jira", "Tidewater Jira", daysOn(9)), row("gitea", "Tidewater Gitea", daysOn(7))];
}

/**
 * Whether an alert row's monitor has been acked or hidden (`monitors.ts` keeps
 * both), as a test made once per read: the alerts are read once, not once per
 * row. A hidden monitor's alert reaches no inbox, as the backend's rule says.
 */
function ackedNow(): (entry: InboxEntry) => boolean {
  const acked = new Set(fakeOpenAlerts().filter((alert) => alert.acked_at !== null).map((alert) => alert.monitor_id));
  for (const id of fakeHiddenMonitors()) acked.add(id);
  return (entry) => entry.item.category === "alert" && acked.has(entry.item.key.slice("alert:".length));
}

/**
 * `?fake-inbox-items=<n>`: `n` generated rows, older than prototype-6's, each
 * about a pull request, ticket, build or monitored asset of its own.
 */
function bulkItems(n: number): InboxEntry[] {
  const made: InboxEntry[] = [];
  for (let i = 0; i < n; i++) {
    const minutes = 2_000 + i;
    switch (i % 5) {
      case 0:
        made.push(entry(`review_request:mock:bulk#${i}`, "review_request", "mock", `mock:bulk#${i}`, "pr", `Bulk change ${i}`, "jonas asked for your review", minutes, ["approve", "comment"]));
        break;
      case 1:
        made.push(entry(`mention:mock:BULK-${i}`, "mention", "mock", `mock:BULK-${i}`, "ticket", `Bulk ticket ${i}`, "you are mentioned here", minutes, ["comment"]));
        break;
      case 2:
        made.push(entry(`failed_build:teamcity:build:Bulk-${i}`, "failed_build", "teamcity", `teamcity:build:Bulk-${i}`, "build", `Bulk #${i}`, `Bulk #${i} failed`, minutes, ["rerun_build"], "Tests failed: 1"));
        break;
      case 3:
        made.push(entry(`new_assignment:mock:BULK-${i}`, "new_assignment", "mock", `mock:BULK-${i}`, "ticket", `Bulk ticket ${i}`, "assigned to you", minutes, []));
        break;
      default:
        made.push(entry(`alert:kuma:bulk-${i}`, "alert", "kuma", `asset:bulk-${i}`, "asset", `bulk-${i}`, `bulk-${i} is down`, minutes, [], "bulk host"));
    }
  }
  return made;
}

/**
 * `?fake-hooks`' `inboxNew` (the still-rows trips, 2026-11-11): an item newer than every other comes in, as a sync
 * brings one, so the inbox reads it on the next `sync:state`.
 */
export let addInboxItem: () => void = () => {};

/** This feature's commands. */
export function handlers(params: URLSearchParams): Handlers {
  let live = [...stream(), ...bulkItems(Number(params.get("fake-inbox-items") ?? 0) || 0)];
  addInboxItem = () => {
    const like = live[0];
    if (!like) return;
    const key = `still-new-${live.length}`;
    live = [{ ...like, item: { ...like.item, key, occurred_at: "2026-12-01T00:00:00Z", title: "An item the sync brought" } }, ...live];
  };
  let later = snoozed();
  /** Items *Done* answered: a snooze brings one back, since the backend's upsert clears `done_at`. */
  let done: InboxEntry[] = [];
  let kinds: InboxCategory[] = KINDS.slice();

  const take = (key: unknown) => {
    const at = live.findIndex((candidate) => candidate.item.key === key);
    if (at < 0) throw { code: "not_found", message: `no inbox item ${String(key)}`, source_id: null };
    return live.splice(at, 1)[0]!;
  };

  return {
    inbox_items: (args) => {
      // On the read and not off the list: *Undo* on an ack brings the row back.
      const acked = ackedNow();
      return (args["shelf"] as InboxShelf) === "snoozed" ? later.slice() : live.filter((candidate) => !acked(candidate));
    },
    inbox_count: () => {
      const acked = ackedNow();
      return live.filter((candidate) => !acked(candidate)).length;
    },
    // Both shelves and the count from one read, as the store asks them.
    inbox_shelves: () => {
      const acked = ackedNow();
      const stream = live.filter((candidate) => !acked(candidate));
      return { stream, snoozed: later.slice(), count: stream.length };
    },
    snooze_inbox_item: (args) => {
      const key = args["itemKey"];
      const moved = later.find((candidate) => candidate.item.key === key) ?? done.find((candidate) => candidate.item.key === key) ?? take(key);
      later = later.filter((candidate) => candidate !== moved);
      done = done.filter((candidate) => candidate !== moved);
      const until = String(args["until"]);
      if (new Date(until).getTime() > Date.now()) {
        moved.item.snoozed_until = until;
        later = [moved, ...later];
      } else {
        moved.item.snoozed_until = null;
        live = [moved, ...live].sort((a, b) => b.item.occurred_at.localeCompare(a.item.occurred_at));
      }
      return null;
    },
    complete_inbox_item: (args) => {
      const key = args["itemKey"];
      const shelved = later.find((candidate) => candidate.item.key === key);
      later = later.filter((candidate) => candidate !== shelved);
      done = [shelved ?? take(key), ...done];
      return null;
    },
    notification_kinds: () => kinds.slice(),
    set_notification_kinds: (args) => {
      const asked = (args["kinds"] as InboxCategory[]).filter((kind) => KINDS.includes(kind));
      kinds = KINDS.filter((kind) => asked.includes(kind));
      return kinds.slice();
    },
    // *Send a test desktop notification*: the browser has no desktop to show it on.
    notify: () => null,
  };
}
