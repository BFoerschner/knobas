/**
 * A command's page (ADR-0060, section 27), Settings › Commands › *Add a command* or a command's *Edit…*: the pure half,
 * its address and its draft, as the folder renderer's page (`folder-page.ts`).
 *
 * Its address is `#/settings/commands/new` or `#/settings/commands/<id>`, then optionally the repository and the file
 * or folder to try it on: `…/<repo entity id>/<path…>`, so a menu's *Add a command* opens the page with that thing.
 */
import type { OneOff, OneOffDraft, OneOffKind, OneOffShows, OneOffStartIn, OneOffStdin } from "../../ipc/oneoff";
import { check, kindOf, many } from "../../render/oneoff";

/** Where the page is. */
export interface CommandPageAt {
  /** The command's id, or `null` to add one. */
  command: string | null;
  /** The repository to try it on, when the address names one. */
  repo: string | null;
  /** The file or folder to try it on, `""` the top; `null` when the address names none. */
  on: string | null;
  /** One of {@link READY_TO_ADD} filled in, for a new one: `#/settings/commands/example/<n>`. */
  example?: number | null;
}

/** The page's address. */
export function commandPageHash(at: Partial<CommandPageAt> = {}): string {
  const parts = ["#/settings/commands", at.command == null ? "new" : encodeURIComponent(at.command)];
  if (at.repo != null) {
    parts.push(encodeURIComponent(at.repo));
    if (at.on) parts.push(...at.on.split("/").map(encodeURIComponent));
  }
  return parts.join("/");
}

/** What follows `#/settings` names the page: `null` for any other place in Settings. */
export function commandPagePlace(rest: readonly string[]): CommandPageAt | null {
  if (rest[0] !== "commands" || !rest[1]) return null;
  if (rest[1] === "example") {
    const n = Number(rest[2]);
    return Number.isInteger(n) && n >= 0 && n < READY_TO_ADD.length && rest.length === 3 ? { command: null, repo: null, on: null, example: n } : null;
  }
  const repo = rest[2] || null;
  return { command: rest[1] === "new" ? null : rest[1], repo, on: repo === null ? null : rest.slice(3).join("/") };
}

/** The page's fields, each as typed. */
export interface CommandDraft {
  name: string;
  command: string;
  stdin: OneOffStdin;
  shows: OneOffShows;
  timeout: string;
  files: string;
  folders: string;
  has: string;
  repoHolds: string;
  startIn: OneOffStartIn;
  path: string;
  env: string;
  colours: boolean;
  wslPaths: boolean;
}

const words = (list: readonly string[]) => list.join(" ");
const split = (typed: string) => typed.split(/\s+/).map((one) => one.trim()).filter((one) => one !== "");

/** The fields for `saved`, or blank ones (or an example's) to add one. */
export function commandDraftFrom(saved: Partial<OneOff> | null): CommandDraft {
  return {
    name: saved?.name ?? "",
    command: saved?.command ?? "",
    stdin: saved?.stdin ?? "none",
    shows: saved?.shows ?? "auto",
    timeout: String(saved?.timeout_secs ?? 120),
    files: words(saved?.files ?? []),
    folders: words(saved?.folders ?? []),
    has: words(saved?.has ?? []),
    repoHolds: words(saved?.repo_holds ?? []),
    startIn: saved?.start_in ?? "root",
    path: (saved?.path ?? []).join("\n"),
    env: (saved?.env ?? []).map((v) => `${v.name}=${v.value}`).join("\n"),
    colours: saved?.colours ?? true,
    wslPaths: saved?.wsl_paths ?? false,
  };
}

/** Whether two drafts would save the same command. */
export function sameCommandDraft(a: CommandDraft, b: CommandDraft): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** A saved command as a draft the backend takes. */
export function draftOf(c: OneOff): OneOffDraft {
  return { ...c, id: c.id, colours: c.colours, timeout_secs: c.timeout_secs };
}

/** The draft as the backend takes it, under `id` (`null` for a new one). */
export function toDraft(draft: CommandDraft, id: string | null, repos: readonly string[] = []): OneOffDraft {
  const env = draft.env
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .map((line) => {
      const at = line.indexOf("=");
      return at < 0 ? { name: line, value: "" } : { name: line.slice(0, at).trim(), value: line.slice(at + 1) };
    });
  const seconds = Number(draft.timeout);
  return {
    id,
    name: draft.name.trim(),
    command: draft.command.trim(),
    stdin: draft.stdin,
    files: split(draft.files),
    folders: split(draft.folders),
    has: split(draft.has),
    repo_holds: split(draft.repoHolds),
    repos: [...repos],
    shows: draft.shows,
    start_in: draft.startIn,
    path: draft.path.split("\n").map((p) => p.trim()).filter((p) => p !== ""),
    env,
    colours: draft.colours,
    timeout_secs: Number.isFinite(seconds) && seconds > 0 ? Math.min(3600, Math.round(seconds)) : null,
    wsl_paths: draft.wslPaths,
  };
}

