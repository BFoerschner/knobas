/**
 * An event storm re-reads at most twice (#753): once now, and once more after
 * the read in flight, however many events arrived meanwhile.
 */
import { expect, test } from "vitest";

import { coalesce } from "./coalesce";

function gate() {
  let open!: () => void;
  const promise = new Promise<void>((resolve) => (open = resolve));
  return { promise, open };
}

test("ten events during a read make one more read, after it", async () => {
  const gates: ReturnType<typeof gate>[] = [];
  const reread = coalesce(() => {
    const next = gate();
    gates.push(next);
    return next.promise;
  });

  void reread();
  for (let n = 0; n < 10; n++) void reread();
  expect(gates, "one read in flight").toHaveLength(1);

  gates[0].open();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(gates, "one trailing read, for everything that arrived").toHaveLength(2);

  gates[1].open();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(gates).toHaveLength(2);

  // Quiet again: the next event reads at once.
  void reread();
  expect(gates).toHaveLength(3);
});

test("a read that failed still lets the trailing one run", async () => {
  let reads = 0;
  let fail!: (cause: unknown) => void;
  const reread = coalesce(() => {
    reads++;
    return reads === 1 ? new Promise<void>((_, reject) => (fail = reject)) : Promise.resolve();
  });
  const first = reread();
  const second = reread();
  fail(new Error("pool timeout"));
  await expect(first).rejects.toThrow("pool timeout");
  await second;
  expect(reads).toBe(2);
});
