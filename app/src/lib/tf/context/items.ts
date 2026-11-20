/**
 * What a right-click's menu offers (2026-11-16, `docs/decisions/2026-11-16-right-click-menus.md`): one table, per
 * thing, of items that are each something the app already does. Pure: the menu draws what this answers and carries
 * out each item's {@link Act} in one place (`TfContextMenu.svelte`'s `carry`), so the table is read and tested without
 * a DOM.
 *
 * The rules, Fable's (xhigh) rulings on Björn's question:
 * - The order is the same on every kind: *Open*, its pages, what it does, *Link to ⟨job⟩*; a rule; *Open in ⟨default
 *   application⟩*, *Open in… ▸*, *Open in ⟨source⟩*; a rule; the copies. *Open* is first, so the release of the press
 *   that opened the menu never lands on a write.
 * - The launcher sheet's actions (`sheet.ts`) are the kinds' own, by the same ids and labels, one-press ones only: a
 *   flow (*Merge…*, *Create pull request…*, *Add to a set…*) stays in the launcher, and so do *Approve* and *Merge*,
 *   which others see and one press doesn't take back.
 * - Nothing here waits for a read: the menu opens in the frame it is asked for. What needs one (a page on the web, a
 *   checkout, a title) is read when the menu opens, and an item names the read in its {@link Text}; the count and
 *   the order of the rows never depend on a read, only a label may (*Clone* in the default application's slot).
 */
import { sheetActions, type SheetAction, type SheetContext } from "../../launcher/sheet";
import type { CheckoutSettings, CheckoutView, EntityDetail } from "../../ipc/entity";
import type { WriteOpPayload } from "../../ipc/sources";
import { keyOf } from "../../time/format";
import { diffHash, shaOfCommit, WORKING_TREE } from "../diff/address";
import { appPageHash } from "../openin/app-page";
import { menuOrder, objectMenu, repoName } from "../openin/openin";
import { compareHash, fileHash, folderRenderHash, lineHash, renderedHash, repoHash } from "../repo/repo";
import { tabHash } from "../builds/pipeline";
import { runTabHash } from "../builds/run";
import type { Thing } from "./thing";

/** A value an item copies or opens: given, or read when the menu opened. */
export type Text =
  | string
  /** The item's page at its source (`entity_head`'s `web_url`). */
  | { read: "web" }
  /** The item's title (`entity_head`'s row). */
  | { read: "title" }
  /** A ticket's key and title, as the job card copies them. */
  | { read: "key-title" }
  /** A path inside the checkout on this disk. */
  | { read: "full-path"; path: string }
  /** A code object's page on the forge (`object_page`), read with the repository's default branch. */
  | { read: "forge" };

/** Where an application runs: a repository's (or branch's) checkout, or a folder, a file or a commit of it. */
export type CodeAt =
  | { kind: "repository" }
  | { kind: "folder"; path: string }
  | { kind: "file"; path: string; line: number | null }
  | { kind: "commit"; sha: string };

/** What choosing an item does. `TfContextMenu`'s `carry` is the one reader. */
export type Act =
  | { do: "go"; hash: string }
  | { do: "copy"; text: Text; said: string }
  /** The document's own copy of the selection, so a page's `copy` handler still shapes what lands. */
  | { do: "copy-selection" }
  | { do: "search"; text: string }
  | { do: "browser"; url: Text }
  /** A web address of a source knobas mirrors: what it names, opened in the app, resolved when chosen. */
  | { do: "in-knobas"; url: string }
  | { do: "timer"; entityId: string }
  | { do: "link"; entityId: string }
  | { do: "write"; payload: WriteOpPayload; said: string; sourceId: string | null; where?: { label: string; hash: string } }
  | { do: "ack"; monitorId: string; own?: () => void }
  /** The Inbox page's own writes on a line (`inbox.svelte.ts`). */
  | { do: "inbox"; op: "ack" | "done"; key: string; title: string }
  /** Run application `app` (by id) on `at` of the checkout of `entityId`; `folderOnly` is an application with no file command, which opens the file's folder. */
  | { do: "app"; entityId: string; app: string; name: string; at: CodeAt }
  | { do: "clone"; repoId: string }
  /** Make a Docs space or page Docs' home (`title` what it is called), or with `id` `null` go back to Docs' own page. */
  | { do: "docs-home"; kind: "space" | "page"; id: string | null; title: Text }
  /** Hide a Docs space from Docs' browsing, or show it again. */
  | { do: "docs-hide"; id: string; name: string; hidden: boolean }
  /** Mark one run read or unread (ux18). */
  | { do: "run-read"; id: string; read: boolean }
  /** *Mark all as read* on a pipeline (ux18): every failed run of it. */
  | { do: "pipeline-read"; id: string };

