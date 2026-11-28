/**
 * The one file drawing (#2581) as its two callers mount it: inside the Code page's repository, and outside any
 * (*Try it on*), where the parts that need a repository stay off.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { allRenderers, type Renderer } from "../../render/registry";
import type { FileDrawRepo } from "./file-draw";
import TfFileDraw from "./TfFileDraw.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const quiet = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
const byId = (id: string): Renderer => allRenderers().find((r) => r.id === id)!;
/** The Code page's repository, as far as these leaves read it. */
function repoAt(file: string, extra: Partial<FileDrawRepo> = {}): FileDrawRepo {
  return {
    id: "repo-1",
    entity: "repo-1",
    file: { path: file, sha: null },
    asRead: () => true,
    path: file,
    place: null,
    line: null,
    images: null,
    jar: null,
    fileLink: (to: string, at: number | null) => `#/repo/r/files/${to}${at === null ? "" : `@${at}`}`,
    ...extra,
  } as FileDrawRepo;
}

function draw(drawn: { path: string; text: string; binary: boolean }, renderer: Renderer, repo: FileDrawRepo | null) {
  view = mount(TfFileDraw, {
    target,
    props: { drawn, renderer, render: quiet, commands: { run: async () => ({ outcome: "drawn", output: "html", body: "" }) }, frame: async () => "t", room: ".x", repo },
  });
  flushSync();
}

test("a JAR outside a repository says it is drawn on the Code page; in a repository with no archive reads it is a binary file", () => {
  const jar = { path: "lib/app.jar", text: "", binary: true };
  draw(jar, byId("jar"), null);
  expect(target.textContent).toBe("A JAR is drawn from a repository’s file on the Code page; here it is not drawn.");
  unmount(view!);
  draw(jar, byId("jar"), repoAt("lib/app.jar"));
  expect(target.textContent).toBe("A binary file, not drawn.");
});

test("a relative link in Markdown opens the repository's file beside it, and outside a repository is text", () => {
  const readme = { path: "docs/README.md", text: "[runbook](runbook.md)", binary: false };
  draw(readme, byId("markdown"), repoAt("docs/README.md"));
  expect(target.querySelector(".tf-rendered a")?.getAttribute("href")).toBe("#/repo/r/files/docs/runbook.md");
  unmount(view!);
  draw(readme, byId("markdown"), null);
  expect(target.querySelector(".tf-rendered a")?.hasAttribute("href") ?? false).toBe(false);
  expect(target.querySelector(".tf-rendered")?.textContent?.trim()).toBe("runbook");
});
