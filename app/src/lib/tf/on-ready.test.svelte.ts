/**
 * The shell's reads once the database answers (review of spec #2234,
 * Standards 8): each waits for ready, reads its port once, hands the answer
 * on, and swallows a refusal (the default holds).
 */
import { flushSync } from "svelte";
import { expect, test } from "vitest";

import { onReady } from "./on-ready.svelte";

test("a read waits for ready, runs once, and hands its answer on", async () => {
  let ready = $state(false);
  let asked = 0;
  const held: number[] = [];
  const stop = $effect.root(() =>
    onReady(
      () => ready,
      () => () => (asked++, Promise.resolve(7)),
      (n) => held.push(n),
    ),
  );
  flushSync();
  expect(asked).toBe(0);

  ready = true;
  flushSync();
  await Promise.resolve();
  expect(asked).toBe(1);
  expect(held).toEqual([7]);
  stop();
});

test("no port reads nothing, and a refused read holds nothing and throws nowhere", async () => {
  const held: string[] = [];
  const stop = $effect.root(() => {
    onReady(() => true, () => undefined, (s: string) => held.push(s));
    onReady(() => true, () => () => Promise.reject(new Error("down")), (s: string) => held.push(s));
  });
  flushSync();
  await Promise.resolve();
  await Promise.resolve();
  expect(held).toEqual([]);
  stop();
});
