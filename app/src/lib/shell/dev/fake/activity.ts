/**
 * `?fake-ipc`: The activity log's lines, globally or for one entity.
 */
import type { Handler } from "../fake-tauri";

/** A handful of log lines, so the History panel has something to draw. */
export const FAKE_ACTIVITY = [
  {
    id: 3,
    at: "2026-08-22T14:30:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:PAY-231",
    detail: { upserted: 9 },
  },
  {
    id: 2,
    at: "2026-08-22T11:49:00Z",
    actor: "user",
    verb: "opened",
    entity_id: "mock:PAY-231",
    detail: {},
  },
  {
    id: 1,
    at: "2026-08-22T08:10:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:INC-1",
    detail: { upserted: 1 },
  },
];

/** `recent_activity`, globally or scoped to one entity. */
function recentActivity(args: Record<string, unknown>) {
  const entityId = args["entityId"];
  const limit = Number(args["limit"] ?? 20);
  const lines =
    typeof entityId === "string"
      ? FAKE_ACTIVITY.filter((line) => line.entity_id === entityId)
      : FAKE_ACTIVITY;
  return lines.slice(0, limit);
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    recent_activity: (args) => recentActivity(args),
  };
}
