/**
 * Rejections nobody caught, recorded so the test that caused one fails.
 *
 * ## The failure this exists for
 *
 * A `bind:` on a field of a **nullable** object is read again *after* the
 * object is set to null: Svelte reads a binding's getter on a later tick, and
 * by then closing the dialog has taken the value away. The read throws into a
 * promise nothing awaits. Every assertion in the file still passed and the run
 * still failed (#69, fixed in #102).
 *
 * The class is generic — an effect, a timer, a binding re-read after teardown
 * — and the signal it produces is the worst shape a signal can have. Vitest
 * prints the leak under *Unhandled Errors* after the summary, where the file
 * that caused it is still counted among the passing ones, so `47 passed` sits
 * directly above the evidence that one of the 47 is broken. Whoever notices
 * the non-zero exit starts by suspecting the wrong file (#103).
 *
 * ## Why this is a module and not an `afterEach` in each test file
 *
 * `test-setup.ts` is already wired into `vite.config.ts` as `test.setupFiles`,
 * so a guard installed there covers every test file *without opting in* — and
 * a file that leaks cannot quietly be the one file that forgot. The state
 * lives here rather than in the setup file so that a test which leaks on
 * purpose can import it and hand the leak over; a setup file is imported for
 * its side effects, and reaching into one for its exports is a second way of
 * doing one thing.
 *
 * ## Node's semantics, and why `rejectionHandled` is honoured
 *
 * Node emits `unhandledRejection` at the end of the turn in which a promise
 * rejected with no handler, and `rejectionHandled` if a handler is attached
 * afterwards. Recording only the first would make the guard fire on ordinary
 * asynchronous code — a `void`-ed call that is awaited a tick later, a retry
 * that attaches its `catch` after an intervening microtask — and a guard that
 * fires on working code gets deleted rather than obeyed. So the promise is
 * kept alongside the reason and a later `rejectionHandled` withdraws it.
 *
 * This module is test-only. It is under `src/` because that is where the rest
 * of the harness lives (`test-setup.ts` next to it), and nothing in the bundle
 * imports it.
 */

/** One recorded leak: the reason to report, and the promise to withdraw by. */
type Leak = { promise: Promise<unknown>; reason: unknown };

const leaks: Leak[] = [];

function onRejection(reason: unknown, promise: Promise<unknown>) {
  leaks.push({ promise, reason });
}

function onHandled(promise: Promise<unknown>) {
  const at = leaks.findIndex((leak) => leak.promise === promise);
  if (at !== -1) leaks.splice(at, 1);
}

/**
 * Start recording, and answer with the stop.
 *
 * Called at the top level of `test-setup.ts` rather than from a `beforeAll`,
 * because a test file's own module graph is imported *after* the setup file
 * runs: a `await import()` at the top of a test file that rejects is exactly
 * the kind of leak this catches, and a `beforeAll` would not be installed yet.
 *
 * The stop matters as much as the start, and for one measured reason: while
 * *any* `unhandledRejection` listener is installed, Vitest's own unhandled-
 * error reporting stays completely silent — a bare no-op listener in this
 * setup file turns a leaking file from `1 passed / 1 error` into `1 passed`
 * and exit 0. So the tail of a file, after the last hook this module can run,
 * has to be handed back to the reporter that would otherwise have covered it.
 *
 * Listener accumulation is *not* the reason, though it is the one that first
 * suggests itself. Vitest isolates by default, and this config takes that
 * default: measured over the whole suite, 48 test files ran in 48 distinct
 * pids, so a listener that outlives its file dies with its own fork. The one
 * path that does strand a listener — a file that throws during collection,
 * whose root `afterAll` never runs — is therefore harmless here, and that
 * file is already failing. Set `isolate: false` and the removal stops being
 * belt-and-braces and starts being load-bearing.
 */
export function watchRejections(): () => void {
  process.on("unhandledRejection", onRejection);
  process.on("rejectionHandled", onHandled);
  return () => {
    process.off("unhandledRejection", onRejection);
    process.off("rejectionHandled", onHandled);
  };
}

/**
 * Let a rejection that is going to happen actually happen.
 *
 * `unhandledRejection` is emitted on a later macrotask, so a microtask drain
 * is not enough: asserting straight after the gesture that caused the leak
 * would assert before Node has decided there was one.
 */
export function settleRejections(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

/** What has been recorded since the last take, emptying the record. */
export function takeUnhandled(): unknown[] {
  return leaks.splice(0, leaks.length).map((leak) => leak.reason);
}
