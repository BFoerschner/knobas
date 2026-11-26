/** A repository's files in the launcher's pane (the everything-search decisions, D10 and D11). */
import { describe, expect, it } from "vitest";

import { countLine, marksIn, pathPieces, quotedLines } from "./files";

const flat = (segments: readonly { text: string; hit: boolean }[]) => segments.map((s) => (s.hit ? `[${s.text}]` : s.text)).join("");

describe("countLine", () => {
  const found = (by_text: number, by_name: number) => ({ by_text, by_name, branch: "main" });
  it("says how many files and how many of them by name, on which branch", () => {
    expect(countLine(found(4, 3), null, 4)).toBe("In 4 files of main, 3 of them by name.");
    expect(countLine(found(1, 0), null, 1)).toBe("In 1 file of main.");
    expect(countLine(found(37, 0), { files: [], by_text: 37, by_name: 0, more: true, branch: "main" }, 8)).toBe("In 37 files of main. The first 8 are shown.");
    expect(countLine(found(0, 3), null, 3)).toBe("3 files of main are named by it.");
    expect(countLine(found(0, 1), null, 1)).toBe("1 file of main is named by it.");
  });
});

describe("marksIn and pathPieces", () => {
  it("marks a word from a part's start, skipping - and _, never inside a part", () => {
    expect(flat(marksIn("src/com/acme/adproxy/ADProxyServer.java", ["adproxy"]))).toBe("src/com/acme/[adproxy]/[ADProxy]Server.java");
    expect(flat(marksIn("charts/ad-proxy/values.yaml", ["adproxy"]))).toBe("charts/[ad-proxy]/values.yaml");
    expect(flat(marksIn("dproxy", ["adproxy"]))).toBe("dproxy");
  });

  it("sets the file's name apart from its folders", () => {
    const pieces = pathPieces("src/adproxy/ADProxyServer.java", ["adproxy"]);
    expect(pieces.filter((p) => p.name).map((p) => p.text).join("")).toBe("ADProxyServer.java");
    expect(pieces.find((p) => p.name && p.hit)!.text).toBe("ADProxy");
  });
});

describe("quotedLines", () => {
  it("drops the indent the three lines share and leaves a blank neighbour out", () => {
    const got = quotedLines({
      place: "A.java:7",
      segments: [{ text: "    public final class ", hit: false }, { text: "ADProxy", hit: true }, { text: "Server {", hit: false }],
      file: { path: "A.java", line: 7, before: "    /** TLS */", after: "        private final Config config;" },
    });
    expect(got.before).toBe("/** TLS */");
    expect(flat(got.line)).toBe("public final class [ADProxy]Server {");
    expect(got.after).toBe("    private final Config config;");
    const blank = quotedLines({ place: "A:1", segments: [{ text: "x", hit: true }], file: { path: "A", line: 1, before: "  ", after: "" } });
    expect([blank.before, blank.after]).toEqual([null, null]);
  });
});
