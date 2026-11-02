/**
 * The Tab sheet's actions (#2426, spec #2419 "The sheet"): one table, per
 * kind, of what a row can have done to it from the box.
 *
 * Every row is data. `does` names an address of the app or a write it has
 * today, and `Launcher.svelte` carries it out in one place (`perform`), so
 * this table can be read and tested without a DOM, a later action key
 * (#2429) binds to an `id`, and a flow (#2427) is one more `does` kind.
 *
 * The order is the spec's: the pair with the chosen job first (*Link to
 * ⟨job⟩*), *Start timer* on a ticket, then what the kind can do, then *Copy
 * key* or *Copy title*. This reverses the 2026-10-21 ruling on #1416 (the
 * two-row chain and the hand-off to the job's dialog); see
 * `docs/decisions/2026-11-02-launcher-sheet.md`.
 *
 * Adapter-declared source actions join under the same rule: a row here whose
 * `does` is a `write` of the op the source declares. Today's are the ones
 * every page already sends (`Approve`, `TriggerBuild`, `RerunBuild`).
 */
import type { InboxItem } from "../ipc/entity";
import type { WriteOpPayload } from "../ipc/sources";
import { canBeTarget } from "../shell/timer";
import { entityAddress } from "../tf/address";
import { opener, rowActions } from "../tf/inbox/inbox";
import { prHash } from "../tf/pr/pr";
import { repoHash } from "../tf/repo/repo";
import { jobHash } from "../tf/route";
import { keyOf } from "../time/format";
import type { LauncherRow } from "./rows";

/** What a sheet acts on: an entity, or one of knobas' own rows. */
export type SheetSubject =
  | { kind: "entity"; entityId: string; entityKind: string; title: string; sourceId: string; path: string | null }
  /** A saved list (#2423): `text` is what it applies; `refusal` is why it cannot, when it cannot. */
  | { kind: "list"; id: string; label: string; text: string; refusal: string | null }
  | { kind: "view"; id: string; title: string }
  | { kind: "set"; id: string; title: string }
  /** An inbox line, with the source ops the backend offered on it (`InboxEntry.actions`). */
  | { kind: "inbox"; item: InboxItem; ops: readonly string[] };

/** What choosing an action does. `Launcher.svelte`'s `perform` is the one reader. */
export type SheetEffect =
  | { kind: "go"; hash: string }
  /** Write the link to the chosen job with the relation picked inline. */
  | { kind: "link" }
  | { kind: "timer" }
  /** A write queued to the source; `said` is the toast. */
  | { kind: "write"; payload: WriteOpPayload; said: string }
  | { kind: "ack"; monitorId: string; said: string }
  | { kind: "copy"; text: string; said: string }
  /** `open_checkout` with one application from Settings › Checkouts. */
  | { kind: "open-in"; application: string; name: string }
  | { kind: "clone" }
  | { kind: "apply" }
  /** The box takes the list's new name. */
  | { kind: "rename" }
  | { kind: "delete" }
  /** The inbox page's own writes on the line. */
  | { kind: "inbox"; op: "ack" | "done" | "rerun" };

export interface SheetAction {
  /** Stable per kind: what an action key (#2429) is kept under. */
  id: string;
  label: string;
  /** The second line: what it does. Narrowed together with the label. */
  detail: string;
  does: SheetEffect;
}

export interface SheetContext {
  /** The chosen job (`breadcrumbJob`), the pair's other end; `null` when none is chosen. */
  job: { entityId: string; label: string } | null;
  /** Whether the shell takes *Start timer* (`ontimer` is supplied). */
  canTime: boolean;
  /** Settings › Checkouts' applications, the default first (`menuOrder`). */
  apps: readonly { id: string; name: string }[];
}

