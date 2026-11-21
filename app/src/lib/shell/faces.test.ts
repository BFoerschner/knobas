import { expect, test, vi } from "vitest";

import { loadEveryFace } from "./faces";

function face(status: string, family = '"Inter"') {
  return { family, status, load: vi.fn(() => Promise.resolve()) };
}

test("every face not yet loaded is loaded once the window is idle, and none before", () => {
  const faces = [face("unloaded"), face("loaded"), face("loading"), face("unloaded")];
  let idle: (() => void) | null = null;
  loadEveryFace(faces, (go) => (idle = go));
  expect(faces.every((f) => f.load.mock.calls.length === 0)).toBe(true);
  idle!();
  expect(faces.map((f) => f.load.mock.calls.length)).toEqual([1, 0, 0, 1]);
});

test("the Cascadia the ligature switch does not draw is left unloaded (2026-11-21)", () => {
  const faces = [face("unloaded", '"Cascadia Mono"'), face("unloaded", "Cascadia Code"), face("unloaded", '"Cascadia Code"'), face("unloaded")];
  // The window's switch, off: Cascadia Code is the one no rule draws.
  loadEveryFace(faces, (go) => go());
  expect(faces.map((f) => f.load.mock.calls.length)).toEqual([1, 0, 0, 1]);
  const again = [face("unloaded", '"Cascadia Mono"'), face("unloaded", '"Cascadia Code"')];
  loadEveryFace(again, (go) => go(), () => "Cascadia Mono");
  expect(again.map((f) => f.load.mock.calls.length)).toEqual([0, 1]);
});

test("a face that fails to load is no error, and a page with no font set does nothing", async () => {
  const bad = { family: "Inter", status: "unloaded", load: vi.fn(() => Promise.reject(new Error("gone"))) };
  loadEveryFace([bad], (go) => go());
  await Promise.resolve();
  expect(bad.load).toHaveBeenCalledOnce();
  expect(() => loadEveryFace(undefined, (go) => go())).not.toThrow();
});