export interface Item {
  id: string;
  label: string;
  /** What a copy will copy, drawn on the right in the code face; `null` for none. */
  hint?: string | null;
  act?: Act;
  /** Why it can't be chosen now; `null` or absent where it can. */
  disabled?: string | null;
  /** *Open in… ▸*: its second level. */
  sub?: Entry[];
}

export type Entry = Item | { rule: true };

/** What the menu knows when it opens and learns while it is open. */
export interface MenuFacts {
  /** `entity_head` of the thing (or of a code thing's repository), once answered. */
  head?: EntityDetail | null;
  /** The checkout of a code thing's repository, once answered; `undefined` while it reads. */
  checkout?: CheckoutView | null;
}

export interface MenuContext {
  /** The chosen job, *Link to ⟨job⟩*'s other end. */
  job: { entityId: string; label: string } | null;
  /** Whether the shell takes *Start timer*. */
  canTime: boolean;
  /** Settings › Open in…'s applications, `null` until read. */
  apps: CheckoutSettings | null;
  /** A source's display name by its id, for *Open in ⟨source⟩*; absent, *Open in browser*. */
  sourceName: (sourceId: string) => string | null;
  /** Whether a web address is on a source knobas mirrors (its base URL). */
  mirrored: (url: string) => boolean;
  /** The page's own thing: drawn without *Open*, since that is where the reader stands. */
  own?: boolean;
  /** The folder renderers that take a folder of a repository (ADR-0044), in the person's order; absent, none. */
  renders?: (repoId: string, path: string) => readonly { id: string; name: string; output?: string }[];
  /** Where Docs opens and the spaces it hides (`docs-choices.svelte.ts`); absent, no Docs item is offered. */
  docs?: { isHome(kind: "space" | "page", id: string): boolean; hidden: ReadonlySet<string> };
  /** Whether a failed run is marked read (`runs-read.svelte.ts`, ux18); absent, no mark is offered. */
  runs?: { isRead(runId: string): boolean };
  /** Whether a repository is known to be cloned here (ux26): a commit offers *Compare with working tree* only then. */
  checkout?: (repoId: string) => boolean;
}

const RULE: Entry = { rule: true };

/** The source an entity id names: what comes before its first `:`. */
export function sourceOf(entityId: string): string {
  const at = entityId.indexOf(":");
  return at < 0 ? "" : entityId.slice(0, at);
}


/** The words of a selection worth searching for: one line, 80 characters or fewer. */
export function searchable(text: string): boolean {
  return text.length <= 80 && !/[\r\n]/.test(text);
}

/** The menu for `thing` (or none) and the words selected where the press was (or none), in the order it is drawn. */
export function menuFor(thing: Thing | null, selection: string | null, ctx: MenuContext, facts: MenuFacts = {}): Entry[] {
  const own = thing === null ? [] : thingEntries(thing, ctx, facts);
  if (selection === null) return own;
  const words: Entry[] = [{ id: "copy-sel", label: "Copy", act: { do: "copy-selection" } }];
  if (searchable(selection)) {
    const shown = selection.length > 24 ? `${selection.slice(0, 23)}…` : selection;
    words.push({ id: "search-sel", label: `Search knobas for “${shown}”`, act: { do: "search", text: selection } });
  }
  return own.length === 0 ? words : [...words, RULE, ...own];
}

/** The kinds the launcher's table names a thing by. */
const SHEET_KIND: Partial<Record<Thing["kind"], string>> = {
  ticket: "ticket",
  pr: "pr",
  repo: "repo",
  pipeline: "pipeline",
  monitor: "monitor",
  page: "page",
  branch: "branch",
  asset: "asset",
};

/** The sheet's one-press actions the menu takes (`sheet.ts`), by id: the kind's own, and the pair with the job. */
const FROM_SHEET = new Set(["work", "timer", "diff", "talk", "prs", "branches", "run", "link"]);

/**
 * The sheet's actions for an entity, as the menu takes them, with `extras` (the menu's own pages for the kind) after
 * the kind's own and before *Link to ⟨job⟩*, which ends the group on every kind.
 */
