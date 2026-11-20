/** Folder renderers' pure half (ADR-0044): which renderer takes a folder, and the lines a run reads as. */
import { expect, test } from "vitest";

import type { FolderRenderer } from "../ipc/app";
import { failureLines, folderMatches, markSets, renderersFor, runningLine, tookLabel } from "./folder";
import { folderRenderHash, readRepoRoute } from "../tf/repo/repo";

const R = (id: string, over: Partial<FolderRenderer> = {}): FolderRenderer => ({ id, name: id, command: "t {path}", holds: [], folders: [], output: "detect", timeout_secs: 30, ...over });

test("a folder glob matches the folder's name, or with a slash its whole path, as the backend does", () => {
  expect(folderMatches("flows", "app/flows")).toBe(true);
  expect(folderMatches("flows", "flows/app")).toBe(false);
  expect(folderMatches("app/*", "app/flows")).toBe(true);
  expect(folderMatches("app/*", "lib/app/flows")).toBe(false);
  expect(folderMatches("FL*", "x/flows")).toBe(true);
});

test("a renderer takes a folder when both lists hold, the file glob by the backend's marks", () => {
  const every = R("every");
  const named = R("named", { folders: ["flows"] });
  const holding = R("holding", { holds: ["*.lcproj"] });
  const marks = markSets([{ renderer: "holding", folders: ["app/flows", ""] }]);
  const saved = [every, named, holding];
  expect(renderersFor("app/flows", saved, marks).map((r) => r.id)).toEqual(["every", "named", "holding"]);
  expect(renderersFor("app/other", saved, marks).map((r) => r.id)).toEqual(["every"]);
  expect(renderersFor("", saved, marks).map((r) => r.id)).toEqual(["every", "holding"]);
  // Marks not read yet: a renderer holding a glob takes nothing.
  expect(renderersFor("app/flows", saved, new Map()).map((r) => r.id)).toEqual(["every", "named"]);
});

test("a run's state and its failures read as lines", () => {
  expect(runningLine("Lc", { state: "copying", done: 1200, total: 3400 }, "forge")).toBe("Copying 1,200 of 3,400 files from the source…");
  expect(runningLine("Lc", { state: "running", elapsed_ms: 10 }, "clone")).toBe("Running Lc…");
  expect(failureLines({ why: "failed", status: null, stderr: "x" })).toEqual({ line: "The command was stopped by a signal.", stderr: "x" });
  expect(failureLines({ why: "no_output_file", found: 2, stderr: "" }).line).toBe("The command wrote 2 files to {out}; knobas draws one.");
  expect(failureLines({ why: "timed_out", seconds: 30, stderr: "s" }).line).toContain("timeout of 30s");
  expect(failureLines({ why: "copy", reason: "../x climbs out of the folder" }).line).toBe("The folder could not be copied: ../x climbs out of the folder.");
  // A folder of files (ADR-0046): keeping, and the three failures of its own.
  expect(runningLine("Lc", { state: "keeping", done: 120, total: 2000 }, "clone")).toBe("Keeping 120 of 2,000 files…");
  expect(runningLine("Lc", { state: "keeping", done: 0, total: 0 }, "clone")).toBe("Keeping what it made…");
  expect(failureLines({ why: "no_output", stderr: "e" })).toEqual({ line: "The command ended without writing a file into {outdir}.", stderr: "e" });
  expect(failureLines({ why: "output_too_big", files: 24310, bytes: 1.4 * 1024 ** 3, cap_files: 20000, cap_bytes: 1024 ** 3 }).line).toBe(
    "The command made 24,310 files and 1.4 GB; knobas keeps up to 20,000 files and 1,024 MB.",
  );
  expect(failureLines({ why: "keep", reason: "D:\\code is gone" }).line).toBe("What it made could not be kept: D:\\code is gone.");
  expect(tookLabel(1234)).toBe("1.2s");
  expect(tookLabel(65_000)).toBe("1m 5s");
  expect(tookLabel(120_000)).toBe("2m");
});

test("a folder render's address names the renderer, then the folder, and reads back", () => {
  const hash = folderRenderHash("gitea:repo:o/r", "low code", "apps/my flow");
  expect(hash).toBe("#/repo/gitea%3Arepo%3Ao%2Fr/render/low%20code/apps/my%20flow");
  const rest = hash.slice(2).split("/").map(decodeURIComponent).slice(1);
  const route = readRepoRoute(rest);
  expect(route).toMatchObject({ repoId: "gitea:repo:o/r", tab: "files", path: null, render: { renderer: "low code", folder: "apps/my flow" } });
  expect(readRepoRoute(["r", "render", "x"]).render).toEqual({ renderer: "x", folder: "" });
  expect(readRepoRoute(["r", "files", "a.md"]).render).toBeUndefined();
});
