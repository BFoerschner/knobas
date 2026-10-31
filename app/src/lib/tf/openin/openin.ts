/**
 * *Open in…* and *Clone* (#844, #846; R-8.21 to R-8.25), the pure half: what
 * the menu, the dialogs and the clone button say.
 *
 * Nothing here runs anything. `open_checkout` fills a command on the backend
 * from this disk (`knobas_core::checkout::open`); what is filled here is only
 * drawn, as prototype-6's `fill`.
 */
import { keyOf } from "../../time/format";
import { caretOf, isSha } from "../diff/sha";
import type {
  CheckoutApplication,
  CheckoutSettings,
  CloneFailureCause,
  DiffFile,
  OpenObject,
  OpenObjectKind,
} from "../../ipc/entity";

/** The placeholders, in the dialog's order, with what each one is (R-8.23, #2274). */
export const PLACEHOLDERS: readonly { name: string; says: string }[] = [
  { name: "{path}", says: "the checkout folder" },
  { name: "{file}", says: "the file you have open, inside the checkout" },
  { name: "{line}", says: "the line you are on, or 1" },
  { name: "{name}", says: "the repository name" },
  { name: "{sha}", says: "the commit, in a commit command" },
  { name: "{base}", says: "where a range or pull request starts" },
  { name: "{head}", says: "where it ends" },
  { name: "{pr}", says: "the pull request’s number" },
  { name: "{branch}", says: "the pull request’s branch" },
  { name: "{url}", says: "its page at the source" },
];

/** A checkout the dialog's preview fills from and *Try it* runs at. */
export interface Sample {
  entityId: string;
  /** The repository's own name, for *Try it on payout-service*. */
  name: string;
  /** The checkout path on this disk. */
  path: string;
  /** A file inside it, relative, for the file command's preview. */
  file: string | null;
}

/** What the placeholders are drawn as. */
export interface FillContext {
  /** The checkout path. */
  path: string;
  /** A file inside it, as a whole path; `{file}` is the checkout without one. */
  file: string | null;
  /** `{line}` is 1 without one. */
  line: number | null;
}

/** The last part of a path: what `{name}` is, the checkout folder's own name. */
export function folderName(path: string): string {
  const parts = path.split("/").filter((part) => part !== "");
  return parts[parts.length - 1] ?? "";
}

/** A command with its placeholders drawn in, as prototype-6's `fill`. */
export function fill(command: string, at: FillContext): string {
  return command
    .replace(/\{path\}/g, at.path)
    .replace(/\{file\}/g, at.file ?? at.path)
    .replace(/\{line\}/g, String(at.line ?? 1))
    .replace(/\{name\}/g, folderName(at.path));
}

/** A word with a space in it, quoted as a person would type it. */
export function quoteIfSpace(word: string): string {
  return /\s/.test(word) ? `'${word}'` : word;
}

/** An argument vector as one line, for *Started VS Code: …*. */
export function argvText(argv: readonly string[]): string {
  return argv.map(quoteIfSpace).join(" ");
}

/** The command an application runs here: its file command at a file, when it has one. */
export function commandFor(app: CheckoutApplication, file: string | null): string {
  return file !== null && app.file_command ? app.file_command : app.command;
}

/** The menu's entries: the default first, then the rest in the list's order (R-8.22). */
export function menuOrder(settings: CheckoutSettings): { app: CheckoutApplication; isDefault: boolean }[] {
  const rows = settings.applications.map((app) => ({ app, isDefault: app.id === settings.default_application }));
  return [...rows.filter((row) => row.isDefault), ...rows.filter((row) => !row.isDefault)];
}

/** The default application, else the first. */
export function defaultApp(settings: CheckoutSettings | null): CheckoutApplication | null {
  if (settings === null) return null;
  return (
    settings.applications.find((app) => app.id === settings.default_application) ?? settings.applications[0] ?? null
  );
}

