import { expect, test } from "vitest";

import type { Attachment, AttachmentBytes } from "../../ipc/entity";
import { ticketImages } from "./attachment-image";
import { createAttachmentPreviews } from "./attachment-previews.svelte";

function file(id: string, name: string, size = 10): Attachment {
  return { id, name, mime: "image/png", size, author: null, created: null, content_url: `https://jira.example/a/${id}`, thumbnail_url: null };
}

const FILES = [file("50000", "psp-409-trace.png"), file("50001", "retry-timeline.png"), file("50009", "huge.png", 6 * 1024 * 1024)];

/** The page's previews over a read that answers every file with one dot, or refuses as `error`. */
function previews(asked: string[], error: unknown = null) {
  const read = (entityId: string, id: string): Promise<AttachmentBytes> => {
    asked.push(`${entityId}/${id}`);
    return error === null ? Promise.resolve({ mime: "image/png", data: "AAAA" }) : Promise.reject(error);
  };
  return createAttachmentPreviews({ entityId: "jira:PAY-231", files: FILES, source: "Tidewater Jira", read, observe: null });
}

test("an inline image is fetched through the page's previews once watched, and loads its data image (#2406, #2405)", async () => {
  const asked: string[] = [];
  const images = ticketImages(previews(asked), "Tidewater Jira", "https://jira.example/browse/PAY-231");
  const loaded = images.load("50001");
  expect(asked, "load alone asks nothing").toEqual([]);
  images.watch!(document.createElement("img"), "50001");
  await expect(loaded).resolves.toBe("data:image/png;base64,AAAA");
  expect(asked).toEqual(["jira:PAY-231/50001"]);
});

test("an id the ticket does not list is refused without a read (#2406)", async () => {
  const asked: string[] = [];
  const images = ticketImages(previews(asked), "Jira", null);
  images.watch!(document.createElement("img"), "49999");
  await expect(images.load("49999")).rejects.toBeDefined();
  expect(asked).toEqual([]);
});

test("a refused image says why, and links the ticket in its source (#2406, #2405)", async () => {
  const forbidden = { code: "unauthorized", message: "Tidewater Jira does not permit this account to read x (HTTP 403)", source_id: "jira" };
  const unreachable = { code: "unreachable", message: "unreachable", source_id: "jira" };
  const words = async (error: unknown, id: string) => {
    const images = ticketImages(previews([], error), "Tidewater Jira", "https://jira.example/browse/PAY-231");
    images.watch!(document.createElement("img"), id);
    const refusal = await images.load(id).then(
      () => null,
      (why: unknown) => why,
    );
    return images.missing("psp-409-trace.png", refusal);
  };
  expect(await words(unreachable, "50000")).toEqual({
    text: "psp-409-trace.png, not reachable in Tidewater Jira right now.",
    link: { href: "https://jira.example/browse/PAY-231", label: "Open in Tidewater Jira" },
  });
  expect((await words(forbidden, "50000")).text).toBe("psp-409-trace.png, not permitted in Tidewater Jira.");
  expect((await words(null, "50009")).text).toBe("psp-409-trace.png is over the 5 MB preview cap.");
  expect(ticketImages(previews([]), "Jira", null).missing("a.png", null).link).toBeNull();
});
