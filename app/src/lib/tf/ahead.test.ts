/**
 * The sections opened ahead of their press (the ux13-overs pass, 2026-11-15): the shell asks the page host to hold
 * each hidden, one at a time, once the window is ready and quiet, never the section the reader is in.
 */
import { afterEach, expect, test, vi } from "vitest";

import { AHEAD, AHEAD_QUIET_MS, forgetAhead, openAhead, quietAndIdle } from "./ahead";
import { takeAhead } from "./kept-pages.svelte";
import type { Section } from "./route";

afterEach(() => {
  forgetAhead();
  takeAhead();
});

/** A wait the test lets pass one at a time, as the reader's quiet and the window's idle do. */
function gate() {
  const waiting: (() => void)[] = [];
  return {
    wait: () => new Promise<void>((resolve) => waiting.push(resolve)),
    async pass() {
      waiting.shift()?.();
      for (let i = 0; i < 5; i += 1) await Promise.resolve();
    },
  };
}

test("each section is held once the window is ready and quiet, one at a time, in order, skipping the one the reader is in", async () => {
  const { wait, pass } = gate();
  let current: Section | null = "builds";
  openAhead(() => true, () => current, undefined, wait);
  expect(takeAhead(), "nothing before the first quiet").toEqual([]);
  await pass();
  expect(takeAhead()).toEqual([`#/${AHEAD[0]}`]);
  await pass();
  expect(takeAhead(), "Builds is where the reader is").toEqual(AHEAD[1] === "builds" ? [] : [`#/${AHEAD[1]}`]);
  current = "time";
  for (const _ of AHEAD) await pass();
  expect(takeAhead()).toEqual(AHEAD.slice(2).map((section) => `#/${section}`));
});

test("nothing is held until the window is ready, and a stop holds no more", async () => {
  const { wait, pass } = gate();
  let ready = false;
  const stop = openAhead(() => ready, () => null, undefined, wait, ["tickets", "code"]);
  await pass();
  await new Promise((resolve) => setTimeout(resolve, 300));
  expect(takeAhead(), "the database is not up yet").toEqual([]);
  ready = true;
  await pass();
  expect(takeAhead()).toEqual(["#/tickets"]);
  stop();
  await pass();
  expect(takeAhead()).toEqual([]);
});

test("once a session: a second call holds nothing more", async () => {
  const first = gate();
  const second = gate();
  openAhead(() => true, () => null, undefined, first.wait, ["tickets"]);
  openAhead(() => true, () => null, undefined, second.wait, ["code"]);
  await first.pass();
  await second.pass();
  expect(takeAhead()).toEqual(["#/tickets"]);
});

test("under jsdom with the window's own wait, nothing is opened: a page test mounts the shell and counts its reads", async () => {
  openAhead(() => true, () => null);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(takeAhead()).toEqual([]);
});

test("the page held is the one the section's nav entry goes to at that moment, whatever the shell remembers of the section", async () => {
  const { wait, pass } = gate();
  const href = (section: string) => (section === "builds" ? "#/builds/live-teamcity%3AbuildType%3AScale" : `#/${section}`);
  openAhead(() => true, () => null, href, wait, ["builds", "code"]);
  await pass();
  await pass();
  expect(takeAhead()).toEqual(["#/builds/live-teamcity%3AbuildType%3AScale", "#/code"]);
});

test("a field's input is the reader's too: a colour picker's drag, which the page hears only as inputs, keeps a page from being built ahead", async () => {
  const field = document.body.appendChild(document.createElement("input"));
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  try {
    let quiet = false;
    void quietAndIdle().then(() => (quiet = true));
    for (let i = 0; i < 5; i += 1) {
      await sleep(AHEAD_QUIET_MS / 2);
      field.dispatchEvent(new Event("input", { bubbles: true }));
    }
    expect(quiet).toBe(false);
    await sleep(AHEAD_QUIET_MS + 400);
    expect(quiet).toBe(true);
  } finally {
    field.remove();
  }
}, 10_000);

test("what warms after the sections waits for them: aheadBuilt settles once the last is built, or the shell stops", async () => {
  for (const stop of [false, true]) {
    vi.resetModules();
    const fresh = await import("./ahead");
    let settled = false;
    void fresh.aheadBuilt.then(() => (settled = true));
    const { wait, pass } = gate();
    const stopIt = fresh.openAhead(() => true, () => null, undefined, wait, ["tickets", "builds"]);
    await pass();
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    expect(settled, "one section still to build").toBe(false);
    if (stop) stopIt();
    else await pass();
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    expect(settled).toBe(true);
    fresh.forgetAhead();
  }
});
