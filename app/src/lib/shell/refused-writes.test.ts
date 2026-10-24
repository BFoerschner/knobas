/** What a source's credential cannot write, as a surface asks and reads it (#2040). */
import { expect, test } from "vitest";

import { askRefused, refusalOf } from "./refused-writes";

const RERUN = { entity: "tc:build:1187", op: "rerun_build" };
const RUN = { entity: "tc:buildType:Payout", op: "run_build" };

test("each ask is sent once, and no ask sends nothing", async () => {
  const sent: unknown[] = [];
  const read = (asks: unknown[]) => {
    sent.push(asks);
    return Promise.resolve([]);
  };
  await askRefused([RERUN, RUN, { ...RERUN }], read);
  await askRefused([], read);
  expect(sent).toEqual([[RERUN, RUN]]);
});

test("a rejection is no verdict: nothing is refused", async () => {
  expect(await askRefused([RUN], () => Promise.reject(new Error("not ready")))).toEqual([]);
});

test("a write's reason is the answer's for that entity and op, and nothing else's", () => {
  const answer = [{ ...RERUN, reason: "cannot re-run" }];
  expect(refusalOf(answer, RERUN.entity, "rerun_build")).toBe("cannot re-run");
  expect(refusalOf(answer, RERUN.entity, "run_build")).toBeNull();
  expect(refusalOf(answer, RUN.entity, "rerun_build")).toBeNull();
  expect(refusalOf(answer, null, "rerun_build")).toBeNull();
});
