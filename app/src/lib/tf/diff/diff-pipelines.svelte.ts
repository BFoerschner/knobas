/**
 * *Run pipelines on diffs* (#2270, spec #2234 story 69): whether a diff runs
 * each file's default pipeline on both sides before diffing. On by default;
 * Settings › Renderers turns it off, and each diff's own toggle overrides it
 * for that diff (`TfDiffView`).
 *
 * Kept as the layout choice is (`diff-layout.svelte.ts`): this viewer's
 * `localStorage` (`knobas.diff.pipelines`), across a restart, not in a backup.
 * Storage missing, blocked or holding rubbish costs the memory, never the
 * setting: it is on then.
 */
export const PIPELINES_KEY = "knobas.diff.pipelines";

export interface DiffPipelinesSetting {
  readonly on: boolean;
  set(on: boolean): void;
  /** Tests only: back to on, nothing stored. */
  reset(): void;
}

type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

export function createDiffPipelines(storage: () => Storage | null): DiffPipelinesSetting {
  const read = (): boolean => {
    try {
      return storage()?.getItem(PIPELINES_KEY) !== "off";
    } catch {
      return true;
    }
  };
  let on = $state(read());
  return {
    get on() {
      return on;
    },
    set(next) {
      on = next;
      try {
        storage()?.setItem(PIPELINES_KEY, next ? "on" : "off");
      } catch {
        // Kept for this run only.
      }
    },
    reset() {
      on = true;
      try {
        storage()?.removeItem(PIPELINES_KEY);
      } catch {
        // Nothing stored to clear.
      }
    },
  };
}

/** The app's one setting, over this viewer's `localStorage` where there is one. */
export const diffPipelines: DiffPipelinesSetting = createDiffPipelines(() =>
  typeof window === "undefined" ? null : window.localStorage,
);
