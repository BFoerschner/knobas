import { describe, expect, it } from "vitest";

import { isText, leafOf, TEXT_CAP, tryDraft, tryName, tryOnOf, tryWhy } from "./try-on";

describe("try-on", () => {
  it("waits for the page's reason first, then for what the source still needs", () => {
    const draft = { ...tryDraft("text"), text: "# Hi" };
    expect(tryWhy(draft, "Name it first.")).toBe("Name it first.");
    expect(tryWhy(draft, null)).toBeNull();
    expect(tryWhy({ ...draft, name: "  " }, null)).toBe("Give the text a file name, such as example.md.");
    expect(tryWhy({ ...draft, name: "a/b.md" }, null)).toBe("A file name holds no / or \\.");
    expect(tryWhy({ ...draft, text: "" }, null)).toBe("Paste some text first.");
    expect(tryWhy(tryDraft("file"), null)).toBe("Choose a file first.");
    expect(tryWhy({ ...tryDraft("file"), file: "/Users/mara/a.md" }, null)).toBeNull();
    expect(tryWhy(tryDraft("folder"), null)).toBe("Choose a folder first.");
    expect(tryWhy({ ...tryDraft("folder"), folder: "/Users/mara/flows" }, null)).toBeNull();
    expect(tryWhy(tryDraft("repo"), null)).toBeNull();
  });

  it("refuses pasted text past what Code hands a renderer", () => {
    const big = "x".repeat(TEXT_CAP + 1);
    expect(tryWhy({ ...tryDraft("text"), text: big }, null)).toBe("The text is 51 MB, past the 50 MB knobas hands a renderer.");
    expect(tryWhy({ ...tryDraft("text"), text: "x".repeat(TEXT_CAP) }, null)).toBeNull();
  });

  it("sends the source picked, the name trimmed, and nothing for a repository", () => {
    expect(tryOnOf({ ...tryDraft("text", " diagram.mmd "), text: "graph TD" })).toEqual({ kind: "text", name: "diagram.mmd", text: "graph TD" });
    const both = { ...tryDraft("file"), file: "/a/b.md", folder: "/a" };
    expect(tryOnOf(both)).toEqual({ kind: "path", path: "/a/b.md" });
    expect(tryOnOf({ ...both, source: "folder" })).toEqual({ kind: "path", path: "/a" });
    expect(tryOnOf({ ...both, source: "repo" })).toBeNull();
    expect(tryOnOf(tryDraft("file"))).toBeNull();
  });

  it("names a try by the pasted name or the chosen path's last name", () => {
    expect(tryName({ kind: "text", name: "a.md", text: "" })).toBe("a.md");
    expect(tryName({ kind: "path", path: "/Users/mara/flows/" })).toBe("flows");
    expect(leafOf(String.raw`C:\Users\mara\order.xml`)).toBe("order.xml");
  });

  it("tells text from bytes as git does", () => {
    expect(isText(new TextEncoder().encode("# Ünïcode\n"))).toBe(true);
    expect(isText(Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0, 1]))).toBe(false);
    expect(isText(Uint8Array.from([0xc3, 0x28]))).toBe(false);
  });
});
