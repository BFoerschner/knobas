/**
 * The ticket page's attachment previews and downloads (spec #2396, #2405)
 * through the app's own mount over the whole fake IPC: opening the panel
 * fetches PAY-231's two screenshots through `attachment_bytes` and draws them
 * as data images; *Download* saves through `download_attachment`. With the
 * read failing (`?fake-attachments=unreachable`), the panel leads with the
 * stale strip, the image rows draw dashed placeholders, *Download* waits,
 * *Open* stays a link and *Try again* reads again; a 403 reads *not
 * permitted*; an image over the 5 MiB cap is never fetched and keeps both
 * buttons. Under `dev/` because only the harness may import the harness.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import App from "../../../App.svelte";
import type { Attachment, EntityDetail } from "../../ipc/entity";
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

function button(row: HTMLElement, name: string): HTMLButtonElement | HTMLAnchorElement {
  return [...row.querySelectorAll<HTMLButtonElement | HTMLAnchorElement>(".tf-rend .tf-btn")].find((b) => b.textContent?.trim() === name)!;
}

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

test("Download saves the file through download_attachment and says where", async () => {
  install();
  const fold = await openPay231();
  unfold(fold);
  const pdf = rows(fold)[2]!;
  const download = button(pdf, "Download") as HTMLButtonElement;
  expect(download.disabled).toBe(false);
  download.click();
  await until(() => target.querySelector(".tf-toasts")?.textContent?.includes("Saved sepa-error-codes.pdf to ~/Downloads") ?? false, "no Saved toast");
  expect(sent.filter((call) => call.cmd === "download_attachment").map((call) => call.args)).toEqual([
    { entityId: "mock:PAY-231", attachmentId: "50002" },
  ]);
});

test("with the source unreachable, the strip leads, images are placeholders, Download waits, Open stays a link and Try again reads again", async () => {
  const handlers = install("fake-attachments=unreachable");
  const fold = await openPay231();
  unfold(fold);
  await until(() => strip(fold) !== null, "no stale strip");

  expect(strip(fold)!.querySelector("span")?.textContent).toBe(
    "Tidewater (mock) cannot be reached, so previews and downloads wait. The names and links are the mirror's.",
  );
  expect(rows(fold).map(cell)).toEqual(["placeholder", "placeholder", "PDF", "CSV"]);
  for (const row of rows(fold)) {
    expect((button(row, "Download") as HTMLButtonElement).disabled).toBe(true);
    const open = button(row, "Open") as HTMLAnchorElement;
    expect(open.tagName).toBe("A");
    expect(open.getAttribute("href")).toMatch(/^http:\/\/127\.0\.0\.1:5399\/secure\/attachment\/5000\d\//);
  }
  expect(reads()).toEqual(["50000", "50001"]);

  // The source answers again: *Try again* reads what failed, and the strip goes.
  const reachable = demoHandlers();
  installFakeTauri(recorded({ ...handlers, attachment_bytes: reachable["attachment_bytes"]! }));
  const again = strip(fold)!.querySelector<HTMLButtonElement>("button")!;
  expect(again.textContent?.trim()).toBe("Try again");
  again.click();
  await until(() => rows(fold).filter((row) => cell(row) === "preview").length === 2, "Try again drew no previews");
  expect(reads()).toEqual(["50000", "50001", "50000", "50001"]);
  expect(strip(fold)).toBeNull();
  expect((button(rows(fold)[0]!, "Download") as HTMLButtonElement).disabled).toBe(false);
});

test("a 403 reads not permitted", async () => {
  install("fake-attachments=forbidden");
  const fold = await openPay231();
  unfold(fold);
  await until(() => strip(fold) !== null, "no stale strip");
  expect(strip(fold)!.querySelector("span")?.textContent).toBe(
    "Tidewater (mock) answers not permitted, so previews and downloads wait. The names and links are the mirror's.",
  );
  expect(rows(fold).map(cell)).toEqual(["placeholder", "placeholder", "PDF", "CSV"]);
});

test("an image over the 5 MiB cap gets no preview, is never fetched, and keeps both buttons", async () => {
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
  expect(reads()).not.toContain("50009");
  expect((button(last, "Download") as HTMLButtonElement).disabled).toBe(false);
  expect(button(last, "Open").tagName).toBe("A");
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
