/**
 * Build chains (#857, R-9.13): a pipeline's place in the chain its source's
 * dependencies make, and each step's run on a run's branch.
 */
import type { ChainStep, ChainStepRow } from "../../ipc/entity";
import { pipelineHash, runHash, runTone } from "./builds";
import { resultWord } from "./pipeline";

/**
 * The pipeline header's chain sentence: *Step 2 of the chain Build › Docker
 * › Deploy.* Empty for a pipeline in no chain, or one the chain does not name.
 */
export function chainSentence(chain: readonly ChainStep[], pipelineId: string): string {
  const at = chain.findIndex((step) => step.pipeline === pipelineId);
  if (chain.length < 2 || at < 0) return "";
  return `Step ${at + 1} of the chain ${chain.map((step) => step.name).join(" › ")}.`;
}

/**
 * Where the pipeline stands in its chain, for the header's sign (the build-header pass, 2026-11-20): *Step 64* of
 * *70*. `null` for a pipeline in no chain, or one the chain does not name, as `chainSentence` says nothing.
 */
export function chainPlace(chain: readonly ChainStep[], pipelineId: string): { step: number; of: number } | null {
  const at = chain.findIndex((step) => step.pipeline === pipelineId);
  if (chain.length < 2 || at < 0) return null;
  return { step: at + 1, of: chain.length };
}

/** A chain of more steps than this draws its boxes with its middle folded (`chainWindow`). */
export const CHAIN_FOLD_OVER = 7;
/** How many steps either side of the run's own a folded chain keeps. */
const CHAIN_AROUND = 2;

/**
 * The steps a run's *Chain on <branch>* panel draws, by index, with `null` where steps are folded (the build-header
 * pass, 2026-11-20: a chain of 70 steps drew 70 boxes over ten rows). Up to `CHAIN_FOLD_OVER` steps, all of them;
 * past it, the first, the run's own with two either side, and the last, each gap of more than one step one fold.
 * With no step of the run's own, the first steps and the last.
 */
export function chainWindow(count: number, here: number): (number | null)[] {
  const all = Array.from({ length: count }, (_, i) => i);
  if (count <= CHAIN_FOLD_OVER) return all;
  const centre = here < 0 ? CHAIN_AROUND : here;
  const kept = new Set([0, count - 1]);
  for (let i = centre - CHAIN_AROUND; i <= centre + CHAIN_AROUND; i += 1) if (i >= 0 && i < count) kept.add(i);
  const out: (number | null)[] = [];
  for (let i = 0; i < count; i += 1) {
    if (kept.has(i)) out.push(i);
    else if (kept.has(i - 1)) {
      // A gap of one step draws the step: a fold would take as much room.
      out.push(kept.has(i + 1) ? i : null);
    }
  }
  return out;
}

/**
 * A repository as the Builds header names it (the build-header pass, 2026-11-20): what the source calls the checkout,
 * or, where that is an address, as TeamCity names a VCS root by default (`https://git.example.com/org/repo.git#refs/
 * heads/main`), the repository's own name, `repo`, with no scheme, host, owner, `.git` or branch. The whole name and
 * address are its title.
 */
export function repositoryShortName(name: string): string {
  const text = name.trim();
  const address = /^[a-z][a-z0-9+.-]*:\/\//i.test(text) || /^[^\s/@]+@[^\s/:]+:/.test(text);
  if (!address) return text;
  const path = text
    .replace(/#.*$/, "")
    .replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*\/?/i, "")
    .replace(/^[^\s/@]+@[^\s/:]+:/, "")
    .replace(/\/+$/, "")
    .replace(/\.git$/i, "");
  const parts = path.split("/").filter((part) => part !== "");
  return parts.at(-1) ?? text;
}

/** One box of the *Chain on <branch>* panel. */
export interface ChainBox {
  name: string;
  /** The step's run, else its pipeline. */
  href: string;
  /** The run's number and result, else *not run on this branch*. */
  line: string;
  /** The box's edge: the run's tone, `none` for a step that has not run. */
  tone: "ok" | "err" | "warn" | "muted" | "run" | "q" | "none";
  /** The run the page is showing. */
  here: boolean;
}

/** A step's box, for the run `runId` shows. */
export function chainBox(step: ChainStepRow, runId: string): ChainBox {
  const run = step.run;
  if (run === null) {
    return {
      name: step.name,
      href: pipelineHash(step.pipeline_id),
      line: "not run on this branch",
      tone: "none",
      here: false,
    };
  }
  const tone = runTone(run);
  const word = tone === "ok" ? "passed" : resultWord(run);
  return {
    name: step.name,
    href: runHash(step.pipeline_id, run.entity_id),
    line: run.number !== null ? `#${run.number} ${word}` : word,
    tone,
    here: run.entity_id === runId,
  };
}
