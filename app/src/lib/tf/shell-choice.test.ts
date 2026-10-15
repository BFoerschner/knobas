import { expect, test } from "vitest";

import { readShellChoice, SHELL_KEY, switchShell } from "./shell-choice";

function memory(): Pick<Storage, "getItem" | "setItem"> & { map: Map<string, string> } {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
  };
}

test("nothing chosen, or no storage at all, is the rooms UI", () => {
  expect(readShellChoice(memory())).toBe("rooms");
  expect(readShellChoice(null)).toBe("rooms");
});

test("switching stores the choice and reloads, and the next start reads it", () => {
  const store = memory();
  let reloads = 0;
  switchShell("tickets", store, () => (reloads += 1));
  expect(reloads).toBe(1);
  expect(store.map.get(SHELL_KEY)).toBe("tickets");
  expect(readShellChoice(store)).toBe("tickets");

  switchShell("rooms", store, () => (reloads += 1));
  expect(readShellChoice(store)).toBe("rooms");
});

test("a storage that throws reads as the rooms UI", () => {
  const broken = {
    getItem: () => {
      throw new Error("denied");
    },
    setItem: () => {},
  };
  expect(readShellChoice(broken)).toBe("rooms");
});

test("the dev override wins over what is stored", () => {
  const store = memory();
  store.setItem(SHELL_KEY, "rooms");
  expect(readShellChoice(store, "?fake-ipc&shell=tickets")).toBe("tickets");
  expect(readShellChoice(store, "?fake-ipc")).toBe("rooms");
});
