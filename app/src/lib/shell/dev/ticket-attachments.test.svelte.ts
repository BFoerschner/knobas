/**
 * The ticket page's attachment previews and downloads (spec #2396, #2405)
 * through the app's own mount over the whole fake IPC: opening the panel
 * fetches PAY-231's two screenshots through `attachment_bytes` and draws them
 * as data images. A row opens the attachment's preview (#2415): the image,
 * the PDF, a text file's lines, or its kind and *No preview*; its *Download* saves through
 * `download_attachment`. With the read failing
 * (`?fake-attachments=unreachable`), the panel leads with the stale strip,
 * the image rows draw dashed placeholders, *Download* waits, the preview
 * says why and *Try again* reads again; a 403 reads *not permitted*; a file
 * over the 5 MiB cap is never fetched and its preview says so. The
 * description's `[^name]` links and inline images, and a comment's, open the
 * same preview. Under `dev/` because only the harness may import the harness.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import type { Attachment, EntityDetail, Thread } from "../../ipc/entity";
import { chosenJob } from "../../tf/job/chosen.svelte";
import { demoHandlers, installFakeTauri, type Handler } from "./fake-tauri";
import { until } from "../test-wait";

/** Every command the app sent, in order, with its arguments. */
let sent: { cmd: string; args: Record<string, unknown> }[] = [];

function recorded(handlers: Record<string, Handler>): Record<string, Handler> {
  return Object.fromEntries(
    Object.entries(handlers).map(([cmd, handler]) => [
      cmd,
      (args: Record<string, unknown>) => {
        sent.push({ cmd, args });
        return handler(args);
      },
    ]),
  );
}

/** The fixture's handlers under `?<query>`, each call written down. */
function install(query = "", extra: Record<string, Handler> = {}): Record<string, Handler> {
  const handlers = { ...demoHandlers(new URLSearchParams(query)), ...extra };
  installFakeTauri(recorded(handlers));
  return handlers;
}

/**
 * An observer that sees every node it is given (#2405): the suite's own never
 * reports an intersection (`test-setup.ts`), and the page fetches an inline
 * image only once it is in view. Answers the undo.
 */
function seeEverything(): () => void {
  const held = globalThis.IntersectionObserver;
  class Seeing {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe(target: Element): void {
      queueMicrotask(() => this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as unknown as IntersectionObserver));
    }
    unobserve(): void {}
    disconnect(): void {}
    takeRecords(): [] {
      return [];
    }
  }
  Object.defineProperty(globalThis, "IntersectionObserver", { writable: true, configurable: true, value: Seeing });
  return () => Object.defineProperty(globalThis, "IntersectionObserver", { writable: true, configurable: true, value: held });
}

let unsee = () => {};
let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** A storage for the chosen job, which this runner's jsdom lacks (`test-no-storage.ts`). */
function memoryStorage(): Storage {
  const held = new Map<string, string>();
  return {
    get length() {
      return held.size;
    },
    clear: () => held.clear(),
    getItem: (key) => held.get(key) ?? null,
    key: (at) => [...held.keys()][at] ?? null,
    removeItem: (key) => void held.delete(key),
    setItem: (key, value) => void held.set(key, value),
  };
}

beforeEach(() => {
  Object.defineProperty(window, "localStorage", { value: memoryStorage(), configurable: true, writable: true });
  sent = [];
  unsee = seeEverything();
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
  chosenJob.reset();
  unsee();
  Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
});

/** PAY-231's page, drawn, with its Attachments panel still closed. */
async function openPay231(): Promise<HTMLDetailsElement> {
  location.hash = "#/ticket/mock%3APAY-231";
  app = mount(App, { target, props: {} });
  await until(() => panel() !== null, "PAY-231 never drew its Attachments section");
  return panel()!;
}

/** The shown page's Attachments disclosure. */
function panel(): HTMLDetailsElement | null {
  return target.querySelector<HTMLDetailsElement>(".tf-kept:not([hidden]) .tf-tkread section[aria-labelledby='tf-tkatt'] details.tf-tkfold");
}

/** Opens the panel as a click on its summary does, toggle event and all. */
function unfold(fold: HTMLDetailsElement): void {
  fold.querySelector("summary")!.click();
  fold.dispatchEvent(new Event("toggle"));
  flushSync();
}

