/**
 * The `?fake-ipc` backend's way back to its seed (#2004).
 *
 * The fake keeps what it is told in module state under `fake/`, as the server
 * it stands in for would, so a test that writes through it hands its writes to
 * the next test in the file. Each module registers how its state goes back here
 * as it loads, and `test-resets.ts` calls {@link resetFake} after every test;
 * a file that never loads the fake loads only this. `fake-seed.test.ts` fails
 * on a module-level value under `fake/` that a handler can change and no
 * registration names.
 *
 * Outside `fake/` because every module there is a feature with handlers.
 */

const restores: (() => void)[] = [];

/**
 * Put each value back in place, after every test, to a copy of what it holds
 * now: call it once the module has seeded them. In place, because other
 * modules hold the same array or map. A `let` goes through {@link onFakeReset}.
 */
export function keepSeed(...values: object[]): void {
  for (const value of values) {
    const seed = seedOf(value);
    restores.push(() => refill(value, seed()));
  }
}

/** A fresh copy of `value` as it is now, each time it is called: a `let`'s seed. */
export function seedOf<T>(value: T): () => T {
  const seed = copy(value);
  return () => copy(seed);
}

/** Run `reset` with the others: for a `let`, which only its own module can reassign. */
export function onFakeReset(reset: () => void): void {
  restores.push(reset);
}

/** Put every fake module loaded so far back to its seed. */
export function resetFake(): void {
  for (const restore of restores) restore();
}

function refill(value: object, seed: object): void {
  if (Array.isArray(value)) value.splice(0, value.length, ...(seed as unknown[]));
  else if (value instanceof Map) {
    value.clear();
    for (const [key, entry] of seed as Map<unknown, unknown>) value.set(key, entry);
  } else if (value instanceof Set) {
    value.clear();
    for (const entry of seed as Set<unknown>) value.add(entry);
  } else {
    for (const key of Object.keys(value)) delete (value as Record<string, unknown>)[key];
    Object.assign(value, seed);
  }
}

/** A deep copy of arrays, maps, sets and plain objects; a function (a fixture's matcher) is shared. */
function copy<T>(value: T): T {
  if (Array.isArray(value)) return value.map(copy) as T;
  if (value instanceof Map) return new Map([...value].map(([key, entry]) => [key, copy(entry)])) as T;
  if (value instanceof Set) return new Set([...value].map(copy)) as T;
  if (value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
    return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, copy(entry)])) as T;
  }
  return value;
}
