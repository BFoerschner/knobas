/** A folder renderer's page, the pure half (ADR-0044): its address, its draft, and what the command still needs. */
import { expect, test } from "vitest";

import type { FolderRenderer } from "../../ipc/app";
import { commandWhy, folderDraftFrom, folderPageHash, folderPagePlace, sameFolderDraft, takesLine, withFolderDraft } from "./folder-page";
import { settingsKnows } from "./settings";

const SAVED: FolderRenderer = { id: "lc", name: "Low code", command: "lc {path} {out}.png", holds: ["*.lcproj"], folders: [], output: "image", timeout_secs: 60 };

test("the page's address reads back, with a repository and folder to try", () => {
  expect(folderPageHash()).toBe("#/settings/renderers/folder/add");
  const hash = folderPageHash({ renderer: "lc", repo: "gitea:repo:o/r", folder: "apps/flows" });
  expect(hash).toBe("#/settings/renderers/folder/lc/gitea%3Arepo%3Ao%2Fr/apps/flows");
  const rest = hash.slice("#/settings/".length).split("/").map(decodeURIComponent);
  expect(folderPagePlace(rest)).toEqual({ renderer: "lc", repo: "gitea:repo:o/r", folder: "apps/flows" });
  expect(folderPagePlace(["renderers", "folder", "add"])).toEqual({ renderer: null, repo: null, folder: null });
  expect(folderPagePlace(["renderers"])).toBeNull();
  expect(settingsKnows(["renderers", "folder", "add"])).toBe(true);
  expect(settingsKnows(["renderers", "nothing"])).toBe(false);
  expect(settingsKnows(["renderers"])).toBe(true);
});

test("a draft goes into the list in its entry's place, or last for a new one", () => {
  const draft = { ...folderDraftFrom(SAVED), holds: " *.lcproj  *.lc ", timeout: "90" };
  const list = withFolderDraft([SAVED, { ...SAVED, id: "b", name: "B" }], draft, "lc");
  expect(list.map((r) => r.id)).toEqual(["lc", "b"]);
  expect(list[0]).toMatchObject({ holds: ["*.lcproj", "*.lc"], timeout_secs: 90, output: "image" });
  expect(withFolderDraft([SAVED], folderDraftFrom(null), null).at(-1)!.id).toBeNull();
  expect(sameFolderDraft(folderDraftFrom(SAVED), { ...folderDraftFrom(SAVED), holds: " *.lcproj " })).toBe(true);
});

test("the command needs {path} and takes no other placeholder, as the backend says", () => {
  expect(commandWhy("lc {path}")).toBeNull();
  expect(commandWhy("lc . --out {out}")).toContain("{path}");
  expect(commandWhy("lc {path} {branch}")).toContain("{branch} is not something a folder renderer is given");
  expect(commandWhy("")).toBeNull();
});

test("what a renderer takes, in words", () => {
  expect(takesLine({ holds: [], folders: [] })).toBe("Every folder");
  expect(takesLine({ holds: ["*.lcproj"], folders: ["flows"] })).toBe("Folders named flows, holding *.lcproj");
});
