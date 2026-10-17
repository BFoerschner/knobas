/**
 * Settings in the ticket-first shell (#830): its sections, its addresses, and
 * how a source reads on the Sources list and on a source's own page, in
 * prototype-6's words.
 *
 * Pure. The components hand in what the IPC answered and draw what comes
 * back, so every sentence on these pages can be tested as a string.
 */
import type { AuthMethod, CredentialHealth, SourceSummary, SourceSyncStatus, SyncRunRow } from "../../ipc/sources";
import { ago } from "../../shell/time";
import { IDENTITY_FIELD } from "../../sources/new-source";

/** A section of Settings, in prototype-6's order (`STG`). */
export type SettingsSection =
  | "sources"
  | "checkouts"
  | "time"
  | "notifications"
  | "standup"
  | "monitoring"
  | "notes"
  | "backup"
  | "appearance"
  | "keyboard"
  | "logs";

/** The left-hand navigation, top to bottom, with prototype-6's names. */
export const SETTINGS_SECTIONS: readonly (readonly [SettingsSection, string])[] = [
  ["sources", "Sources"],
  ["checkouts", "Checkouts and Open in…"],
  ["time", "Time"],
  ["notifications", "Notifications"],
  ["standup", "Standup"],
  ["monitoring", "Monitoring"],
  ["notes", "Notes and capture"],
  ["backup", "Backup and export"],
  ["appearance", "Appearance"],
  ["keyboard", "Keyboard"],
  ["logs", "Logs and database"],
];

/** Where in Settings the reader is: a section, or one source's page. */
export type SettingsPlace =
  | { section: SettingsSection }
  | { section: "sources"; source: string; add?: ListAddition };

/** A value an address adds to a list field of a source's config, unsaved (#879). */
export interface ListAddition {
  field: string;
  value: string;
}

/**
 * Read what follows `#/settings`. `source/<id>` is a source's page, which
 * lights Sources in the navigation; an unknown section is Sources, as in the
 * prototype (`STGV[sec] || STGV.sources`). `source/<id>/add/<field>/<value>`
 * is that page with `value` added to its list field `field`, unsaved: a Jira
 * view's *Sync WEB too* (#879, R-3.23).
 */
export function settingsPlace(rest: readonly string[]): SettingsPlace {
  const [head, arg, verb, field, value] = rest;
  if (head === "source" && arg && verb === "add" && field && value)
    return { section: "sources", source: arg, add: { field, value } };
  if (head === "source" && arg) return { section: "sources", source: arg };
  const known = SETTINGS_SECTIONS.find(([key]) => key === head);
  return { section: known ? known[0] : "sources" };
}

/** The address of a source's page; with `add`, the page with that value added to a list field (#879). */
export function sourceHash(id: string, add?: ListAddition): string {
  const page = `#/settings/source/${encodeURIComponent(id)}`;
  return add ? `${page}/add/${encodeURIComponent(add.field)}/${encodeURIComponent(add.value)}` : page;
}

/** A state dot's tone. */
export type Tone = "ok" | "warn" | "err" | "run" | "muted";

/** What one source reads as, on its row and at the top of its page. */
export interface SourceState {
  tone: Tone;
  /** The row's last column: *Synced 4 min ago*, *Sign-in failed*. */
  text: string;
  /** The credential needs the reader: err tone and *Re-enter password*. */
  failed: boolean;
}

/** Credential states the reader has to fix, and what the row calls them. */
const FAILED: Partial<Record<CredentialHealth["state"], string>> = {
  unauthorized: "Sign-in failed",
  missing_secret: "No password stored",
};

export function sourceState(
  source: Pick<SourceSummary, "enabled" | "last_run">,
  health: CredentialHealth | null,
  status: SourceSyncStatus | null,
  now: Date = new Date(),
): SourceState {
  const failure = health ? FAILED[health.state] : undefined;
  if (failure) return { tone: "err", text: failure, failed: true };
  if (status?.running) return { tone: "run", text: "Syncing…", failed: false };
  if (!source.enabled) return { tone: "muted", text: "Paused", failed: false };
  const finished = status?.last_finished_at ?? source.last_run?.finished_at ?? null;
  if (!finished) return { tone: "muted", text: "Not synced yet", failed: false };
  if (health?.state === "unreachable") {
    return { tone: "warn", text: `Unreachable, synced ${ago(finished, now)}`, failed: false };
  }
  const outcome = status?.last_outcome ?? source.last_run?.outcome ?? null;
  if (outcome === "error") return { tone: "warn", text: `Failed ${ago(finished, now)}`, failed: false };
  return { tone: "ok", text: `Synced ${ago(finished, now)}`, failed: false };
}

/**
 * What a source holds: *213 tickets* when it brings one kind, *213 items*
 * when it brings several. The count is one number across kinds, so a split
 * by kind would be a guess.
 */
export function holdings(source: Pick<SourceSummary, "item_count" | "kinds">): string {
  const n = source.item_count.toLocaleString("en-GB");
  const only = source.kinds.length === 1 ? source.kinds[0]! : null;
  if (only) return `${n} ${(source.item_count === 1 ? only.label : only.plural).toLowerCase()}`;
  return `${n} ${source.item_count === 1 ? "item" : "items"}`;
}

