/**
 * Binary files in the Files tab (#2260): a PDF and a binary image drawn from
 * the byte read (#2256), read at the commit the file was read at, over a fake
 * renderer port that records what pdf.js would be asked. The real pdf.js is
 * `render/pdf-core.test.ts`'s; the drawn page is the headless Chrome witness's.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { plainPort, type PdfDocument, type PdfLink, type RenderPort } from "../../render/port";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { BYTES_CAP } from "../../render/registry";

const opened = vi.hoisted(() => [] as string[]);
vi.mock("../../shell/open-external", () => ({
  openExternal: async (url: string) => void opened.push(url),
}));

const REPO = "gitea:tidewater/payout-service";
const PDF_BYTES = new TextEncoder().encode("%PDF-1.7 fake");

/** A source with one binary file at `path`, `size` bytes, read at commit `c0mmit`. */
function binarySource(path: string, size = PDF_BYTES.length) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  return async (_at: string, section: string | null): Promise<SourceView> => {
    const blocks =
      section === "files"
        ? [{ type: "tree", trail: [], entries: [{ name, kind: "file", size, section: `file:${path}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path, text: "", language: null, size, truncated: false, binary: true, url: null, sha: "b10b", commit: "c0mmit" }];
    return { sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] } as unknown as SourceView;
  };
}

/** A pdf.js stand-in: pages of the given sizes and links; records every draw (page, zoom), and which documents closed. */
function pdfRender(pages: { width: number; height: number; links?: PdfLink[] }[], fails: string | null = null) {
  const asked: Uint8Array[] = [];
  const draws: string[] = [];
  let closed = 0;
  const port: RenderPort = {
    ...plainPort,
    async pdf(bytes) {
      asked.push(bytes);
      if (fails !== null) return { error: fails };
      const pdf: PdfDocument = {
        pages: pages.length,
        page: async (n) => {
          const { width, height, links = [] } = pages[n - 1]!;
          return {
            width,
            height,
            draw: (_canvas, scale) => (draws.push(`${n}@${scale}`), { done: Promise.resolve(), cancel() {} }),
            text: async (into) => void into.append(Object.assign(document.createElement("span"), { textContent: `text of page ${n}` })),
            links: async () => links,
            lines: async () => [`text of page ${n}`],
            print: async () => ({ key: `page ${n}`, unsure: false }),
            pixels: async () => null,
          };
        },
        close: () => void (closed += 1),
      };
      return { pdf };
    },
  };
  return { port, asked, draws, closed: () => closed };
}

/**
 * An `IntersectionObserver` that says a page box is in view when its page is in `near` (every page unless a test
 * says otherwise), and `reveal(n)` scrolls page `n` near.
 */
let near: Set<number> | null = null;
const watching = new Set<{ fire: (box: Element) => void; boxes: Set<Element> }>();
class NearObserver {
  readonly entry: { fire: (box: Element) => void; boxes: Set<Element> };
  constructor(callback: (entries: { isIntersecting: boolean; target: Element }[]) => void) {
    this.entry = { boxes: new Set(), fire: (box) => callback([{ isIntersecting: true, target: box }]) };
    watching.add(this.entry);
  }
  observe(box: Element) {
    this.entry.boxes.add(box);
    const page = Number((box as HTMLElement).dataset["page"]);
    if (near === null || near.has(page)) queueMicrotask(() => this.entry.boxes.has(box) && this.entry.fire(box));
  }
  disconnect() {
    this.entry.boxes.clear();
    watching.delete(this.entry);
  }
}
function reveal(page: number) {
  near?.add(page);
  for (const entry of [...watching])
    for (const box of [...entry.boxes]) if ((box as HTMLElement).dataset["page"] === String(page)) entry.fire(box);
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const strict = globalThis.IntersectionObserver;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  opened.length = 0;
  near = null;
  globalThis.IntersectionObserver = NearObserver as unknown as typeof IntersectionObserver;
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  globalThis.IntersectionObserver = strict;
  watching.clear();
});

function draw(path: string, render: RenderPort, fileBytes: ((entityId: string, path: string, commit: string | null) => Promise<Uint8Array>) | null, size?: number) {
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: binarySource(path, size),
        ...(fileBytes ? { fileBytes } : {}),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      line: null,
      changed: new Set<string>(),
      // The stand-ins are Gitea-shaped and list a deep file at the top: it opens by the declared scheme (#2298).
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render,
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
/** The failure the PDF shows: its title and what was said. */
const failure = () => [text(target.querySelector(".tf-stepfail b")), text(target.querySelector(".tf-stepfail pre"))];
const pageBoxes = () => [...target.querySelectorAll<HTMLElement>(".tf-fpdf .tf-pdf-page")];
const sizes = () => pageBoxes().map((box) => `${box.style.width}x${box.style.height}`);
const button = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-pdf-bar button")].find((b) => (b.getAttribute("aria-label") ?? text(b)) === name)!;

const LETTER = { width: 816, height: 1056 };

test("a binary .pdf is read at the file's commit and drawn by PDF, every page a box at its size with its text", async () => {
  const render = pdfRender([LETTER, LETTER, { width: 1056, height: 816 }]);
  const reads: string[] = [];
  draw("docs/spec.pdf", render.port, async (entityId, path, commit) => (reads.push(`${entityId} ${path} ${commit}`), PDF_BYTES));
  await settle();

  expect(reads).toEqual([`${REPO} docs/spec.pdf c0mmit`]);
  expect(render.asked).toEqual([PDF_BYTES]);
  expect(text(target.querySelector(".tf-fileview .tf-rsw-face"))).toBe("Render as PDF");
  expect(text(target.querySelector(".tf-pdf-bar .tf-dim"))).toBe("3 pages");
  expect(sizes()).toEqual(["816pxx1056px", "816pxx1056px", "1056pxx816px"]);
  expect(render.draws).toEqual(["1@1", "2@1", "3@1"]);
  expect(pageBoxes().map((box) => text(box.querySelector(".tf-pdf-text")))).toEqual(["text of page 1", "text of page 2", "text of page 3"]);
  expect(target.querySelector(".tf-fileview")?.textContent).not.toContain("A binary file, not drawn.");
});

test("only the pages near the view are drawn; a page is drawn once it scrolls near", async () => {
  near = new Set([1]);
  const render = pdfRender([LETTER, LETTER, LETTER]);
  draw("spec.pdf", render.port, async () => PDF_BYTES);
  await settle();
  expect(sizes()).toHaveLength(3);
  expect(render.draws).toEqual(["1@1"]);

  reveal(3);
  await settle();
  expect(render.draws).toEqual(["1@1", "3@1"]);
});

test("zooming in and out draws every page again at the new zoom, and sizes its box to it", async () => {
  const render = pdfRender([LETTER, LETTER]);
  draw("spec.pdf", render.port, async () => PDF_BYTES);
  await settle();
  render.draws.length = 0;

  button("Zoom in").click();
  await settle();
  expect(text(target.querySelector(".tf-pdf-zoom"))).toBe("125%");
  expect(render.draws).toEqual(["1@1.25", "2@1.25"]);
  expect(sizes()).toEqual(["1020pxx1320px", "1020pxx1320px"]);

  button("Zoom out").click();
  button("Zoom out").click();
  await settle();
  expect(text(target.querySelector(".tf-pdf-zoom"))).toBe("80%");
  expect(sizes()).toEqual(["653pxx845px", "653pxx845px"]);
});

test("an external link goes to the opener on a click and a middle click; a link to a page scrolls to it", async () => {
  const render = pdfRender([
    { ...LETTER, links: [{ rect: [96, 150, 400, 20], target: { url: "https://psp.example.com/status" } }, { rect: [96, 200, 300, 20], target: { page: 2 } }] },
    LETTER,
  ]);
  draw("spec.pdf", render.port, async () => PDF_BYTES);
  await settle();

  const links = [...target.querySelectorAll<HTMLAnchorElement>(".tf-pdf-page[data-page='1'] .tf-pdf-links a")];
  expect(links.map((a) => [a.getAttribute("href"), a.className, a.style.left, a.style.top, a.style.width, a.style.height])).toEqual([
    ["https://psp.example.com/status", "tf-ext", "96px", "150px", "400px", "20px"],
    ["#", "", "96px", "200px", "300px", "20px"],
  ]);

  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  links[0]!.dispatchEvent(click);
  expect(click.defaultPrevented).toBe(true);
  const middle = new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
  links[0]!.dispatchEvent(middle);
  expect(middle.defaultPrevented).toBe(true);
  await settle();
  expect(opened).toEqual(["https://psp.example.com/status", "https://psp.example.com/status"]);

  const second = target.querySelector<HTMLElement>(".tf-pdf-page[data-page='2']")!;
  const scrolled = vi.fn();
  second.scrollIntoView = scrolled;
  const inner = new MouseEvent("click", { bubbles: true, cancelable: true });
  links[1]!.dispatchEvent(inner);
  expect(inner.defaultPrevented).toBe(true);
  expect(scrolled).toHaveBeenCalledOnce();
  expect(opened).toHaveLength(2);
});

test("bytes pdf.js cannot open show its error, not a page", async () => {
  draw("broken.pdf", pdfRender([], "Invalid PDF structure.").port, async () => PDF_BYTES);
  await settle();
  expect(failure()).toEqual(["PDF could not draw this.", "Invalid PDF structure."]);
  expect(pageBoxes()).toEqual([]);
});

test("a refused byte read shows its message, and a PDF past the cap is not read at all", async () => {
  draw("big.pdf", pdfRender([LETTER]).port, async () => {
    throw { code: "invalid", message: "big.pdf is 63.4 MiB; knobas reads a file's bytes up to 50 MiB" };
  });
  await settle();
  expect(failure()).toEqual(["Could not read this PDF.", "big.pdf is 63.4 MiB; knobas reads a file's bytes up to 50 MiB"]);
  unmount(view!);

  const reads: string[] = [];
  const render = pdfRender([LETTER]);
  draw("huge.pdf", render.port, async (_e, path) => (reads.push(path), PDF_BYTES), BYTES_CAP + 1);
  await settle();
  expect(failure()).toEqual(["This PDF is too large to draw.", "It is 50.0 MiB; knobas reads a file's bytes up to 50.0 MiB."]);
  expect([reads, render.asked]).toEqual([[], []]);
});

test("leaving a PDF lets its document go", async () => {
  const render = pdfRender([LETTER]);
  draw("spec.pdf", render.port, async () => PDF_BYTES);
  await settle();
  expect(render.closed()).toBe(0);
  unmount(view!);
  view = null;
  expect(render.closed()).toBe(1);
});

test("without a PDF engine (plainPort) or without a byte read, a PDF is not drawn", async () => {
  draw("spec.pdf", plainPort, async () => PDF_BYTES);
  await settle();
  expect(failure()).toEqual(["PDFs are not drawn here.", "No PDF engine runs in this window."]);
  unmount(view!);

  draw("spec.pdf", pdfRender([LETTER]).port, null);
  await settle();
  expect(text(target.querySelector(".tf-fileview .tf-empty"))).toBe("A binary file, not drawn.");
});

test("a binary PNG is drawn from its bytes as a blob image, which is let go with the file", async () => {
  const made: Blob[] = [];
  const revoked: string[] = [];
  const create = URL.createObjectURL;
  const revoke = URL.revokeObjectURL;
  URL.createObjectURL = (blob: Blob) => (made.push(blob), `blob:fake/${made.length}`);
  URL.revokeObjectURL = (url: string) => void revoked.push(url);
  try {
    const png = Uint8Array.from([0x89, 0x50, 0x4e, 0x47]);
    draw("docs/img/logo.png", plainPort, async () => png);
    await settle();
    const img = target.querySelector<HTMLImageElement>(".tf-fileview .tf-fimage img")!;
    expect([img.getAttribute("src"), img.alt, made[0]?.type]).toEqual(["blob:fake/1", "docs/img/logo.png", "image/png"]);
    expect(new Uint8Array(await made[0]!.arrayBuffer())).toEqual(png);
    unmount(view!);
    view = null;
    expect(revoked).toEqual(["blob:fake/1"]);
  } finally {
    URL.createObjectURL = create;
    URL.revokeObjectURL = revoke;
  }
});

// -- A text file past the source's cut (spec #2234 story 16) ------------------

/** A source whose one text file is cut after `cut` of its lines, as Gitea cuts at 10,000 lines or 1 MB. */
function cutSource(path: string, whole: string, cut: number) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const size = new TextEncoder().encode(whole).length;
  const text = whole.split("\n").slice(0, cut).join("\n");
  return async (_at: string, section: string | null): Promise<SourceView> => {
    const blocks =
      section === "files"
        ? [{ type: "tree", trail: [], entries: [{ name, kind: "file", size, section: `file:${path}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path, text, language: null, size, truncated: true, binary: false, url: "https://gitea.example/raw", sha: "b10b", commit: "c0mmit" }];
    return { sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] } as unknown as SourceView;
  };
}

