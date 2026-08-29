/**
 * The settings view — the minimum shell issue #69 was ruled to create for
 * itself (Fable, 2026-08-29, under delegation): titled sections in one
 * scrollable pane, **no tabs, no router, no section registry**, with Backup as
 * the only section.
 *
 * So what is worth pinning here is small and blunt: the view is one scrollable
 * pane, it says what it is, and the backup section is in it. The day a second
 * section arrives it is one component and one import, and that is the shape
 * this test is protecting — a test that pinned a registry or a tab strip would
 * be pinning the thing the ruling forbade.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

/** The section's own behaviour is `BackupSection.test.svelte.ts`; here it only has to be there. */
vi.mock("../ipc/backup", () => ({
  backupStatus: () =>
    Promise.resolve({
      schedule: { enabled: true, hour: 3, minute: 0, keep: 7 },
      directory: "/tmp/knobas-backups",
      last: null,
      next_due_at: null,
      archives: [],
    }),
  backupNow: () => Promise.reject(new Error("not used here")),
  setBackupSchedule: () => Promise.reject(new Error("not used here")),
  restoreBackup: () => Promise.reject(new Error("not used here")),
}));

const { default: SettingsView } = await import("./SettingsView.svelte");

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

async function render() {
  app = mount(SettingsView, { target, props: {} });
  flushSync();
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the view names itself and carries the backup section", async () => {
  await render();

  expect(target.querySelector("h1")?.textContent).toContain("Settings");
  expect(target.textContent).toContain("Backup");
  expect(target.textContent).toContain("after 03:00");
});

/**
 * One scrollable pane, the same `.view` / `.view-b` frame the sources view
 * uses — which is what makes a second section an import rather than a layout.
 */
test("the sections sit in one scrolling pane, not in tabs", async () => {
  await render();

  expect(target.querySelector(".view-b")).not.toBeNull();
  expect(
    target.querySelector('[role="tablist"], [role="tab"]'),
    "the ruling on #69 is explicit: no tabs",
  ).toBeNull();
});
