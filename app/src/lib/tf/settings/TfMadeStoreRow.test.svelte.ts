/** Where generated folders are kept (ADR-0046): the row's words, *Change…* refused in the backend's words, *Clear*. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { MadeStore } from "../../ipc/app";
import { toasts } from "../../shell/toasts.svelte";
import TfMadeStoreRow from "./TfMadeStoreRow.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  toasts.items = [];
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const MB = 1024 * 1024;
const STORE: MadeStore = { dir: "D:\\code\\knobas-rendered", set: null, default_dir: "D:\\code", place: "clones", folders: 3, bytes: 212 * MB, cap_bytes: 4096 * MB };

async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  flushSync();
}
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text)!;

test("the row says where, how much of the cap, and why there", () => {
  for (const [place, words] of [
    ["clones", "Beside your clones."],
    ["set", "In the folder you chose."],
    ["profile", "In knobas's own data, until you set a clones root."],
  ] as const) {
    app = mount(TfMadeStoreRow, { target, props: { store: { ...STORE, place }, onstore: () => {} } });
    flushSync();
    expect(target.querySelector(".tf-stg-l small")?.textContent).toBe(`3 folders, 212 MB of 4 GB. ${words}`);
    unmount(app);
    app = null;
  }
});

test("Change… says the backend's refusal under the field; Use the default sets none", async () => {
  const asked: (string | null)[] = [];
  let now = STORE;
  app = mount(TfMadeStoreRow, {
    target,
    props: {
      store: { ...STORE, set: "D:\\elsewhere" },
      set: (path: string | null) => {
        asked.push(path);
        if (path !== null) return Promise.reject({ code: "invalid", message: "D:\\code\\payout is inside the repository at D:\\code\\payout" });
        return Promise.resolve(STORE);
      },
      onstore: (next: MadeStore) => (now = next),
    },
  });
  flushSync();
  button("Change…").click();
  flushSync();
  const field = document.querySelector<HTMLInputElement>("#tf-stg-made-in")!;
  expect(field.placeholder).toBe("D:\\code");
  field.value = "D:\\code\\payout\\out";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  button("Save").click();
  await settle();
  expect(document.querySelector(".tf-dbanner")?.textContent).toContain("is inside the repository at");
  button("Use the default").click();
  await settle();
  expect(asked).toEqual(["D:\\code\\payout\\out", null]);
  expect(now).toBe(STORE);
});

test("Clear removes them and says how many and how much; at zero it waits, its room kept", async () => {
  let now = STORE;
  app = mount(TfMadeStoreRow, { target, props: { store: STORE, clear: () => Promise.resolve({ ...STORE, folders: 0, bytes: 0 }), onstore: (next: MadeStore) => (now = next) } });
  flushSync();
  button("Clear").click();
  await settle();
  expect(now.folders).toBe(0);
  expect(toasts.items.at(-1)?.text).toBe("Cleared 3 generated folders, 212 MB");
  unmount(app);
  app = mount(TfMadeStoreRow, { target, props: { store: { ...STORE, folders: 0, bytes: 0 }, onstore: () => {} } });
  flushSync();
  expect(button("Clear").disabled).toBe(true);
});
