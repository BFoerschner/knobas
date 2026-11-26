/**
 * The app's one way to call a command: every module that asks the backend
 * imports `invoke` from here, never from `@tauri-apps/api/core`.
 *
 * It goes through the answer cache (`answer-cache.ts`), which answers a read
 * of the mirror from memory when it holds one and passes everything else on
 * to Tauri. The cache used to sit in front of the bridge by replacing
 * `window.__TAURI_INTERNALS__.invoke`, but Tauri defines that property
 * read-only (`Object.defineProperty` with `value` only), so in a packaged app
 * the assignment threw and the window never left *Starting the local
 * database* (v3.2.9-ux7, 2026-11-12). Nothing here writes to the bridge.
 *
 * A probe that times or counts commands (`testenv/perf/`, the press trips)
 * sets `window.__knobasIpcTap`, which sees every command this module sends,
 * answered from memory or not, and must call `next` to send it on.
 */
import type { InvokeArgs, InvokeOptions } from "@tauri-apps/api/core";

import { cachedInvoke } from "./answer-cache";

/** What a probe may set to see each command; it must return `next(cmd, args, options)`'s answer, or one like it. */
export type IpcTap = (
  cmd: string,
  args: InvokeArgs | undefined,
  options: InvokeOptions | undefined,
  next: (cmd: string, args?: InvokeArgs, options?: InvokeOptions) => Promise<unknown>,
) => Promise<unknown>;

declare global {
  interface Window {
    __knobasIpcTap?: IpcTap;
  }
}

/** Send a command to the backend, through the answer cache (and a probe's tap, when one is set). */
export function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  const tap = typeof window === "undefined" ? undefined : window.__knobasIpcTap;
  return (tap ? tap(cmd, args, options, cachedInvoke) : cachedInvoke(cmd, args, options)) as Promise<T>;
}
