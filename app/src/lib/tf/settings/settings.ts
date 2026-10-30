/**
 * Settings in the ticket-first shell (#830): its sections, its addresses, and
 * how a source reads on the Sources list and on a source's own page, in
 * prototype-6's words.
 *
 * Pure. The components hand in what the IPC answered and draw what comes
 * back, so every sentence on these pages can be tested as a string.
 */
import type { AuthMethod, CredentialHealth, SourceSummary, SourceSyncStatus, SyncRunRow } from "../../ipc/sources";
import type { ExpiryNote } from "../../shell/health.svelte";
import { ago } from "../../shell/time";
import { secondsLabel, stampLabel } from "../../time/format";
import { CI, CODE_HOST, MONITORING, TICKET_SYSTEM, WIKI } from "../roles.svelte";
import { IDENTITY_FIELD } from "../../sources/new-source";
import type { Walk } from "../../sources/walk";

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
  | "performance"
  | "renderers"
  | "keyboard"
  | "logs";

/** The left-hand navigation, top to bottom, with prototype-6's names. */
export const SETTINGS_SECTIONS: readonly (readonly [SettingsSection, string])[] = [
  ["sources", "Sources"],
  ["checkouts", "Checkouts and Open in…"],
  ["time", "Time"],
  ["notifications", "Desktop notifications"],
  ["standup", "Standup"],
  ["monitoring", "Monitoring"],
  ["notes", "Notes and capture"],
  ["backup", "Backup and export"],
  ["appearance", "Appearance"],
  ["performance", "Performance"],
  ["renderers", "Renderers"],
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

/** The address of a section of Settings. */
export function settingsHash(section: SettingsSection): string {
  return `#/settings/${section}`;
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
  /** The credential needs the reader: err tone and *Re-enter password* (or *token*). */
  failed: boolean;
}

/** Credential states the reader has to fix, and what the row calls them, in the source's noun. */
const FAILED: Partial<Record<CredentialHealth["state"], (noun: string) => string>> = {
  unauthorized: () => "Sign-in failed",
  missing_secret: (noun) => `No ${noun} stored`,
};

/**
 * The limit an external adapter's call passed, read from the host's fixed
 * wording ("... was stopped at its deadline of 600 s" / "its memory cap of
 * 64 MiB", `Exceeded` in knobas-wasm-host's limits.rs). Null for any other error.
 */
export function stoppedAt(error: string | null | undefined): string | null {
  const m = /was stopped at (its (?:deadline|memory cap) of \d+ (?:s|MiB))/.exec(error ?? "");
  return m ? m[1]! : null;
}

export function sourceState(
  source: Pick<SourceSummary, "enabled" | "last_run" | "role_misses" | "auth_kind">,
  health: CredentialHealth | null,
  status: SourceSyncStatus | null,
  now: Date = new Date(),
): SourceState {
  const failure = health ? FAILED[health.state] : undefined;
  if (failure) return { tone: "err", text: failure(secretNoun(source.auth_kind)), failed: true };
  if (status?.running) return { tone: "run", text: "Syncing…", failed: false };
  if (!source.enabled) return { tone: "muted", text: "Paused", failed: false };
  const finished = status?.last_finished_at ?? source.last_run?.finished_at ?? null;
  if (!finished) return { tone: "muted", text: "Not synced yet", failed: false };
  if (health?.state === "unreachable") {
    return { tone: "warn", text: `Unreachable, synced ${ago(finished, now)}`, failed: false };
  }
  const outcome = status?.last_outcome ?? source.last_run?.outcome ?? null;
  if (outcome === "error") {
    const limit = stoppedAt(source.last_run?.error);
    const text = `Failed ${ago(finished, now)}`;
    return { tone: "warn", text: limit ? `${text}, stopped at ${limit}` : text, failed: false };
  }
  const misses = source.role_misses;
  if (misses.length > 0) {
    const [first] = misses;
    const more = misses.length > 1 ? ` and ${misses.length - 1} more` : "";
    return {
      tone: "warn",
      text: `Synced ${ago(finished, now)}, ${roleWord(first!.role)} not drawn (${first!.path})${more}`,
      failed: false,
    };
  }
  return { tone: "ok", text: `Synced ${ago(finished, now)}`, failed: false };
}

/** How the health line names each built-in role: on the row, and in *Not drawn …*. */
const ROLE_WORDS: Record<string, { word: string; as: string }> = {
  [CODE_HOST]: { word: "code host", as: "as a code host" },
  [TICKET_SYSTEM]: { word: "ticket system", as: "as a ticket system" },
  [CI]: { word: "CI", as: "as CI" },
  [WIKI]: { word: "wiki", as: "as a wiki" },
  [MONITORING]: { word: "monitoring", as: "for monitoring" },
};

/** A role as the health line says it: *ticket system*, *CI*. A custom role keeps its own name. */
export function roleWord(role: string): string {
  return ROLE_WORDS[role]?.word ?? role;
}

/**
 * The lines on top of a source's page for the roles its records did not fill
 * (#1199, ADR-0028): which surface leaves it out, and the declared path that
 * missed. *Not drawn as a ticket system: no ticket resolves priority at
 * fields.priority.name. Its tickets stay searchable and linkable.*
 */
export function roleMissLines(source: Pick<SourceSummary, "kinds" | "role_misses">): string[] {
  return source.role_misses.map((miss) => {
    const kind = source.kinds.find((info) => info.id === miss.kind);
    const one = (kind?.label ?? miss.kind).toLowerCase();
    const many = (kind?.plural ?? miss.kind).toLowerCase();
    const as = ROLE_WORDS[miss.role]?.as ?? `as ${miss.role}`;
    return `Not drawn ${as}: no ${one} resolves ${miss.field} at ${miss.path}. Its ${many} stay searchable and linkable.`;
  });
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

/**
 * The class a credential expiry reading draws in (#1499): plain in the row's
 * own grey, amber in the warning tone, expired in the failure tone.
 */
export const EXPIRY_CLASS: Record<ExpiryNote["tone"], string> = { plain: "", amber: "tf-stg-warnt", fail: "tf-stg-errt" };

/**
 * The line under *Fetch everything again* on a source's page. A backfill runs
 * with no stored position and keeps the one it ends on, so besides reading
 * fields a knobas update added it is the way out of a sync that stopped on a
 * position the server no longer has: TeamCity's refusal of a deleted newest
 * build names it (#1477). There is no separate cursor reset.
 */
export const BACKFILL_HINT =
  "Reads everything again and forgets where the last sync stopped: after a knobas update reads more fields, or when a sync stopped and names Backfill. Takes a few minutes; the app stays usable.";

/**
 * The schedules *Every* can offer (#1188). A source is offered the steps at or
 * above its adapter's floor (`SourceDescriptor.min_sync_interval_secs`), which
 * is also what the backend refuses under.
 */
export const INTERVALS: readonly number[] = [20, 30, 45, 60, 300, 900, 1800, 3600];

/**
 * *Every*'s options: the steps at or above `floor`, plus the stored one if it
 * is not among them. The floor is the adapter's declaration, never a table of
 * kinds here.
 */
export function intervalOptions(current: number, floor: number): number[] {
  const offered = INTERVALS.filter((secs) => secs >= floor);
  return offered.includes(current) ? offered : [...offered, current].sort((a, b) => a - b);
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

/**
 * What the credential is called on the Password row and in its dialog, and
 * the one place every *Re-enter …* and *no … stored* takes its noun from (#1405).
 */
export function secretWord(auth: AuthMethod | null | undefined): "Password" | "Token" {
  return auth === "UserPassword" || auth === null || auth === undefined ? "Password" : "Token";
}

/** The same word inside a sentence: *no token stored*, *re-enter the password*. */
export function secretNoun(auth: AuthMethod | null | undefined): "password" | "token" {
  return secretWord(auth) === "Password" ? "password" : "token";
}

/** The fix for a credential the reader has to re-enter: *Re-enter password*, *Re-enter token*. */
export function reenterWords(auth: AuthMethod | null | undefined): string {
  return `Re-enter ${secretNoun(auth)}`;
}

/** One row of *Sync runs*. */
export interface RunRow {
  id: number;
  started: string;
  took: string;
  result: string;
  /** What the source said, behind *Details*, where the result is a sentence of knobas's own (#1478). */
  detail: string | null;
  tone: "ok" | "err" | "run";
}

/** *14:28* today, else *yesterday 14:28* or *2026-10-12 14:28* (`stampLabel`, #1986). */
export function startedWords(iso: string, now: Date = new Date()): string {
  return stampLabel(iso, now, true) || "—";
}

/**
 * A watched run's line (#1384), the wizard's reading on a source's page:
 * *120 of 300 repositories · 1,240 items · 83 s*, without the walk until the
 * run reports one and without the total when the adapter has none.
 */
export function runProgressWords(items: number, elapsedMs: number, walk: Walk | null): string {
  const parts = [`${items.toLocaleString("en-GB")} items`, `${Math.round(elapsedMs / 1000)} s`];
  if (walk !== null) {
    const of = walk.of === undefined ? "" : ` of ${walk.of.toLocaleString("en-GB")}`;
    parts.unshift(`${walk.walked.toLocaleString("en-GB")}${of} ${walk.unit}`.trimEnd());
  }
  return parts.join(" · ");
}

/** Why a run happened, where it says more than *updated*. */
const TRIGGER_NOTE: Partial<Record<SyncRunRow["trigger"], string>> = {
  first_run: ", first sync",
  backfill: ", backfill",
};

/**
 * One row of *Sync runs*, for the source it ran on. A refused run names the
 * source's credential (#1405). An unreachable run says the source did not
 * answer, and keeps the transport error (the HTTP client's message with the
 * whole request URL) as its detail rather than its sentence (#1478).
 */
export function runRow(
  run: SyncRunRow,
  source: Pick<SourceSummary, "id" | "display_name" | "auth_kind">,
  now: Date = new Date(),
): RunRow {
  const started = startedWords(run.started_at, now);
  const row = { id: run.id, started, detail: null };
  if (run.finished_at === null || run.outcome === null) {
    return { ...row, took: "—", result: "Running…", tone: "run" };
  }
  const took = secondsLabel(Math.max(0, Math.round((new Date(run.finished_at).getTime() - new Date(run.started_at).getTime()) / 1000)));
  if (run.outcome === "unauthorized") {
    return { ...row, took, result: `${run.error ?? "401 Unauthorized"}. Re-enter the ${secretNoun(source.auth_kind)}.`, tone: "err" };
  }
  if (run.outcome === "unreachable") {
    const name = source.display_name || source.id;
    return { ...row, took, result: `${name} did not answer.`, detail: run.error, tone: "err" };
  }
  if (run.outcome !== "ok") {
    return { ...row, took, result: run.error ?? "Failed", tone: "err" };
  }
  const parts = [`${run.upserted.toLocaleString("en-GB")} updated`];
  if (run.deleted + run.swept > 0) parts.push(`${(run.deleted + run.swept).toLocaleString("en-GB")} removed`);
  return { ...row, took, result: parts.join(", ") + (TRIGGER_NOTE[run.trigger] ?? ""), tone: "ok" };
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