function fromSheet(kind: string, id: string, title: string, ctx: MenuContext, extras: Item[] = []): Entry[] {
  const sheet: SheetContext = { job: ctx.job, canTime: ctx.canTime, apps: [], favourites: new Set() };
  const actions = sheetActions({ kind: "entity", entityId: id, entityKind: kind, title, sourceId: sourceOf(id), path: null }, sheet);
  const items = actions.filter((action) => FROM_SHEET.has(action.id)).flatMap((action) => sheetItem(action, id));
  const link = items.filter((item) => item.id === "link");
  return [...items.filter((item) => item.id !== "link"), ...extras, ...link];
}

function sheetItem(action: SheetAction, id: string): Item[] {
  const does = action.does;
  switch (does.kind) {
    case "go":
      return [{ id: action.id, label: action.label, act: { do: "go", hash: does.hash } }];
    case "timer":
      return [{ id: action.id, label: action.label, act: { do: "timer", entityId: id } }];
    case "link":
      return [{ id: action.id, label: action.label, act: { do: "link", entityId: id } }];
    case "write":
      return [{ id: action.id, label: action.label, act: { do: "write", payload: does.payload, said: does.said, sourceId: sourceOf(id), ...(does.where ? { where: does.where } : {}) } }];
    default:
      return [];
  }
}

/** *Open in ⟨source⟩*: the source's name when it is known, else the browser. */
function inSource(id: string, ctx: MenuContext): Item {
  const name = ctx.sourceName(sourceOf(id));
  return { id: "web", label: name ? `Open in ${name}` : "Open in browser", act: { do: "browser", url: { read: "web" } } };
}

/**
 * A file's, a folder's or a line's page on its repository's code host: *Open in ⟨source⟩* where the source is known,
 * as `inSource` names an item's (the build-header pass, 2026-11-20: it said *Open in browser* while the source was known).
 */
function inForge(repoId: string, ctx: MenuContext): Item {
  const name = ctx.sourceName(sourceOf(repoId));
  return { id: "forge", label: name ? `Open in ${name}` : "Open in browser", act: { do: "browser", url: { read: "forge" } } };
}

/** The copy of the item's page at its source. */
function copyLink(): Item {
  return { id: "copy-link", label: "Copy link", act: { do: "copy", text: { read: "web" }, said: "Copied the link" } };
}

/** The menu's lines between *Open* and the copies, with the rules: empty groups leave no rule. */
function grouped(...groups: Entry[][]): Entry[] {
  const out: Entry[] = [];
  for (const group of groups) {
    if (group.length === 0) continue;
    if (out.length > 0) out.push(RULE);
    out.push(...group);
  }
  return out;
}

function open(hash: string, ctx: MenuContext): Item[] {
  return ctx.own ? [] : [{ id: "open", label: "Open", act: { do: "go", hash } }];
}

/**
 * *Open Docs here* on a Docs space or page (PR #2479, 2026-11-17): it becomes Docs' home, where the Docs entry and the
 * launcher's Docs row go; on Docs' home it reads *Stop opening Docs here*, which goes back to Docs' own page.
 */
function docsHome(kind: "space" | "page", id: string, title: Text, ctx: MenuContext): Item[] {
  if (ctx.docs === undefined) return [];
  return ctx.docs.isHome(kind, id)
    ? [{ id: "docs-home", label: "Stop opening Docs here", act: { do: "docs-home", kind, id: null, title } }]
    : [{ id: "docs-home", label: "Open Docs here", act: { do: "docs-home", kind, id, title } }];
}

