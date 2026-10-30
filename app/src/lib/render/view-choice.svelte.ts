/**
 * *Rendered* or *Source* (#2255, spec story 23): the file viewer's toggle on a
 * file whose default renderer is not Source, remembered per file type (its
 * extension, lowercase) in this viewer's `localStorage`, so every `.md` opens
 * the way the reader last left one. The renderer switcher and the defaults per
 * file pattern (#2263) take this over.
 */

export type ViewChoice = "rendered" | "source";

const KEY = "knobas.render.view";

/** A file's type: its extension, lowercase, or its whole name where it has none. */
export function fileType(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot) : name;
}

type Store = Pick<Storage, "getItem" | "setItem">;

export function createViewChoices(storage: () => Store | null) {
  const read = (): Record<string, ViewChoice> => {
    try {
      const parsed: unknown = JSON.parse(storage()?.getItem(KEY) ?? "{}");
      return parsed !== null && typeof parsed === "object" ? (parsed as Record<string, ViewChoice>) : {};
    } catch {
      return {};
    }
  };
  const choices = $state<Record<string, ViewChoice>>(read());
  return {
    /** How the file at `path` is shown: *Rendered* unless the reader picked *Source* for its type. */
    of(path: string): ViewChoice {
      return choices[fileType(path)] === "source" ? "source" : "rendered";
    },
    pick(path: string, choice: ViewChoice) {
      choices[fileType(path)] = choice;
      try {
        storage()?.setItem(KEY, JSON.stringify(choices));
      } catch {
        // Not remembered past this session; the choice still holds in it.
      }
    },
    /** Drops every choice made in this session (tests); what storage holds stays. */
    forget() {
      for (const type of Object.keys(choices)) delete choices[type];
    },
  };
}

/** The app's one store. */
export const viewChoices = createViewChoices(() => {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
});
