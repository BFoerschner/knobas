/**
 * The ticket page's attachment previews (spec #2396, #2405): one object per
 * open page, asked through its read, never stored. Driven through its own
 * interface with a read the test answers, so what is asked, when, and what
 * each answer leaves behind is on the record.
 */
import { flushSync } from "svelte";
import { describe, expect, test } from "vitest";

import type { Attachment, AttachmentBytes } from "../../ipc/entity";
import { createAttachmentPreviews } from "./attachment-previews.svelte";

function file(id: string, name: string, mime: string | null, size: number | null): Attachment {
  return { id, name, mime, size, author: null, created: null, content_url: `https://jira.example/secure/attachment/${id}/${name}`, thumbnail_url: null };
}

const FILES = [
  file("1", "trace.png", "image/png", 180_000),
  file("2", "codes.pdf", "application/pdf", 1_200_000),
  file("3", "huge.png", "image/png", 6 * 1024 * 1024),
  file("4", "timeline.png", "image/png", 90_000),
];

/** A read the test answers by hand, with every ask written down. */
function hand() {
  const asked: string[] = [];
  const pending = new Map<string, { resolve: (bytes: AttachmentBytes) => void; reject: (error: unknown) => void }>();
  const read = (entityId: string, attachmentId: string) =>
    new Promise<AttachmentBytes>((resolve, reject) => {
      asked.push(`${entityId}/${attachmentId}`);
      pending.set(attachmentId, { resolve, reject });
    });
  return { asked, pending, read };
}

/** Lets a settled read's handlers run, then the state they set. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

describe("attachment previews", () => {
  test("fetching the images asks for each image under the cap once, and nothing else", async () => {
    const { asked, read } = hand();
    const previews = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read });
    previews.fetchImages();
    previews.fetchImages();
    previews.fetch("1");
    expect(asked).toEqual(["jira:PAY-231/1", "jira:PAY-231/4"]);
    expect(previews.of("1")).toEqual({ state: "loading" });
    expect(previews.of("2")).toEqual({ state: "none" });
    expect(previews.of("3")).toEqual({ state: "too-large" });
  });

  test("an answer is a data image, and a second look asks nothing", async () => {
    const { asked, pending, read } = hand();
    const previews = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read });
    previews.fetch("1");
    pending.get("1")!.resolve({ mime: "image/png", data: "iVBOR0JZVEVT" });
    await settle();
    expect(previews.of("1")).toEqual({ state: "ready", src: "data:image/png;base64,iVBOR0JZVEVT" });
    await expect(previews.load("1")).resolves.toBe("data:image/png;base64,iVBOR0JZVEVT");
    previews.fetch("1");
    expect(asked).toEqual(["jira:PAY-231/1"]);
    expect(previews.failure).toBeNull();
  });

  test("an unreachable source fails the read and names the strip; Try again asks again what failed", async () => {
    const { asked, pending, read } = hand();
    const previews = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read });
    previews.fetchImages();
    pending.get("1")!.reject({ code: "unreachable", message: "unreachable: connection refused", source_id: "jira" });
    pending.get("4")!.resolve({ mime: "image/png", data: "QQ==" });
    await settle();
    expect(previews.of("1")).toEqual({ state: "failed", reason: "unreachable", message: "unreachable: connection refused" });
    expect(previews.failure).toEqual({ reason: "unreachable", source: "Tidewater Jira" });

    previews.retry();
    expect(asked).toEqual(["jira:PAY-231/1", "jira:PAY-231/4", "jira:PAY-231/1"]);
    expect(previews.of("1")).toEqual({ state: "loading" });
    expect(previews.failure).toBeNull();
    expect(previews.of("4")).toEqual({ state: "ready", src: "data:image/png;base64,QQ==" });
  });

  test("a 403 is not permitted, another refusal is the credential, and a rejected load says why", async () => {
    const { pending, read } = hand();
    const previews = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read });
    const loaded = previews.load("1");
    previews.fetch("4");
    pending.get("1")!.reject({ code: "unauthorized", message: "Tidewater Jira does not permit this account to read trace.png (HTTP 403)", source_id: "jira" });
    pending.get("4")!.reject({ code: "unauthorized", message: "unauthorized", source_id: "jira" });
    await settle();
    expect(previews.of("1")).toMatchObject({ state: "failed", reason: "forbidden" });
    expect(previews.of("4")).toMatchObject({ state: "failed", reason: "refused" });
    expect(previews.failure).toEqual({ reason: "forbidden", source: "Tidewater Jira" });
    await expect(loaded).rejects.toMatchObject({ state: "failed", reason: "forbidden" });
    await expect(previews.load("3")).rejects.toEqual({ state: "too-large" });
  });

  test("an answer that arrives after the page dropped its previews is not kept", async () => {
    const { pending, read } = hand();
    const previews = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read });
    previews.fetch("1");
    previews.drop();
    pending.get("1")!.resolve({ mime: "image/png", data: "QQ==" });
    await settle();
    expect(previews.of("1")).toEqual({ state: "none" });
  });

  test("watch asks once the node is in view, and at once where nothing can tell", () => {
    const { asked, read } = hand();
    const seen: ((entries: { isIntersecting: boolean }[]) => void)[] = [];
    const observe = (callback: (entries: { isIntersecting: boolean }[]) => void) => {
      seen.push(callback);
      return { observe: () => {}, disconnect: () => {} };
    };
    const watched = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read, observe });
    const stop = watched.watch(document.createElement("img"), "1");
    expect(asked).toEqual([]);
    seen[0]!([{ isIntersecting: false }]);
    expect(asked).toEqual([]);
    seen[0]!([{ isIntersecting: true }]);
    expect(asked).toEqual(["jira:PAY-231/1"]);
    stop();

    const blind = createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read, observe: null });
    blind.watch(document.createElement("img"), "4");
    expect(asked).toEqual(["jira:PAY-231/1", "jira:PAY-231/4"]);
  });
});
