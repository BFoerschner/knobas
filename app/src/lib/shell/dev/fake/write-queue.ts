/**
 * `?fake-ipc`: The write queue (#42): one held write, one refused, one waiting, a refused worklog (#828),
 * and a monitor pause refused with a second pause waiting behind it (#1473).
 */
import type { Handler } from "../fake-tauri";

/** Thursday of this week at 14:00, by the browser's clock: the held worklog's day. */
function thisThursday(): string {
  const at = new Date();
  at.setDate(at.getDate() - ((at.getDay() + 6) % 7) + 3);
  at.setHours(14, 0, 0, 0);
  return at.toISOString();
}

/**
 * Prototype-6's held worklog (#828): an hour of PAY-228 on this week's
 * Thursday, refused because the ticket moved on. The Week tab draws its reason
 * and *Open the queue*; the week's numbers (`time.ts`) already hold the hour.
 */
const HELD_WORKLOG = {
  id: 32,
  source_id: "mock",
  entity_id: "mock:PAY-228",
  op: "log_work",
  payload: { LogWork: { entity: "mock:PAY-228", started: thisThursday(), seconds: 3600, comment: "" } },
  target_snapshot: null,
  state: "refused",
  wait_reason: null,
  detail: "Jira refused the worklog because the ticket moved to In Review with time tracking closed",
  queued_at: thisThursday(),
  attempted_at: thisThursday(),
  attempts: 1,
  held_snapshot: null,
  settled_at: null,
  source_enabled: true,
};

/** The monitor both pauses target: a PAY-231 monitor the mirror knows by name. */
const PAUSED_MONITOR = "mock:payouts-stage-health";

/**
 * The live pass's case (#1473): a monitor pause the source refused, and a
 * second pause on the same monitor queued after it. One item's writes go in
 * order, so the second waits for the first to be sent or discarded, and the
 * panel says so rather than promising it goes on its own.
 */
const BLOCKED_PAUSES = [
  {
    id: 33,
    source_id: "mock",
    entity_id: PAUSED_MONITOR,
    op: "pause_monitor",
    payload: { PauseMonitor: { entity: PAUSED_MONITOR } },
    target_snapshot: { op: "pause_monitor", live: true },
    state: "refused",
    wait_reason: null,
    detail: "the monitor is in a maintenance window and cannot be paused",
    queued_at: "2026-08-25T11:30:00Z",
    attempted_at: "2026-08-25T11:31:00Z",
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
  {
    id: 34,
    source_id: "mock",
    entity_id: PAUSED_MONITOR,
    op: "pause_monitor",
    payload: { PauseMonitor: { entity: PAUSED_MONITOR } },
    target_snapshot: { op: "pause_monitor", live: true },
    state: "pending",
    wait_reason: null,
    detail: null,
    queued_at: "2026-08-25T11:48:00Z",
    attempted_at: null,
    attempts: 0,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
];

/**
 * The write queue a QA pass sees: one write held on a changed target, one the
 * source refused, one still waiting on a server that did not answer, the
 * Week tab's refused worklog, and the two pauses of `BLOCKED_PAUSES`.
 *
 * The held row's two snapshots differ by a reply, which is exactly what
 * `knobas_core::write_queue::project` holds a comment against -- so the
 * side-by-side in the panel shows the real thing rather than two copies of one
 * string.
 */
export const FIXTURE_QUEUE = [
  {
    id: 31,
    source_id: "mock",
    entity_id: "mock:PAY-231",
    op: "comment",
    payload: {
      Comment: {
        entity: "mock:PAY-231",
        body: "Taking this — the retry job is dropping the idempotency key.",
      },
    },
    target_snapshot: {
      op: "comment",
      live: true,
      text: "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled.",
    },
    state: "held",
    wait_reason: null,
    detail: null,
    queued_at: "2026-08-25T11:41:00Z",
    attempted_at: "2026-08-25T11:52:00Z",
    attempts: 2,
    held_snapshot: {
      op: "comment",
      live: true,
      text:
        "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled." +
        "\n\njonas.weiss: already on it — it is the idempotency key.",
    },
    settled_at: null,
    source_enabled: true,
  },
  {
    id: 30,
    source_id: "mock",
    entity_id: "mock:PAY-228",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-228", body: "Closing — superseded by PAY-231." } },
    target_snapshot: { op: "comment", live: true, text: "Payout retries pile up" },
    state: "refused",
    wait_reason: null,
    detail: "this issue type does not accept comments",
    queued_at: "2026-08-25T10:12:00Z",
    attempted_at: "2026-08-25T10:12:00Z",
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
  {
    id: 29,
    source_id: "mock",
    entity_id: "mock:PAY-300",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-300", body: "Ledger drift is the same root cause." } },
    target_snapshot: { op: "comment", live: true, text: "Payout ledger drift" },
    state: "pending",
    wait_reason: "unreachable",
    detail: "connection timed out",
    queued_at: "2026-08-25T11:55:00Z",
    attempted_at: "2026-08-25T11:56:00Z",
    attempts: 3,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
  HELD_WORKLOG,
  ...BLOCKED_PAUSES,
];

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    // The write queue (issue #42). Three rows, and the mix is the point: one
    // held write, one refused, one merely waiting -- because the whole surface
    // exists to keep those three apart, and a fixture with one of them cannot
    // show that it does.
    pending_writes: () => FIXTURE_QUEUE,
    write_queue_counts: () => ({
      pending: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
      held: FIXTURE_QUEUE.filter((row) => row.state === "held").length,
      refused: FIXTURE_QUEUE.filter((row) => row.state === "refused").length,
    }),
    // Answered, never performed -- the same rule as the credential in `sources.ts`, and
    // it matters more here: a fixture that appeared to *send* a held write
    // would be showing a reader the one outcome this feature exists to make
    // impossible without their say-so.
    flush_writes: () => null,
    apply_held_write: () => null,
    amend_write: () => null,
    discard_write: () => null,
  };
}
