/** Settings › Backup and export's restore and import dialogs (#831, #2518) over `backup_status`. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { BackupStatus } from "../../ipc/backup";
import { createLayers } from "../layers.svelte";
import TfBackupSection from "./TfBackupSection.svelte";

const DIRECTORY = String.raw`C:\Users\someone with a long name\AppData\Roaming\knobas\folders with spaces in them\backups`;
const BACKUP = `${DIRECTORY}\\knobas-backup-of-the-payments-team-laptop-before-the-move-2026-11-26-0200.knobas`;
const SHARE = `${DIRECTORY}\\knobas-share-export-for-the-new-colleagues-in-payments-2026-11-26.knobas`;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let status: BackupStatus;

beforeEach(() => {
  status = {
    schedule: { enabled: true, hour: 2, minute: 0, keep: 14 },
    directory: DIRECTORY,
    last: { taken_at: "2026-11-26T01:00:00Z", file: BACKUP, bytes: 48 * 1024 * 1024 },
    next_due_at: null,
    archives: [
      { file: BACKUP, bytes: 48 * 1024 * 1024, share: false },
      { file: SHARE, bytes: 3 * 1024 * 1024, share: true },
    ],
  };
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

async function draw() {
  const ports = {
    backupStatus: () => Promise.resolve(status),
    backupNow: () => Promise.reject(new Error("not here")),
    setBackupSchedule: () => Promise.resolve(status),
    shareExport: () => Promise.reject(new Error("not here")),
    restoreBackup: () => Promise.resolve(),
    smartLists: () => Promise.resolve([]),
    favourites: () => Promise.resolve([]),
    keys: () => Promise.resolve({ lists: [], actions: [] }),
    ticketViews: () => Promise.resolve([]),
    linkSets: () => Promise.resolve([]),
    stateColours: () => Promise.resolve([]),
    openBackupFolder: () => Promise.resolve(),
  };
  view = mount(TfBackupSection, { target, props: { ports: ports as never, layers: createLayers(), now: new Date("2026-11-26T12:00:00Z") } });
  await settle();
}

async function open(name: string) {
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === name)!.click();
  await settle();
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
}

for (const [name, file, end] of [
  ["Restore from a file…", BACKUP, "6-0200.knobas"],
  ["Import…", SHARE, "11-26.knobas"],
] as const) {
  test(`${name}'s dialog cuts the folder's path and each file's name in their middle, whole on hover (#2518)`, async () => {
    await draw();
    const dialog = await open(name);
    // The folder: a path that keeps its end, on one line, whole in its title.
    const folder = dialog.querySelector(`.tf-dsub .tf-path[title="${CSS.escape(DIRECTORY)}"]`);
    expect(text(folder)).toBe(DIRECTORY);
    expect(text(folder?.querySelector(".tf-path-tail"))).toBe("backups");
    // A file: its name only, keeping its date and extension at the end, the whole path in its title.
    const pick = dialog.querySelector(`.tf-pickrow .tf-path[title="${CSS.escape(file)}"]`);
    expect(text(pick)).toBe(file.slice(DIRECTORY.length + 1));
    expect(text(pick?.querySelector(".tf-path-tail")).endsWith(end)).toBe(true);
  });
}
