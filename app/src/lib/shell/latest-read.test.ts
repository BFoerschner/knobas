/**
 * The stale-read guard, tested directly rather than only through its callers.
 *
 * `latestRead` is a shared primitive with two call sites and more coming as
 * the settings shell grows sections, so its properties are pinned here rather
 * than being re-derived from whichever component happened to exercise them.
 * The gap this closes is not hypothetical: the module shipped in #107 with
 * `landing.ok` inside the `try`, so an exception from a caller's *ok-handler*
 * was caught by the `catch` that exists for the fetch and reported as the
 * fetch having failed. Nothing found it for two PRs, because every test that
 * touched the module drove it through a component whose ok-handler worked
 * (#129).
 *
 * The vocabulary is `latest-read.ts`'s own: a read is **current** while no
 * later read of the same kind has been issued, and only a current read is
 * allowed to write.
 */
import { expect, test } from "vitest";

import { latestRead } from "./latest-read";

/** A promise plus the handles to settle it, so a test controls the ordering. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/**
 * A bug in the ok-handler is a bug in the ok-handler, not a failed fetch.
 *
 * In `SourcesView` the ok-handler calls `health.replace(...)`, so a rendering
 * fault there used to be shown to the person as `list_sources` having failed:
 * the fetch blamed for the renderer's mistake, and — worse — a plausible
 * enough error about the network that nobody goes looking at the handler.
 * Measured before the fix, the log below read
 * `["ok", "fail: bug in the ok handler"]`.
 */
test("an exception from the ok-handler is not laundered into the fail path", async () => {
  const read = latestRead<string>();
  const log: string[] = [];
  const bug = new Error("bug in the ok handler");

  await expect(
    read(() => Promise.resolve("fetched fine"), {
      ok: () => {
        log.push("ok");
        throw bug;
      },
      fail: (cause) => {
        log.push(`fail: ${String(cause)}`);
      },
    }),
  ).rejects.toBe(bug);

  expect(log, "the ok-handler's own exception was reported as a failed read").toEqual(["ok"]);
});

/**
 * …and the same for `fail`, which is the half that was already right.
 *
 * `landing.fail` was never inside the `try`, so an exception from it always
 * propagated. Pinned here so the two handlers cannot drift apart again: the
 * `try` covers the fetch and nothing else, in both directions.
 */
test("an exception from the fail-handler propagates rather than being swallowed", async () => {
  const read = latestRead<string>();
  const bug = new Error("bug in the fail handler");

  await expect(
    read(() => Promise.reject(new Error("the fetch really did fail")), {
      ok: () => {},
      fail: () => {
        throw bug;
      },
    }),
  ).rejects.toBe(bug);
});

test("a current answer is handed to ok, and a current failure to fail", async () => {
  const read = latestRead<string>();
  const seen: string[] = [];

  await read(() => Promise.resolve("rows"), {
    ok: (value) => seen.push(`ok:${value}`),
    fail: (cause) => seen.push(`fail:${String(cause)}`),
  });
  await read(() => Promise.reject(new Error("nope")), {
    ok: (value) => seen.push(`ok:${value}`),
    fail: (cause) => seen.push(`fail:${(cause as Error).message}`),
  });

  expect(seen).toEqual(["ok:rows", "fail:nope"]);
});

test("an overtaken answer is dropped, and the newest one writes", async () => {
  const read = latestRead<string>();
  const slow = deferred<string>();
  const fast = deferred<string>();
  const written: string[] = [];
  const landing = {
    ok: (value: string) => written.push(value),
    fail: (cause: unknown) => written.push(`fail:${String(cause)}`),
  };

  const first = read(() => slow.promise, landing);
  const second = read(() => fast.promise, landing);

  fast.resolve("newest");
  await second;
  slow.resolve("the snapshot from before the run");
  await first;

  expect(written, "an overtaken read wrote its answer anyway").toEqual(["newest"]);
});

/**
 * The half that is dropped when this guard is hand-copied, and the reason the
 * module exists (#107).
 *
 * A stale *rejection* let through blanks a list that has since been read
 * fine, and does it while showing an error message that was already out of
 * date — worse than the stale-answer case the guard is usually explained by.
 * `fail` is reachable only through the same currency check as `ok`, so a
 * caller cannot keep half of it.
 */
test("an overtaken failure is dropped: a stale rejection cannot blank state that read fine", async () => {
  const read = latestRead<string>();
  const slow = deferred<string>();
  const fast = deferred<string>();
  const written: string[] = [];
  const landing = {
    ok: (value: string) => written.push(value),
    fail: (cause: unknown) => written.push(`fail:${(cause as Error).message}`),
  };

  const first = read(() => slow.promise, landing);
  const second = read(() => fast.promise, landing);

  fast.resolve("rows that read fine");
  await second;
  slow.reject(new Error("the network was down a moment ago"));
  await first;

  expect(written, "a stale rejection reached `fail` and blanked a current answer").toEqual([
    "rows that read fine",
  ]);
});

/**
 * The returned call does not reject for the *fetch's* own failure.
 *
 * Both callers `void` it from an `$effect`, so a rejection there would be one
 * nobody is holding — which the repo-wide guard in `test-setup.ts` turns into
 * a failure of whichever test ran next. A handler's exception is a different
 * thing and does propagate; that is the pair of tests at the top of this file.
 */
test("a fetch that rejects does not reject the returned call, current or not", async () => {
  const read = latestRead<string>();
  const noop = { ok: () => {}, fail: () => {} };

  await expect(read(() => Promise.reject(new Error("current")), noop)).resolves.toBeUndefined();

  const slow = deferred<string>();
  const overtaken = read(() => slow.promise, noop);
  await read(() => Promise.resolve("newer"), noop);
  slow.reject(new Error("overtaken"));
  await expect(overtaken).resolves.toBeUndefined();
});

/**
 * Two reads of *different* things must not cancel each other.
 *
 * The generic used to sit on the returned call rather than on `latestRead`,
 * so one `read` typechecked against two unrelated fetches — and each would
 * have made the other stale, silently, because "which read is newest" is a
 * single counter. With the parameter on the factory, the second call site is
 * a compile error rather than a race; this test pins the run-time half of the
 * same rule, that a `latestRead` counts only its own reads.
 */
test("two latestReads are independent counters", async () => {
  const sources = latestRead<string>();
  const backups = latestRead<string>();
  const written: string[] = [];
  const landing = {
    ok: (value: string) => written.push(value),
    fail: () => {},
  };

  const slow = deferred<string>();
  const first = sources(() => slow.promise, landing);
  await backups(() => Promise.resolve("backup status"), landing);
  slow.resolve("sources");
  await first;

  expect(written, "a read of one thing made a read of another thing stale").toEqual([
    "backup status",
    "sources",
  ]);
});
