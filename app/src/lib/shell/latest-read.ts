/**
 * A read that only the newest one of its kind is allowed to write.
 *
 * ## The failure
 *
 * A view that re-reads itself on an event has no control over how many reads
 * are in flight at once. *Sync all* emits one terminal `sync:state` per
 * source, so five sources put five `list_sources` in flight with nothing
 * sequencing them; two *Retry* presses over a failing `backup_status` do the
 * same thing more slowly. Whichever answer lands last wins, so a slow *early*
 * read can write the snapshot from before the run it had just watched finish
 * — which is #83 again, arriving through the fix for it (#99, #107).
 *
 * ## Why it is a module rather than a rule people follow
 *
 * The guard is four lines and it is copied wrong in one predictable way: the
 * **rejection** path gets dropped. A stale failure that is allowed through
 * blanks a list that has since read fine, and it does it while showing an
 * error message that was already out of date — a worse outcome than the
 * stale-answer case the guard is usually explained by. Here the caller cannot
 * drop it: `fail` is reached only through the same check `ok` is.
 *
 * ## Use
 *
 * One `latestRead` per thing being read, made where the state it writes lives:
 *
 * ```ts
 * const read = latestRead();
 * async function load() {
 *   await read(listSources, {
 *     ok: (rows) => { sources = rows; error = null; },
 *     fail: (cause) => { error = ipcErrorMessage(cause); },
 *   });
 * }
 * ```
 *
 * It sequences *writes*, not requests: every read is still issued, and an
 * overtaken one simply drops its answer. That is deliberate — the answer a
 * component wants is the newest one, and cancelling the earlier request would
 * mean the IPC layer knowing about a display concern.
 */

/** What an answer, or a failure, is allowed to do once it is still current. */
export type Landing<T> = {
  ok: (value: T) => void;
  fail: (cause: unknown) => void;
};

/**
 * Answer with a `read` that stamps each call and writes only if still current.
 *
 * The returned function never rejects: a failure that is still current is
 * handed to `fail`, and one that has been overtaken is dropped. So a caller
 * may `void` it without leaving a rejection nobody is holding.
 */
export function latestRead(): <T>(
  fetch: () => Promise<T>,
  landing: Landing<T>,
) => Promise<void> {
  let issued = 0;

  return async <T>(fetch: () => Promise<T>, landing: Landing<T>) => {
    const mine = (issued += 1);
    try {
      const value = await fetch();
      if (mine !== issued) return;
      landing.ok(value);
    } catch (cause) {
      if (mine !== issued) return;
      landing.fail(cause);
    }
  };
}