/** The list with the draft in `id`'s place, or at its end for a new one. */
export function withCommandDraft(list: readonly OneOff[], draft: CommandDraft, id: string | null): OneOffDraft[] {
  const own = id === null ? null : list.find((c) => c.id === id);
  const next = toDraft(draft, id, own?.repos ?? []);
  const drafts = list.map(draftOf);
  if (id === null || own === undefined) return [...drafts, next];
  return drafts.map((d) => (d.id === id ? next : d));
}

/** The placeholders the page offers, in its order, each with what it is. */
export const COMMAND_PLACEHOLDERS: readonly { name: string; says: string }[] = [
  { name: "{file}", says: "the file" },
  { name: "{relfile}", says: "the file, from the top" },
  { name: "{files}", says: "every file of a batch" },
  { name: "{dir}", says: "the folder" },
  { name: "{reldir}", says: "the folder, from the top" },
  { name: "{root}", says: "the repository" },
  { name: "{line}", says: "the first selected line" },
  { name: "{endline}", says: "the last selected line" },
  { name: "{sha}", says: "the commit" },
  { name: "{base}", says: "where a comparison starts" },
  { name: "{head}", says: "where it ends" },
  { name: "{branch}", says: "the checkout's branch" },
  { name: "{out}", says: "a file to write a result to" },
  { name: "{outdir}", says: "a folder to write files into" },
];

/** *It shows*, in the page's order. */
export const SHOWS: readonly { value: OneOffShows; label: string }[] = [
  { value: "auto", label: "As the bytes say" },
  { value: "text", label: "Text" },
  { value: "problems", label: "Problems first" },
  { value: "code", label: "Code, as the file it ran on" },
  { value: "diff", label: "A diff" },
  { value: "json", label: "JSON" },
  { value: "markdown", label: "Markdown" },
  { value: "page", label: "A page" },
  { value: "image", label: "An image" },
  { value: "pdf", label: "A PDF" },
];

/** What a kind reads as: *A file*, *A folder*. */
export const KIND_WORDS: Record<OneOffKind, string> = {
  file: "A file",
  folder: "A folder",
  repository: "The repository",
  commit: "A commit",
  range: "A comparison",
};

/** *It is offered on*, one line read from the placeholders and the globs as they are typed; `null` while it cannot run. */
export function offeredOn(draft: Pick<CommandDraft, "command" | "stdin" | "files" | "folders" | "has" | "repoHolds">): string | null {
  if (draft.command.trim() === "" || check(draft.command, draft.stdin) !== null) return null;
  const kind = kindOf(draft.command, draft.stdin);
  if (kind === null) return null;
  const parts = [KIND_WORDS[kind]];
  if (kind === "file" && split(draft.files).length > 0) parts.push(`matching ${split(draft.files).join(" ")}`);
  if (kind === "folder" && split(draft.folders).length > 0) parts.push(`named ${split(draft.folders).join(" ")}`);
  if (kind === "folder" && split(draft.has).length > 0) parts.push(`holding ${split(draft.has).join(" ")}`);
  if (split(draft.repoHolds).length > 0) parts.push(`where the repository holds ${split(draft.repoHolds).join(" ")}`);
  return `${parts.join(", ")}.`;
}

/** Why the page cannot be saved, in its words; `null` when it can. */
export function commandWhy(draft: CommandDraft): string | null {
  if (draft.name.trim() === "") return draft.command.trim() === "" ? "Name it, and give its command." : "Name it first.";
  if (draft.command.trim() === "") return "Give its command.";
  return check(draft.command, draft.stdin);
}

/** Six examples that save nothing until pressed (*Ready to add*). */
export const READY_TO_ADD: readonly (Partial<OneOff> & { name: string; command: string })[] = [
  { name: "Ruff", command: "ruff check --output-format concise {file}", files: ["*.py"] },
  { name: "ShellCheck", command: "shellcheck -f gcc {file}", files: ["*.sh", "*.bash"] },
  { name: "Clippy", command: "cargo clippy --all-targets", repo_holds: ["Cargo.toml"] },
  { name: "Type check", command: "npx tsc --noEmit -p .", repo_holds: ["tsconfig.json"], shows: "problems" },
  { name: "Format check", command: "npx prettier --check {file}", files: ["*.ts", "*.tsx", "*.js", "*.json", "*.css", "*.md"], repo_holds: ["package.json"] },
  { name: "Blame the selected lines", command: "git blame -L {line},{endline} -- {relfile}" },
];

/** How a command is offered, for the table's *On* column. */
export function onWords(c: OneOff): string {
  const kind = kindOf(c.command, c.stdin);
  const base = kind === null ? "Nothing" : KIND_WORDS[kind];
  const globs = kind === "file" ? c.files : kind === "folder" ? [...c.folders, ...c.has] : [];
  return globs.length > 0 ? `${base}: ${globs.join(" ")}` : base;
}

/** The table's count line under its heading. */
export function countLine(list: readonly OneOff[]): string {
  return list.length === 0 ? "No command yet: the menus offer none until you add one." : `${many(list.length, "command", "commands")}, in the order the menus offer them.`;
}
