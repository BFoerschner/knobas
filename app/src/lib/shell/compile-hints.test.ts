// Only the chunk of the pages whose first open compiled the most gets Chromium's compile hint, and only when the start
// loads it (the ux30 Windows pass, 2026-11-23).
import { describe, expect, it } from "vitest";

import { COMPILE_HINT, FIRST_OPENS, FIRST_OPENS_MODULES, hintStartChunks, withHint, type HintableChunk } from "./compile-hints";

const chunk = (fields: Partial<HintableChunk>): HintableChunk => ({ type: "chunk", imports: [], code: "export {};", ...fields });

describe("hintStartChunks", () => {
  it("hints the first opens' chunk the start imports, at any depth, once, and no other chunk", () => {
    const bundle: Record<string, HintableChunk> = {
      "assets/index-a.js": chunk({ isEntry: true, name: "index", imports: ["assets/ipc-b.js"], code: "import './ipc-b.js';" }),
      "assets/ipc-b.js": chunk({ name: "ipc", imports: ["assets/first-opens-c.js"] }),
      "assets/first-opens-c.js": chunk({ name: FIRST_OPENS, imports: ["assets/index-a.js"] }),
      "assets/d2-d.js": chunk({ name: "d2", code: "big()" }),
      "assets/index.css": { type: "asset" },
    };
    expect(hintStartChunks(bundle)).toEqual(["assets/first-opens-c.js"]);
    expect(hintStartChunks(bundle)).toEqual(["assets/first-opens-c.js"]);
    const code = bundle["assets/first-opens-c.js"]!.code!;
    expect(code.startsWith(COMPILE_HINT)).toBe(true);
    expect(code.indexOf(COMPILE_HINT, 1), "hinted twice").toBe(-1);
    for (const file of ["assets/index-a.js", "assets/ipc-b.js", "assets/d2-d.js"]) expect(bundle[file]!.code!.startsWith(COMPILE_HINT), file).toBe(false);
  });

  it("hints no first opens' chunk that only a page loaded on demand imports", () => {
    const bundle = {
      "assets/index-a.js": chunk({ isEntry: true, name: "index" }),
      "assets/first-opens-c.js": chunk({ name: FIRST_OPENS }),
    };
    expect(hintStartChunks(bundle)).toEqual([]);
    expect(bundle["assets/first-opens-c.js"].code).toBe("export {};");
  });

  it("is the line Chromium reads, first and once", () => {
    expect(COMPILE_HINT).toBe("//# allFunctionsCalledOnLoad\n");
    expect(withHint("const __vite__mapDeps = 1;")).toBe("//# allFunctionsCalledOnLoad\nconst __vite__mapDeps = 1;");
    expect(withHint(withHint("x"))).toBe("//# allFunctionsCalledOnLoad\nx");
  });

  it("names the pages whose first open compiled the most", () => {
    for (const path of ["src/lib/tf/repo/TfRepo.svelte", "src/lib/tf/job/TfJob.svelte", "src/lib/tf/builds/TfRun.svelte", "src/lib/tf/docs/TfDocs.svelte", "src/lib/tf/tickets/TicketsPage.svelte"]) expect(FIRST_OPENS_MODULES.test(path), path).toBe(true);
    expect(FIRST_OPENS_MODULES.test("src/lib/tf/monitors/TfMonitors.svelte")).toBe(false);
  });
});
