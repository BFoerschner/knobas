/**
 * The application page (Settings › Checkouts and Open in › *Add an
 * application* / an application's name; PR #2479, 2026-11-13, replacing the
 * dialog of #844, R-8.23), the pure half: its address, what it tries a
 * command on, and how a try is taken back.
 *
 * Its address is `#/settings/checkouts/add` or `#/settings/checkouts/app/<id>`,
 * then optionally the repository to try it on and an object of it:
 * `…/<repo entity id>/<kind>/<value>`, so *Open in… › Add an application* on
 * a commit opens the page with that commit to try.
 */
import type { CheckoutApplication, OpenObject, OpenObjectKind } from "../../ipc/entity";

/** Where the page is: which application, and what *Open in…* it came from. */
export interface AppPageAt {
  /** The application's id, or `null` to add one. */
  app: string | null;
  /** The repository to try it on, when the address names one. */
  repo: string | null;
  /** The object to try, when the address names one. */
  kind: OpenObjectKind | null;
  /** The object's value: a file's path, a commit's sha, a pull request's number. */
  value: string | null;
}

const KINDS: readonly OpenObjectKind[] = ["repository", "file", "commit", "range", "pull_request"];

/** The page's address. */
export function appPageHash(at: Partial<AppPageAt> = {}): string {
  const parts = ["#/settings/checkouts", at.app == null ? "add" : `app/${encodeURIComponent(at.app)}`];
  if (at.repo != null) {
    parts.push(encodeURIComponent(at.repo));
    if (at.kind != null && at.kind !== "repository") {
      parts.push(at.kind);
      if (at.value != null) parts.push(encodeURIComponent(at.value));
    }
  }
  return parts.join("/");
}

/** What follows `#/settings` names the page: `null` for any other place in Settings. */
export function appPagePlace(rest: readonly string[]): AppPageAt | null {
  if (rest[0] !== "checkouts") return null;
  const tail = rest[1] === "add" ? { app: null, after: rest.slice(2) } : rest[1] === "app" && rest[2] ? { app: rest[2], after: rest.slice(3) } : null;
  if (tail === null) return null;
  const [repo, kind, value] = tail.after;
  const known = KINDS.find((one) => one === kind) ?? null;
  return { app: tail.app, repo: repo || null, kind: known, value: known === null ? null : (value ?? null) };
}

/** An application's fields as the page holds them: each a string, blank for none. */
export interface AppDraft {
  name: string;
  command: string;
  file: string;
  commit: string;
  range: string;
  pr: string;
}

/** The page's fields for `app`, or blank ones to add one. */
export function draftFrom(app: CheckoutApplication | null): AppDraft {
  return {
    name: app?.name ?? "",
    command: app?.command ?? "",
    file: app?.file_command ?? "",
    commit: app?.commit_command ?? "",
    range: app?.range_command ?? "",
    pr: app?.pull_request_command ?? "",
  };
}

/** Whether two drafts say the same, blanks and outer spaces aside. */
export function sameDraft(a: AppDraft, b: AppDraft): boolean {
  return (Object.keys(a) as (keyof AppDraft)[]).every((key) => a[key].trim() === b[key].trim());
}

/** The application `draft` makes, under `id` (blank for a new one, which the save names). */
export function entryOf(draft: AppDraft, id: string): CheckoutApplication {
  return {
    id,
    name: draft.name.trim(),
    command: draft.command.trim(),
    file_command: draft.file.trim() || null,
    ...(draft.commit.trim() ? { commit_command: draft.commit.trim() } : {}),
    ...(draft.range.trim() ? { range_command: draft.range.trim() } : {}),
    ...(draft.pr.trim() ? { pull_request_command: draft.pr.trim() } : {}),
  };
}

/** `list` with `entry` in place of the application with its id, or at the end when none has it. */
export function withEntry(list: readonly CheckoutApplication[], entry: CheckoutApplication): CheckoutApplication[] {
  const at = entry.id === "" ? -1 : list.findIndex((app) => app.id === entry.id);
  return at < 0 ? [...list, entry] : list.map((app, index) => (index === at ? entry : app));
}

/**
 * The list a try is taken back to: the tried entry (by the id its save gave
 * it) as it was before the first try, or gone when it was new. Only that
 * entry: anything else changed meanwhile (a default made elsewhere) stays.
 */
export function withoutTry(list: readonly CheckoutApplication[], triedId: string, before: CheckoutApplication | null): CheckoutApplication[] {
  if (before === null) return list.filter((app) => app.id !== triedId);
  return list.map((app) => (app.id === triedId ? before : app));
}

/** The command an object of `kind` runs in `draft`, blank where it has none: a file falls back to the repository's. */
export function draftCommand(draft: AppDraft, kind: OpenObjectKind): string {
  switch (kind) {
    case "repository":
      return draft.command.trim();
    case "file":
      return draft.file.trim() || draft.command.trim();
    case "commit":
      return draft.commit.trim();
    case "range":
      return draft.range.trim();
    case "pull_request":
      return draft.pr.trim();
  }
}

/** The object the page tries a command for `kind` on, or `null` while the picks lack one. */
export function tryObject(
  kind: OpenObjectKind,
  picks: { file: string | null; at: string | null; commits: readonly string[]; commit: string | null; pr: { number: number; base: string; head: string; item: string } | null },
): OpenObject | null {
  switch (kind) {
    case "repository":
      return { kind: "repository" };
    case "file":
      return picks.file === null ? null : { kind: "file", path: picks.file, line: null, at: picks.at };
    case "commit":
      return picks.commit === null ? null : { kind: "commit", sha: picks.commit };
    case "range": {
      // The picked commit and the one before it in the list, which is newest first.
      if (picks.commit === null) return null;
      const before = picks.commits[picks.commits.indexOf(picks.commit) + 1];
      return before === undefined ? null : { kind: "range", base: before, head: picks.commit };
    }
    case "pull_request":
      return picks.pr === null ? null : { kind: "pull_request", number: picks.pr.number, base: picks.pr.base, head: picks.pr.head, branch: picks.pr.head, item: picks.pr.item };
  }
}

/** Browser state: the repository last tried on, the page's default when it came from none. */
export const LAST_TRIED_KEY = "knobas.openin.tried-on";

export function lastTried(): string | null {
  try {
    return localStorage.getItem(LAST_TRIED_KEY);
  } catch {
    return null;
  }
}

export function noteTried(repo: string): void {
  try {
    localStorage.setItem(LAST_TRIED_KEY, repo);
  } catch {
    // A private window: the page falls back to the first repository.
  }
}
