/** The pick per folder (ux26): held at once, kept as the backend stored it, one a folder, the repository's. */
import { expect, test } from "vitest";

import type { FolderPick } from "../ipc/app";
import { createFolderPicks } from "./folder-picks.svelte";

test("a pick is held at once, then the backend's answer; another pick of the folder replaces it; none takes it away", async () => {
  const saves: string[] = [];
  let stored: FolderPick[] = [];
  const picks = createFolderPicks({
    read: async () => [{ repo: "g:o/a", folder: "flows", renderer: "lowcode" }],
    save: async (id, folder, renderer) => {
      saves.push(`${id} ${folder} ${renderer}`);
      stored = renderer === null ? [] : [{ repo: "g:o/a", folder, renderer }];
      return stored;
    },
  });
  expect(picks.of("g:o/a", "flows")).toBeNull();
  await picks.ensure();
  expect(picks.of("g:o/a", "flows")).toBe("lowcode");
  expect(picks.of("g:o/b", "flows")).toBeNull();
  const going = picks.set("g:o/a@refs/heads/main", "g:o/a", "/flows/", "flowviz");
  expect(picks.of("g:o/a", "flows"), "held in the press's frame").toBe("flowviz");
  await going;
  expect(saves).toEqual(["g:o/a@refs/heads/main flows flowviz"]);
  await picks.set("g:o/a", "g:o/a", "flows", null);
  expect(picks.of("g:o/a", "flows")).toBeNull();
});