/** The address as the row shows it: without its scheme. */
export function bareAddress(url: string): string {
  return url.replace(/^[a-z]+:\/\//i, "").replace(/\/$/, "");
}

/** An interval in prototype-6's words: *1 min*, *15 min*, *1 hour*. */
export function intervalWords(secs: number): string {
  if (secs % 3600 === 0) return secs === 3600 ? "1 hour" : `${secs / 3600} hours`;
  if (secs % 60 === 0) return `${secs / 60} min`;
  return `${secs} s`;
}

/** The schedules *Every* offers. The backend refuses anything below a minute. */
export const INTERVALS: readonly number[] = [60, 300, 900, 1800, 3600];

/** *Every*'s options: the offered ones, plus the stored one if it is not among them. */
export function intervalOptions(current: number): number[] {
  return INTERVALS.includes(current) ? [...INTERVALS] : [...INTERVALS, current].sort((a, b) => a - b);
}

/**
 * The line under a source's name: *Jira Data Center, 177 tickets. Syncs every
 * 5 min, last 4 min ago.*
 */
export function sourceLine(
  source: Pick<SourceSummary, "item_count" | "kinds" | "enabled" | "sync_interval_secs" | "last_run">,
  adapterName: string,
  status: SourceSyncStatus | null,
  now: Date = new Date(),
): string {
  const head = `${adapterName}, ${holdings(source)}.`;
  const finished = status?.last_finished_at ?? source.last_run?.finished_at ?? null;
  const last = finished ? `last ${ago(finished, now)}` : "not synced yet";
  if (!source.enabled) return `${head} Paused, ${last}.`;
  return `${head} Syncs every ${intervalWords(source.sync_interval_secs)}, ${last}.`;
}

/** The alert on top of a source's page, or `null` when its credential works. */
export function credentialAlert(
  name: string,
  word: "Password" | "Token",
  health: CredentialHealth | null,
  status: SourceSyncStatus | null,
  now: Date = new Date(),
): string | null {
  if (health?.state === "unauthorized") {
    const at = status?.last_outcome === "unauthorized" ? status.last_finished_at : health.checked_at;
    return `Sign-in failed ${ago(at, now)} (401). Writes to ${name} wait in the queue until you re-enter the ${word.toLowerCase()}.`;
  }
  if (health?.state === "missing_secret") {
    return `No ${word.toLowerCase()} is stored for ${name}. Writes to ${name} wait in the queue until you enter one.`;
  }
  return null;
}

/** What the credential is called on the Password row and in its dialog. */
export function secretWord(auth: AuthMethod | null): "Password" | "Token" {
  return auth === "UserPassword" || auth === null ? "Password" : "Token";
}

/** One row of *Sync runs*. */
export interface RunRow {
  id: number;
  started: string;
  took: string;
  result: string;
  tone: "ok" | "err" | "run";
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function hhmm(at: Date): string {
  return `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
}

/** *14:28* today, *12 Oct 14:28* before. */
export function startedWords(iso: string, now: Date = new Date()): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "—";
  return at.toDateString() === now.toDateString() ? hhmm(at) : `${at.getDate()} ${MONTHS[at.getMonth()]} ${hhmm(at)}`;
}

/** *4 s*, *3 min 12 s*. */
export function tookWords(ms: number): string {
  const secs = Math.max(0, Math.round(ms / 1000));
  if (secs < 60) return `${secs} s`;
  const rest = secs % 60;
  return rest === 0 ? `${Math.floor(secs / 60)} min` : `${Math.floor(secs / 60)} min ${rest} s`;
}

/** Why a run happened, where it says more than *updated*. */
const TRIGGER_NOTE: Partial<Record<SyncRunRow["trigger"], string>> = {
  first_run: ", first sync",
  backfill: ", backfill",
};

export function runRow(run: SyncRunRow, now: Date = new Date()): RunRow {
  const started = startedWords(run.started_at, now);
  if (run.finished_at === null || run.outcome === null) {
    return { id: run.id, started, took: "—", result: "Running…", tone: "run" };
  }
  const took = tookWords(new Date(run.finished_at).getTime() - new Date(run.started_at).getTime());
  if (run.outcome === "unauthorized") {
    return { id: run.id, started, took, result: `${run.error ?? "401 Unauthorized"}. Re-enter the password.`, tone: "err" };
  }
  if (run.outcome !== "ok") {
    return { id: run.id, started, took, result: run.error ?? (run.outcome === "unreachable" ? "Unreachable" : "Failed"), tone: "err" };
  }
  const parts = [`${run.upserted.toLocaleString("en-GB")} updated`];
  if (run.deleted + run.swept > 0) parts.push(`${(run.deleted + run.swept).toLocaleString("en-GB")} removed`);
  return { id: run.id, started, took, result: parts.join(", ") + (TRIGGER_NOTE[run.trigger] ?? ""), tone: "ok" };
}

/**
 * The config property a source keeps its user in, by the convention every
 * adapter follows (`sources/new-source.ts`): `username`. Keyed on the name so
 * this page holds no table of adapters.
 */
export { IDENTITY_FIELD };

/** Whether an adapter's config schema declares the identity field. */
export function declaresUser(schema: unknown): boolean {
  if (typeof schema !== "object" || schema === null) return false;
  const properties = (schema as { properties?: unknown }).properties;
  return typeof properties === "object" && properties !== null && IDENTITY_FIELD in properties;
}

/** The user stored in a source's config, or `""`. */
export function userOf(config: unknown): string {
  if (typeof config !== "object" || config === null) return "";
  const value = (config as Record<string, unknown>)[IDENTITY_FIELD];
  return typeof value === "string" ? value : "";
}
