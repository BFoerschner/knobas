/**
 * The job's pure half (#816): what the job card and the Code section say,
 * computed from what the reads answered, so the rules are testable as values.
 *
 * Nothing here reads a source's payload. The card's type, priority and
 * estimate arrive as `get_entity` fields the backend read through the
 * adapter's declarations (ADR-0007); the blocked chip reads the confirmed
 * links the detail already carries.
 */
import type { AssetStatus } from "../../ipc/assets";
import type { EntityDetail, EntityRow, Reached, Tone } from "../../ipc/entity";
import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";

import type { Week } from "../../ipc/time";
import type { SourceSummary } from "../../ipc/sources";
import { signInFails } from "../../shell/health.svelte";
import { kindRegistry, type KindRegistry } from "../../shell/kind-registry.svelte";
import { protocolDayOf } from "../../standup/protocol";
import { ageLabel, keyOf, minutesLabel } from "../../time/format";
import { minutesOf } from "../../time/week";
import { CI, claims } from "../roles.svelte";
import { jobHash } from "../route";
import { reenterWords, secretNoun } from "../settings/settings";

/** What the job reads of a `list_sources` row: the fields of the wire row it uses, all always sent. */
export type SourceFacts = Pick<SourceSummary, "id" | "display_name" | "config" | "roles" | "auth_kind">;

/** Whether an address segment is an entity id (`jira:PAY-231`) rather than a key. */
export function isEntityId(segment: string): boolean {
  return segment.includes(":");
}

export { keyOf };

/**
 * The ticket a job address names, given how to read one entity.
 *
 * `#/job/<key>` carries the key a person reads (prototype-6's address), and
 * an entity id is `<source>:<key>`, so a bare key is looked for under every
 * configured source, in the order the sources are listed, and the first
 * ticket wins. An address that already holds an id (`jira:PAY-231`) is read
 * as one: the address that tells two sources' PAY-231 apart (#1476). `null`
 * when nothing in the mirror answers to it. The job page reads
 * {@link resolveJob}, which also names the other sources holding the key.
 */
export async function resolveTicket(
  segment: string,
  sources: readonly { id: string }[],
  read: (entityId: string) => Promise<EntityDetail>,
): Promise<EntityDetail | null> {
  const candidates = isEntityId(segment) ? [segment] : sources.map((source) => `${source.id}:${segment}`);
  return (await ticketsOf(candidates, read))[0] ?? null;
}

/** What a job address opens (#1476): its ticket, and the other sources' tickets under the same key. */
export interface ResolvedJob {
  ticket: EntityDetail;
  /** Every other listed source's ticket of that key, in list order; empty when the key is one source's. */
  others: EntityDetail[];
}

/**
 * The ticket a job address names, as {@link resolveTicket} reads it, with
 * every other listed source's ticket under the same key (#1476), so a key two
 * sources hold is never opened silently: a bare key opens the first listed,
 * an id opens itself, and the page names the rest. `null` when nothing
 * answers.
 */
export async function resolveJob(
  segment: string,
  sources: readonly { id: string }[],
  read: (entityId: string) => Promise<EntityDetail>,
): Promise<ResolvedJob | null> {
  const named = isEntityId(segment);
  const key = named ? keyOf(segment) : segment;
  const candidates = sources.map((source) => `${source.id}:${key}`);
  if (named && !candidates.includes(segment)) candidates.unshift(segment);
  const tickets = await ticketsOf(candidates, read);
  const ticket = named ? tickets.find((each) => each.row.entity_id === segment) : tickets[0];
  if (ticket === undefined) return null;
  return { ticket, others: tickets.filter((each) => each !== ticket) };
}

/**
 * Every candidate that answers as a ticket, in order. A source that does not
 * hold the key answers `not_found`, the ordinary miss; anything else (the
 * database down, the bridge gone) is a failure to say as one, never "not in
 * the mirror", when no candidate answered.
 */
async function ticketsOf(candidates: string[], read: (entityId: string) => Promise<EntityDetail>): Promise<EntityDetail[]> {
  const answers = await Promise.allSettled(candidates.map((id) => read(id)));
  const tickets = answers.flatMap((answer) =>
    answer.status === "fulfilled" && answer.value.row.kind === "ticket" ? [answer.value] : [],
  );
  if (tickets.length > 0) return tickets;
  const failed = answers.find(
    (answer): answer is PromiseRejectedResult =>
      answer.status === "rejected" && (answer.reason as { code?: unknown } | null)?.code !== "not_found",
  );
  if (failed) throw failed.reason;
  return [];
}