/** The sheet's subject for a row, or `null` for a row nothing can be done to. */
export function subjectOf(row: LauncherRow, inboxOps: (key: string) => readonly string[] = () => []): SheetSubject | null {
  switch (row.kind) {
    case "hit": {
      const { hit } = row;
      if (hit.kind === "list") {
        return { kind: "list", id: hit.entity_id, label: hit.title, text: hit.path ?? "", refusal: hit.state?.text ?? null };
      }
      if (hit.kind === "view") return { kind: "view", id: hit.entity_id, title: hit.title };
      if (hit.kind === "set") return { kind: "set", id: hit.entity_id, title: hit.title };
      return { kind: "entity", entityId: hit.entity_id, entityKind: hit.kind, title: hit.title, sourceId: hit.source_id, path: hit.path };
    }
    case "recent": {
      const item = row.row;
      return { kind: "entity", entityId: item.entity_id, entityKind: item.kind, title: item.title, sourceId: item.source_id, path: item.path };
    }
    case "inbox":
      return { kind: "inbox", item: row.item, ops: inboxOps(row.item.key) };
    case "action":
    case "syntax":
    case "docs":
      return null;
  }
}

/** What a subject is called in a toast: a ticket by its key, anything else by its title. */
export function nameOf(subject: SheetSubject): string {
  switch (subject.kind) {
    case "entity":
      return subject.entityKind === "ticket" ? keyOf(subject.entityId) : subject.title;
    case "list":
      return subject.label;
    case "inbox":
      return subject.item.title;
    case "view":
    case "set":
      return subject.title;
  }
}

/** The kinds *Open in ⟨app⟩* is offered on: a checkout is a repository's or a branch's, and a pull request's repository's. */
const OPENS_IN = new Set(["repo", "pr", "branch"]);

/** The table: `subject`'s actions, in the order the sheet draws them. */
export function sheetActions(subject: SheetSubject, ctx: SheetContext): SheetAction[] {
  switch (subject.kind) {
    case "list":
      return [
        {
          id: "apply",
          label: "Apply",
          detail: subject.refusal ?? `${subject.text} in the box, as Enter does`,
          does: { kind: "apply" },
        },
        { id: "rename", label: "Rename", detail: "The box takes the new name; Enter keeps it", does: { kind: "rename" } },
        { id: "delete", label: "Delete", detail: "The saved search is gone", does: { kind: "delete" } },
      ];
    case "view": {
      const hash = entityAddress("view", subject.id, subject.title);
      return [
        { id: "open", label: "Open", detail: "The Tickets page on this view", does: { kind: "go", hash } },
        { id: "copy", label: "Copy address", detail: hash, does: { kind: "copy", text: hash, said: `Copied the address of ${subject.title}` } },
      ];
    }
    case "set":
      return [
        {
          id: "open",
          label: "Open",
          detail: "The set, its members and its rules",
          does: { kind: "go", hash: entityAddress("set", subject.id, subject.title) },
        },
      ];
    case "inbox":
      return inboxActions(subject.item, subject.ops);
    case "entity":
      return entityActions(subject, ctx);
  }
}

function inboxActions(item: InboxItem, ops: readonly string[]): SheetAction[] {
  const actions: SheetAction[] = [
    { id: "open", label: "Open", detail: opener(item)?.label ?? "The Inbox page", does: { kind: "go", hash: opener(item)?.href ?? "#/inbox" } },
  ];
  // The Inbox page's own buttons for the line, in its order; *Snooze* is a flow (#2427).
  for (const op of rowActions({ item, actions: [...ops] })) {
    if (op === "rerun_build" && item.entity_id !== null) {
      actions.push({ id: "rerun", label: "Re-run", detail: "Queued as a write to the source", does: { kind: "inbox", op: "rerun" } });
    } else if (op === "ack") {
      actions.push({ id: "ack", label: "Acknowledge", detail: "The alert stops asking", does: { kind: "inbox", op: "ack" } });
    } else if (op === "done") {
      actions.push({ id: "done", label: "Done", detail: "Leaves the inbox", does: { kind: "inbox", op: "done" } });
    }
  }
  return actions;
}

