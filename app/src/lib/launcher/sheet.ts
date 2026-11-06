/**
 * The Tab sheet's actions (#2426, spec #2419 "The sheet"): one table, per
 * kind, of what a row can have done to it from the box.
 *
 * Every row is data. `does` names an address of the app or a write it has
 * today, and `sheet-mode.svelte.ts` carries it out in one place (`#carry`), so
 * this table can be read and tested without a DOM, a later action key
 * (#2429) binds to an `id`, and a flow (#2427) is one more `does` kind.
 *
 * An action that asks something is a flow (#2427): `does` is `{ kind: "flow" }`
 * with its steps, each a title and an async source of choices given the picks
 * so far, and a `finish` that turns the picks into one more `does`, the write
 * or the address the last pick runs. The sheet shows the step's choices, the
 * box narrows them, Enter picks; Esc or Backspace on the empty box step back.
 *
 * The order holds still from row to row (F-637): what the kind does first (on a ticket *Work on this*, then
 * *Start timer*), then the pair with the chosen job (*Link to ⟨job⟩*, then the set), then *Open in*, *Clone*,
 * *Favourite* (#2428), and *Copy key* or *Copy title*. A chosen job used to put the link on top, which shifted every
 * position by one and made `Tab`, `Enter` write a link. This reverses the 2026-10-21 ruling on #1416 (the two-row
 * chain and the hand-off to the job's dialog); see `docs/decisions/2026-11-02-launcher-sheet.md`.
 *
 * Adapter-declared source actions join under the same rule: a row here whose
 * `does` is a `write` of the op the source declares. Today's are the
 * declared ops the app already builds (`Approve`, `TriggerBuild`, `RerunBuild`).
 */
import type { EntityRow, InboxItem, LinkSetDraft, LinkSetMemberKind, LinkSetRow, ViewAction } from "../ipc/entity";
import type { FavouriteTarget } from "../ipc/search";
import type { WriteOpPayload } from "../ipc/sources";
import { canBeTarget } from "../shell/timer";
import { entityAddress } from "../tf/address";
import { actionPayload, initialValues } from "../detail/view/actions";
import { snoozePresets } from "../inbox/inbox.svelte";
import { expiryOf, opener, rowActions, untilLabel } from "../tf/inbox/inbox";
import { tabHash } from "../tf/builds/pipeline";
import { prHash } from "../tf/pr/pr";
import { compareHash, repoHash } from "../tf/repo/repo";
import { draftOf, withHandRule } from "../tf/sets/apply";
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

/** One choice of a flow's step: its `id` is the pick. */
export interface Choice {
  id: string;
  label: string;
  detail: string;
}

/** One step of a flow: what the box asks (its label), and the choices it narrows, given the picks so far. */
export interface Step {
  title: string;
  choices: (picks: readonly string[]) => Promise<Choice[]>;
}

/** What choosing an action does. `SheetMode`'s `#carry` is the one reader. */
export type SheetEffect =
  /** Ask each step in turn; the last pick runs what `finish` makes of the picks. */
  | { kind: "flow"; steps: Step[]; finish: (picks: readonly string[]) => SheetEffect }
  /** `save_link_set` with the set as it should be stored. */
  | { kind: "save-set"; draft: LinkSetDraft; said: string }
  /** The inbox's own snooze of the line, until `until`. */
  | { kind: "snooze"; key: string; until: Date; said: string }
  /** An action the source declares disabled: its reason, said, and nothing written. */
  | { kind: "refused"; said: string }
  | { kind: "go"; hash: string }
  /** Write the link to the chosen job with the relation picked inline. */
  | { kind: "link" }
  | { kind: "timer" }
  /** A write queued to the source; `said` is the toast. */
  /** `said` is what its toast queues, as `toastQueued` takes it. */
  | { kind: "write"; payload: WriteOpPayload; said: string; where?: { label: string; hash: string } }
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
  | { kind: "inbox"; op: "ack" | "done" | "rerun" }
  /** *Favourite* (`on`) or *Unfavourite* the subject (#2428); the empty box redraws. */
  | { kind: "favourite"; target: FavouriteTarget; on: boolean }
  /** *Assign key…* on a saved list (#2429): the panel that binds the chord which opens it from anywhere. */
  | { kind: "bind" };

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
  /** Every link set (`link_sets`): *Add to a set…*'s choices; none, no row. */
  sets?: readonly LinkSetRow[];
  /**
   * The pull request's declared `merge_pull_request` action, read from its
   * view when the sheet opens on it: *Merge…* follows it, as the pull request
   * page's button does; `null` (none declared, or not read yet), no row.
   */
  merge?: ViewAction | null;
  /** A repository's branches, from its Branches view: *Create pull request…*'s choices; absent, no row. */
  branches?: (repoId: string) => Promise<readonly { name: string; isDefault: boolean }[]>;
  /** The clock the snooze presets count from. */
  now?: Date;
  /** What is a favourite now, as {@link favouriteKeys} reads the board's block: *Unfavourite* on those. */
  favourites: ReadonlySet<string>;
}