function drawCut(path: string, whole: string, cut: number, fileBytes: ((entityId: string, path: string, commit: string | null) => Promise<Uint8Array>) | null, line: number | null) {
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: cutSource(path, whole, cut),
        ...(fileBytes ? { fileBytes } : {}),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      line,
      changed: new Set<string>(),
      // The stand-ins are Gitea-shaped and list a deep file at the top: it opens by the declared scheme (#2298).
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render: plainPort,
    },
  });
  flushSync();
}

const LONG = Array.from({ length: 12_000 }, (_, i) => `line ${i + 1}`).join("\n");
const cutBanner = () => [...target.querySelectorAll(".tf-banner")].find((b) => text(b).startsWith("The file is longer than knobas shows"));

test("a text file past the source's cut is read whole through the byte read, at its commit, and drawn to its last line", async () => {
  const reads: string[] = [];
  drawCut("data/ledger.csv", LONG, 10_000, async (entityId, path, commit) => (reads.push(`${entityId} ${path} ${commit}`), new TextEncoder().encode(LONG)), 11_500);
  await settle();

  expect(reads).toEqual([`${REPO} data/ledger.csv c0mmit`]);
  expect(cutBanner()).toBeUndefined();
  const at = target.querySelector<HTMLElement>(".tf-fileview .tf-code .tf-l.tf-at");
  expect(at?.dataset["n"]).toBe("11500");
  expect(at?.lastElementChild?.textContent).toBe("line 11500");
});

test("a cut file past the byte cap, or with no byte read, keeps the cut and says so", async () => {
  const reads: string[] = [];
  drawCut("data/ledger.csv", LONG, 10_000, null, null);
  await settle();
  expect(cutBanner()).toBeDefined();
  unmount(view!);
  view = null;

  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: async (_at: string, section: string | null) => {
          const answer = await cutSource("data/huge.csv", LONG, 10_000)(_at, section);
          const block = answer.sections[0]!.blocks[0] as unknown as { size: number };
          block.size = BYTES_CAP + 1;
          if (section === "files") (answer.sections[0]!.blocks[0] as unknown as { entries: { size: number }[] }).entries[0]!.size = BYTES_CAP + 1;
          return answer;
        },
        fileBytes: async (entityId: string, path: string) => (reads.push(`${entityId} ${path}`), new Uint8Array()),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: "data/huge.csv",
      line: null,
      changed: new Set<string>(),
      // The stand-ins are Gitea-shaped and list a deep file at the top: it opens by the declared scheme (#2298).
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render: plainPort,
    },
  });
  flushSync();
  await settle();
  expect(reads).toEqual([]);
  expect(cutBanner()).toBeDefined();
});
