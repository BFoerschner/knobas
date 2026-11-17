/**
 * Folder renderers (ux17, ADR-0044), the pure half: which saved renderer takes
 * a folder, and what a run's state and failure read as.
 *
 * A renderer takes a folder when both of its lists hold, an empty one always:
 * `folders`, globs its path matches (one with no `/` against the folder's own
 * name), and `holds`, globs one of its own files matches. The backend
 * answers the second for a whole tree at once (`folder_render_marks`), so the
 * tree marks a folder before its listing is read; the first is matched here,
 * as the backend matches it (`folder_render::folder_matches`).
 */
import type { FolderFailure, FolderMarks, FolderRenderer, FolderRunState, ReadFrom } from "../ipc/app";
import { secondsLabel } from "../time/format";
import { thousands } from "../tf/counted";

function glob(pattern: string): RegExp {
  const body = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp(`^${body}$`, "i");
}

/** Whether folder glob `pattern` takes the folder at `path` (`""` the top). */
export function folderMatches(pattern: string, path: string): boolean {
  const target = pattern.includes("/") ? path : path.slice(path.lastIndexOf("/") + 1);
  return glob(pattern).test(target);
}

/** The folders each renderer holding a file glob takes, by renderer id, from the backend's marks. */
export function markSets(marks: readonly FolderMarks[]): Map<string, ReadonlySet<string>> {
  return new Map(marks.map((m) => [m.renderer, new Set(m.folders)]));
}

/**
 * The saved renderers that take the folder at `path`, in the person's order. `marks` is the backend's answer for
 * the tree; a renderer holding a file glob that the marks do not name (not read yet) does not take it.
 */
export function renderersFor(path: string, saved: readonly FolderRenderer[], marks: ReadonlyMap<string, ReadonlySet<string>>): FolderRenderer[] {
  return saved.filter((r) => {
    if (r.folders.length > 0 && !r.folders.some((g) => folderMatches(g, path))) return false;
    return r.holds.length === 0 || (marks.get(r.id)?.has(path) ?? false);
  });
}

const where = (from: ReadFrom | null) => (from === "forge" ? "the source" : "your clone");

/** What a run is doing, in a line. */
export function runningLine(name: string, state: FolderRunState, from: ReadFrom | null): string {
  switch (state.state) {
    case "queued":
      return `${name} waits for another folder renderer to finish.`;
    case "copying":
      return state.total === 0
        ? `Copying the folder from ${where(from)}…`
        : `Copying ${thousands(state.done)} of ${thousands(state.total)} files from ${where(from)}…`;
    case "running":
      return `Running ${name}…`;
  }
}

const mib = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0)} MB`;

/** Why a run drew nothing, in a line, and the stderr to show under it. */
export function failureLines(failure: FolderFailure): { line: string; stderr: string } {
  switch (failure.why) {
    case "failed":
      return {
        line: failure.status === null ? "The command was stopped by a signal." : `The command exited with status ${failure.status}.`,
        stderr: failure.stderr,
      };
    case "timed_out":
      return { line: `The command took longer than its timeout of ${secondsLabel(failure.seconds)} and was stopped.`, stderr: failure.stderr };
    case "too_large":
      return { line: `The command made more than ${mib(failure.cap_bytes)} and was stopped.`, stderr: "" };
    case "not_started":
      return { line: failure.reason, stderr: "" };
    case "no_output_file":
      return {
        line:
          failure.found === 0
            ? "The command ended without writing a file to {out}."
            : `The command wrote ${failure.found} files to {out}; knobas draws one.`,
        stderr: failure.stderr,
      };
    case "copy":
      return { line: `The folder could not be copied: ${failure.reason}.`, stderr: "" };
    case "too_big":
      return {
        line:
          `The folder holds ${thousands(failure.files)} files and ${mib(failure.bytes)}; ` +
          `knobas copies up to ${thousands(failure.cap_files)} files and ${mib(failure.cap_bytes)} from ${where(failure.from)}.`,
        stderr: "",
      };
    case "no_folder":
      return { line: "There is no folder at this path at this commit.", stderr: "" };
    case "stopped":
      return { line: "Stopped.", stderr: "" };
  }
}

/** How long a run took, as the app writes a duration: `0.4s`, `12s`, `2m 5s`. */
export function tookLabel(ms: number): string {
  return ms < 10_000 ? `${(ms / 1000).toFixed(1)}s` : secondsLabel(Math.round(ms / 1000));
}
