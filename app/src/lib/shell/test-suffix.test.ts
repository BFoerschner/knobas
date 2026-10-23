import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

/** vite.config.ts: `.test.svelte.ts` exists so vite-plugin-svelte compiles runes; keep it for tests that need it. */
const RUNE_USE = /\$state|\$derived|\$effect|\bmount\(|\bflushSync\(/;

function* walk(dir: string): Generator<string> {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* walk(path);
    else if (name.endsWith(".test.svelte.ts")) yield path;
  }
}

test("a .test.svelte.ts file uses a rune, mount or flushSync; otherwise it is a .test.ts", () => {
  const rune_free = [...walk(join(__dirname, "../.."))].filter(
    (path) => !RUNE_USE.test(readFileSync(path, "utf8")),
  );
  expect(rune_free).toEqual([]);
});
