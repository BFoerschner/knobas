/**
 * Puts Chromium's compile hint on the chunks the shell loads at the start (`src/lib/shell/compile-hints.ts`), in the
 * files as written: Vite puts its preload map (`__vite__mapDeps`) at the top of a chunk after every plugin's
 * `generateBundle`, which pushed a hint added there to the second line, where Chromium does not read it.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Plugin } from "vite";

import { hintStartChunks, withHint, type HintableChunk } from "./src/lib/shell/compile-hints";

export function compileHints(): Plugin {
  return {
    name: "knobas-compile-hints",
    apply: "build",
    enforce: "post",
    writeBundle(options, bundle) {
      const chunks: Record<string, HintableChunk> = {};
      for (const [file, out] of Object.entries(bundle)) {
        chunks[file] = out.type === "chunk" ? { type: "chunk", isEntry: out.isEntry, name: out.name, imports: out.imports } : { type: "asset" };
      }
      for (const file of hintStartChunks(chunks)) {
        const path = join(options.dir ?? "dist", file);
        writeFileSync(path, withHint(readFileSync(path, "utf8")));
      }
    },
  };
}
