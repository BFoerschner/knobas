/**
 * Themes in backups (Björn, 2026-11-22: "they are included in the backups"). A backup is the database's dump, and the
 * themes live in this computer's `localStorage`, so the store's snapshot is mirrored into one `knobas.setting` row
 * (`themes.kept`, `commands/window.rs`) a second after it changes; a restore reads that row back and puts the themes
 * as they were kept. A share export carries no setting, so none carries themes.
 */
import { setThemesKept, themesKept } from "../ipc/window";
import { defaultTimers, type Themes } from "./themes.svelte";

export const KEEP_AFTER = 1000;
/** How often a write the database was not up for is tried again, and how many times. */
export const RETRY_AFTER = 5000;
export const RETRIES = 24;

export function mirrorThemes(themes: Themes, keep: typeof setThemesKept = setThemesKept, timers: { set: typeof setTimeout; clear: typeof clearTimeout } = defaultTimers): () => void {
  if (themes.startedDefault) return () => {};
  let kept = "";
  let waiting: ReturnType<typeof setTimeout> | null = null;
  const write = (text: string, tries: number) => {
    waiting = null;
    keep(text)
      .then(() => (kept = text))
      .catch(() => {
        // The database may not be up yet: try again a little later.
        if (tries > 0) waiting = timers.set(() => write(text, tries - 1), RETRY_AFTER);
      });
  };
  return $effect.root(() => {
    $effect(() => {
      const text = JSON.stringify(themes.snapshot());
      if (text === kept) return;
      if (waiting !== null) timers.clear(waiting);
      waiting = timers.set(() => write(text, RETRIES), KEEP_AFTER);
    });
    return () => {
      if (waiting !== null) timers.clear(waiting);
    };
  });
}

/** After a restore: the themes the restored database keeps, put back. Answers whether there were any. */
export async function restoreThemes(themes: Themes, read: typeof themesKept = themesKept): Promise<boolean> {
  const text = await read().catch(() => null);
  if (text === null) return false;
  try {
    return themes.restore(JSON.parse(text));
  } catch {
    return false;
  }
}
