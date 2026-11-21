/** The updater's state in the window (ADR-0052): seeded once, moved by `update:state`, one toast after an update. */
import { expect, test } from "vitest";

import type { UpdateState } from "../ipc/update";
import { updateSample as fakeUpdate } from "./settings/update-samples";
import { createUpdates, type UpdatePorts } from "./update.svelte";

function rig(first: UpdateState) {
  let hear: ((event: { payload: unknown }) => void) | null = null;
  const toasts: string[] = [];
  const answer = async () => first;
  const ports: UpdatePorts = {
    state: answer,
    check: answer,
    set: answer,
    download: answer,
    cancel: answer,
    install: async () => {
      throw { code: "invalid", message: "there is no checked download to install" };
    },
    later: answer,
    notes: async (version) => ({ version, markdown: null, page_url: "" }),
    listen: async (_event, handler) => {
      hear = handler;
      return () => (hear = null);
    },
  };
  const store = createUpdates(ports, (state) => toasts.push(state.current));
  return { store, toasts, emit: (state: UpdateState) => hear?.({ payload: state }) };
}

const settle = async () => {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
};

test("the store is seeded once and moved by update:state", async () => {
  const { store, emit } = rig(fakeUpdate("up-to-date"));
  expect(store.state).toBeNull();
  const stop = store.start();
  await settle();
  expect(store.state?.offer).toBeNull();
  emit(fakeUpdate("available"));
  expect(store.state?.offer?.version).toBe("3.2.9-ux28");
  stop();
});

test("an update that went in toasts once, however often its state arrives", async () => {
  const { store, toasts, emit } = rig({ ...fakeUpdate("up-to-date"), current: "3.2.9-ux28", just_updated: true });
  store.start();
  await settle();
  emit({ ...fakeUpdate("up-to-date"), current: "3.2.9-ux28", just_updated: true });
  expect(toasts).toEqual(["3.2.9-ux28"]);
});

test("a refused press changes nothing", async () => {
  const { store } = rig(fakeUpdate("available"));
  store.start();
  await settle();
  await store.install();
  expect(store.state?.phase.phase).toBe("idle");
});