function rows(fold: HTMLDetailsElement): HTMLElement[] {
  return [...fold.querySelectorAll<HTMLElement>(".tf-tkatt .tf-r")];
}

/** One row's first cell, as drawn: the preview's source, a placeholder or the kind. */
function cell(row: HTMLElement): string {
  const img = row.querySelector<HTMLImageElement>("img.tf-tkthumb");
  if (img) return img.getAttribute("src")?.startsWith("data:image/") ? "preview" : `img ${img.getAttribute("src")}`;
  if (row.querySelector(".tf-tkthumb.tf-tknone")) return "placeholder";
  return row.querySelector(".tf-tkkind")?.textContent?.trim() ?? "?";
}

/** The open attachment preview, if one is open. */
function modal(): HTMLElement | null {
  return target.querySelector<HTMLElement>(".tf-modal [role='dialog'].tf-viewer");
}

/** A button of the preview by its words. */
function inModal(name: string): HTMLButtonElement | undefined {
  return [...(modal()?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find((b) => (b.getAttribute("aria-label") ?? b.textContent?.trim()) === name);
}

/** Opens the preview of `row` as a click on it does. */
function preview(row: HTMLElement): HTMLElement {
  row.click();
  flushSync();
  expect(modal(), "the row opened no preview").not.toBeNull();
  return modal()!;
}

const squash = (node: Element | null | undefined) => node?.textContent?.replace(/\s+/g, " ").trim() ?? "";

const reads = () => sent.filter((call) => call.cmd === "attachment_bytes").map((call) => call.args["attachmentId"]);

const strip = (fold: HTMLDetailsElement) => fold.querySelector<HTMLElement>(".tf-tkatt > .tf-stale");

test("opening PAY-231's panel draws its two screenshots' previews; only the one the description shows was fetched before", async () => {
  install();
  const fold = await openPay231();
  // The description's inline screenshot is fetched once in view; the panel's
  // other image waits for the panel.
  await until(() => reads().length > 0, "the inline screenshot was never fetched");
  expect(reads()).toEqual(["50000"]);

  unfold(fold);
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "the previews never drew");
  expect(rows(fold).map(cell)).toEqual(["preview", "preview", "PDF", "CSV"]);
  expect(reads()).toEqual(["50000", "50001"]);
  expect(sent.find((call) => call.cmd === "attachment_bytes")!.args["entityId"]).toBe("mock:PAY-231");
  expect(strip(fold)).toBeNull();

  // Closed and opened again: the previews are kept for the page, not asked again.
  unfold(fold);
  unfold(fold);
  expect(reads()).toEqual(["50000", "50001"]);
});

test("a row opens the preview of its file: an image draws whole, Escape closes it and focus comes back to the row (#2415)", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  const row = rows(fold)[1]!;
  row.focus();
  const shown = preview(row);
  expect(squash(shown.querySelector("h2"))).toBe("retry-timeline.png");
  expect(squash(shown.querySelector(".tf-dsub"))).toBe("Mara Lindqvist, 2026-08-19, 96 KB");
  expect(inModal("Download")).toBeDefined();
  expect(inModal("Close")).toBeDefined();
  expect(shown.querySelector("a[href]"), "no Open").toBeNull();
  await until(() => shown.querySelector<HTMLImageElement>(".tf-vstage img")?.getAttribute("src")?.startsWith("data:image/") ?? false, "the image never drew");
  expect(shown.contains(document.activeElement)).toBe(true);

  shown.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  expect(modal()).toBeNull();
  expect(document.activeElement).toBe(row);
});

test("× and a click on the backdrop close the preview", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  preview(rows(fold)[3]!);
  inModal("Close")!.click();
  flushSync();
  expect(modal()).toBeNull();
  preview(rows(fold)[3]!);
  const backdrop = modal()!.closest<HTMLElement>(".tf-modal")!;
  backdrop.click();
  flushSync();
  expect(modal()).toBeNull();
});

test("a PDF's preview is the PDF viewer over the file's bytes", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  const shown = preview(rows(fold)[2]!);
  await until(() => shown.querySelector(".tf-vstage .tf-fpdf, .tf-vstage .tf-empty") !== null, "no PDF viewer");
  expect(reads()).toContain("50002");
});

