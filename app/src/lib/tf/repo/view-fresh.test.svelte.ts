/**
 * Pages show what knobas last read (ADR-0042): the line over a view drawn from
 * what knobas held, and the ear that hears its read behind land or fail.
 */
import { flushSync, tick } from "svelte";
import { expect, test } from "vitest";

import { EVENTS } from "../../ipc";
import type { ViewFresh } from "../../ipc/entity";
import type { EchoListen } from "../queued.svelte";
import { HELD_AFTER_MS, heldLine, onViewFresh } from "./view-fresh.svelte";

const NOW = new Date("2026-11-09T12:00:00Z");
const ago = (ms: number) => new Date(NOW.getTime() - ms).toISOString();

test("a read made now, or a moment ago, says nothing", () => {
  expect(heldLine(undefined, "Gitea", null, NOW)).toBeNull();
  expect(heldLine(null, "Gitea", null, NOW)).toBeNull();
  expect(heldLine(ago(HELD_AFTER_MS - 1_000), "Gitea", null, NOW)).toBeNull();
});

test("an older one says from when it is, and that the source is read behind it", () => {
  expect(heldLine(ago(2 * 3_600_000), "Tidewater Git", null, NOW)).toBe("As knobas read it 2 h ago. Checking Tidewater Git…");
});

test("and why the source did not answer, when it did not", () => {
  expect(heldLine(ago(5 * 60_000), "Tidewater CI", "unreachable: connection refused", NOW)).toBe(
    "As knobas read it 5 min ago. Tidewater CI did not answer: unreachable: connection refused",
  );
});

/** An ear the test speaks into, as the bridge would. */
function fakeEar() {
  const handlers: ((event: { payload: unknown }) => void)[] = [];
  const listen: EchoListen = async (event, handler) => {
    if (event === EVENTS.viewFresh) handlers.push(handler);
    return () => {};
  };
  return { listen, say: (fresh: ViewFresh) => handlers.forEach((handler) => handler({ payload: fresh })) };
}

async function settle() {
  for (let n = 0; n < 4; n++) await tick();
  flushSync();
}

test("a confirmed or moved read behind reads again; a failed one says why and reads nothing", async () => {
  const ear = fakeEar();
  let again = 0;
  const failed: string[] = [];
  const stop = $effect.root(() => {
    onViewFresh(
      () => "gitea:o/r#1",
      null,
      () => again++,
      ear.listen,
      (why) => failed.push(why),
    );
  });
  await settle();

  ear.say({ entity_id: "gitea:o/r#1", section: null });
  expect(again).toBe(1);
  ear.say({ entity_id: "gitea:o/r#1", section: null, failed: "unreachable" });
  expect(again).toBe(1);
  expect(failed).toEqual(["unreachable"]);
  ear.say({ entity_id: "gitea:o/r#1", section: "files" });
  ear.say({ entity_id: "gitea:o/r#2", section: null });
  expect(again).toBe(1);
  stop();
});