function thingEntries(thing: Thing, ctx: MenuContext, facts: MenuFacts): Entry[] {
  const title = facts.head?.row.title ?? "";
  switch (thing.kind) {
    case "ticket": {
      const key = keyOf(thing.id);
      return grouped(
        [...open(thing.hash, ctx), ...fromSheet("ticket", thing.id, title, ctx)],
        [inSource(thing.id, ctx)],
        [
          { id: "copy-key", label: "Copy key", hint: key, act: { do: "copy", text: key, said: `Copied ${key}` } },
          { id: "copy-key-title", label: "Copy key and title", act: { do: "copy", text: { read: "key-title" }, said: `Copied ${key} and its title` } },
          copyLink(),
        ],
      );
    }
    case "pr":
      return grouped(
        [...open(thing.hash, ctx), ...fromSheet("pr", thing.id, title, ctx)],
        [inSource(thing.id, ctx)],
        [{ id: "copy-title", label: "Copy title", act: { do: "copy", text: { read: "title" }, said: "Copied the title" } }, copyLink()],
      );
    case "repo": {
      const name = keyOf(thing.id);
      return grouped(
        [...open(thing.hash, ctx), ...fromSheet("repo", thing.id, title, ctx, [{ id: "commits", label: "Commits", act: { do: "go", hash: repoHash(thing.id, "commits") } }])],
        [...openIn(thing.id, thing.id, { kind: "repository" }, ctx, facts), inSource(thing.id, ctx)],
        [
          { id: "copy-name", label: "Copy name", hint: name, act: { do: "copy", text: name, said: `Copied ${name}` } },
          fullPath("", facts),
          copyLink(),
        ],
      );
    }
    case "branch":
      return grouped(
        [...open(thing.hash, ctx), ...fromSheet("branch", thing.id, title, ctx)],
        [...openIn(thing.id, null, { kind: "repository" }, ctx, facts), inSource(thing.id, ctx)],
        [{ id: "copy-name", label: "Copy name", act: { do: "copy", text: { read: "title" }, said: "Copied the branch name" } }, copyLink()],
      );
    case "branch-name":
      return grouped(
        [
          { id: "compare", label: "Compare", act: { do: "go", hash: repoHash(thing.repoId, "compare", encodeURIComponent(thing.name)) } },
          { id: "compare-working", label: "Compare with working tree", act: { do: "go", hash: compareHash(thing.repoId, WORKING_TREE, thing.name) } },
          { id: "builds", label: "Builds", act: { do: "go", hash: repoHash(thing.repoId, "builds", encodeURIComponent(thing.name)) } },
        ],
        [{ id: "copy-name", label: "Copy name", hint: thing.name, act: { do: "copy", text: thing.name, said: `Copied ${thing.name}` } }],
      );
    case "commit": {
      const sha = shaOfCommit(thing.id);
      const copies: Item[] = sha
        ? [
            { id: "copy-sha", label: "Copy hash", hint: sha.slice(0, 12), act: { do: "copy", text: sha, said: `Copied ${sha.slice(0, 7)}` } },
            { id: "copy-short", label: "Copy short hash", hint: sha.slice(0, 7), act: { do: "copy", text: sha.slice(0, 7), said: `Copied ${sha.slice(0, 7)}` } },
          ]
        : [];
      const code = thing.repoId !== null && sha !== null ? openIn(thing.repoId, thing.repoId, { kind: "commit", sha }, ctx, facts) : [];
      // The working tree against this commit (ux26), where the repository is known to be cloned here.
      const working: Item[] =
        thing.repoId !== null && sha !== null && ctx.checkout?.(thing.repoId) === true
          ? [{ id: "compare-working", label: "Compare with working tree", act: { do: "go", hash: diffHash(thing.repoId, sha, WORKING_TREE) } }]
          : [];
      return grouped([...open(thing.hash, ctx), ...working], [...code, inSource(thing.id, ctx)], [...copies, copyLink()]);
    }
    case "run":
      return grouped(
        [
          ...open(thing.hash, ctx),
          ...(thing.pipelineId === null
            ? []
            : [
                { id: "log", label: "Log", act: { do: "go", hash: runTabHash(thing.pipelineId, thing.id, "log") } } satisfies Item,
                { id: "tests", label: "Tests", act: { do: "go", hash: runTabHash(thing.pipelineId, thing.id, "tests") } } satisfies Item,
              ]),
          {
            id: "rerun",
            label: "Re-run",
            act: {
              do: "write",
              payload: { RerunBuild: { entity: thing.id } },
              said: "re-run this run",
              sourceId: sourceOf(thing.id),
              ...(thing.pipelineId === null ? {} : { where: { label: "Show in runs", hash: tabHash(thing.pipelineId, "runs") } }),
            },
          },
          // A finished failed run can be marked; any other is read by its nature and offers neither (ux18).
          ...(thing.failed === true && ctx.runs !== undefined
            ? [
                ctx.runs.isRead(thing.id)
                  ? ({ id: "unread", label: "Mark as unread", act: { do: "run-read", id: thing.id, read: false } } satisfies Item)
                  : ({ id: "read", label: "Mark as read", act: { do: "run-read", id: thing.id, read: true } } satisfies Item),
              ]
            : []),
        ],
        [inSource(thing.id, ctx)],
        [copyLink()],
      );
    case "pipeline":
      return grouped(
        [
          ...open(thing.hash, ctx),
          ...fromSheet("pipeline", thing.id, title, ctx),
          // Every failed run of it, older ones too (ux18), with *Undo* on the toast.
          ...(ctx.runs !== undefined ? [{ id: "read-all", label: "Mark all as read", act: { do: "pipeline-read", id: thing.id } } satisfies Item] : []),
        ],
        [inSource(thing.id, ctx)],
        [{ id: "copy-name", label: "Copy name", act: { do: "copy", text: { read: "title" }, said: "Copied the name" } }, copyLink()],
      );
    case "monitor":
    case "page":
    case "asset":
    case "set":
    case "note":
    case "item": {
      const sheetKind = SHEET_KIND[thing.kind];
      const own = sheetKind === undefined ? [] : fromSheet(sheetKind, thing.id, title, ctx);
      const ack: Item[] = thing.kind === "monitor" && thing.alerting === true ? [{ id: "ack", label: "Acknowledge", act: { do: "ack", monitorId: thing.id, ...(thing.ack ? { own: thing.ack } : {}) } }] : [];
      const web = thing.kind === "set" || thing.kind === "note" ? [] : [inSource(thing.id, ctx)];
      const copies: Item[] = [
        { id: "copy-title", label: thing.kind === "page" || thing.kind === "note" ? "Copy title" : "Copy name", act: { do: "copy", text: { read: "title" }, said: "Copied" } },
        ...(web.length > 0 ? [copyLink()] : []),
      ];
      const home = thing.kind === "page" ? docsHome("page", thing.id, { read: "title" }, ctx) : [];
      return grouped([...open(thing.hash, ctx), ...own, ...ack, ...home], web, copies);
    }
    case "space": {
      const hidden = ctx.docs?.hidden.has(thing.id) ?? false;
      return grouped(
        [
          ...open(thing.hash, ctx),
          ...docsHome("space", thing.id, thing.name, ctx),
          ...(ctx.docs === undefined
            ? []
            : [{ id: "docs-hide", label: hidden ? "Unhide" : "Hide from Docs", act: { do: "docs-hide", id: thing.id, name: thing.name, hidden: !hidden } } satisfies Item]),
        ],
        [{ id: "copy-name", label: "Copy name", act: { do: "copy", text: thing.name, said: "Copied the name" } }],
      );
    }
    case "file":
    case "folder":
    case "line": {
      const line = thing.kind === "line" ? thing.line : null;
      const at: CodeAt = thing.kind === "folder" ? { kind: "folder", path: thing.path } : { kind: "file", path: thing.path, line };
      const hash = thing.kind === "folder" ? null : line === null ? fileHash(thing.repoId, thing.path) : lineHash(thing.repoId, thing.path, line);
      const pathLine = line === null ? thing.path : `${thing.path}:${line}`;
      return grouped(
        [
          ...(hash === null || ctx.own ? [] : [{ id: "open", label: "Open", act: { do: "go", hash } } satisfies Item]),
          ...(thing.kind === "folder" ? [] : [{ id: "history", label: "History", act: { do: "go", hash: repoHash(thing.repoId, "commits") } } satisfies Item]),
          // A folder a folder renderer takes: its drawing, by each that takes it (ADR-0044).
          ...(thing.kind === "folder"
            ? (ctx.renders?.(thing.repoId, thing.path) ?? []).map(
                (r): Item => ({ id: `render-${r.id}`, label: `Render with ${r.name}`, act: {
                    do: "go",
                    // A folder of files is browsed in the folder's place (ADR-0046); the rest are drawn in the file's.
                    hash: r.output === "folder" ? renderedHash(thing.repoId, r.id, thing.path) : folderRenderHash(thing.repoId, r.id, thing.path),
                  },
                }),
              )
            : []),
        ],
        [
          ...openIn(thing.repoId, thing.repoId, at, ctx, facts),
          inForge(thing.repoId, ctx),
        ],
        [
          {
            id: "copy-path",
            label: line === null ? "Copy path" : "Copy path and line",
            hint: pathLine,
            act: { do: "copy", text: pathLine, said: `Copied ${pathLine}` },
          },
          fullPath(thing.path, facts),
          {
            id: "copy-link",
            label: line === null ? "Copy link" : "Copy link to line",
            act: { do: "copy", text: { read: "forge" }, said: "Copied the link" },
          },
        ],
      );
    }
    case "inbox": {
      // The Inbox page's buttons for the line, in its order (`rowActions`); *Snooze…* asks, so it stays on the row.
      const acts: Item[] = [];
      for (const op of thing.ops) {
        if (op === "rerun_build" && thing.entityId !== null) {
          acts.push({ id: "rerun", label: "Re-run", act: { do: "write", payload: { RerunBuild: { entity: thing.entityId } }, said: `re-run ${thing.title}`, sourceId: thing.sourceId } });
        } else if (op === "ack") {
          acts.push({ id: "ack", label: "Acknowledge", act: { do: "inbox", op: "ack", key: thing.key, title: thing.title } });
        } else if (op === "done") {
          acts.push({ id: "done", label: "Done", act: { do: "inbox", op: "done", key: thing.key, title: thing.title } });
        }
      }
      return grouped([...(thing.open === null ? [] : [{ id: "open", label: "Open", act: { do: "go", hash: thing.open } } satisfies Item]), ...acts]);
    }
    case "test":
      return [{ id: "copy-name", label: "Copy name", hint: thing.name, act: { do: "copy", text: thing.name, said: "Copied the test's name" } }];
    case "web": {
      const knobas: Item[] = ctx.mirrored(thing.url) ? [{ id: "in-knobas", label: "Open in knobas", act: { do: "in-knobas", url: thing.url } }] : [];
      return grouped(
        [...knobas, { id: "browser", label: "Open in browser", act: { do: "browser", url: thing.url } }],
        [{ id: "copy-link", label: "Copy link address", act: { do: "copy", text: thing.url, said: "Copied the link" } }],
      );
    }
  }
}

