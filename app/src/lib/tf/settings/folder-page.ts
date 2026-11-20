/**
 * A folder renderer's page (ADR-0044), Settings › Renderers › *Add a folder
 * renderer* or a folder renderer's name: the pure half, its address and its
 * draft. As the *Open in…* application page (`openin/app-page.ts`).
 *
 * Its address is `#/settings/renderers/folder/add` or
 * `#/settings/renderers/folder/<id>`, then optionally the repository and the
 * folder to try it on: `…/<repo entity id>/<folder…>`, so the Code page's
 * *Add a folder renderer* opens the page with that folder to try.
 */
import type { FolderOutput, FolderRenderer, FolderRendererDraft } from "../../ipc/app";

/** Where the page is. */
export interface FolderPageAt {
  /** The renderer's id, or `null` to add one. */
  renderer: string | null;
  /** The repository to try it on, when the address names one. */
  repo: string | null;
  /** The folder to try it on, `""` the top; `null` when the address names none. */
  folder: string | null;
}

/** The page's address. */
export function folderPageHash(at: Partial<FolderPageAt> = {}): string {
  const parts = ["#/settings/renderers/folder", at.renderer == null ? "add" : encodeURIComponent(at.renderer)];
  if (at.repo != null) {
    parts.push(encodeURIComponent(at.repo));
    if (at.folder) parts.push(...at.folder.split("/").map(encodeURIComponent));
  }
  return parts.join("/");
}

/** What follows `#/settings` names the page: `null` for any other place in Settings. */
export function folderPagePlace(rest: readonly string[]): FolderPageAt | null {
  if (rest[0] !== "renderers" || rest[1] !== "folder" || !rest[2]) return null;
  const repo = rest[3] || null;
  return { renderer: rest[2] === "add" ? null : rest[2], repo, folder: repo === null ? null : rest.slice(4).join("/") };
}

/** The page's fields, each a string as typed. */
export interface FolderDraft {
  name: string;
  command: string;
  holds: string;
  folders: string;
  output: FolderOutput;
  timeout: string;
}

const words = (list: readonly string[]) => list.join(" ");
const split = (typed: string) => typed.split(/\s+/).map((one) => one.trim()).filter((one) => one !== "");

/** The fields for `saved`, or blank ones to add one. */
export function folderDraftFrom(saved: FolderRenderer | null): FolderDraft {
  return {
    name: saved?.name ?? "",
    command: saved?.command ?? "",
    holds: words(saved?.holds ?? []),
    folders: words(saved?.folders ?? []),
    output: saved?.output ?? "detect",
    timeout: String(saved?.timeout_secs ?? 30),
  };
}

/** Whether two drafts say the same, spaces aside. */
export function sameFolderDraft(a: FolderDraft, b: FolderDraft): boolean {
  return (Object.keys(a) as (keyof FolderDraft)[]).every((key) => String(a[key]).trim().split(/\s+/).join(" ") === String(b[key]).trim().split(/\s+/).join(" "));
}

/** The saved list with `draft` in its place (`id` blank for a new one, which goes last). */
export function withFolderDraft(list: readonly FolderRenderer[], draft: FolderDraft, id: string | null): FolderRendererDraft[] {
  const timeout = Number.parseInt(draft.timeout, 10);
  const entry: FolderRendererDraft = {
    id,
    name: draft.name.trim(),
    command: draft.command.trim(),
    holds: split(draft.holds),
    folders: split(draft.folders),
    // `{outdir}` decides the kind, as the backend's `settle` does.
    output: makesFolder(draft.command) ? "folder" : draft.output,
    timeout_secs: Number.isFinite(timeout) ? timeout : null,
  };
  const kept: FolderRendererDraft[] = list.map((r) => ({ ...r }));
  const at = id === null ? -1 : kept.findIndex((r) => r.id === id);
  if (at === -1) return [...kept, entry];
  kept[at] = entry;
  return kept;
}

/** The placeholders a folder renderer takes, and what each is. */
export const FOLDER_PLACEHOLDERS = [
  { name: "{path}", says: "the folder: a copy knobas writes at the commit shown" },
  { name: "{out}", says: "a file to write the output to, instead of printing it" },
  { name: "{outdir}", says: "an empty folder to write files into: they are browsed in the folder's place" },
] as const;

const TAKEN = new Set(["{path}", "{out}", "{outdir}"]);

/** Every `{word}` the command holds that knobas does not fill. */
export function unknownPlaceholders(command: string): string[] {
  return [...command.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map((m) => m[0]).filter((p) => !TAKEN.has(p));
}

/** Whether the command holds `{outdir}`, which makes it a folder of files (ADR-0046). */
export function makesFolder(command: string): boolean {
  return command.includes("{outdir}");
}

/**
 * What the command still needs, in words, or `null`; in the backend's words
 * (`folder_render::check_placeholders` and `settle`). `output` is *It makes*:
 * a folder of files with no `{outdir}` is said.
 */
export function commandWhy(command: string, output?: FolderOutput): string | null {
  const trimmed = command.trim();
  if (trimmed === "") return null;
  const unknown = unknownPlaceholders(trimmed);
  if (unknown.length > 0) return `${unknown[0]} is not something a folder renderer is given: it takes {path}, and {out} or {outdir}.`;
  if (trimmed.includes("{out}") && makesFolder(trimmed)) return "{out} is one file and {outdir} is a folder of files: use one of them.";
  if (!trimmed.includes("{path}")) return "Put {path} where the folder goes: the command runs in an empty folder of its own.";
  if (output === "folder" && !makesFolder(trimmed)) return "Put {outdir} where the tool takes its output folder.";
  return null;
}

/** The output kinds, as Settings names them. */
export const OUTPUTS: readonly { value: FolderOutput; label: string }[] = [
  { value: "detect", label: "Tell from what it makes" },
  { value: "image", label: "An image (PNG, JPEG, GIF, WebP or SVG)" },
  { value: "text", label: "Text" },
  { value: "markdown", label: "Markdown" },
  { value: "html", label: "A page (HTML, drawn without scripts)" },
];

/** What a renderer takes, in words, for the table. */
export function takesLine(r: Pick<FolderRenderer, "holds" | "folders">): string {
  const holds = r.holds.length === 0 ? null : `holding ${r.holds.join(" or ")}`;
  const folders = r.folders.length === 0 ? null : `named ${r.folders.join(" or ")}`;
  if (holds === null && folders === null) return "Every folder";
  return `Folders ${[folders, holds].filter((one) => one !== null).join(", ")}`;
}