test("a text file draws its lines, read through attachment_bytes", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  const shown = preview(rows(fold)[3]!);
  await until(() => shown.querySelector(".tf-vstage .tf-code .tf-l") !== null, "the CSV never drew");
  const lines = [...shown.querySelectorAll(".tf-vstage .tf-code .tf-l")].map((line) => line.textContent);
  expect(lines[0]).toBe("1payout_id,iban,amount_eur,reason_code,attempts,last_attempt");
  expect(lines).toHaveLength(5);
  expect(shown.querySelector(".tf-vnote")).toBeNull();
  expect(reads()).toContain("50003");
});

test("Download in the preview saves the file through download_attachment and says where", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  preview(rows(fold)[2]!);
  const download = inModal("Download")!;
  expect(download.disabled).toBe(false);
  download.click();
  await until(() => target.querySelector(".tf-toasts")?.textContent?.includes("Saved sepa-error-codes.pdf to ~/Downloads") ?? false, "no Saved toast");
  expect(sent.filter((call) => call.cmd === "download_attachment").map((call) => call.args)).toEqual([
    { entityId: "mock:PAY-231", attachmentId: "50002" },
  ]);
});

test("with the source unreachable, the strip leads, images are placeholders, the preview says why with Try again, Download waits, and Try again reads again", async () => {
  const handlers = install("fake-attachments=unreachable");
  const fold = await openPay231();
  unfold(fold);
  await until(() => strip(fold) !== null, "no stale strip");

  expect(strip(fold)!.querySelector("span")?.textContent).toBe(
    "Tidewater (mock) cannot be reached, so previews and downloads wait. The names and links are the mirror's.",
  );
  expect(rows(fold).map(cell)).toEqual(["placeholder", "placeholder", "PDF", "CSV"]);
  expect(reads()).toEqual(["50000", "50001"]);

  const shown = preview(rows(fold)[1]!);
  await until(() => squash(shown.querySelector(".tf-vstage")).includes("not reachable"), "the preview never said why");
  expect(squash(shown.querySelector(".tf-vstage p"))).toBe("retry-timeline.png, not reachable in Tidewater (mock) right now.");
  const download = inModal("Download")!;
  expect(download.disabled).toBe(true);
  expect(download.title).toBe("Waits until Tidewater (mock) can be read");

  // The source answers again: *Try again* reads what failed, and the image draws.
  const reachable = demoHandlers();
  installFakeTauri(recorded({ ...handlers, attachment_bytes: reachable["attachment_bytes"]! }));
  inModal("Try again")!.click();
  await until(() => shown.querySelector<HTMLImageElement>(".tf-vstage img")?.getAttribute("src")?.startsWith("data:image/") ?? false, "Try again drew no image");
  await until(() => strip(fold) === null, "the strip stayed");
  expect(inModal("Download")!.disabled).toBe(false);
  inModal("Close")!.click();
  flushSync();
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "Try again drew no previews");
  // The description's placeholder asks again too, and draws the screenshot.
  const inline = () => target.querySelector(".tf-kept:not([hidden]) .tf-tkdesc .tf-md");
  await until(() => inline()?.querySelector("img.tf-md-att")?.getAttribute("src")?.startsWith("data:image/") ?? false, "the inline screenshot stayed a placeholder");
  expect(inline()!.querySelector(".tf-md-noimg")).toBeNull();
});

test("a 403 reads not permitted, in the strip and in the preview", async () => {
  install("fake-attachments=forbidden");
  const fold = await openPay231();
  unfold(fold);
  await until(() => strip(fold) !== null, "no stale strip");
  expect(strip(fold)!.querySelector("span")?.textContent).toBe(
    "Tidewater (mock) answers not permitted, so previews and downloads wait. The names and links are the mirror's.",
  );
  expect(rows(fold).map(cell)).toEqual(["placeholder", "placeholder", "PDF", "CSV"]);
  const shown = preview(rows(fold)[2]!);
  await until(() => squash(shown.querySelector(".tf-vstage")).includes("not permitted"), "the PDF's preview never said why");
  expect(squash(shown.querySelector(".tf-vstage p"))).toBe("sepa-error-codes.pdf, not permitted in Tidewater (mock).");
});

