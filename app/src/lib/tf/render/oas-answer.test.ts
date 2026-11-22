/** An answer's words (ux31, section 19): the tone a status takes, the size in words, a JSON line's tokens. */
import { expect, test } from "vitest";

import { jsonTokens, sizeWords, statusTone } from "./oas-answer";

test("a status takes its tone: ok under 300, warn under 400, err from 400", () => {
  expect([200, 204, 299, 300, 304, 399, 400, 404, 500].map(statusTone)).toEqual(["ok", "ok", "ok", "warn", "warn", "warn", "err", "err", "err"]);
});

test("a size in words", () => {
  expect([0, 1023, 1024, 2150, 3 * 1024 * 1024].map(sizeWords)).toEqual(["0 B", "1023 B", "1.0 KB", "2.1 KB", "3.0 MB"]);
});

test("a JSON line's keys, strings, numbers and words are told apart, and markup stays text", () => {
  expect(jsonTokens('  "id": 8134, "ok": true, "s": "<b>x</b>", "n": null')).toEqual([
    { t: "  ", k: "" }, { t: '"id"', k: "k" }, { t: ":", k: "" }, { t: " ", k: "" }, { t: "8134", k: "n" }, { t: ", ", k: "" },
    { t: '"ok"', k: "k" }, { t: ":", k: "" }, { t: " ", k: "" }, { t: "true", k: "b" }, { t: ", ", k: "" },
    { t: '"s"', k: "k" }, { t: ":", k: "" }, { t: " ", k: "" }, { t: '"<b>x</b>"', k: "s" }, { t: ", ", k: "" },
    { t: '"n"', k: "k" }, { t: ":", k: "" }, { t: " ", k: "" }, { t: "null", k: "b" },
  ]);
  expect(jsonTokens("}").map((t) => t.t).join("")).toBe("}");
});