/**
 * The tickets blocking this one: the other end of every confirmed `blocks`
 * link this ticket is the **`to`** end of (the standup digest's reading of
 * the same word), withdrawn ones left out.
 */
export function blockers(detail: Pick<EntityDetail, "row" | "links">): string[] {
  return detail.links
    .filter(
      (entry) =>
        entry.link.relation === "blocks" &&
        entry.link.to_id === detail.row.entity_id &&
        entry.other.deleted_at === null,
    )
    .map((entry) => keyOf(entry.other.entity_id));
}

/** One ticket blocking this one, for the banner (R-4.8). */
export interface Blocker {
  entityId: string;
  key: string;
  title: string;
  /** The link's note, where somebody wrote why. */
  note: string | null;
}

/** {@link blockers}, with what the banner says about each. */
export function blockerEntries(detail: Pick<EntityDetail, "row" | "links">): Blocker[] {
  return detail.links
    .filter(
      (entry) =>
        entry.link.relation === "blocks" &&
        entry.link.to_id === detail.row.entity_id &&
        entry.other.deleted_at === null,
    )
    .map((entry) => ({
      entityId: entry.other.entity_id,
      key: keyOf(entry.other.entity_id),
      title: entry.other.title,
      note: entry.link.note ?? null,
    }));
}

/** The card's facts line: `Story, High, est. 3h`, each part only where known. */
export function factsLine(detail: Pick<EntityDetail, "issue_type" | "priority" | "estimate_seconds">): string {
  const parts: string[] = [];
  if (detail.issue_type) parts.push(detail.issue_type);
  if (detail.priority) parts.push(detail.priority);
  if (detail.estimate_seconds !== null && detail.estimate_seconds > 0) {
    parts.push(`est. ${minutesLabel(minutesOf(detail.estimate_seconds))}`);
  }
  return parts.join(", ");
}

/** One weekday of *Time this week*. */
export interface WeekDay {
  logged: number;
  unlogged: number;
  today: boolean;
}

/** *Time this week* for one ticket: Monday to Friday and the two totals. */
export interface WeekReading {
  days: WeekDay[];
  logged: number;
  unlogged: number;
}

/**
 * What *Time this week* draws for `entityId`, out of `week_timesheet`.
 *
 * Logged is worklog time sent or waiting to send; *to log* is the tracked
 * time no worklog covers (`unlogged_seconds`), the week's own split. Held
 * worklogs are neither, as on the timesheet.
 */
export function weekReading(week: Week, entityId: string, today: string): WeekReading {
  const row = week.rows.find(
    (candidate) => candidate.target?.kind === "entity" && candidate.target.entity_id === entityId,
  );
  const days = week.days.slice(0, 5).map((day, index) => {
    const cell = row?.cells[index];
    return { logged: cell?.logged_seconds ?? 0, unlogged: cell?.unlogged_seconds ?? 0, today: day === today };
  });
  const sum = (field: "logged_seconds" | "unlogged_seconds") =>
    (row?.cells ?? []).reduce((total, cell) => total + cell[field], 0);
  return { days, logged: sum("logged_seconds"), unlogged: sum("unlogged_seconds") };
}

/** The line under *Time this week*. */
export function weekLine(reading: WeekReading): string {
  const parts: string[] = [];
  if (reading.logged >= 60) parts.push(`${minutesLabel(minutesOf(reading.logged))} logged`);
  if (reading.unlogged >= 60) parts.push(`${minutesLabel(minutesOf(reading.unlogged))} to log`);
  return parts.length === 0 ? "Nothing yet" : parts.join(", ");
}

/** The kinds the Code section draws, in the words of R-4.7. */
export const CODE_KINDS = ["pr", "branch", "commit", "repo"] as const;

/** How loud a row's state is: what asks for you first (R-4.6). */
const SETTLED = 3;
const RANK: Record<Tone, number> = { err: 0, warn: 1, info: 2, ok: SETTLED, muted: SETTLED };

/**
 * A section's rows, sorted by what asks for you, then newest first (R-4.6).
 * A row with no state sorts with the settled ones; a row the source never
 * dated sorts last among its rank.
 */
