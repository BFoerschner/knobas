/**
 * `monitorRoster` asks for no samples (the monitors-page pass of PR #2479, 2026-11-19): the Monitors tab draws each
 * row's `bar`, 48 letters, and a day of samples was 1.4 MB of the 1.42 MB it read on every Uptime Kuma poll.
 */
import { expect, test, vi } from "vitest";

import { monitorRoster } from "./assets";

const asked: [string, unknown][] = [];

vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: (command: string, args: unknown) => {
    asked.push([command, args]);
    return Promise.resolve([]);
  },
}));

test("the roster is read with its bars and without its samples", async () => {
  await monitorRoster();
  expect(asked).toEqual([["monitor_roster", { withSamples: false }]]);
});
