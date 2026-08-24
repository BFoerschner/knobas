/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** One line of the activity log — `knobas_core::activity::ActivityRow`. */
export interface ActivityRow {
  id: number;
  /** RFC 3339 timestamp. */
  at: string;
  /** `"user"`, or `"sync:<source_id>"`. */
  actor: string;
  verb: string;
  entity_id: string | null;
  /** Free-form jsonb; always an object, never a JSON null. */
  detail: Record<string, unknown>;
}

/** The `limit` most recent activity lines, newest first. */
export function recentActivity(limit: number): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit });
}