export function byAttention(rows: readonly EntityRow[]): EntityRow[] {
  const rank = (row: EntityRow) => (row.state ? RANK[row.state.tone] : SETTLED);
  return [...rows].sort(
    (a, b) => rank(a) - rank(b) || (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
  );
}

/** The repositories a set of code rows belongs to, as the mirror names them. */
export function reposOf(rows: readonly EntityRow[]): string[] {
  const names = rows.map((row) => (row.kind === "repo" ? row.title : row.path)).filter((name): name is string => !!name);
  return [...new Set(names)];
}

/** How many open pull requests: a pull request whose state says *open*. */
export function openPrs(rows: readonly EntityRow[]): number {
  return rows.filter((row) => row.kind === "pr" && row.state?.text.toLowerCase() === "open").length;
}

/**
 * The id of the ticket view a Jira view projects to: the projection's own rule
 * (`knobas_sync::jira_views`), so `jira:board-21` is `jira-board-21` and
 * `#/tickets/<slug>` is stable (#1094).
 */
export function jiraViewSlug(entityId: string): string {
  return entityId
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * The run kind a row is routed by while the adapters' claims are not known:
 * the built-in CI adapter's, so a registry that failed to read routes what it
 * routed before #1230. What Builds reads is the mirror's answer (#1295).
 */
const UNKNOWN_RUN_KINDS = ["build"] as const;

/** What routing a run needs of the adapters' claims: the kind registry's `run` frame (#1256). */
export type FrameKinds = Pick<KindRegistry, "frameKinds" | "load">;

/**
 * The CI role's run kinds as the claims stand, the built-in adapter's while
 * they are not known (#1230, #1256). For routing a run's address only: which
 * row opens Builds' run page ({@link itemHash}) and, from the same rule, which
 * section a linked run names (`connections.ts`), where a row arrives with its
 * kind and nothing else. No read and no count uses it: the mirror answers a
 * role's runs and the job's tally says which section counted each line (#1300,
 * ADR-0029 as amended). It goes once a row carries its frame (#1293, out of scope there).
 */
export function runKindsOf(registry: Pick<KindRegistry, "frameKinds">): readonly string[] {
  return registry.frameKinds(CI, "run") ?? UNKNOWN_RUN_KINDS;
}

/**
 * The full view a row opens (R-4.10): the item's own address in the new
 * shell, never a panel. The address carries the entity id, because a pull
 * request's number is unique only inside its repository and a ticket's key
 * only inside its source (#1977). A branch and a
 * commit keep addresses of their own, which send the reader on to the
 * repository page that draws them (#1440). A kind with no view
 * of its own opens the section that lists it: a Jira view its ticket view, a
 * build configuration Builds, a route Dependencies' asset tree, a note its
 * own page (`#/note/<id>`, #1504, which sends the reader to Settings › Notes
 * while notes hide, D10) or, titled by the standup protocol rule, Time ›
 * Standup at its date, anything else its item page, `#/item/<id>`, which draws the source's block
 * view (a custom role's item, #1201, ADR-0029). Never Code: a kind that is not code does
 * not belong there (#1094). Every surface opens a kind through this one rule
 * (#1101), and that includes a run of another kind than `build`: `runKinds`
 * (the CI role's `run` frame from the adapters' claims, #1256) defaults to the
 * shared registry's, so no caller can leave it unasked (#1278).
 */
export function itemHash(row: Pick<EntityRow, "entity_id" | "kind"> & { title?: string | undefined }, runKinds: readonly string[] = runKindsOf(kindRegistry)): string {
  const id = encodeURIComponent(row.entity_id);
  if (runKinds.includes(row.kind)) return `#/build/${id}`;
  switch (row.kind) {
    case "pr":
      return `#/pr/${id}`;
    case "repo":
      return `#/repo/${id}`;
    case "branch":
      return `#/branch/${id}`;
    case "commit":
      return `#/commit/${id}`;
    case "ticket":
      return jobHash(row.entity_id);
    case "build":
      return `#/build/${id}`;
    case "page":
      return `#/page/${id}`;
    case "asset":
      return `#/asset/${id}`;
    case "monitor":
      return "#/monitors";
    case "jira_view":
      return `#/tickets/${jiraViewSlug(row.entity_id)}`;
    case "build_config":
      return "#/builds";
    case "route":
      return `#/route/${id}`;
    case "note": {
      // A protocol is a note its title names (#1504); the title is its whole identity.
      const day = row.title === undefined ? null : protocolDayOf(row.title);
      return day === null ? `#/note/${id}` : `#/time/standup/${day}`;
    }
    default:
      // A kind no surface draws, such as a custom role's (#1201, ADR-0029).
      return `#/item/${id}`;
  }
}

// ---------------------------------------------------------------------------
// The other four sections (#817, R-4.6, R-4.7) and the job header (#818).
// ---------------------------------------------------------------------------

/**
 * The label a row from the parent or a link set carries (R-4.6, R-7.3,
 * #866): "Parent PAY-231", or the set's name, "Payout stack". `null` for a
 * row the ticket reaches itself.
 */
export function originLabel(reached: Pick<Reached, "origin"> | undefined): string | null {
  if (reached?.origin.kind === "parent") return `Parent ${keyOf(reached.origin.ticket)}`;
  if (reached?.origin.kind === "link_set") return reached.origin.name;
  return null;
}

/** How many rows carry `tone`. */
function toned(rows: readonly { state: EntityRow["state"] }[], tone: Tone): number {
  return rows.filter((row) => row.state?.tone === tone).length;
}

/** One coloured count of a rollup: "1 failing" beside a dot of its tone. */
export interface Signal {
  tone: Tone;
  text: string;
}

/** A section's one-line rollup: the lead words and the counts beside it. */
export interface Rollup {
  lead: string;
  signals: Signal[];
}

/**
 * Builds' rollup, prototype-6's words: "7 configurations: 1 failing, 1
 * running, 5 passing". A row is a configuration's latest run, so the count
 * is of configurations. Failing is `err`, running `info`, passing `ok`.
 */
export function buildsRollup(rows: readonly EntityRow[]): Rollup {
  const signals: Signal[] = [];
  const add = (tone: Tone, word: string) => {
    const n = toned(rows, tone);
    if (n > 0) signals.push({ tone, text: `${n} ${word}` });
  };
  add("err", "failing");
  add("info", "running");
  add("ok", "passing");
  return { lead: `${rows.length} ${rows.length === 1 ? "configuration" : "configurations"}:`, signals };
}

/** Monitors' rollup: "4 watched: 1 down, 3 fine". */
export function monitorsRollup(rows: readonly EntityRow[]): Rollup {
  const signals: Signal[] = [];
  const add = (tone: Tone, word: string) => {
    const n = toned(rows, tone);
    if (n > 0) signals.push({ tone, text: `${n} ${word}` });
  };
  add("err", "down");
  add("warn", "warning");
  add("ok", "fine");
  return { lead: `${rows.length} watched:`, signals };
}

/**
 * The space a page sits in: the first part of its path line, which for a
 * Confluence page starts at the space (#284). `null` with no path.
 */
export function spaceOf(row: Pick<EntityRow, "path">): string | null {
  const first = row.path?.split(/ › | \/ /)[0]?.trim();
  return first ? first : null;
}

/** Pages' rollup: "3 pages in ENG, OPS", and how many ask something of you. */
export function pagesRollup(rows: readonly EntityRow[]): Rollup {
  const spaces = [...new Set(rows.map(spaceOf).filter((space): space is string => space !== null))];
  const noun = rows.length === 1 ? "page" : "pages";
  const asking = toned(rows, "warn");
  return {
    lead: spaces.length > 0 ? `${rows.length} ${noun} in ${spaces.join(", ")}` : `${rows.length} ${noun}`,
    signals: asking > 0 ? [{ tone: "warn", text: `${asking} ask something of you` }] : [],
  };
}

/** The tone an asset's health draws in; `none` draws none. */
export function assetTone(health: AssetStatus): Tone | null {
  switch (health) {
    case "down":
      return "err";
    case "warn":
      return "warn";
    case "up":
      return "ok";
    default:
      return null;
  }
}

/** One asset as the section draws it. */
export interface AssetLine {
  id: string;
  name: string;
  type: string;
  environment: string | null;
  /** What holds it, by name. */
  on: string | null;
  tone: Tone | null;
}

/** Assets' rollup: "5 assets in prod, stage", and how many have a problem. */
export function assetsRollup(lines: readonly AssetLine[]): Rollup {
  const envs = [...new Set(lines.map((line) => line.environment).filter((env): env is string => !!env))];
  const noun = lines.length === 1 ? "asset" : "assets";
  const bad = lines.filter((line) => line.tone === "err" || line.tone === "warn").length;
  return {
    lead: envs.length > 0 ? `${lines.length} ${noun} in ${envs.join(", ")}` : `${lines.length} ${noun}`,
    signals: bad > 0 ? [{ tone: "err", text: `${bad} with a problem` }] : [],
  };
}

/** One stale-data warning on the Builds section (R-4.7). */
export interface Staleness {
  sourceId: string;
  text: string;
  /** The fix's words, in the source's noun: *Re-enter password*, *Re-enter token* (#1405). */
  fix: string;
}

/**
 * The Builds section's warnings (R-4.7): every source that holds builds --
 * one that claims the CI role (#1196, ADR-0029), or that a drawn row came
 * from -- whose credential the reader has to fix. "TeamCity data is 38 min
 * old: sign-in failing." The age is since the sign-in started failing, the
 * moment the status bar dates the failure from. The fix is *Re-enter
 * password* (or *token*, the source's noun), on the source's settings page.
 */
export function staleBuilds(
  sources: readonly SourceFacts[],
  health: readonly CredentialHealth[],
  sync: readonly SourceSyncStatus[],
  rows: readonly EntityRow[],
  now: Date,
): Staleness[] {
  // A source whose claims the job was not handed claims nothing.
  const holders = sources.filter(
    (source) =>
      claims({ roles: source.roles }, CI) || rows.some((row) => row.source_id === source.id),
  );
  const out: Staleness[] = [];
  for (const source of holders) {
    const state = health.find((row) => row.source_id === source.id);
    if (!state || !signInFails(state.state)) continue;
    const run = sync.find((row) => row.source_id === source.id);
    const since = run?.last_outcome === "unauthorized" ? run.last_finished_at : state.checked_at;
    const name = source.display_name || source.id;
    const why = state.state === "missing_secret" ? `no ${secretNoun(source.auth_kind)} stored` : "sign-in failing";
    out.push({
      sourceId: source.id,
      text: since ? `${name} data is ${ageLabel(since, now)} old: ${why}.` : `${name} data is stale: ${why}.`,
      fix: reenterWords(source.auth_kind),
    });
  }
  return out;
}

/** The username knobas knows the reader by in a source, or `null`. */
export function username(source: SourceFacts | undefined): string | null {
  const config = source?.config as Record<string, unknown> | null | undefined;
  const name = config?.["username"];
  return typeof name === "string" && name.trim() !== "" ? name.trim() : null;
}

/**
 * Whether the ticket is assigned to the reader: its assignee (a ticket's
 * `author`, as the Jira adapter maps it) is the username its source is
 * configured with -- the rule `mini_board`'s `mine` reads, on the same two
 * values.
 */
export function isMine(
  detail: Pick<EntityDetail, "author" | "row">,
  sources: readonly SourceFacts[],
): boolean {
  const me = username(sources.find((source) => source.id === detail.row.source_id));
  return me !== null && detail.author !== null && detail.author === me;
}

/**
 * Whether the job offers *Start work* (R-4.9): a To Do ticket assigned to the
 * reader, the same test the Tickets preview's button makes.
 */
export function offersStartWork(status: string | null, mine: boolean): boolean {
  return mine && (status?.toLowerCase() ?? "") === "to do";
}

/** One row of a job section, whatever its kind. */
export interface SectionRow {
  id: string;
  /** The item's full view (R-4.10). */
  href: string;
  title: string;
  mono: boolean;
  /** Where it sits: a repository, a space path. Drawn mono before `sub`. */
  repo: string | null;
  sub: string;
  tone: Tone | null;
  chip: { text: string; tone: Tone } | null;
  /** The label of a row from the parent (or, from TF5, a link set). */
  from: string | null;
}

/** Whether a section row matches the in-section filter, ignoring case. */
export function rowMatches(row: SectionRow, text: string): boolean {
  const needle = text.trim().toLowerCase();
  if (needle === "") return true;
  return [row.title, row.repo ?? "", row.sub, row.from ?? "", row.chip?.text ?? ""].some((field) =>
    field.toLowerCase().includes(needle),
  );
}
