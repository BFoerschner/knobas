import { afterEach, expect, test } from "vitest";

import type { Attachment } from "../../ipc/entity";
import { fillAttachmentImages, ticketImages } from "./attachment-image";

const DOT = "data:image/png;base64,AAAA";

function file(id: string, name: string): Attachment {
  return { id, name, mime: "image/png", size: 10, author: null, created: null, content_url: `https://jira.example/a/${id}`, thumbnail_url: null };
}

const FILES = [file("50000", "psp-409-trace.png"), file("50001", "retry-timeline.png")];
let undo = () => {};
afterEach(() => undo());

test("an inline image loads the ticket's attachment of that id through the filled read (#2406)", async () => {
  const asked: [string, string][] = [];
  undo = fillAttachmentImages(async (entityId, attachment) => (asked.push([entityId, attachment.name]), DOT));
  const images = ticketImages("jira:PAY-231", FILES, "Tidewater Jira", "https://jira.example/browse/PAY-231");
  await expect(images.load("50001")).resolves.toBe(DOT);
  expect(asked).toEqual([["jira:PAY-231", "retry-timeline.png"]]);
});

test("an id the ticket does not list is refused without a read (#2406)", async () => {
  const asked: string[] = [];
  undo = fillAttachmentImages(async (_, attachment) => (asked.push(attachment.id), DOT));
  await expect(ticketImages("jira:PAY-231", FILES, "Jira", null).load("49999")).rejects.toThrow();
  expect(asked).toEqual([]);
});

test("until a read is filled, every inline image is refused (#2406)", async () => {
  await expect(ticketImages("jira:PAY-231", FILES, "Jira", null).load("50000")).rejects.toThrow();
});

test("a refused image says it is not reachable in the source right now, and links the ticket there (#2406)", () => {
  expect(ticketImages("jira:PAY-231", FILES, "Tidewater Jira", "https://jira.example/browse/PAY-231").missing("psp-409-trace.png", new Error("x"))).toEqual({
    text: "psp-409-trace.png, not reachable in Tidewater Jira right now.",
    link: { href: "https://jira.example/browse/PAY-231", label: "Open in Tidewater Jira" },
  });
  expect(ticketImages("jira:PAY-231", FILES, "Jira", null).missing("a.png", null).link).toBeNull();
});
