/** An image's size from the head of its bytes (the ux21 README-image pass, 2026-11-19): every kind a README draws. */
import { expect, test } from "vitest";

import { dataUrlSize, imageSize } from "./image-size";

const bytes = (...parts: (number[] | string)[]) =>
  Uint8Array.from(parts.flatMap((part) => (typeof part === "string" ? [...part].map((c) => c.charCodeAt(0)) : part)));
const be32 = (n: number) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
const be16 = (n: number) => [(n >>> 8) & 255, n & 255];
const le16 = (n: number) => [n & 255, (n >>> 8) & 255];
const le24 = (n: number) => [n & 255, (n >>> 8) & 255, (n >>> 16) & 255];
const le32 = (n: number) => [n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255];
const base64 = (b: Uint8Array) => btoa(String.fromCharCode(...b));

const png = (w: number, h: number) => bytes([0x89], "PNG\r\n\x1a\n", be32(13), "IHDR", be32(w), be32(h), [8, 6, 0, 0, 0]);

/** A JPEG: SOI, an EXIF block saying `orientation` (when given), a comment, and a baseline frame of `w` by `h`. */
function jpeg(w: number, h: number, orientation?: number, order: "II" | "MM" = "II") {
  const u16 = order === "II" ? le16 : be16;
  const u32 = order === "II" ? le32 : be32;
  const tiff = orientation === undefined ? [] : [...[...order].map((c) => c.charCodeAt(0)), ...u16(42), ...u32(8), ...u16(1), ...u16(0x0112), ...u16(3), ...u32(1), ...u16(orientation), 0, 0, ...u32(0)];
  const exif = orientation === undefined ? [] : [0xff, 0xe1, ...be16(2 + 6 + tiff.length), ...bytes("Exif\0\0"), ...tiff];
  const comment = [0xff, 0xfe, ...be16(5), ...bytes("abc")];
  return bytes([0xff, 0xd8], exif, comment, [0xff, 0xc0, ...be16(17), 8, ...be16(h), ...be16(w), 3]);
}

test("a PNG's size is its header's", () => {
  expect(imageSize(png(96, 32))).toEqual({ w: 96, h: 32 });
  expect(imageSize(png(0, 32)), "an empty side is no size").toBeNull();
  expect(imageSize(png(96, 32).subarray(0, 20)), "a head cut short").toBeNull();
});

test("a GIF's size is its screen's", () => {
  expect(imageSize(bytes("GIF89a", le16(320), le16(200), [0, 0, 0]))).toEqual({ w: 320, h: 200 });
  expect(imageSize(bytes("GIF87a", le16(1), le16(2), [0, 0, 0]))).toEqual({ w: 1, h: 2 });
});

test("a JPEG's size is its frame's, turned where its EXIF orientation is a quarter turn, as the webview draws it", () => {
  expect(imageSize(jpeg(640, 480))).toEqual({ w: 640, h: 480 });
  expect(imageSize(jpeg(640, 480, 1))).toEqual({ w: 640, h: 480 });
  expect(imageSize(jpeg(640, 480, 3)), "a half turn keeps the sides").toEqual({ w: 640, h: 480 });
  expect(imageSize(jpeg(640, 480, 6))).toEqual({ w: 480, h: 640 });
  expect(imageSize(jpeg(640, 480, 8, "MM"))).toEqual({ w: 480, h: 640 });
  expect(imageSize(bytes([0xff, 0xd8, 0xff, 0xda, 0, 4, 0, 0])), "scan data before any frame").toBeNull();
});

test("a progressive JPEG's frame (SOF2) is read, and a Huffman table (C4) is not taken for one", () => {
  const table = [0xff, 0xc4, ...be16(6), 1, 2, 3, 4];
  expect(imageSize(bytes([0xff, 0xd8], table, [0xff, 0xc2, ...be16(17), 8, ...be16(90), ...be16(120), 3]))).toEqual({ w: 120, h: 90 });
});