/** A saved list's row on the board: its `#l` hit's shape, kind and source `list`. */
export function isListRow(row: Pick<EntityRow, "kind" | "source_id">): boolean {
  return row.kind === "list" && row.source_id === "list";
}

/** A favourite's key: the column it is kept in and its id, so a list and an entity never share one. */
function keyOfTarget(target: FavouriteTarget): string {
  return "list" in target ? `list:${target.list}` : `entity:${target.entity}`;
}

/**
 * The keys of the board's favourites block (`LauncherHome.favourites`): a
 * saved list's row is its `#l` hit's (kind and source `list`), every other
 * row an entity.
 */
export function favouriteKeys(rows: readonly Pick<EntityRow, "kind" | "source_id" | "entity_id">[]): Set<string> {
  return new Set(
    rows.map((row) =>
      keyOfTarget(isListRow(row) ? { list: row.entity_id } : { entity: row.entity_id }),
    ),
  );
}

/** *Favourite* or *Unfavourite* on what can be one: an entity or a saved list (#2428, spec #2419 story 39). */
function favouriteAction(target: FavouriteTarget, ctx: SheetContext): SheetAction {
  return ctx.favourites.has(keyOfTarget(target))
    ? { id: "fav", label: "Unfavourite", detail: "Leaves the top of the empty box", does: { kind: "favourite", target, on: false } }
    : { id: "fav", label: "Favourite", detail: "At the top of the empty box", does: { kind: "favourite", target, on: true } };
}

/**
 * The saved list a row is, or `null`: a `#l` hit (kind `list`), or a
 * favourite drawn as one ({@link isListRow}). The one place that asks.
 */
export function savedListOf(row: LauncherRow): Extract<SheetSubject, { kind: "list" }> | null {
  const item = row.kind === "hit" && row.hit.kind === "list" ? row.hit : row.kind === "favourite" && isListRow(row.row) ? row.row : null;
  return item && { kind: "list", id: item.entity_id, label: item.title, text: item.path ?? "", refusal: item.state?.text ?? null };
}

/** The sheet's subject for a row, or `null` for a row nothing can be done to. */
export function subjectOf(row: LauncherRow, inboxOps: (key: string) => readonly string[] = () => []): SheetSubject | null {
  const list = savedListOf(row);
  if (list) return list;
  switch (row.kind) {
    case "hit": {
      const { hit } = row;
      if (hit.kind === "view") return { kind: "view", id: hit.entity_id, title: hit.title };
      if (hit.kind === "set") return { kind: "set", id: hit.entity_id, title: hit.title };
      return { kind: "entity", entityId: hit.entity_id, entityKind: hit.kind, title: hit.title, sourceId: hit.source_id, path: hit.path };
    }
    case "favourite":
    case "recent": {
      const item = row.row;
      return { kind: "entity", entityId: item.entity_id, entityKind: item.kind, title: item.title, sourceId: item.source_id, path: item.path };
    }
    case "inbox":
      return { kind: "inbox", item: row.item, ops: inboxOps(row.item.key) };
    case "action":
    case "syntax":
    case "docs":
    case "widen":
      return null;
  }
}

