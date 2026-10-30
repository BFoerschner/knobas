/**
 * *Run pipelines on diffs* (#2270, spec #2234 story 69): whether a diff runs
 * each file's default pipeline on both sides before diffing. On by default;
 * Settings › Renderers turns it off, and each diff's own toggle overrides it
 * for that diff (`TfDiffView`).
 *
 * A Settings entry, so a `knobas.setting` row (`render.diff_pipelines`) like
 * the rest of Settings › Renderers and *Pages kept open*, and a backup
 * carries it (review of spec #2234, Standards 6). The split/unified toggle in
 * a diff's own toolbar is not a Settings entry and stays the viewer's
 * `localStorage` (`diff-layout.svelte.ts`). The shell holds the stored value
 * once the database answers (`TfShell`); until then, or when the read fails,
 * it is on.
 */
import { diffPipelines as readDiffPipelines, setDiffPipelines } from "../../ipc/app";

export interface DiffPipelinesPort {
  read(): Promise<boolean>;
  save(on: boolean): Promise<boolean>;
}

export const ipcDiffPipelines: DiffPipelinesPort = { read: () => readDiffPipelines(), save: (on) => setDiffPipelines(on) };

export interface DiffPipelinesSetting {
  readonly on: boolean;
  /** Holds `on` as read elsewhere (the shell's read once the database answers). */
  hold(on: boolean): void;
  /** Stores `on` and holds what the backend answers; a refusal rejects and changes nothing. */
  set(on: boolean): Promise<void>;
  /** Tests only: back to on. */
  reset(): void;
}

export function createDiffPipelines(port: DiffPipelinesPort = ipcDiffPipelines): DiffPipelinesSetting {
  let on = $state(true);
  return {
    get on() {
      return on;
    },
    hold(next) {
      on = next;
    },
    async set(next) {
      on = await port.save(next);
    },
    reset() {
      on = true;
    },
  };
}

/** The app's one setting. */
export const diffPipelines: DiffPipelinesSetting = createDiffPipelines();
