/** A folder render's ask and listen loop (ADR-0044, ADR-0046): open while shown, look when not, again on a run's end. */
import { flushSync } from "svelte";
import { expect, test } from "vitest";

import type { FolderAsk, FolderRenderView } from "../ipc/app";
import { FolderRun, type FolderRenderPorts, type FolderRunAt } from "./folder-run.svelte";

async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  flushSync();
}

function harness(start: boolean) {
  const asked: FolderAsk[] = [];
  let heard: ((event: { payload: unknown }) => void) | null = null;
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      return Promise.resolve({ renderer: at.renderer, folder: at.folder, drawn: null, current: false, running: null, failure: null, from: "clone", commit: null } satisfies FolderRenderView);
    },
    stop: () => Promise.resolve(),
    leave: () => Promise.resolve(),
    output: () => Promise.resolve(new Uint8Array()),
    frame: () => Promise.resolve("t"),
    listen: (_event, handler) => {
      heard = handler;
      return Promise.resolve(() => {});
    },
  };
  const at: FolderRunAt = $state({ entityId: "r", renderer: "site", command: "", folder: "flows", commit: null, paths: null, start });
  let run!: FolderRun;
  const stop = $effect.root(() => {
    run = new FolderRun(() => at, ports);
  });
  flushSync();
  const tell = (running: unknown, folder = "flows") => heard?.({ payload: { entity_id: "r", renderer: "site", folder, running } });
  return { asked, at, run: () => run, stop, tell };
}

test("open while shown, look when not, and open when a hidden page is shown again", async () => {
  const shown = harness(true);
  await settle();
  expect(shown.asked.map((a) => a.ask)).toEqual(["open"]);
  shown.stop();

  const hidden = harness(false);
  await settle();
  expect(hidden.asked.map((a) => a.ask)).toEqual(["look"]);
  hidden.at.start = true;
  await settle();
  expect(hidden.asked.map((a) => a.ask).at(-1)).toBe("open");
  hidden.stop();
});

test("progress is taken without an ask, and a run's end asks once", async () => {
  const h = harness(true);
  await settle();
  const before = h.asked.length;
  h.tell({ state: "keeping", done: 3, total: 9 });
  flushSync();
  expect(h.run().view?.running).toEqual({ state: "keeping", done: 3, total: 9 });
  expect(h.asked.length).toBe(before);
  h.tell(null, "elsewhere");
  await settle();
  expect(h.asked.length, "another folder's move asks nothing").toBe(before);
  h.tell(null);
  await settle();
  expect(h.asked.slice(before).map((a) => a.ask)).toEqual(["look"]);
  h.stop();
});
