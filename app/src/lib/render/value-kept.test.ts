/** Chains kept for fields (the XML view's design, 2026-11-23, section 18). */
import { expect, test } from "vitest";

import type { NamedPipeline } from "../ipc/app";
import { endsAs, keptFor, offeredField } from "./value-kept";

const pdf: NamedPipeline = { id: "mime-pdf", name: "MIME_DATA as PDF", steps: [{ step: "base64_decode" }], fields: [{ files: "*.xml", field: "MIME_EMBEDDED/MIME_DATA", end: "pdf" }] };
const any: NamedPipeline = { id: "data", name: "Any data", steps: [{ step: "base64_decode" }], fields: [{ files: "*.xml", field: "MIME_DATA", end: "hex" }] };
const json: NamedPipeline = { id: "att", name: "Attachment", steps: [{ step: "base64_decode" }], fields: [{ files: "*.json", field: "attachments[].content", end: "pdf" }] };

test("a field matches where its names end, under the header and under an item, not under another name", () => {
  expect(keptFor("ORDER/ORDER_HEADER/MIME_INFO/MIME/MIME_EMBEDDED/MIME_DATA", "order.xml", [pdf])?.pipeline.id).toBe("mime-pdf");
  expect(keptFor("ORDER/ORDER_ITEM_LIST/ORDER_ITEM/MIME_INFO/MIME/MIME_EMBEDDED/MIME_DATA", "a/b/order.xml", [pdf])?.pipeline.id).toBe("mime-pdf");
  expect(keptFor("ORDER/OTHER/MIME_DATA", "order.xml", [pdf])).toBeNull();
  expect(keptFor("ORDER/XMIME_EMBEDDED/MIME_DATA", "order.xml", [pdf])).toBeNull();
});

test("of two kept chains the longer field wins, and the file pattern counts", () => {
  expect(keptFor("A/MIME_EMBEDDED/MIME_DATA", "x.xml", [any, pdf])?.pipeline.id).toBe("mime-pdf");
  expect(keptFor("A/OTHER/MIME_DATA", "x.xml", [any, pdf])?.pipeline.id).toBe("data");
  expect(keptFor("A/MIME_EMBEDDED/MIME_DATA", "x.json", [any, pdf])).toBeNull();
});

test("a JSON field: an array's items are []", () => {
  expect(keptFor("orders[].attachments[].content", "export.json", [json])?.pipeline.id).toBe("att");
  expect(endsAs("orders[].attachments[].content", "content")).toBe(true);
  expect(endsAs("orders[].attachments[].xcontent", "content")).toBe(false);
});

test("the field offered is the last two names", () => {
  expect(offeredField("ORDER/A/MIME_EMBEDDED/MIME_DATA")).toBe("MIME_EMBEDDED/MIME_DATA");
  expect(offeredField("Signature/SignedInfo/@Id")).toBe("SignedInfo/@Id");
  expect(offeredField("orders[].attachments[].content")).toBe("attachments[].content");
});
