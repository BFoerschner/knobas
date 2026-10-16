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
