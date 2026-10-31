/**
 * The run page's Artifacts tab (#856) over `run_artifacts`,
 * `artifact_preview` and `download_run_file`: the tree with its folds and
 * counts, a deeper folder read when opened, *Preview* for text, and every
 * download through knobas.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ArtifactEntry, DownloadTarget } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import TfRunArtifacts from "./TfRunArtifacts.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  toasts.items = [];
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const file = (path: string, size: number): ArtifactEntry => ({ path, name: path.split("/").at(-1)!, size, folder: false, children: null });
const folder = (path: string, children: number): ArtifactEntry => ({ path, name: path.split("/").at(-1)!, size: null, folder: true, children });

const TREE: Record<string, ArtifactEntry[]> = {
  "": [folder("test-report", 2), file("SHA256SUMS", 1_024), file("payout-service.tar.gz", 6_501_171)],
  "test-report": [file("test-report/junit.xml", 14_541), folder("test-report/html", 1)],
  "test-report/html": [file("test-report/html/index.html", 90_112)],
};

interface Drawn {
  read: string[];
  downloads: DownloadTarget[];
}

function draw(tree: Record<string, ArtifactEntry[]> = TREE): Drawn {
  const drawn: Drawn = { read: [], downloads: [] };
  page = mount(TfRunArtifacts, {
    target,
    props: {
      runId: "teamcity:build:1187",
      sourceName: "Tidewater TeamCity",
      ports: {
        artifacts: (_id, path) => {
          drawn.read.push(path);
          return Promise.resolve(tree[path] ?? []);
        },
        preview: (_id, path) => Promise.resolve({ text: path.endsWith(".xml") ? "<testsuites>\n</testsuites>" : null, truncated: false }),
        download: (_id, what) => {
          drawn.downloads.push(what);
          const name = what.kind === "all" ? "Payout_IntegrationTests-1187-artifacts.zip" : what.kind === "folder" ? `${what.path}.zip` : what.kind === "file" ? what.path.split("/").at(-1)! : "log";
          return Promise.resolve(`/Users/mara/Downloads/${name}`);
        },
      },
    },
  });
  return drawn;
}

const rows = () =>
  [...target.querySelectorAll(".tf-arow")].map((r) => [r.querySelector(".tf-fold, .tf-nm")?.textContent?.trim(), r.querySelector(".tf-sz")?.textContent, r.className.match(/tf-ad\d/)?.[0] ?? ""]);
const button = (row: number, name: string) =>
  [...target.querySelectorAll(".tf-arow")[row]!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);

test("the tree opens its top-level folders, counts them, sizes the files and offers Preview only for text", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.read).toEqual(["", "test-report"]);
  expect(rows()).toEqual([
    ["▾ test-report/", "2 files", "tf-ad0"],
    ["junit.xml", "14.2 KB", "tf-ad1"],
    ["▸ html/", "1 file", "tf-ad1"],
    ["SHA256SUMS", "1.0 KB", "tf-ad0"],
    ["payout-service.tar.gz", "6.2 MB", "tf-ad0"],
  ]);
  expect(button(1, "Preview")).toBeDefined();
  expect(button(3, "Preview")).toBeDefined();
  expect(button(4, "Preview")).toBeUndefined();
  expect(target.querySelector(".tf-aall .tf-dim")?.textContent).toBe(
    "Downloads go through knobas with the Tidewater TeamCity sign-in it already has, into ~/Downloads.",
  );
});

test("a deeper folder is read when it is opened, and folds closed again", async () => {
  const drawn = draw();
  await settle();
  target.querySelectorAll<HTMLButtonElement>(".tf-fold")[1]!.click();
  await settle();
  expect(drawn.read).toEqual(["", "test-report", "test-report/html"]);
  expect(rows()[3]).toEqual(["index.html", "88.0 KB", "tf-ad2"]);
  target.querySelectorAll<HTMLButtonElement>(".tf-fold")[0]!.click();
  flushSync();
  expect(rows().map((r) => r[0])).toEqual(["▸ test-report/", "SHA256SUMS", "payout-service.tar.gz"]);
});

test("every download goes through knobas: a file, a folder as a zip, and all as a zip", async () => {
  const drawn = draw();
  await settle();
  button(1, "Download")!.click();
  button(0, "Download folder")!.click();
  const all = [...target.querySelectorAll<HTMLButtonElement>(".tf-aall button")].find((b) => b.textContent === "Download all as zip")!;
  // A page body's button is small (the Controls section, #2287).
  expect(all.classList.contains("tf-sm")).toBe(true);
  all.click();
  await settle();
  expect(drawn.downloads).toEqual([{ kind: "file", path: "test-report/junit.xml" }, { kind: "folder", path: "test-report" }, { kind: "all" }]);
  expect(toasts.items.map((t) => t.text)).toEqual([
    "Saved junit.xml to ~/Downloads",
    "Saved test-report.zip to ~/Downloads",
    "Saved Payout_IntegrationTests-1187-artifacts.zip to ~/Downloads",
  ]);
});

test("Preview draws the text with line numbers under the tree, and says when a file is not text", async () => {
  draw();
  await settle();
  button(1, "Preview")!.click();
  await settle();
  const view = target.querySelector(".tf-apreview .tf-fileview")!;
  expect(view.querySelector(".tf-p")?.textContent).toBe("test-report/junit.xml");
  expect([...view.querySelectorAll(".tf-code .tf-l")].map((l) => l.textContent)).toEqual(["1<testsuites>", "2</testsuites>"]);
  button(3, "Preview")!.click();
  await settle();
  expect(target.querySelector(".tf-apreview")?.textContent).toContain("SHA256SUMS is not text.");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-apreview button")].find((b) => b.textContent === "Close")!.click();
  flushSync();
  expect(target.querySelector(".tf-apreview")).toBeNull();
});

test("a run with no artifacts says so and offers no zip", async () => {
  draw({ "": [] });
  await settle();
  expect(target.querySelector(".tf-atree .tf-empty")?.textContent).toBe("This run has published no artifacts yet.");
  expect(target.querySelector(".tf-aall")).toBeNull();
});
