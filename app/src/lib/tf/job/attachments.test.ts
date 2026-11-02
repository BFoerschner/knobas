import { describe, expect, test } from "vitest";

import { attachmentKind, attachmentLine } from "./attachments";

describe("an attachment's kind (#2404)", () => {
  test("is the file name's extension in capitals", () => {
    expect(attachmentKind({ name: "sepa-error-codes.pdf", mime: "application/pdf" })).toBe("PDF");
    expect(attachmentKind({ name: "psp-409-trace.png", mime: "image/png" })).toBe("PNG");
    expect(attachmentKind({ name: "archive.tar.gz", mime: null })).toBe("GZ");
  });

  test("falls back to the media type's subtype where the name has no short extension", () => {
    expect(attachmentKind({ name: "README", mime: "text/plain" })).toBe("PLAIN");
    expect(attachmentKind({ name: "notes.markdown", mime: "text/x-web" })).toBe("FILE");
    expect(attachmentKind({ name: "notes.markdown", mime: "text/markdown" })).toBe("MARKDOWN");
    expect(attachmentKind({ name: ".env", mime: null })).toBe("FILE");
  });

  test("is FILE where neither says", () => {
    expect(attachmentKind({ name: "dump", mime: null })).toBe("FILE");
    expect(attachmentKind({ name: "dump", mime: "application/vnd.ms-excel.sheet.macroEnabled.12" })).toBe("FILE");
  });
});

describe("an attachment's line (#2404)", () => {
  test("says who, when and how big", () => {
    expect(attachmentLine({ author: "Priya Nair", created: "2026-08-12T11:30:00Z", size: 1_258_291 })).toBe("Priya Nair, 2026-08-12, 1.2 MB");
  });

  test("leaves out what the source did not say", () => {
    expect(attachmentLine({ author: null, created: "2026-08-14T08:45:00Z", size: 49_152 })).toBe("2026-08-14, 48 KB");
    expect(attachmentLine({ author: "Lena Hoffmann", created: null, size: null })).toBe("Lena Hoffmann");
    expect(attachmentLine({ author: null, created: "not a time", size: null })).toBe("");
  });
});