/** *Copy full path*: the path inside the checkout on this disk; greyed once the read says there is none. */
function fullPath(path: string, facts: MenuFacts): Item {
  const none = facts.checkout !== undefined && (facts.checkout === null || facts.checkout.path === null);
  return {
    id: "copy-full",
    label: "Copy full path",
    act: { do: "copy", text: { read: "full-path", path }, said: "Copied the full path" },
    disabled: none ? "Not cloned on this machine" : null,
  };
}

/**
 * The code slot: *Open in ⟨default application⟩*, flat, and *Open in… ▸* with every application that opens `at`, then
 * *Add an application* and *Manage applications*, as every *Open in…* ends. Not cloned (the read said so): *Clone
 * ⟨repository⟩* in the default's slot, and *Open in…* greyed. `repoId` is the repository *Clone* and the application
 * page take; `null` for a branch, whose repository the backend finds.
 */
function openIn(entityId: string, repoId: string | null, at: CodeAt, ctx: MenuContext, facts: MenuFacts): Item[] {
  const apps = ctx.apps === null ? [] : at.kind === "commit" ? objectMenu(ctx.apps, "commit") : menuOrder(ctx.apps);
  const notCloned = facts.checkout !== undefined && (facts.checkout === null || facts.checkout.path === null);
  const name = repoId === null ? "" : repoName(keyOf(repoId));
  const run = (app: { id: string; name: string }): Act => ({ do: "app", entityId, app: app.id, name: app.name, at: { ...at } });
  const first = apps[0];
  const slot: Item =
    notCloned && repoId !== null
      ? { id: "clone", label: `Clone ${name}`, act: { do: "clone", repoId } }
      : first
        ? { id: "open-default", label: `Open in ${first.app.name}`, act: run(first.app), disabled: notCloned ? "Not cloned on this machine" : null }
        : { id: "open-default", label: "Open in an application", act: { do: "go", hash: appPageHash({ repo: repoId, ...tryObject(at) }) } };
  const sub: Entry[] = [
    ...apps.map(({ app, isDefault }) => ({ id: `app-${app.id}`, label: app.name, hint: isDefault ? "default" : null, act: run(app) }) satisfies Item),
    ...(apps.length > 0 ? [RULE] : []),
    { id: "add-app", label: "Add an application", act: { do: "go", hash: appPageHash({ repo: repoId, ...tryObject(at) }) } },
    { id: "manage-apps", label: "Manage applications", act: { do: "go", hash: "#/settings/checkouts" } },
  ];
  return [slot, { id: "open-in", label: "Open in…", sub, disabled: notCloned ? "Not cloned on this machine" : null }];
}

/** What the application page tries a new application on, from where the menu was opened. */
function tryObject(at: CodeAt): { kind: "file" | "commit"; value: string } | Record<string, never> {
  if (at.kind === "file") return { kind: "file", value: at.path };
  if (at.kind === "commit") return { kind: "commit", value: at.sha };
  return {};
}