/** The dialog's warnings for what is typed (R-8.23), in prototype-6's words. */
export function warnings(command: string): string[] {
  const out: string[] = [];
  if (command !== "" && !/\{path\}|\{name\}/.test(command)) {
    out.push("The command has no {path}, so the program won’t know which repository to open.");
  }
  if (/\{file\}|\{line\}/.test(command)) {
    out.push("{file} and {line} belong in “For a file”; outside a file they become the checkout path and 1.");
  }
  if (/[|;&><`$]/.test(command)) {
    out.push("Shell characters like | ; & > are passed through as plain text. Use a script for anything shell-like.");
  }
  return out;
}

/**
 * A placeholder put in at the cursor, with a space before it when the text
 * there does not end in one, as prototype-6's buttons do. Answers the new
 * value and where the cursor goes.
 */
export function insertAt(value: string, at: number | null, placeholder: string): { value: string; cursor: number } {
  const cut = at ?? value.length;
  const before = value.slice(0, cut);
  const gap = before !== "" && !/\s$/.test(before) ? " " : "";
  const inserted = `${gap}${placeholder}`;
  return { value: before + inserted + value.slice(cut), cursor: cut + inserted.length };
}

/** *Clone*'s tooltip: the folder first (R-8.25). */
export function cloneTip(root: string | null, repoName: string): string {
  return `Clones into ${root ?? "your clones root"}/${repoName}`;
}

/** The repository's own name: the part after the owner. */
export function repoName(title: string): string {
  return title.split("/").pop() ?? title;
}

/** The host of a URL, for the sign-in failure; the URL itself when it does not parse. */
function hostOf(url: string | null): string {
  if (url === null) return "the server";
  try {
    return new URL(url).host || url;
  } catch {
    const ssh = /@([^:/]+)[:/]/.exec(url);
    return ssh?.[1] ?? url;
  }
}

/** A failed clone said in place, with what to do (R-8.25). */
export function cloneFailure(cause: CloneFailureCause, reason: string, repoUrl: string | null): string {
  if (cause === "auth") {
    return `git could not sign in to ${hostOf(repoUrl)}. knobas uses your own git credentials (ssh agent or credential helper); check them in a terminal`;
  }
  if (cause === "disk_full") return "The disk is full. Free some space, then try again.";
  return reason;
}

// -- code objects (#2274) ------------------------------------------------------

/** What each object's placeholders are, for the dialog and the preview. */
export const OBJECT_PLACEHOLDERS: Readonly<Record<OpenObjectKind, readonly string[]>> = {
  repository: ["{path}", "{file}", "{line}", "{name}", "{url}"],
  file: ["{path}", "{file}", "{line}", "{name}", "{url}"],
  commit: ["{path}", "{name}", "{sha}", "{url}"],
  range: ["{path}", "{name}", "{base}", "{head}", "{url}"],
  pull_request: ["{path}", "{name}", "{pr}", "{base}", "{head}", "{branch}", "{url}"],
};

/** The command an application runs for an object of `kind`, or `null` when it does not accept one. */
export function objectCommand(app: CheckoutApplication, kind: OpenObjectKind): string | null {
  switch (kind) {
    case "repository":
      return app.command;
    case "file":
      return app.file_command ?? app.command;
    case "commit":
      return app.commit_command ?? null;
    case "range":
      return app.range_command ?? null;
    case "pull_request":
      return app.pull_request_command ?? null;
  }
}

/** The menu for an object: the applications that accept it, the default first (R-8.22). */
export function objectMenu(settings: CheckoutSettings, kind: OpenObjectKind): { app: CheckoutApplication; isDefault: boolean }[] {
  return menuOrder(settings).filter((row) => objectCommand(row.app, kind) !== null);
}

/** What an object's command is drawn with: the checkout, and the object's own values. */
export interface ObjectFill {
  path: string;
  object: OpenObject;
  url: string | null;
}

/** A command with its placeholders drawn in for an object, as the backend fills them. */
export function fillObject(command: string, at: ObjectFill): string {
  const { object } = at;
  const file = object.kind === "file" ? `${at.path}/${object.path}` : null;
  const line = object.kind === "file" ? object.line : null;
  let out = fill(command, { path: at.path, file, line });
  const value = (name: string, given: string | number | null | undefined) => {
    if (given !== null && given !== undefined) out = out.replaceAll(`{${name}}`, String(given));
  };
  value("url", at.url);
  if (object.kind === "commit") value("sha", object.sha);
  if (object.kind === "range" || object.kind === "pull_request") {
    value("base", object.base);
    value("head", object.head);
  }
  if (object.kind === "pull_request") {
    value("pr", object.number);
    value("branch", object.branch);
  }
  return out;
}

/** The line a diff file opens at: its first added line, else its first line after the change. */
export function firstLine(file: DiffFile): number | null {
  const lines = file.hunks.flatMap((hunk) => hunk.lines);
  return lines.find((line) => line.kind === "add")?.new ?? lines.find((line) => line.new !== null)?.new ?? null;
}

// -- a pull request's worktree and the Diff tool (#2275, #2285) ----------------

/**
 * Where an application opens pull request `number` of repository `repoEntityId`
 * (ADR-0035): `<clones root>/pull-requests/<owner>/<repo>-<number>`, as the
 * backend's `knobas_core::checkout::worktree::folder` puts it. `null` without a
 * clones root or an `owner/repo` key.
 */
export function worktreeOf(clonesRoot: string | null, repoEntityId: string | null, number: number): string | null {
  if (clonesRoot === null || repoEntityId === null) return null;
  const [owner, repo, ...rest] = keyOf(repoEntityId).split("/");
  if (!owner || !repo || rest.length > 0) return null;
  return `${clonesRoot.replace(/\/+$/, "")}/pull-requests/${owner}/${repo}-${number}`;
}

/** Whether `end` is a sha the Diff tool takes: 7 to 64 hex digits, a base with one `^` allowed. */
export function isDiffEnd(end: string, base: boolean): boolean {
  return isSha(base ? caretOf(end)[0] : end);
}
