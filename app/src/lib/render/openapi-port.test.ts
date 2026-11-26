/**
 * The page's side of the spec port (the ux30 end gate): a spec opened again with the same text takes its index in
 * the press's own task, and the worker is still told, so its session holds the document the later calls read.
 */
import { expect, test } from "vitest";

import { SpecSession, localEngine, type SpecCall, type SpecEngine } from "./openapi-port";
import { bigSpec } from "./openapi-fixture";

test("a spec opened again takes the index it was given before, without waiting for the worker's answer", async () => {
  const text = bigSpec(400);
  const local = localEngine();
  const first = await new SpecSession(local, null).open("api/big.yaml", text);
  expect(first.ok).toBe(true);
  // A worker that is told everything and answers nothing.
  const told: SpecCall["kind"][] = [];
  const silent: SpecEngine = (_session, call) => {
    told.push(call.kind);
    return new Promise(() => {});
  };
  const again = new SpecSession(silent, null);
  // Another copy of the same text, as the file read again is.
  const opened = await Promise.race([again.open("api/big.yaml", ` ${text}`.slice(1)), new Promise((resolve) => setTimeout(() => resolve("waited"), 50))]);
  expect(opened).toBe(first);
  expect(told).toEqual(["open"]);
});

test("a spec that did not parse is asked again, not kept", async () => {
  const broken = "openapi: 3.0.3\ninfo: [\n";
  const asked: string[] = [];
  const counting: SpecEngine = (session, call) => {
    asked.push(call.kind);
    return localEngine()(session, call);
  };
  const a = await new SpecSession(counting, null).open("x.yaml", broken);
  const b = await new SpecSession(counting, null).open("x.yaml", broken);
  expect(a.ok || b.ok).toBe(false);
  expect(asked).toEqual(["open", "open"]);
});
