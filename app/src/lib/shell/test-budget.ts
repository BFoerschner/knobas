/**
 * **How long a test may take on this machine** (#2035, #2044).
 *
 * `vite.config.ts` takes `testTimeout` from here, and the whole-tree guards
 * take `wholeTreeBudget()`.
 *
 * `just check` runs eight threads at once: vitest's four workers
 * (`maxWorkers`) beside cargo's four rustc jobs (`CARGO_BUILD_JOBS`). The
 * 12-core Mac gives each a core, and 15 s there is three times Vitest's
 * default, room for a neighbour's gate. The two-core `check.yml` runner gives
 * each a quarter of one, and there the same tests ran eight to ten times
 * their Mac time, with a 1.6 times spread between runs on one day: the
 * launcher-chain, TfCode and `browser-globals` tests each passed 15 s in one
 * of runs 40120926029, 40124502573 and 40127451817. So the budget grows by
 * eight over the core count, four times on the runner, and stays 15 s from
 * eight cores up. A test that hangs still fails, in a minute on the runner.
 */
import { readdirSync } from "node:fs";
import { availableParallelism } from "node:os";
import { join } from "node:path";

/** The threads `just check` runs at once: four vitest workers, four rustc jobs. */
const THREADS = 8;

/** Every test's timeout: 15 s with a core per thread, more on a machine with fewer cores. */
export const TEST_TIMEOUT_MS = 15_000 * Math.max(1, THREADS / availableParallelism());

/**
 * 100 ms a file on top: the costliest whole-tree guard, `nullish-fallback`,
 * took 40 s for about 500 files (81 ms a file) on the runner in 40127451817,
 * the slowest of the three runs.
 */
const MS_PER_FILE = 100;

/**
 * The timeout for a test that scans `app/src`, counted over every `.ts` and
 * `.svelte` file there, tests included. Such a guard costs in proportion to
 * the tree, so a budget per file keeps its pass from depending on how far the
 * tree has grown.
 */
export function wholeTreeBudget(): number {
  const files = readdirSync(join(process.cwd(), "src"), { recursive: true, encoding: "utf8" }).filter((name) =>
    /\.(svelte|ts)$/.test(name),
  ).length;
  return TEST_TIMEOUT_MS + files * MS_PER_FILE;
}
