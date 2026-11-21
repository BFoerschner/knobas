/**
 * Each engine's answer for sixteen fixtures, checked in (the highlighter re-ruling, 2026-11-21, test 4): a version bump
 * or a table change that changes a role shows as a diff someone reads. `node scripts/gen-golden.mjs` writes them again
 * (it runs this file with `UPDATE_GOLDEN=1`).
 */
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import type { Engine, RunContext } from "./engines/engine";
import { createPierre } from "./engines/pierre";
import { createShiki } from "./engines/shiki";
import { createTanstack } from "./engines/tanstack";
import { languageOf, nameIn, type EngineId } from "./engines";
import type { LineTokens } from "./port";

const DIR = join(process.cwd(), "src/lib/render/golden");
const FIXTURES = readdirSync(join(DIR, "fixtures")).sort();
const ENGINES: Record<EngineId, Engine> = { pierre: createPierre(), tanstack: createTanstack(), shiki: createShiki({ tokenizeTimeLimit: 60_000 }) };

async function answer(engine: Engine, text: string, name: string, lang: string): Promise<LineTokens[]> {
  const lines: LineTokens[] = [];
  const ctx: RunContext = {
    post: (from, got) => got.forEach((line, i) => (lines[from + i] = line)),
    pause: async () => {},
    cancelled: () => false,
    takeFocus: () => null,
  };
  await engine.run(text, name, lang, ctx);
  return lines;
}

/** One source line a JSON line: its runs as `role:text`, plain as `:text`. */
const written = (lines: LineTokens[]) => `[\n${lines.map((line) => `  ${JSON.stringify(line.map(([text, role]) => `${role}:${text}`))}`).join(",\n")}\n]\n`;

test("there are sixteen fixtures, and each has a language", () => {
  expect(FIXTURES).toHaveLength(16);
  for (const file of FIXTURES) expect(languageOf(file), file).not.toBeNull();
});

for (const engine of ["pierre", "tanstack", "shiki"] as const)
  for (const file of FIXTURES) {
    const lang = languageOf(file)!;
    const name = nameIn(engine, lang);
    if (name === undefined) continue;
    test(`${engine} on ${file} answers as its golden`, async () => {
      const text = readFileSync(join(DIR, "fixtures", file), "utf8");
      const got = written(await answer(ENGINES[engine], text, name, lang));
      const path = join(DIR, engine, `${file}.json`);
      if (process.env.UPDATE_GOLDEN === "1") {
        mkdirSync(join(DIR, engine), { recursive: true });
        writeFileSync(path, got);
      }
      expect(got).toBe(readFileSync(path, "utf8"));
    });
  }
