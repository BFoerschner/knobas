/**
 * The backup section of the settings view — spec §14's "small settings surface
 * (export now / schedule / restore)", issue #69.
 *
 * What is pinned here is what a reader can find out and what they can do about
 * it: what the schedule will actually do, where the archives are, that an
 * export reports where it went, and that a restore says what it will do before
 * it does it and says what failed when it does not. The internal shape of the
 * component is not tested — every assertion below goes through the rendered
 * view, the way a person meets it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BackupSchedule, BackupStatus } from "../ipc/backup";

const NOW = new Date("2026-08-29T09:14:00Z");
const DIR = "/Users/x/Library/Application Support/dev.knobas.desktop/backups";

/** Plain recorders, not `vi.fn`s, so the mock factory can close over them. */
const calls = {
  status: 0,
  now: 0,
  setSchedule: [] as BackupSchedule[],
  restore: [] as string[],
};

let status: BackupStatus;
let statusFails: unknown = null;
let nowFails: unknown = null;
let restoreFails: unknown = null;

vi.mock("../ipc/backup", () => ({
  backupStatus: () => {
    calls.status += 1;
    return statusFails ? Promise.reject(statusFails) : Promise.resolve(status);
  },
  backupNow: () => {
    calls.now += 1;
    if (nowFails) return Promise.reject(nowFails);
    return Promise.resolve({
      taken_at: NOW.toISOString(),
      file: "knobas-20260829-091400.knobas",
      bytes: 4_194_304,
    });
  },
  setBackupSchedule: (schedule: BackupSchedule) => {
    calls.setSchedule.push(schedule);
    status = { ...status, schedule };
    return Promise.resolve(status);
  },
  restoreBackup: (file: string) => {
    calls.restore.push(file);
    return restoreFails ? Promise.reject(restoreFails) : Promise.resolve();
  },
}));

const { default: BackupSection } = await import("./BackupSection.svelte");
const { toasts } = await import("../shell/toasts.svelte");

function schedule(over: Partial<BackupSchedule> = {}): BackupSchedule {
  return { enabled: true, hour: 3, minute: 0, keep: 7, ...over };
}

function statusOf(over: Partial<BackupStatus> = {}): BackupStatus {
  return {
    schedule: schedule(),
    directory: DIR,
    last: {
      taken_at: "2026-08-29T01:00:00Z",
      file: "knobas-20260829-030000.knobas",
      bytes: 4_194_304,
    },
    next_due_at: "2026-08-30T01:00:00Z",
    archives: [
      { file: "knobas-20260829-030000.knobas", bytes: 4_194_304 },
      { file: "knobas-20260828-030000.knobas", bytes: 4_100_000 },
    ],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render() {
  app = mount(BackupSection, { target, props: { now: NOW } });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

/** A button by its visible label, anywhere in the section. */
function button(label: string, within: ParentNode = target) {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
}

function dialog() {
  return target.querySelector<HTMLElement>('[role="dialog"]');
}

function text() {
  return target.textContent ?? "";
}

beforeEach(() => {
  calls.status = 0;
  calls.now = 0;
  calls.setSchedule = [];
  calls.restore = [];
  status = statusOf();
  statusFails = null;
  nowFails = null;
  restoreFails = null;
  toasts.items = [];
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

/**
 * The reading a person came for: what will happen, and where the files are.
 *
 * The `at 03:00` assertion is the load-bearing one and it is a *negative*:
 * issue #69 rules that the wording may not promise a fixed time, and the way
 * that rule gets broken is by someone writing the shorter, wronger sentence.
 */
test("the section states the schedule as a boundary, the retention, and the directory", async () => {
  render();
  await settle();

  expect(text()).toContain("after 03:00");
  expect(text()).not.toContain("at 03:00");
  expect(text()).toContain("7 archives");
  expect(text()).toContain(DIR);
});

/** A stamp and a size, not just "backed up": the question is *when*, and *did it work*. */
test("the last export is named, sized and dated", async () => {
  render();
  await settle();

  expect(text()).toContain("knobas-20260829-030000.knobas");
  expect(text()).toContain("4 MB");
  expect(text()).toContain("8 h ago");
});

/**
 * "No backups yet" is a fact; an empty stamp is a shrug. The engine returns
 * `last: null` on a knobas that has never exported, and that is the state a
 * new install is in for its first day.
 */
test("a knobas that has never exported says so", async () => {
  status = statusOf({ last: null, archives: [] });
  render();
  await settle();

  expect(text()).toMatch(/no backup|never/i);
  expect(text()).not.toContain("—  ·");
});

/**
 * *Export now* has to say **where the file went**, not merely that it worked.
 *
 * An archive is a thing a person later has to find in a file manager, and
 * "Backup complete" is the message that makes them go looking. The name and
 * the directory are both on screen afterwards: the directory because it is
 * always there, the name because the re-read has moved the *last export* line
 * onto the file that was just written.
 */
test("Export now names the file it wrote and where it went", async () => {
  render();
  await settle();

  status = statusOf({
    last: { taken_at: NOW.toISOString(), file: "knobas-20260829-091400.knobas", bytes: 4_194_304 },
    archives: [{ file: "knobas-20260829-091400.knobas", bytes: 4_194_304 }],
  });
  button("Export now")!.click();
  await settle();

  expect(calls.now).toBe(1);
  expect(text()).toContain("knobas-20260829-091400.knobas");
  expect(text()).toContain(DIR);
  expect(toasts.items.map((toast) => toast.text).join(" ")).toContain(
    "knobas-20260829-091400.knobas",
  );
});

/**
 * The archive list is re-read, not appended to.
 *
 * `export_now` writes a file *and* prunes past `keep`, so the list afterwards
 * is not the list before plus a row — an eighth export with `keep: 7` removes
 * one. A section that patched its own list would show an archive that is no
 * longer on disk, and offer to restore it.
 */
test("an export re-reads what is on disk rather than assuming", async () => {
  render();
  await settle();
  const readOnce = calls.status;

  button("Export now")!.click();
  await settle();

  expect(calls.status).toBeGreaterThan(readOnce);
});

/**
 * `pg_dump`'s stderr is the message, because it is the only thing that says
 * *why* — a full disk and a missing tool are different problems with different
 * fixes, and "Export failed" distinguishes neither.
 */
test("a failed export says what failed", async () => {
  nowFails = {
    code: "internal",
    message: "pg_dump: error: could not write to output file: No space left on device",
    source_id: null,
  };
  render();
  await settle();

  button("Export now")!.click();
  await settle();

  expect(toasts.items.map((toast) => toast.text).join(" ")).toContain("No space left on device");
});

/**
 * One press, one archive.
 *
 * An export of a real corpus takes long enough to look like nothing happened,
 * and the reader's response to a button that looks inert is to press it again.
 * Two concurrent `pg_dump`s writing two archives a second apart is not a
 * crash — it is a retention window quietly one night shorter.
 */
test("the export button is not offered twice while one is running", async () => {
  render();
  await settle();

  button("Export now")!.click();
  // No `settle`: this is the state a reader is looking at *during* the round
  // trip, which is the whole window in which a second press is possible.
  flushSync();
  expect(button("Export now"), "a second press would write a second archive").toBeUndefined();
  expect(target.querySelector("button[disabled]")).not.toBeNull();

  await settle();
  expect(calls.now).toBe(1);
  expect(button("Export now"), "and it comes back when the export is done").toBeTruthy();
});