/** The kind a key is kept on (#2429): an entity's own kind (`repo`, `pr`, `ticket`, ...), else the row's (`list`, `view`, `set`, `inbox`). */
export function kindOf(subject: SheetSubject): string {
  return subject.kind === "entity" ? subject.entityKind : subject.kind;
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
        favouriteAction({ list: subject.id }, ctx),
        { id: "key", label: "Assign key…", detail: "A chord that opens this list from anywhere", does: { kind: "bind" } },
      ];
    case "view": {
      const hash = entityAddress("view", subject.id, subject.title);
      return [
        { id: "open", label: "Open", detail: "The Tickets page on this view", does: { kind: "go", hash } },
        { id: "copy", label: "Copy address", detail: hash, does: { kind: "copy", text: hash, said: `Copied the address of ${subject.title}` } },
      ];
    }
    case "set": {
      const actions: SheetAction[] = [
        {
          id: "open",
          label: "Open",
          detail: "The set, its members and its rules",
          does: { kind: "go", hash: entityAddress("set", subject.id, subject.title) },
        },
      ];
      const show = showOnJob(subject.id, ctx);
      if (show) actions.push(show);
      return actions;
    }
    case "inbox":
      return inboxActions(subject.item, subject.ops, ctx.now ?? new Date());
    case "entity":
      return entityActions(subject, ctx);
  }
}

