/** What knobas takes a value for (the XML view's design, 2026-11-23, section 17). */
import { expect, test } from "vitest";

import { answerDoc } from "./doc-model";
import type { ValueAt } from "./doc-model";
import { fixture } from "./doc-fixture";
import { guess, sniffBytes } from "./value-guess";

/** Section 3's look-alikes: letters and digits nothing should offer to decode, and two that are real base64 of something. */
const NOT = [
  "TW2026A7781KQ40Z9XLMR3", "TW2026A7781KQ40Z9XLMR3AB", "da39a3ee5e6b4b0d3255bfef95601890afd80709", "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709",
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "550e8400-e29b-41d4-a716-446655440000", "550e8400e29b41d4a716446655440000",
  "4019537441633", "WERKZEUG-AKKUSCHRAUBER-18V-SET-2026-A", "AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "ghp_16C7e42F292c6912E7710c838347Ae178B4a", "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA", "Zm9v", "Zm9vYmFy", "SGVsbG8sIFdvcmxkIQ==",
  "QWxhZGRpbjpvcGVuIHNlc2FtZQ==", "LOT20260099466ABCDEFGH12", "Mime descr Mime descr Mime descr", "abcdefghijklmnopqrstuvwx",
  "ABCDEFGHIJKLMNOPQRSTUVWX", "0123456789012345678901234567", "xK9mPq2LvR8tYw3ZnB5cJh7dFg1sA4uE",
  "c2VjcmV0LXRva2VuLXZhbHVlLWZvci10aGUtYXBpLWdhdGV3YXktMjAyNi0xMS0yMw==", "9j4AAQSkZJRgABAQAAAQABAAD",
  "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8U",
];

test("of the 26 look-alikes two are offered, and both are base64 of something readable", () => {
  const offered = NOT.filter((v) => guess(v) !== null);
  expect(offered).toEqual(["c2VjcmV0LXRva2VuLXZhbHVlLWZvci10aGUtYXBpLWdhdGV3YXktMjAyNi0xMS0yMw==", "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8U"]);
  expect(guess(offered[0]!)?.sniffed?.kind).toBe("text");
  expect(guess(offered[1]!)?.sniffed?.word).toBe("JPEG");
});

test("no SHA-1, SHA-256 or UUID in either case is ever offered", () => {
  let seed = 7;
  const hex = (n: number) => Array.from({ length: n }, () => "0123456789abcdef"[(seed = (seed * 1103515245 + 12345) & 0x7fffffff) % 16]).join("");
  for (let i = 0; i < 300; i += 1) {
    for (const v of [hex(40), hex(64), `${hex(8)}-${hex(4)}-${hex(4)}-${hex(4)}-${hex(12)}`, hex(32)]) {
      expect(guess(v), v).toBeNull();
      expect(guess(v.toUpperCase()), v).toBeNull();
    }
  }
});

test("said by the file: encoding=base64, a data: address; read from its shape: a JWT, an XML or JSON string", () => {
  expect(guess("AAECAwQFBgcICQ==", { base64: true })?.end).toBe("hex");
  expect(guess("data:image/png;base64,iVBORw0KGgo=")?.chip.bold).toBe("png");
  expect(guess("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI0NzExIn0.c2ln")?.what).toBe("jwt");
  expect(guess('<?xml version="1.0"?><a>1</a>')?.what).toBe("xml");
  expect(guess('{"route":"express"}')?.what).toBe("json");
  expect(guess("{not json")).toBeNull();
});

test("the bytes overrule the label beside them: a PNG said to be image/jpeg is drawn as an image", () => {
  const png = "iVBORw0KGgoAAAANSUhEUgAAAIAAAACACAYAAADDPmHLAAAH3UlEQVR42u1d";
  const g = guess(png, { mime: "image/jpeg" })!;
  expect(g.sniffed?.mime).toBe("image/png");
  expect(g.end).toBe("image");
  expect(sniffBytes([0x89, 0x50, 0x4e, 0x47])?.kind).toBe("image");
  // A PNG said to be a PDF is drawn as an image too.
  expect(guess(png, { mime: "application/pdf" })!.end).toBe("image");
});

test("gzip in base64 is unwrapped, and drawn as text", () => {
  const g = guess("H4sIAAAAAAAAE1XKsQ3CMBAF0D5T/BIkgmwnAZQNEBQULOCIi3USvkP2kYnYhMUogVe/4MKu9b51")!;
  expect(g.steps).toEqual([{ step: "base64_decode" }, { step: "gunzip" }]);
  expect(g.end).toBe("text");
});

test("of the order's values six are taken for something, all six rightly; the batch number above them is not", () => {
  const text = fixture("order-4710.xml");
  const sessions = new Map();
  answerDoc(sessions, 1, { kind: "open", text });
  const lines = text.split("\n").map((_, i) => i + 1);
  const values = answerDoc(sessions, 1, { kind: "values", lines, all: false }) as Record<number, ValueAt[]>;
  const taken: string[] = [];
  for (const list of Object.values(values)) for (const v of list) {
    const g = guess(v.head, v.said, { length: v.length, bare: v.bare });
    if (g !== null) taken.push(`${v.name}:${g.what}:${g.sniffed?.word ?? ""}`);
  }
  expect(taken).toEqual([
    "MIME_DATA:base64:PDF",
    "UDX.TIDEWATER.APPROVAL:jwt:",
    "UDX.TIDEWATER.ROUTING:json:",
    "UDX.TIDEWATER.LABEL:xml:",
    "UDX.TIDEWATER.THUMBNAIL:base64:PNG",
    "UDX.TIDEWATER.AUDIT:base64:gzip",
  ]);
});
