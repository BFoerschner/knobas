/**
 * The settings view — the minimum shell issue #69 was ruled to create for
 * itself (Fable, 2026-08-29, under delegation): titled sections in one
 * scrollable pane, **no tabs, no router, no section registry**.
 *
 * So what is worth pinning here is small and blunt: the view is one scrollable
 * pane, it says what it is, and both sections are in it. The second one
 * arrived with #282 and cost what the ruling said it would — one component and
 * one import, as did the fifth (#443) — which is the shape this test is protecting; a test that pinned
 * a registry or a tab strip would be pinning the thing the ruling forbade.
 *
 * Each section's own behaviour is its own file.
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
  // The section reads these on mount to seed the share dialog; a mock that
  // replaces the module has to carry them or the component throws.
  shareDefaults: { links: true, assets: true, contexts: true, notes: false, time: false, sources: true },
  shareExport: () => Promise.reject(new Error("not used here")),
  restoreBackup: () => Promise.reject(new Error("not used here")),
}));

/** Likewise: the toggle's behaviour is `PassiveSection.test.svelte.ts`. */
vi.mock("../ipc/time", () => ({
  passiveAttribution: () => Promise.resolve(false),
  setPassiveAttribution: () => Promise.reject(new Error("not used here")),
}));

/** And the fifth section's is `MonitoringSection.test.svelte.ts`. */
vi.mock("../ipc/assets", () => ({
  monitoringSettings: () =>
    Promise.resolve({ sample_retention_days: 90, response_time_warn_ms: 1500 }),
  setMonitoringSettings: () => Promise.reject(new Error("not used here")),
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

test("the view names itself and carries its sections", async () => {
  await render();

  expect(target.querySelector("h1")?.textContent).toContain("Settings");
  expect(target.textContent).toContain("Backup");
  expect(target.textContent).toContain("after 03:00");
  expect(target.textContent).toContain("Monitoring");
  expect(target.textContent).toContain("90 days");
  expect(target.textContent).toContain("Passive attribution");
  expect(
    target.querySelector('input[type="checkbox"]'),
    "the passive attribution section draws its switch once the read answers",
  ).not.toBeNull();
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
