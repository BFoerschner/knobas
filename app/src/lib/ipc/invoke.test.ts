// The app calls the backend through its own `invoke` (`ipc/invoke.ts`) and never writes to Tauri's bridge: v3.2.9-ux7
// assigned to the bridge's read-only `invoke` and hung every packaged build at start. These pin both halves.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(__dirname, "..", "..");

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "node_modules" ? [] : sources(path);
    return /\.(ts|svelte)$/.test(name) && !/\.test\.|\.fixture\./.test(name) ? [path] : [];
  });
}

describe("the app's one way to call a command", () => {
  it("is ipc/invoke.ts: no other module imports Tauri's invoke", () => {
    const direct = sources(SRC)
      .filter((path) => /import\s*\{[^}]*\binvoke\b[^}]*\}\s*from\s*["']@tauri-apps\/api\/core["']/.test(readFileSync(path, "utf8")))
      .map((path) => relative(SRC, path));
    // The answer cache sits behind `ipc/invoke.ts` and is the one caller of Tauri's own.
    expect(direct).toEqual(["lib/ipc/answer-cache.ts"]);
  });

  it("never assigns to the bridge's invoke, which Tauri defines read-only", () => {
    const writes = sources(SRC)
      .filter((path) => /__TAURI_INTERNALS__[^;\n]*\.invoke\s*=[^=]|\bbridge\.invoke\s*=[^=]/.test(readFileSync(path, "utf8")))
      .map((path) => relative(SRC, path));
    expect(writes).toEqual([]);
  });
});