function entityActions(subject: Extract<SheetSubject, { kind: "entity" }>, ctx: SheetContext): SheetAction[] {
  const { entityId: id, entityKind: kind, title } = subject;
  const key = keyOf(id);
  const actions: SheetAction[] = [];
  // The pair: an entity cannot be linked to itself, so the job's own ticket has no row.
  if (ctx.job !== null && ctx.job.entityId !== id) {
    actions.push({ id: "link", label: `Link to ${ctx.job.label}`, detail: "← → pick the relation", does: { kind: "link" } });
  }
  if (kind === "ticket" && ctx.canTime && canBeTarget({ entityId: id, kind })) {
    actions.push({ id: "timer", label: "Start timer", detail: `Stops the running one first; ${key} becomes the job`, does: { kind: "timer" } });
  }
  const go = (hash: string): SheetEffect => ({ kind: "go", hash });
  switch (kind) {
    case "ticket":
      actions.push({ id: "work", label: "Work on this", detail: `Makes ${key} the job and opens it`, does: go(jobHash(id)) });
      break;
    case "pr":
      actions.push({ id: "diff", label: "Review the files", detail: "Files changed", does: go(prHash(id, "diff")) });
      actions.push({ id: "talk", label: "Conversation", detail: "The review thread", does: go(prHash(id, "conversation")) });
      actions.push({
        id: "approve",
        label: "Approve",
        detail: "Queued as a write to the source",
        does: { kind: "write", payload: { Approve: { entity: id, body: "" } }, said: `Approval queued for ${title}` },
      });
      break;
    case "repo":
      actions.push({ id: "prs", label: "Pull requests", detail: "The repository's pull requests", does: go(repoHash(id, "prs")) });
      actions.push({ id: "branches", label: "Branches", detail: "Every branch and where it stands", does: go(repoHash(id, "branches")) });
      break;
    case "pipeline":
      actions.push({
        id: "run",
        label: "Run",
        detail: "Queued as a write to the source",
        does: { kind: "write", payload: { TriggerBuild: { entity: id } }, said: `Run queued for ${title}` },
      });
      break;
    case "monitor":
      actions.push({ id: "ack", label: "Acknowledge", detail: "The open alert on it stops asking", does: { kind: "ack", monitorId: id, said: `Acknowledged ${title}` } });
      break;
    default:
  }
  if (OPENS_IN.has(kind)) {
    const where = kind === "pr" ? "The repository's checkout" : "The checkout on this machine";
    for (const app of ctx.apps) {
      actions.push({ id: `open-${app.id}`, label: `Open in ${app.name}`, detail: where, does: { kind: "open-in", application: app.id, name: app.name } });
    }
  }
  if (kind === "repo") {
    actions.push({ id: "clone", label: "Clone", detail: "Into a folder under the clones root", does: { kind: "clone" } });
  }
  actions.push(
    kind === "ticket"
      ? { id: "copy", label: "Copy key", detail: key, does: { kind: "copy", text: key, said: `Copied ${key}` } }
      : { id: "copy", label: "Copy title", detail: title, does: { kind: "copy", text: title, said: `Copied ${title}` } },
  );
  return actions;
}

/**
 * The actions holding every typed letter in order, in the label or the
 * detail, ignoring case and spaces; the tightest span first, then the
 * earliest start, then the table's order. Nothing typed: all, in order.
 */
export function narrowActions(actions: readonly SheetAction[], typed: string): SheetAction[] {
  const needle = typed.toLowerCase().replace(/\s+/gu, "");
  if (needle === "") return [...actions];
  const scored: { action: SheetAction; span: number; start: number; at: number }[] = [];
  actions.forEach((action, at) => {
    const best = tightest(action.label.toLowerCase(), needle) ?? tightest(`${action.label} ${action.detail}`.toLowerCase(), needle);
    if (best) scored.push({ action, ...best, at });
  });
  scored.sort((a, b) => a.span - b.span || a.start - b.start || a.at - b.at);
  return scored.map((entry) => entry.action);
}

/** The shortest window of `hay` holding `needle`'s letters in order, or `null`. */
function tightest(hay: string, needle: string): { span: number; start: number } | null {
  let best: { span: number; start: number } | null = null;
  for (let start = hay.indexOf(needle[0]!); start >= 0; start = hay.indexOf(needle[0]!, start + 1)) {
    let at = start;
    for (const char of needle.slice(1)) {
      at = hay.indexOf(char, at + 1);
      if (at < 0) return best;
    }
    const span = at - start;
    if (best === null || span < best.span) best = { span, start };
  }
  return best;
}
