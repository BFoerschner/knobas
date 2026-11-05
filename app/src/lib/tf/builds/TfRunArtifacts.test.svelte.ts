/**
 * The run page's Artifacts tab (#856) over `run_artifacts`,
 * `artifact_preview`, `artifact_bytes` and `download_run_file`: the tree with its folds and
 * counts, a deeper folder read when opened, a file's name opening it in the
 * file viewer (#2415), and every
 * download through knobas.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ArtifactEntry, ArtifactPreview, AttachmentBytes, DownloadTarget } from "../../ipc/entity";
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
  /** The paths whose whole bytes were asked for. */
  bytes: string[];
  downloads: DownloadTarget[];
}

/** The head of a file: `.xml` is text, anything else is not. */
const textHeads = (path: string): Promise<ArtifactPreview | null> =>
  Promise.resolve({ text: path.endsWith(".xml") ? "<testsuites>\n</testsuites>" : null, truncated: false });

/** The whole of a file: one PNG's first bytes, whatever its name. */
const wholeFiles = (): Promise<AttachmentBytes | null> => Promise.resolve({ mime: "image/png", data: "iVBORw==" });

function draw(
  tree: Record<string, ArtifactEntry[]> = TREE,
  head: (path: string) => Promise<ArtifactPreview | null> = textHeads,
  whole: (path: string) => Promise<AttachmentBytes | null> = wholeFiles,
): Drawn {
  const drawn: Drawn = { read: [], previewed: [], bytes: [], downloads: [] };
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
        preview: (_id, path) => (drawn.previewed.push(path), head(path)),
        bytes: (_id, path) => (drawn.bytes.push(path), whole(path)),
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

test("a head that cannot be read says why in the viewer and offers Try again, which reads it again; a cut head says so", async () => {
  let fails = true;
  const drawn = draw(TREE, (path) =>
    fails ? Promise.reject(new Error("TeamCity answered 503")) : Promise.resolve({ text: `<testsuites>\n${path}`, truncated: true }),
  );
  await settle();
  nameButton("junit.xml").click();
  await settle();
  const stage = () => viewer()!.querySelector(".tf-vstage")!;
  expect(stage().getAttribute("data-shows")).toBe("missing");
  expect(stage().querySelector(".tf-vempty p")?.textContent).toContain("junit.xml could not be read:");
  fails = false;
  [...stage().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Try again")!.click();
  await settle();
  expect(drawn.previewed).toEqual(["test-report/junit.xml", "test-report/junit.xml"]);
  expect([...stage().querySelectorAll(".tf-code .tf-l")].map((l) => l.textContent)).toEqual(["1<testsuites>", "2test-report/junit.xml"]);
  expect(stage().querySelector(".tf-vnote")?.textContent).toBe("The first 256 KB. Download it for the rest.");
});

const CAP = 5 * 1024 * 1024;
const stage = () => viewer()!.querySelector(".tf-vstage")!;

test("an image artifact opens as the image, whatever case its extension, read whole and not as a text head", async () => {
  for (const name of ["shot.png", "shot.JPG", "shot.jpeg", "shot.gif", "shot.webp"]) {
    const drawn = draw({ "": [file(name, 2_048)] }, textHeads, () => Promise.resolve({ mime: "image/x-test", data: "AAEC" }));
    await settle();
    nameButton(name).click();
    await settle();
    expect(stage().getAttribute("data-shows"), name).toBe("image");
    expect(stage().querySelector("img")?.getAttribute("src"), name).toBe("data:image/x-test;base64,AAEC");
    expect(drawn.bytes, name).toEqual([name]);
    expect(drawn.previewed, name).toEqual([]);
    unmount(page!);
    page = null;
  }
});

test("a PDF artifact opens in the PDF component, and a file of no other kind is read whole", async () => {
  const drawn = draw({ "": [file("report.pdf", 2_048), file("trace.bin", 2_048)] });
  await settle();
  nameButton("report.pdf").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("pdf");
  expect(drawn.bytes).toEqual(["report.pdf"]);
  viewer()!.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  nameButton("trace.bin").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("none");
  expect(drawn.bytes).toEqual(["report.pdf"]);
});

test("a file named only like an extension is no image or PDF: it stays a text head", async () => {
  for (const name of ["png", "PDF", ".png", ".pdf"]) {
    const drawn = draw({ "": [file(name, 2_048)] });
    await settle();
    nameButton(name).click();
    await settle();
    expect(drawn.bytes, name).toEqual([]);
    expect(drawn.previewed, name).toEqual([name]);
    unmount(page!);
    page = null;
  }
});

test("an artifact over the cap says so without a read, offers no Try again, and keeps Download", async () => {
  const drawn = draw({ "": [file("big.png", CAP + 1), file("edge.png", CAP)] });
  await settle();
  nameButton("big.png").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("missing");
  expect(stage().querySelector(".tf-vempty p")?.textContent).toBe("big.png is over the 5 MB preview cap.");
  expect(stage().querySelector("button")).toBeNull();
  expect(drawn.bytes).toEqual([]);
  const download = viewer()!.querySelector<HTMLButtonElement>(".tf-dhead .tf-dctl .tf-acts .tf-btn")!;
  expect(download.textContent?.trim()).toBe("Download");
  download.click();
  await settle();
  expect(drawn.downloads).toEqual([{ kind: "file", path: "big.png" }]);
  viewer()!.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  nameButton("edge.png").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("image");
});

test("a file of no listed size the source finds over the cap says the same, with no Try again", async () => {
  const refused = { code: "invalid", message: "big.pdf is over 5 MiB; knobas previews an artifact up to 5 MiB", source_id: null };
  draw({ "": [{ ...file("big.pdf", 0), size: null }] }, textHeads, () => Promise.reject(refused));
  await settle();
  nameButton("big.pdf").click();
  await settle();
  expect(stage().querySelector(".tf-vempty p")?.textContent).toBe("big.pdf is over the 5 MB preview cap.");
  expect(stage().querySelector("button")).toBeNull();
});

test("a byte read that fails says why, and Try again reads it again", async () => {
  let fails = true;
  const drawn = draw({ "": [file("shot.png", 2_048)] }, textHeads, () =>
    fails ? Promise.reject({ code: "unreachable", message: "TeamCity answered 503", source_id: null }) : wholeFiles(),
  );
  await settle();
  nameButton("shot.png").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("missing");
  expect(stage().querySelector(".tf-vempty p")?.textContent).toBe("shot.png could not be read: TeamCity answered 503");
  fails = false;
  [...stage().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Try again")!.click();
  await settle();
  expect(drawn.bytes).toEqual(["shot.png", "shot.png"]);
  expect(stage().querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,iVBORw==");
});

test("a source that reads no artifact's bytes leaves the file at No preview", async () => {
  draw({ "": [file("shot.png", 2_048)] }, textHeads, () => Promise.resolve(null));
  await settle();
  nameButton("shot.png").click();
  await settle();
  expect(stage().getAttribute("data-shows")).toBe("none");
});

test("a read that lands after another file was opened draws nothing of it", async () => {
  let land: (bytes: AttachmentBytes) => void = () => {};
  draw({ "": [file("slow.png", 2_048), file("quick.png", 2_048)] }, textHeads, (path) =>
    path === "slow.png" ? new Promise((resolve) => (land = resolve)) : Promise.resolve({ mime: "image/png", data: "AAEC" }),
  );
  await settle();
  nameButton("slow.png").click();
  await settle();
  viewer()!.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  nameButton("quick.png").click();
  await settle();
  land({ mime: "image/png", data: "c2xvdw==" });
  await settle();
  expect(stage().querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAEC");
});

test("a run with no artifacts says so and offers no zip", async () => {
  draw({ "": [] });
  await settle();
  expect(target.querySelector(".tf-atree .tf-empty")?.textContent).toBe("This run has published no artifacts yet.");
  expect(target.querySelector(".tf-aall")).toBeNull();
});