function inboxActions(item: InboxItem, ops: readonly string[], now: Date): SheetAction[] {
  const actions: SheetAction[] = [
    { id: "open", label: "Open", detail: opener(item)?.label ?? "The Inbox page", does: { kind: "go", hash: opener(item)?.href ?? "#/inbox" } },
  ];
  // The Inbox page's own buttons for the line, in its order; *Snooze* is a flow over its presets (#2427).
  for (const op of rowActions({ item, actions: [...ops] })) {
    if (op === "rerun_build" && item.entity_id !== null) {
      actions.push({ id: "rerun", label: "Re-run", detail: "Queued as a write to the source", does: { kind: "inbox", op: "rerun" } });
    } else if (op === "ack") {
      actions.push({ id: "ack", label: "Acknowledge", detail: "The alert stops asking", does: { kind: "inbox", op: "ack" } });
    } else if (op === "snooze") {
      actions.push({ id: "snooze", label: "Snooze…", detail: "Until one of the Inbox page's presets", does: snoozeFlow(item, now) });
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
  const go = (hash: string): SheetEffect => ({ kind: "go", hash });
  switch (kind) {
    case "ticket":
      actions.push({ id: "work", label: "Work on this", detail: `Makes ${key} the job and opens it`, does: go(jobHash(id)) });
      if (ctx.canTime && canBeTarget({ entityId: id, kind })) {
        actions.push({ id: "timer", label: "Start timer", detail: `Stops the running one first; ${key} becomes the job`, does: { kind: "timer" } });
      }
      break;
    case "pr":
      actions.push({ id: "diff", label: "Review the files", detail: "Files changed", does: go(prHash(id, "diff")) });
      actions.push({ id: "talk", label: "Conversation", detail: "The review thread", does: go(prHash(id, "conversation")) });
      actions.push({
        id: "approve",
        label: "Approve",
        detail: "Queued as a write to the source",
        does: { kind: "write", payload: { Approve: { entity: id, body: "" } }, said: `approve ${title}` },
      });
      if (ctx.merge) actions.push(mergeAction(id, title, ctx.merge));
      break;
    case "repo":
      if (ctx.branches) {
        actions.push({
          id: "newpr",
          label: "Create pull request…",
          detail: "Pick the branch with the change, then the branch it goes into",
          does: newPrFlow(id, ctx.branches),
        });
      }
      actions.push({ id: "prs", label: "Pull requests", detail: "The repository's pull requests", does: go(repoHash(id, "prs")) });
      actions.push({ id: "branches", label: "Branches", detail: "Every branch and where it stands", does: go(repoHash(id, "branches")) });
      break;
    case "pipeline":
      actions.push({
        id: "run",
        label: "Run",
        detail: "Queued as a write to the source",
        does: { kind: "write", payload: { TriggerBuild: { entity: id } }, said: `run ${title}`, where: { label: "Show in runs", hash: tabHash(id, "runs") } },
      });
      break;
    case "monitor":
      actions.push({ id: "ack", label: "Acknowledge", detail: "The open alert on it stops asking", does: { kind: "ack", monitorId: id, said: `Acknowledged ${title}` } });
      break;
    default:
  }
  // The pair: an entity cannot be linked to itself, so the job's own ticket has no row.
  if (ctx.job !== null && ctx.job.entityId !== id) {
    actions.push({ id: "link", label: `Link to ${ctx.job.label}`, detail: "← → pick the relation", does: { kind: "link" } });
  }
  // The pair's other half: a set it joins, or on a ticket a set applied to it.
  const set = setAction(subject, ctx.sets ?? []);
  if (set) actions.push(set);
  if (OPENS_IN.has(kind)) {
    const where = kind === "pr" ? "The repository's checkout" : "The checkout on this machine";
    for (const app of ctx.apps) {
      actions.push({ id: `open-${app.id}`, label: `Open in ${app.name}`, detail: where, does: { kind: "open-in", application: app.id, name: app.name } });
    }
  }
  if (kind === "repo") {
    actions.push({ id: "clone", label: "Clone", detail: "Into a folder under the clones root", does: { kind: "clone" } });
  }
  actions.push(favouriteAction({ entity: id }, ctx));
  actions.push(
    kind === "ticket"
      ? { id: "copy", label: "Copy key", detail: key, does: { kind: "copy", text: key, said: `Copied ${key}` } }
      : { id: "copy", label: "Copy title", detail: title, does: { kind: "copy", text: title, said: `Copied ${title}` } },
  );
  return actions;
}

/** What a set can hold as a fixed member (R-7.7's five panels); a ticket is shown a set by a manual rule instead. */
const MEMBER_KINDS: ReadonlySet<string> = new Set<LinkSetMemberKind>(["page", "repo", "pipeline", "asset", "monitor"]);

/**
 * *Add to a set…* on what a set holds, *Apply a link set…* on a ticket, as the ticket page calls it: a
 * flow over every set by name; the pick re-saves that set with one fixed
 * member, or one manual rule (`withHandRule`, the ticket page's *Apply a link
 * set…*). Nothing else, or no sets, no row.
 */
function setAction(subject: Extract<SheetSubject, { kind: "entity" }>, sets: readonly LinkSetRow[]): SheetAction | null {
  const { entityId: id, entityKind: kind, title } = subject;
  const member = MEMBER_KINDS.has(kind) ? (kind as LinkSetMemberKind) : null;
  if (sets.length === 0 || (member === null && kind !== "ticket")) return null;
  const key = keyOf(id);
  const choices = sets.map((set) => ({ id: set.id, label: set.name, detail: set.description }));
  return {
    id: "set",
    label: member ? "Add to a set…" : "Apply a link set…",
    detail: member ? "A fixed member, shown on every ticket the set applies to" : "The set's members appear on this ticket",
    does: {
      kind: "flow",
      steps: [{ title: member ? "Add to" : `Apply to ${key}`, choices: async () => choices }],
      finish: ([setId]) => {
        const set = sets.find((candidate) => candidate.id === setId);
        if (!set) return { kind: "refused", said: "That set is gone." };
        if (member === null) {
          const held = set.rules.some((rule) => rule.kind === "manual" && rule.value === id);
          return { kind: "save-set", draft: held ? draftOf(set) : withHandRule(set, id), said: `${set.name} now shows on ${key}` };
        }
        const draft = draftOf(set);
        if (!draft.members.some((row) => row.kind === member && row.entity_id === id)) draft.members.push({ kind: member, entity_id: id });
        return { kind: "save-set", draft, said: `Added ${title} to ${set.name}` };
      },
    },
  };
}

/**
 * *Show on ⟨job⟩* on a set (story 38): the set re-saved with one manual rule
 * on the chosen job's ticket (`withHandRule`, as *Apply a link set…* on
 * the ticket does). No job chosen, or the set not among the sets read yet,
 * no row.
 */
function showOnJob(setId: string, ctx: SheetContext): SheetAction | null {
  const set = ctx.sets?.find((candidate) => candidate.id === setId);
  const job = ctx.job;
  if (!set || job === null) return null;
  const held = set.rules.some((rule) => rule.kind === "manual" && rule.value === job.entityId);
  return {
    id: "show",
    label: `Apply to ${job.label}`,
    detail: "The set's members appear on the job's ticket",
    does: { kind: "save-set", draft: held ? draftOf(set) : withHandRule(set, job.entityId), said: `${set.name} now shows on ${job.label}` },
  };
}

/** The SPI's three styles in words; another is the source's own label. */
const STYLES: Record<string, Choice> = {
  merge: { id: "merge", label: "Merge commit", detail: "Keeps every commit and adds one" },
  squash: { id: "squash", label: "Squash", detail: "One commit on the base" },
  rebase: { id: "rebase", label: "Rebase", detail: "The commits replayed on the base" },
};

/**
 * *Merge…*, over the source's declared `merge_pull_request`: disabled with
 * its reason where the source draws it so (the pull request page's button),
 * else a flow over its `style` choices that queues the write with the picked
 * style and every other field as the source declared it.
 */
function mergeAction(id: string, title: string, declared: ViewAction): SheetAction {
  const base = { id: "merge", label: "Merge…" };
  if (declared.disabled !== null) return { ...base, detail: declared.disabled, does: { kind: "refused", said: declared.disabled } };
  const field = declared.fields.find((one) => one.name === "style");
  const options = field?.kind.type === "choice" ? field.kind.options : [];
  const choices = options.length > 0 ? options.map((option) => STYLES[option.value] ?? { id: option.value, label: option.label, detail: "" }) : Object.values(STYLES);
  return {
    ...base,
    detail: "Pick the style; queued as a write to the source",
    does: {
      kind: "flow",
      steps: [{ title: "Merge as", choices: async () => choices }],
      finish: ([style]) => {
        const payload = actionPayload(declared, id, { ...initialValues(declared), style: style ?? "merge" });
        if (payload === null) return { kind: "refused", said: `The source takes no merge of ${title} from here.` };
        return { kind: "write", payload, said: `merge ${title}` };
      },
    },
  };
}

/**
 * *Create pull request…*: the branch with the change, then the branch it
 * goes into (the head left out), both from the repository's branches with
 * the default first; the compare page opens with both in its address.
 */
function newPrFlow(repoId: string, branches: NonNullable<SheetContext["branches"]>): SheetEffect {
  const step = (title: string): Step => ({
    title,
    async choices(picks) {
      const read = await branches(repoId);
      const ordered = [...read.filter((one) => one.isDefault), ...read.filter((one) => !one.isDefault)];
      return ordered
        .filter((one) => one.name !== picks[0])
        .map((one) => ({ id: one.name, label: one.name, detail: one.isDefault ? "The default branch" : "" }));
    },
  });
  return {
    kind: "flow",
    steps: [step("From branch"), step("Into branch")],
    finish: ([head, base]) => ({ kind: "go", hash: compareHash(repoId, head ?? "", base ?? null) }),
  };
}

/** *Snooze…*: the Inbox page's own presets for the line (the day after a credential expires, where it does). */
function snoozeFlow(item: InboxItem, now: Date): SheetEffect {
  const presets = snoozePresets(now, expiryOf(item));
  return {
    kind: "flow",
    steps: [
      {
        title: "Snooze until",
        // Keyed by label: on a Sunday *Tomorrow* and *Next Monday* are the same morning.
        choices: async () => presets.map((preset) => ({ id: preset.label, label: preset.label, detail: untilLabel(preset.at, now) })),
      },
    ],
    finish: ([picked]) => {
      const preset = presets.find((one) => one.label === picked);
      if (!preset) return { kind: "refused", said: "That preset is gone." };
      return { kind: "snooze", key: item.key, until: preset.at, said: `Snoozed until ${untilLabel(preset.at, now)}` };
    },
  };
}

/**
 * The actions holding every typed letter in order, in the label or the
 * detail, ignoring case and spaces; the tightest span first, then the
 * earliest start, then the table's order. Nothing typed: all, in order.
 */
export function narrowActions<T extends { label: string; detail: string }>(actions: readonly T[], typed: string): T[] {
  const needle = typed.toLowerCase().replace(/\s+/gu, "");
  if (needle === "") return [...actions];
  const scored: { action: T; span: number; start: number; at: number }[] = [];
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
