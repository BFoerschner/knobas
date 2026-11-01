/**
 * **How long a test may take on this machine** (#2035, #2044).
 *
 * `vite.config.ts` takes `testTimeout` and `hookTimeout` from here, and the
 * whole-tree guards and `test-setup.ts`'s store reset take `wholeTreeBudget()`.
 *
 * `just check` runs eight threads at once: vitest's four workers
 * (`maxWorkers`) beside cargo's four rustc jobs (`CARGO_BUILD_JOBS`). The
 * 12-core Mac gives each a core, and 15 s there is three times Vitest's
 * default, room for a neighbour's gate. On the `check.yml` runner the same
 * tests ran eight to ten times their Mac time, with a 1.6 times spread between
 * runs on one day: the launcher-chain, TfCode and `browser-globals` tests each
 * passed 15 s in one of runs 40120926029, 40124502573 and 40127451817, and a
 * hook passed its 10 s in 40130991453. So the budgets grow by `budgetFactor`.
 * A test that hangs still fails, in a minute on the runner.
 */
import { readdirSync } from "node:fs";
import { availableParallelism } from "node:os";
import { join } from "node:path";

/** The threads `just check` runs at once: four vitest workers, four rustc jobs. */
const THREADS = 8;

/**
 * The runner's floor. A public repository's `ubuntu-latest` has four cores
 * (GitHub's documented standard runner; a private one has two), which the
 * thread count alone scales by two. That misses the other half of the eight
 * to ten times: each runner core is slower than a Mac core, and counting
 * cores cannot see that. Four keeps the slowest runner test, launcher-chain's
 * 11.3 s in 40124502573, at a fifth of its 60 s budget, and under half of it
 * at the 1.6 times spread (18 s).
 */
const RUNNER_FACTOR = 4;

/**
 * What a budget is multiplied by: threads over cores, never below one, and
 * never below `RUNNER_FACTOR` on GitHub Actions (`GITHUB_ACTIONS=true`, set on
 * every hosted runner). 1 on the Mac, 4 on the `check.yml` runner.
 */
export function budgetFactor(cores: number, env: Record<string, string | undefined>): number {
  const byThreads = Math.max(1, THREADS / cores);
  return env.GITHUB_ACTIONS === "true" ? Math.max(RUNNER_FACTOR, byThreads) : byThreads;
}

const FACTOR = budgetFactor(availableParallelism(), process.env);

/** Every test's timeout: 15 s on the Mac, 60 s on the runner. */
export const TEST_TIMEOUT_MS = 15_000 * FACTOR;

/**
 * Every hook's: Vitest's 10 s, scaled the same way. `test-setup.ts`'s
 * `afterEach(resetStores)` passed 10 s on the runner after a file's first
 * test (Launcher, 40130991453).
 */
export const HOOK_TIMEOUT_MS = 10_000 * FACTOR;

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
