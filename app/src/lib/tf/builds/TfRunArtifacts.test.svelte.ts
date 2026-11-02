/**
 * The run page's Artifacts tab (#856) over `run_artifacts`,
 * `artifact_preview` and `download_run_file`: the tree with its folds and
 * counts, a deeper folder read when opened, a file's name opening it in the
 * file viewer (#2415), and every
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
  /** The paths whose head was asked for. */
  previewed: string[];
  downloads: DownloadTarget[];
}

function draw(tree: Record<string, ArtifactEntry[]> = TREE): Drawn {
  const drawn: Drawn = { read: [], previewed: [], downloads: [] };
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
        preview: (_id, path) => (drawn.previewed.push(path), Promise.resolve({ text: path.endsWith(".xml") ? "<testsuites>\n</testsuites>" : null, truncated: false })),
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

test("the tree opens its top-level folders, counts them, sizes the files, and each file's name opens it", async () => {
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
  expect([...target.querySelectorAll(".tf-arow > button.tf-aopen")].map((b) => b.textContent)).toEqual(["junit.xml", "SHA256SUMS", "payout-service.tar.gz"]);
  expect(target.querySelector(".tf-arow button")?.textContent).not.toBe("Preview");
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

const viewer = () => target.querySelector<HTMLElement>(".tf-modal [role='dialog'].tf-viewer");
const nameButton = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-arow > button.tf-aopen")].find((b) => b.textContent === name)!;

test("a file's name opens the viewer: a text file's lines, its folder and size, Download on the title line, × closes", async () => {
  const drawn = draw();
  await settle();
  nameButton("junit.xml").click();
  await settle();
  const view = viewer()!;
  expect(view.querySelector("h2")?.textContent).toBe("junit.xml");
  expect(view.querySelector(".tf-dsub")?.textContent).toBe("test-report/, 14.2 KB");
  expect([...view.querySelectorAll(".tf-vstage .tf-code .tf-l")].map((l) => l.textContent)).toEqual(["1<testsuites>", "2</testsuites>"]);
  const download = view.querySelector<HTMLButtonElement>(".tf-dhead .tf-dctl .tf-acts .tf-btn")!;
  expect(download.textContent?.trim()).toBe("Download");
  download.click();
  await settle();
  expect(drawn.downloads).toEqual([{ kind: "file", path: "test-report/junit.xml" }]);
  view.querySelector<HTMLButtonElement>(".tf-dctl .tf-dclose")!.click();
  flushSync();
  expect(viewer()).toBeNull();
});

test("a file that is not text opens with No preview, and one with no text extension is not read at all", async () => {
  const drawn = draw();
  await settle();
  nameButton("SHA256SUMS").click();
  await settle();
  expect(viewer()?.querySelector(".tf-vempty")?.textContent).toContain("No preview for this kind of file.");
  viewer()!.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  nameButton("payout-service.tar.gz").click();
  flushSync();
  expect(viewer()?.querySelector(".tf-vstage")?.getAttribute("data-shows")).toBe("none");
  expect(viewer()?.querySelector(".tf-tkkind")?.textContent).toBe("GZ");
  expect(drawn.previewed).toEqual(["SHA256SUMS"]);
});

test("a run with no artifacts says so and offers no zip", async () => {
  draw({ "": [] });
  await settle();
  expect(target.querySelector(".tf-atree .tf-empty")?.textContent).toBe("This run has published no artifacts yet.");
  expect(target.querySelector(".tf-aall")).toBeNull();
});
