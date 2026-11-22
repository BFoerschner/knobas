/**
 * How a changed class in a JAR's comparison is diffed (2026-11-22, Fable's ruling): its decompiled *Source* or its
 * *Outline*. One choice app-wide, kept in this browser's storage (`knobas.diff-class`), *Source* where none is kept.
 */
export type ClassMode = "outline" | "source";

export interface ClassModeChoice {
  readonly current: ClassMode;
  set(mode: ClassMode): void;
  /** Back to *Source*, nothing kept (tests). */
  reset(): void;
}

const KEY = "knobas.diff-class";

function kept(): ClassMode {
  try {
    return globalThis.localStorage?.getItem(KEY) === "outline" ? "outline" : "source";
  } catch {
    return "source";
  }
}

export function createClassMode(start: ClassMode = kept()): ClassModeChoice {
  let current = $state<ClassMode>(start);
  return {
    get current() {
      return current;
    },
    set(mode) {
      current = mode;
      try {
        globalThis.localStorage?.setItem(KEY, mode);
      } catch {
        // Storage refused (a private window): the choice lasts the visit.
      }
    },
    reset() {
      current = "source";
      try {
        globalThis.localStorage?.removeItem(KEY);
      } catch {
        // Nothing kept to forget.
      }
    },
  };
}

/** The app's one choice. */
export const classMode: ClassModeChoice = createClassMode();
