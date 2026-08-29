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
 * ## What the `try` covers, and why it is only the fetch
 *
 * The one thing routed to `fail` is the **fetch's** own rejection. An
 * exception from a landing handler is the caller's own bug and propagates:
 * `ok` calls `health.replace(...)` in `SourcesView`, and a fault there used to
 * be caught by the `catch` that exists for the network and shown to the person
 * as `list_sources` having failed — the fetch blamed for the renderer's
 * mistake, behind an error message plausible enough that nobody looks at the
 * handler (#129). A guard that mislabels one failure as another is the class
 * of wrongness this module exists to remove, so it may not commit it itself.
 *
 * ## Use
 *
 * One `latestRead` per thing being read, made where the state it writes lives,
 * and named with what it reads:
 *
 * ```ts
 * const read = latestRead<SourceSummary[]>();
 * async function load() {
 *   await read(listSources, {
 *     ok: (rows) => { sources = rows; error = null; },
 *     fail: (cause) => { error = ipcErrorMessage(cause); },
 *   });
 * }
 * ```
 *
 * The type parameter is on `latestRead` rather than on the call it answers
 * with, and that is the "one per thing read" rule made checkable: a single
 * counter decides which read is newest, so serving two *different* fetches
 * from one `read` makes each of them make the other stale. With the parameter
 * here, that second call site is a type error instead of a race nobody sees.
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
 * The returned function never rejects **for the read's own failure**: one that
 * is still current is handed to `fail`, and one that has been overtaken is
 * dropped. So a caller may `void` it without leaving a rejection nobody is
 * holding, which is how both call sites drive it from an `$effect`.
 *
 * An exception thrown by `ok` or by `fail` is not the read failing and is not
 * absorbed: it rejects the returned call, and in a test the repo-wide guard in
 * `test-setup.ts` turns that into a named failure of the test that caused it.
 */
export function latestRead<T>(): (
  fetch: () => Promise<T>,
  landing: Landing<T>,
) => Promise<void> {
  let issued = 0;

  return async (fetch: () => Promise<T>, landing: Landing<T>) => {
    const mine = (issued += 1);
    // Scoped to the fetch alone. Widening it by one line to hold `landing.ok`
    // is what turned a broken renderer into a report of a broken network
    // (#129), and it reads as harmless right up until it happens.
    let value: T;
    try {
      value = await fetch();
    } catch (cause) {
      // The same currency check `ok` is behind, and not a second one: a stale
      // *failure* let through blanks a list that has since read fine, which is
      // the half a hand-copied guard drops (#107).
      if (mine !== issued) return;
      landing.fail(cause);
      return;
    }
    if (mine !== issued) return;
    landing.ok(value);
  };
}
