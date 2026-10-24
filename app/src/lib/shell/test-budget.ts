/**
 * **The budget of a test that reads the whole of `app/src`** (#2035).
 *
 * `testTimeout` (15 s, `vite.config.ts`) is sized for a unit test slowed by a
 * neighbour's build. A guard that type-checks every source file costs in
 * proportion to the tree instead, and the `check.yml` runner, where vitest
 * shares two cores with cargo, runs it eight to ten times slower than a Mac:
 * `browser-globals` passed 15 s there at 544 files and turned main red. A
 * budget per file keeps such a guard's pass from depending on how far the
 * tree has grown, and a guard that hangs still fails.
 *
 * 100 ms a file: the costliest guard, `nullish-fallback`, took 31 s for about
 * 500 files (62 ms a file) in the slower of the two runner runs measured,
 * 40124502573. Run-to-run spread on that runner was 1.6 times.
 */
import { readdirSync } from "node:fs";
import { join } from "node:path";

/** What any test may take before its own work: `vite.config.ts`'s `testTimeout`. */
const BASE_MS = 15_000;
const MS_PER_FILE = 100;

/** The timeout for a test that scans `app/src`, counted over every `.ts` and `.svelte` file there, tests included. */
export function wholeTreeBudget(): number {
  const files = readdirSync(join(process.cwd(), "src"), { recursive: true, encoding: "utf8" }).filter((name) =>
    /\.(svelte|ts)$/.test(name),
  ).length;
  return BASE_MS + files * MS_PER_FILE;
}
