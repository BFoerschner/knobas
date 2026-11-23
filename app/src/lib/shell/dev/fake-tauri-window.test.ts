/**
 * `?fake-ipc` inside a real Tauri window (the ux30 end gate, 2026-11-23): the probes' build served into knobas's own
 * WebView2 window stayed on *Starting the local database*, because Tauri's first script locks
 * `window.__TAURI_INTERNALS__` and the fake's `defineProperty` over it threw.
 *
 * The window is set up here the way tauri 2's own scripts set it up (`manager/webview.rs`, `scripts/core.js`,
 * `scripts/ipc-protocol.js`, `event/init.js`): the internals defined with a value only, `invoke` posting each command to
 * `http://ipc.localhost/<cmd>` with its callbacks' ids in headers. Its own file, since a locked property outlives a test.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeAll, expect, test, vi } from "vitest";

import { installFakeTauri, type FakeBridge } from "./fake-tauri";

/** What reached the network: the window's own backend, had the fake let it through. */
const network = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response("from the network"));

function tauriWindow() {
  window.fetch = network as typeof window.fetch;
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: { plugins: {} } });
  const internals = (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__;
  const callbacks = new Map<number, (data: unknown) => void>();
  let next = 0;
  const register = (callback: (data: unknown) => void, once = false) => {
    const id = ++next;
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      callback(data);
    });
    return id;
  };
  Object.defineProperty(internals, "transformCallback", { value: register });
  Object.defineProperty(internals, "unregisterCallback", { value: (id: number) => callbacks.delete(id) });
  Object.defineProperty(internals, "runCallback", { value: (id: number, data: unknown) => callbacks.get(id)?.(data) });
  Object.defineProperty(internals, "callbacks", { value: callbacks });
  Object.defineProperty(internals, "convertFileSrc", { value: (path: string, protocol = "asset") => ["http:/", `${protocol}.localhost`, encodeURIComponent(path)].join("/") });
  Object.defineProperty(internals, "ipc", { value: () => {} });
  Object.defineProperty(internals, "invoke", {
    value: (cmd: string, payload: unknown = {}) =>
      new Promise((resolve, reject) => {
        const callback = register((answer) => {
          resolve(answer);
          callbacks.delete(error);
        }, true);
        const error = register((failure) => {
          reject(failure);
          callbacks.delete(callback);
        }, true);
        const headers = new Headers({ "Content-Type": "application/json", "Tauri-Callback": String(callback), "Tauri-Error": String(error) });
        void fetch(`http://ipc.localhost/${encodeURIComponent(cmd)}`, { method: "POST", body: JSON.stringify(payload), headers }).then(
          () => reject(new Error(`${cmd} reached the real backend`)),
          () => reject(new Error(`${cmd} fell back to postMessage`)),
        );
      }),
  });
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", { value: { unregisterListener: () => {} } });
  return callbacks;
}

let callbacks: Map<number, unknown>;
let bridge: FakeBridge;

beforeAll(() => {
  callbacks = tauriWindow();
  bridge = installFakeTauri({
    echo: (args) => ({ got: args }),
    refuse: () => {
      throw { code: "internal", message: "no", source_id: null };
    },
  });
});

test("a command is answered from the fixture, not the window's backend", async () => {
  await expect(invoke("echo", { entityId: "e-1", limit: 3 })).resolves.toEqual({ got: { entityId: "e-1", limit: 3 } });
  await expect(invoke("refuse")).rejects.toEqual({ code: "internal", message: "no", source_id: null });
  await expect(invoke("unknown_command")).rejects.toMatchObject({ code: "internal", message: "fake-tauri: no handler for unknown_command" });
  expect(network).not.toHaveBeenCalled();
});

test("an event reaches its listener until it is unlistened, and the listener's callback goes with it", async () => {
  const heard: unknown[] = [];
  const before = callbacks.size;
  const off = await listen<number>("db-state", (event) => heard.push(event.payload));
  bridge.emit("db-state", 1);
  bridge.emit("other", 2);
  expect(heard).toEqual([1]);
  off();
  await vi.waitFor(() => expect(callbacks.size).toBe(before));
  bridge.emit("db-state", 3);
  expect(heard).toEqual([1]);
});

test("a request that is not a command still goes out", async () => {
  const answer = await fetch("http://tauri.localhost/assets/x.js");
  expect(await answer.text()).toBe("from the network");
  expect(network).toHaveBeenCalledTimes(1);
});
