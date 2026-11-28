/**
 * *Try it on* (#2562): what a settings page's panel holds, why its *Try it* waits, and what it sends.
 *
 * One panel (`TfTryOn.svelte`) on every renderer's, pipeline's, folder renderer's, command's and application's settings
 * page. The example is one of:
 *
 * - **pasted text** under a file name, whose ending picks the renderers that take it as a file's does in Code;
 * - **a file** or **a folder** chosen with the native picker, anywhere on this disk, read on the press that tries it;
 * - **a repository** with a checkout here, where the page offered one before (a folder renderer, a command, an
 *   application): its own rows, drawn in the panel.
 *
 * Nothing here reads or writes: the page saves its draft (only a saved one runs, ADR-0016) and sends {@link tryOnOf}.
 */
import type { TryOn } from "../../ipc/app";

export type TrySource = "text" | "file" | "folder" | "repo";

/** What the panel holds: the source picked, and what each source was given. */
export interface TryDraft {
  source: TrySource;
  /** The pasted text's file name. */
  name: string;
  text: string;
  /** The chosen file's and folder's paths, as the picker answered them. */
  file: string | null;
  folder: string | null;
}

/** The most pasted text a try sends: what Code hands a renderer (`try_on::BYTES_CAP`). */
export const TEXT_CAP = 50 * 1024 * 1024;

/** The source buttons' words, in the order a page offers them. */
export const SOURCE_WORDS: Record<TrySource, string> = {
  text: "Pasted text",
  file: "A file",
  folder: "A folder",
  repo: "A repository",
};

export function tryDraft(source: TrySource, name = "example.md"): TryDraft {
  return { source, name, text: "", file: null, folder: null };
}

/** The pasted text's size in bytes, as it is sent. */
export function textBytes(text: string): number {
  return new TextEncoder().encode(text).length;
}

/**
 * Why *Try it* waits, in words, or `null` when it can go: the page's own reason first (the draft is not ready), then
 * what the source picked still needs. A repository's rows have their own *Try it*, so its source never waits here.
 */
export function tryWhy(draft: TryDraft, page: string | null): string | null {
  if (page !== null) return page;
  switch (draft.source) {
    case "text": {
      const name = draft.name.trim();
      if (name === "") return "Give the text a file name, such as example.md.";
      if (name.includes("/") || name.includes("\\")) return "A file name holds no / or \\.";
      if (draft.text === "") return "Paste some text first.";
      const size = textBytes(draft.text);
      if (size > TEXT_CAP) return `The text is ${Math.ceil(size / (1024 * 1024))} MB, past the 50 MB knobas hands a renderer.`;
      return null;
    }
    case "file":
      return draft.file === null ? "Choose a file first." : null;
    case "folder":
      return draft.folder === null ? "Choose a folder first." : null;
    case "repo":
      return null;
  }
}

/** What the press sends for `draft`'s source; `null` for a repository, which the page tries on its own rows. */
export function tryOnOf(draft: TryDraft): TryOn | null {
  switch (draft.source) {
    case "text":
      return { kind: "text", name: draft.name.trim(), text: draft.text };
    case "file":
      return draft.file === null ? null : { kind: "path", path: draft.file };
    case "folder":
      return draft.folder === null ? null : { kind: "path", path: draft.folder };
    case "repo":
      return null;
  }
}

/** The last name of a chosen path, either separator. */
export function leafOf(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.slice(Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\")) + 1) || trimmed;
}

/** What a try was on, as a person reads it: the pasted text's name, else the chosen path's last name. */
export function tryName(on: TryOn): string {
  return on.kind === "text" ? on.name : leafOf(on.path);
}

/** Whether `bytes` read as text: valid UTF-8 with no NUL in its first 8,000 bytes, as git tells a binary file. */
export function isText(bytes: Uint8Array): boolean {
  if (bytes.subarray(0, 8000).includes(0)) return false;
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return true;
  } catch {
    return false;
  }
}
