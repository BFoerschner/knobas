/** knobas's language table (the highlighter re-ruling, 2026-11-21, test 5). */
import { execFileSync } from "node:child_process";
import { join } from "node:path";

import { bundledLanguagesInfo } from "shiki/langs";
import { expect, test } from "vitest";

import { BY_EXT, BY_FENCE, LANGUAGES } from "./languages";

test("generating it again from the installed packages gives the checked-in file", () => {
  const out = execFileSync(process.execPath, [join(process.cwd(), "../scripts/gen-languages.mjs"), "--check"], { encoding: "utf8" });
  expect(out).toContain("gen-languages: ok");
});

test("every name Shiki 4.5.0 knows keeps its id, as an extension and as a fence", () => {
  for (const info of bundledLanguagesInfo) {
    expect(LANGUAGES[info.id]?.shiki, info.id).toBe(info.id);
    expect(BY_FENCE[info.id]).toBe(info.id);
    if (info.id !== "ini") expect(BY_EXT[info.id], info.id).toBe(info.id);
  }
  expect(BY_EXT.properties).toBe("properties");
  expect(LANGUAGES.properties).toEqual({ pierre: "properties", tanstack: "properties", shiki: "ini" });
});

test("a V file is V, not TanStack's Verilog, and a shell script is Pierre's bash", () => {
  expect(BY_EXT.v).toBe("v");
  expect(LANGUAGES.v?.tanstack).toBeUndefined();
  expect(LANGUAGES.shellscript).toEqual({ shiki: "shellscript", pierre: "bash", tanstack: "shell" });
  for (const id of ["cuda", "groq", "promql", "tsrx", "ejs"]) expect(LANGUAGES[id], id).toBeDefined();
});