test("an image over the 5 MiB cap gets no preview, is never fetched, and its preview says so with Download", async () => {
  const getEntity = demoHandlers()["get_entity"]!;
  const huge: Attachment = {
    id: "50009",
    name: "full-trace.png",
    mime: "image/png",
    size: 6 * 1024 * 1024,
    author: "Lena Hoffmann",
    created: "2026-08-20T10:00:00Z",
    content_url: "http://127.0.0.1:5399/secure/attachment/50009/full-trace.png",
    thumbnail_url: null,
  };
  install("", {
    get_entity: async (args) => {
      const detail = (await getEntity(args)) as EntityDetail;
      return args["entityId"] === "mock:PAY-231" ? { ...detail, attachments: [...(detail.attachments ?? []), huge] } : detail;
    },
  });
  const fold = await openPay231();
  unfold(fold);
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "the previews never drew");
  const last = rows(fold)[4]!;
  expect(cell(last)).toBe("PNG");
  const shown = preview(last);
  expect(squash(shown.querySelector(".tf-vstage p"))).toBe("full-trace.png is over the 5 MB preview cap.");
  expect(inModal("Try again")).toBeUndefined();
  expect(inModal("Download")!.disabled).toBe(false);
  expect(reads()).not.toContain("50009");
});

test("the description's [^name] link, its inline image and a Jira comment's inline image open the preview", async () => {
  const thread = demoHandlers()["entity_thread"]!;
  install("", {
    // One comment in Jira's wiki markup, as a Jira ticket's thread holds them all.
    entity_thread: async (args) => {
      const read = (await thread(args)) as Thread;
      const wiki = { ...read.comments[0]!, body: "The retry timeline as I see it:\n\n!retry-timeline.png|thumbnail!", markup: "jira_wiki" as const };
      return { ...read, comments: [wiki, ...read.comments.slice(1)] };
    },
  });
  await openPay231();
  const desc = () => target.querySelector(".tf-kept:not([hidden]) .tf-tkdesc .tf-md");
  const link = [...desc()!.querySelectorAll<HTMLAnchorElement>("a.tf-md-attlink")].find((a) => squash(a) === "sepa-error-codes.pdf")!;
  expect(link).toBeDefined();
  link.click();
  flushSync();
  expect(squash(modal()?.querySelector("h2"))).toBe("sepa-error-codes.pdf");
  inModal("Close")!.click();
  flushSync();

  await until(() => desc()?.querySelector("img.tf-md-att")?.getAttribute("src")?.startsWith("data:image/") ?? false, "the inline screenshot never drew");
  desc()!.querySelector<HTMLImageElement>("img.tf-md-att")!.click();
  flushSync();
  expect(squash(modal()?.querySelector("h2"))).toBe("psp-409-trace.png");
  inModal("Close")!.click();
  flushSync();

  const talk = () => target.querySelector(".tf-kept:not([hidden]) .tf-convo");
  await until(() => talk()?.querySelector("img.tf-md-att")?.getAttribute("src")?.startsWith("data:image/") ?? false, "the comment's screenshot never drew");
  const img = talk()!.querySelector<HTMLImageElement>("img.tf-md-att")!;
  expect(img.alt).toBe("retry-timeline.png");
  expect(squash(img.closest(".tf-msg"))).not.toContain("!retry-timeline.png");
  img.click();
  flushSync();
  expect(squash(modal()?.querySelector("h2"))).toBe("retry-timeline.png");
});

test("previews live with the page: a page drawn again fetches them again", async () => {
  install();
  let fold = await openPay231();
  unfold(fold);
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "the previews never drew");
  unmount(app!);
  app = undefined;

  app = mount(App, { target, props: {} });
  await until(() => panel() !== null, "PAY-231 never drew again");
  fold = panel()!;
  expect(rows(fold)[1] && cell(rows(fold)[1]!), "nothing kept from the closed page").toBe("PNG");
  unfold(fold);
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "the previews never drew again");
  expect(reads()).toEqual(["50000", "50001", "50000", "50001"]);
});
