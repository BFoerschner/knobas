/**
 * One value kept in the webview's `localStorage` under `key`, for the
 * per-machine conveniences the shell reads when it mounts (the chosen job,
 * the ticket page card's width, the diff layout). Storage missing, blocked or
 * throwing costs the memory, never the caller: a read that fails reads as
 * nothing kept, a write that fails keeps the value for this run only.
 */
export type KeptStorage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

export interface KeptValue {
  /** What is kept, or `null` when nothing is or the read fails. */
  read(): string | null;
  write(value: string): void;
  clear(): void;
}

export function keptValue(storage: () => KeptStorage | null, key: string): KeptValue {
  return {
    read() {
      try {
        return storage()?.getItem(key) ?? null;
      } catch {
        return null;
      }
    },
    write(value) {
      try {
        storage()?.setItem(key, value);
      } catch {
        // Kept for this run only.
      }
    },
    clear() {
      try {
        storage()?.removeItem(key);
      } catch {
        // Nothing kept to clear.
      }
    },
  };
}

/** This viewer's `localStorage` where there is one. */
export const windowStorage = (): KeptStorage | null => (typeof window === "undefined" ? null : window.localStorage);
