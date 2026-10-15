/**
 * `?fake-ipc`: The inbox and its notification kinds (#834; Settings > Notifications, #831).
 *
 * Answers `inbox_items`, `inbox_count`, `snooze_inbox_item`,
 * `complete_inbox_item`, `notification_kinds`, `set_notification_kinds` and
 * `notify`, which the census used to list as out of the walk's reach.
 *
 * **Shape from prototype-6's `inbox` array**, in its order: a review asked of
 * you, a monitor down, a mention, a failed build, an assignment, and (on the
 * snoozed shelf) the two credential expiries its footer names. Times are
 * minutes before the browser's clock, so "today 13:50" stays today whenever
 * the page is opened.
 *
 * **Where this diverges from prototype-6 and the mock adapter:**
 *
 * - The mirror holds five entities (`corpus.ts`), not prototype-6's forty, so
 *   PR #144 and PAY-240 cannot be the subject of an item; the rows use the
 *   entities that exist (PR #142, PAY-231, PAY-228, OPS-77, INC-1). Their
 *   `reason` text is prototype-6's `what` column, reworded to the backend's
 *   "why this is here" voice.
 * - Prototype-6 draws each row's action buttons by hand (*Re-run*, *Ack*);
 *   the backend sends `actions` as `WriteOp` names the source declares, and
 *   so does this, with only `comment`, `transition` and `re_run` (the three
 *   the screen needs to prove "render exactly these").
 * - A credential expiry has `entity_id: null`, as in the backend.
 * - `notification_kinds` starts with all six on, as prototype-6's Notifications
 *   page draws it; the backend's default is none ("every kind is off by
 *   default"), so a screenshot of the empty state needs the toggles clicked off.
 * - Prototype-6's "Done" removes a row for the session; so does this, and a
 *   snooze moves it to the snoozed shelf until `until`. Neither survives a
 *   reload.
 */
import type { Handler } from "../fake-tauri";
import { CORPUS } from "./corpus";
import type { InboxCategory, InboxEntry, InboxShelf } from "../../../ipc/entity";

const KINDS: InboxCategory[] = [
  "review_request",
  "mention",
  "failed_build",
  "new_assignment",
  "credential_expiry",
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

/** The stream, newest first (prototype-6's order). */
function stream(): InboxEntry[] {
  return [
    entry("review_request:mock:payout-service#142", "review_request", "mock", "mock:payout-service#142", "pr",
      "Add retry classification for TEMP_UNAVAILABLE", "Jonas asked you to review this pull request", 40, ["comment"]),
    entry("alert:mock:INC-1", "alert", "mock", "mock:INC-1", "incident",
      "Payout queue backed up for 40 minutes", "flows-stage is down (HTTP, 1 h 52 m)", 70, []),
    entry("mention:mock:ENG-SEPA", "mention", "mock", "mock:ENG-SEPA", "page",
      "SEPA payout retry design", "Priya mentioned you: “can you add the manual-review queue SLA here?”", 105, ["comment"]),
    entry("failed_build:mock:payout-service#318", "failed_build", "mock", "mock:payout-service#318", "build",
      "payout-service #318", "Build #318 failed on feature/PAY-231-sepa-retry", 235, ["re_run"]),
    entry("mention:mock:payout-service#142", "mention", "mock", "mock:payout-service#142", "pr",
      "Add retry classification for TEMP_UNAVAILABLE", "Jonas commented on your pull request: “Should the jitter be bounded? ±20 % feels wide.”", 270, ["comment"]),
    entry("new_assignment:mock:PAY-198", "new_assignment", "mock", "mock:PAY-198", "ticket",
      "Reconcile ledger drift on partial refunds", "Priya assigned this ticket to you", 330, ["comment", "transition"]),
    entry("new_assignment:mock:PAY-228", "new_assignment", "mock", "mock:PAY-228", "ticket",
      "Fix ledger drift on partial refunds", "PAY-228 is blocked by OPS-77 On-call runbook outdated", 1_290, ["comment", "transition"]),
    entry("mention:mock:PAY-231", "mention", "mock", "mock:PAY-231", "ticket",
      "Retry failed SEPA payouts", "Lena mentioned you: “will the manual review queue get an SLA?”", 1_380, ["comment"]),
  ];
}

/** The snoozed shelf: prototype-6's footer, back on 2026-08-29. */
function snoozed(): InboxEntry[] {
  const back = "2026-08-29T07:00:00Z";
  const row = (key: string, title: string, reason: string, source: string): InboxEntry => {
    const made = entry(key, "credential_expiry", source, null, null, title, reason, 3_000, []);
    made.item.snoozed_until = back;
    return made;
  };
  return [
    row("credential_expiry:jira", "Jira PAT", "The Jira token expires in 12 days", "jira"),
    row("credential_expiry:gitea", "Payouts certificate", "The payouts certificate expires in 9 days", "gitea"),
  ];
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  let live = stream();
  let later = snoozed();
  let kinds: InboxCategory[] = KINDS.slice();

  const take = (key: unknown) => {
    const at = live.findIndex((candidate) => candidate.item.key === key);
    if (at < 0) throw { code: "not_found", message: `no inbox item ${String(key)}`, source_id: null };
    return live.splice(at, 1)[0]!;
  };

  return {
    inbox_items: (args) => ((args["shelf"] as InboxShelf) === "snoozed" ? later.slice() : live.slice()),
    inbox_count: () => live.length,
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
    // *Send a test notification*: the browser has no desktop to show it on.
    notify: () => null,
  };
}