test("a WebP's size is read from each of its three kinds", () => {
  const riff = (chunk: string, body: number[]) => bytes("RIFF", le32(4 + 8 + body.length), "WEBP", chunk, le32(body.length), body);
  // Lossy: a frame tag, the start code, then 14-bit sides.
  expect(imageSize(riff("VP8 ", [0, 0, 0, 0x9d, 0x01, 0x2a, ...le16(400), ...le16(300), 0, 0]))).toEqual({ w: 400, h: 300 });
  // Lossless: the signature, then the sides less one in 14 bits each.
  const bits = (400 - 1) | ((300 - 1) << 14);
  expect(imageSize(riff("VP8L", [0x2f, ...le32(bits), 0, 0, 0, 0, 0]))).toEqual({ w: 400, h: 300 });
  // Extended: flags, then the canvas less one in 24 bits each.
  expect(imageSize(riff("VP8X", [0, 0, 0, 0, ...le24(4999), ...le24(1), 0, 0]))).toEqual({ w: 5000, h: 2 });
});

test("a BMP's size is its header's, a bottom-up one's height unsigned", () => {
  expect(imageSize(bytes("BM", le32(0), le32(0), le32(54), le32(40), le32(16), le32(-9), [0, 0]))).toEqual({ w: 16, h: 9 });
});

test("an SVG's size is its root's pixel sides, else its shape fitted as an <img> draws it", () => {
  const svg = (attrs: string) => new TextEncoder().encode(`<?xml version="1.0"?>\n<!-- a comment -->\n<svg version="1.1" ${attrs}><rect/></svg>`);
  expect(imageSize(svg('width="120" height="40"'))).toEqual({ w: 120, h: 40 });
  expect(imageSize(svg("width='120px' height='40.4px'"))).toEqual({ w: 120, h: 40 });
  expect(imageSize(svg('width="200" viewBox="0 0 100 50"'))).toEqual({ w: 200, h: 100 });
  expect(imageSize(svg('height="60" viewBox="0 0 100 50"'))).toEqual({ w: 120, h: 60 });
  expect(imageSize(svg('viewBox="0 0 100 100"')), "a square fitted to 300x150").toEqual({ w: 150, h: 150 });
  expect(imageSize(svg('viewBox="0,0,800,100"')), "a wide one fitted to 300 wide").toEqual({ w: 300, h: 38 });
  expect(imageSize(svg('width="100%" height="50%"')), "a percentage is the page's to lay out").toBeNull();
  expect(imageSize(svg('width="10em" viewBox="0 0 10 10"')), "another unit says nothing").toEqual({ w: 150, h: 150 });
  expect(imageSize(svg('width="10"')), "one side and no shape").toBeNull();
  expect(imageSize(svg("")), "nothing").toBeNull();
});

test("bytes of no known kind, and nothing, have no size", () => {
  expect(imageSize(new Uint8Array())).toBeNull();
  expect(imageSize(bytes("not an image at all, just words"))).toBeNull();
  expect(imageSize(png(70_000, 10)), "a side past any real image is not believed").toBeNull();
});

test("a data URL's size is read from the head of its base64, of any image kind", () => {
  expect(dataUrlSize(`data:image/png;base64,${base64(png(96, 32))}`)).toEqual({ w: 96, h: 32 });
  expect(dataUrlSize(`data:image/jpeg;name=a.jpg;base64,${base64(jpeg(640, 480, 6))}`)).toEqual({ w: 480, h: 640 });
  expect(dataUrlSize(`data:image/svg+xml,${encodeURIComponent('<svg width="12" height="34"></svg>')}`)).toEqual({ w: 12, h: 34 });
  expect(dataUrlSize("data:text/plain;base64,aGVsbG8=")).toBeNull();
  expect(dataUrlSize("blob:x")).toBeNull();
  expect(dataUrlSize("data:image/png;base64,!!!")).toBeNull();
});
