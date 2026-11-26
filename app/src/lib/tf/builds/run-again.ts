/**
 * *Run again…* from Builds' *Recent* (2026-11-25): the Run… dialog belongs to a run's page, which reads the pipeline's
 * settings, presets and refusals it needs. A press in *Recent* opens the run's page with its dialog open: the page takes
 * the ask once, as it mounts on that run.
 */
let asked: string | null = null;

/** Open the dialog on `runId`'s page when it next mounts. */
export function askRunAgain(runId: string): void {
  asked = runId;
}

/** Whether the dialog was asked for on `runId`; the ask is spent either way. */
export function takeRunAgain(runId: string): boolean {
  const was = asked === runId;
  asked = null;
  return was;
}
