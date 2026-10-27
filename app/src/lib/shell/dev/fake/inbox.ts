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
 *   asked for your review", "you are mentioned here", "this build failed: …",
 *   "assigned to you", "<asset path> is down", "the <source> credential
 *   expires on …"), not prototype-6's richer sentences, which no rule writes
 *   (#834): the screenshot shows what the app will say.
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
 *   snoozed shelf until `until`. Neither survives a reload.
 * - `?fake-inbox-items=<n>` appends `n` generated rows after prototype-6's,
 *   a minute apart and cycling the five kinds the chips name, to measure the
 *   Inbox at size (`&fake-inbox-items=4000`).
 */
import type { Handlers } from "../fake-tauri";
import { CORPUS } from "./corpus";
import { signInFails } from "../../health.svelte";
import { fakeOpenAlerts } from "./monitors";
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
      title: CORPUS.find((row) => row.entity_id === entity_id)?.title ?? title,
      reason,
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
      "jira", "knobas test estate / devs-MacBook-Pro / OrbStack Linux VM / Docker engine (OrbStack) / knobas-jira is down", 80, []),
    entry("mention:confluence:sepa", "mention", "confluence", "confluence:sepa", "page",
      "SEPA retry design", "you are mentioned here", 115, ["comment"]),
    entry("failed_build:teamcity:build:Payout_IntegrationTests-1187", "failed_build", "teamcity", "teamcity:build:Payout_IntegrationTests-1187", "build",
      "Payout_IntegrationTests #1187", "this build failed: Tests failed: 3 (3 new), passed: 212", 226, ["rerun_build"]),
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
  const back = "2026-08-29T07:00:00Z";
  const row = (source: string, name: string, expires: string): InboxEntry => {
    const made = entry(`credential_expiry:${source}`, "credential_expiry", source, null, null, name,
      `the ${name} credential expires on ${expires}`, 3_000, []);
    made.item.snoozed_until = back;
    return made;
  };
  return [row("jira", "Tidewater Jira", "Thursday 3 Sep 2026"), row("gitea", "Tidewater Gitea", "Monday 31 Aug 2026")];
}

/**
 * Whether an alert row's monitor has been acked (`monitors.ts` keeps the ack),
 * as a test made once per read: the alerts are read once, not once per row.
 */
function ackedNow(): (entry: InboxEntry) => boolean {
  const acked = new Set(fakeOpenAlerts().filter((alert) => alert.acked_at !== null).map((alert) => alert.monitor_id));
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
        made.push(entry(`failed_build:teamcity:build:Bulk-${i}`, "failed_build", "teamcity", `teamcity:build:Bulk-${i}`, "build", `Bulk #${i}`, "this build failed: Tests failed: 1", minutes, ["rerun_build"]));
        break;
      case 3:
        made.push(entry(`new_assignment:mock:BULK-${i}`, "new_assignment", "mock", `mock:BULK-${i}`, "ticket", `Bulk ticket ${i}`, "assigned to you", minutes, []));
        break;
      default:
        made.push(entry(`alert:kuma:bulk-${i}`, "alert", "kuma", `asset:bulk-${i}`, "asset", `bulk-${i}`, `bulk host ${i} is down`, minutes, []));
    }
  }
  return made;
}

/** This feature's commands. */
export function handlers(params: URLSearchParams): Handlers {
  let live = [...stream(), ...bulkItems(Number(params.get("fake-inbox-items") ?? 0) || 0)];
  let later = snoozed();
  let kinds: InboxCategory[] = KINDS.slice();

  const take = (key: unknown) => {
    const at = live.findIndex((candidate) => candidate.item.key === key);
    if (at < 0) throw { code: "not_found", message: `no inbox item ${String(key)}`, source_id: null };
    return live.splice(at, 1)[0]!;
  };

  return {
    inbox_items: (args) => {
      const acked = ackedNow();
      live = live.filter((candidate) => !acked(candidate));
      return (args["shelf"] as InboxShelf) === "snoozed" ? later.slice() : live.slice();
    },
    inbox_count: () => {
      const acked = ackedNow();
      return live.filter((candidate) => !acked(candidate)).length;
    },
    snooze_inbox_item: (args) => {
      const moved = take(args["itemKey"]);
      moved.item.snoozed_until = String(args["until"]);
      later = [moved, ...later];
      return null;
    },
    complete_inbox_item: (args) => {
      const key = args["itemKey"];
      if (later.some((candidate) => candidate.item.key === key)) {
        later = later.filter((candidate) => candidate.item.key !== key);
        return null;
      }
      take(key);
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
