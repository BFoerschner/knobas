/**
 * The Settings sections besides Sources (#831), against stand-ins for every
 * command, so what each control sends is what the test reads.
 */
import { pageContext } from "../kept-pages.svelte";
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BackupStatus } from "../../ipc/backup";
import type { InboxCategory, WriteAsk } from "../../ipc/entity";
import type { SourceSummary } from "../../ipc/sources";
import { createHealth } from "../../shell/health.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { keptPages } from "../kept-pages.svelte";
import { createLayers } from "../layers.svelte";
import { createTheme, type Theme } from "../theme.svelte";
import type { SectionPorts, SettingsPorts } from "./ports";
import TfSettings from "./TfSettings.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  location.hash = "";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const NOW = new Date(2026, 9, 15, 14, 30);
const DIR = "~/Library/Application Support/knobas/backups";

function status(over: Partial<BackupStatus["schedule"]> = {}): BackupStatus {
  return {
    schedule: { enabled: true, hour: 2, minute: 0, keep: 14, ...over },
    directory: DIR,
    last: { taken_at: new Date(2026, 9, 15, 2, 0).toISOString(), file: "knobas-20261015-020000.knobas", bytes: 48 * 1024 * 1024 },
    next_due_at: null,
    archives: [
      { file: "knobas-20261015-020000.knobas", bytes: 48 * 1024 * 1024, share: false },
      { file: "knobas-share-20261012-101500.knobas", bytes: 2 * 1024 * 1024, share: true },
    ],
  };
}

const sourcePorts: SettingsPorts = {
  listSources: () => Promise.resolve([]),
  listAdapters: () => Promise.resolve([]),
  syncStatus: () => Promise.resolve([]),
  listSyncRuns: () => Promise.resolve([]),
  updateSource: () => Promise.reject(new Error("unused")),
  backfillSource: () => Promise.resolve(0),
  deleteSource: () => Promise.resolve(),
  setSourceSecret: () => Promise.reject(new Error("unused")),
  testSource: () => Promise.reject(new Error("unused")),
  syncNow: () => Promise.resolve(0),
  watchSync: () => Promise.resolve(null),
  syncAll: () => Promise.resolve([]),
  addSource: () => Promise.reject(new Error("unused")),
  previewInstallAdapter: () => Promise.reject(new Error("unused")),
  installAdapter: () => Promise.reject(new Error("unused")),
  listInstalledAdapters: () => Promise.resolve([]),
  checkAdapterUpdates: () => Promise.resolve([]),
  previewUpdateAdapter: () => Promise.reject(new Error("unused")),
  updateAdapter: () => Promise.reject(new Error("unused")),
  uninstallAdapter: () => Promise.reject(new Error("unused")),
  listen: () => Promise.resolve(() => {}),
  ticketViews: () => Promise.resolve([]),
  hideSavedView: () => Promise.resolve(),
  appStatus: () => Promise.resolve({ secrets_in_memory: false }),
};

function sectionPorts(over: Partial<SectionPorts> = {}) {
  return {
    pagesKeptOpen: vi.fn(() => Promise.resolve(5)),
    setPagesKeptOpen: vi.fn((count: number) => Promise.resolve(Math.min(15, count))),
    passiveAttribution: vi.fn(() => Promise.resolve(true)),
    setPassiveAttribution: vi.fn((on: boolean) => Promise.resolve(on)),
    notify: vi.fn(() => Promise.resolve()),
    standupPublishTarget: vi.fn(() => Promise.resolve({ source_id: "confluence", parent: "confluence:98400" })),
    setStandupPublishTarget: vi.fn((target) => Promise.resolve(target)),
    getEntity: vi.fn(() =>
      Promise.resolve({
        row: {
          entity_id: "confluence:98400",
          kind: "page",
          source_id: "confluence",
          title: "Standups",
          updated_at: null,
          synced_at: "2026-10-15T00:00:00Z",
          path: "Payments",
          state: null,
        },
      }),
    ),
    search: vi.fn(() => Promise.resolve({ groups: [] })) as unknown as SectionPorts["search"],
    listSources: vi.fn(() => Promise.resolve([])),
    listAdapters: vi.fn(() => Promise.resolve([])),
    monitoringSettings: vi.fn(() => Promise.resolve({ sample_retention_days: 30, response_time_warn_ms: 2000 })),
    setMonitoringSettings: vi.fn((settings) => Promise.resolve(settings)),
    backupStatus: vi.fn(() => Promise.resolve(status())),
    backupNow: vi.fn(() => Promise.resolve({ taken_at: NOW.toISOString(), file: "knobas-new.knobas", bytes: 48 * 1024 * 1024 })),
    setBackupSchedule: vi.fn((schedule) => Promise.resolve({ ...status(), schedule })),
    shareExport: vi.fn(() => Promise.resolve({ taken_at: NOW.toISOString(), file: "knobas-share-new.knobas", bytes: 1024 })),
    restoreBackup: vi.fn(() => Promise.resolve()),
    smartLists: vi.fn(() => Promise.resolve([{ saved: false }])),
    ticketViews: vi.fn(() => Promise.resolve([])),
    linkSets: vi.fn(() => Promise.resolve([])),
    appStatus: vi.fn(() => Promise.resolve({ app_version: "1.6.0" })),
    dbStats: vi.fn(() =>
      Promise.resolve({
        db_bytes: 1.2 * 1024 ** 3,
        entity_count: 400,
        item_count: 18400,
        per_source: [],
        oldest_synced_at: null,
        newest_synced_at: null,
      }),
    ),
    reindexFts: vi.fn(() => Promise.resolve()),
    openLogs: vi.fn(() => Promise.resolve()),
    openBackupFolder: vi.fn(() => Promise.resolve()),
    ...over,
  } satisfies SectionPorts;
}

function notificationsStub(on: InboxCategory[]) {
  let kinds = $state(on);
  return {
    get kinds() {
      return kinds;
    },
    loaded: true,
    permission: null,
    error: null,
    busy: false,
    reseed: vi.fn(() => Promise.resolve()),
    choose: vi.fn((kind: InboxCategory, yes: boolean) => {
      kinds = yes ? [...kinds, kind] : kinds.filter((k) => k !== kind);
      return Promise.resolve();
    }),
  };
}

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function open(
  section: string,
  p = sectionPorts(),
  extra: { theme?: Theme; notifications?: ReturnType<typeof notificationsStub> } = {},
) {
  view = mount(TfSettings, {
    target,
    context: hiddenable,
    props: {
      rest: [section],
      job: null,
      ready: true,
      ports: sourcePorts,
      sectionPorts: p,
      layers: createLayers(),
      now: () => NOW,
      theme: extra.theme ?? createTheme(),
      ...(extra.notifications ? { notifications: extra.notifications } : {}),
    },
  });
  await settle();
  return p;
}

const body = () => target.querySelector(".tf-stg-body")!;

function button(text: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find((el) => el.textContent?.trim() === text);
  if (!found) throw new Error(`no button "${text}"`);
  return found;
}

function switchFor(label: string): HTMLButtonElement {
  const found = target.querySelector<HTMLButtonElement>(`[role="switch"][aria-label="${label}"]`);
  if (!found) throw new Error(`no switch "${label}"`);
  return found;
}

function rowValue(label: string): string {
  const row = [...target.querySelectorAll(".tf-stg-row")].find((el) =>
    el.querySelector(".tf-stg-l")?.firstChild?.textContent?.trim().startsWith(label),
  );
  return row?.querySelector(".tf-stg-c")?.textContent?.trim() ?? "";
}

test("Appearance and the top bar's toggle are the same setting", async () => {
  const theme = createTheme();
  await open("appearance", sectionPorts(), { theme });
  const pressed = () => target.querySelector('[aria-label="Theme"] [aria-pressed="true"]')?.textContent;
  expect(pressed()).toBe("Like the system");

  button("Dark").click();
  flushSync();
  expect(theme.choice).toBe("dark");

  // The top bar's toggle, from dark: light, and Appearance says so.
  theme.toggle(false);
  flushSync();
  expect(pressed()).toBe("Light");
});

test("Time: the passive switch writes passive_attribution and draws what came back", async () => {
  const p = await open("time");
  expect(body().querySelector("h1")?.textContent).toBe("Time");
  expect(switchFor("Passive blocks").getAttribute("aria-checked")).toBe("true");
  expect(rowValue("Kept for")).toBe("About a month");
  switchFor("Passive blocks").click();
  await settle();
  expect(p.setPassiveAttribution).toHaveBeenCalledWith(false);
  expect(switchFor("Passive blocks").getAttribute("aria-checked")).toBe("false");
});

test("Performance: Pages kept open writes set_pages_kept_open and hands the page host what was stored", async () => {
  const p = await open("performance");
  expect(body().querySelector("h1")?.textContent).toBe("Performance");
  const field = body().querySelector<HTMLInputElement>("#tf-perf-kept")!;
  expect(field.value).toBe("5");

  field.value = "2";
  field.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.setPagesKeptOpen).toHaveBeenCalledWith(2);
  expect(keptPages.value, "the page host reads it at once").toBe(2);

  field.value = "40";
  field.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.setPagesKeptOpen).toHaveBeenLastCalledWith(15);
  expect(field.value).toBe("15");
  expect(keptPages.value).toBe(15);
});

test("Desktop notifications: a switch per inbox kind over the notifier's store, and a test notification", async () => {
  const store = notificationsStub(["review_request", "mention"]);
  const p = await open("notifications", sectionPorts(), { notifications: store });
  expect(target.querySelector("h1")?.textContent).toBe("Desktop notifications");
  expect([...target.querySelectorAll('[role="switch"]')].map((el) => el.getAttribute("aria-label"))).toEqual([
    "Review asked of you",
    "You are mentioned",
    "A build you started or care about failed",
    "A ticket is assigned to you",
    "A source's credential is about to expire",
    "A source cannot sign in",
    "A monitor goes down",
  ]);
  // Prototype-6's words for a failed sign-in, which #1439 took off the
  // expiry's switch because no rejected sign-in fired it; since #1571 one does,
  // under its own switch.
  switchFor("A source cannot sign in").click();
  await settle();
  expect(store.choose).toHaveBeenCalledWith("failed_sign_in", true);
  switchFor("You are mentioned").click();
  await settle();
  expect(store.choose).toHaveBeenCalledWith("mention", false);
  expect(switchFor("You are mentioned").getAttribute("aria-checked")).toBe("false");

  const before = toasts.items.length;
  button("Send a test desktop notification").click();
  await settle();
  expect(p.notify).toHaveBeenCalledWith(expect.objectContaining({ address: "#/settings/notifications" }));
  expect(toasts.items.slice(before).map((toast) => toast.text)).toEqual(["Sent a test desktop notification"]);
});

test("Standup: the target named by its page, and Change… asks with the publish picker", async () => {
  const p = await open("standup");
  expect(p.getEntity).toHaveBeenCalledWith("confluence:98400");
  expect(rowValue("Publish under")).toContain("Payments › Standups");
  button("Change…").click();
  await settle();
  expect(target.querySelector('[role="dialog"] h2')?.textContent).toBe("Publish standups under");
  expect(p.listAdapters).toHaveBeenCalled();
});

test("Standup: a stored page the credential cannot add pages under says why in Change…, and Save waits for another (#2053)", async () => {
  const reason = "This Confluence user cannot add pages to Payments: it lacks the Add page permission";
  const p = sectionPorts({
    listSources: vi.fn(() =>
      Promise.resolve([{ id: "confluence", display_name: "Confluence", adapter_kind: "confluence", enabled: true }] as never),
    ),
    listAdapters: vi.fn(() => Promise.resolve([{ adapter_kind: "confluence", write_ops: ["create_page"] }] as never)),
    refusedWrites: vi.fn(async (asks: WriteAsk[]) => asks.map((ask) => ({ ...ask, reason }))),
  });
  await open("standup", p);
  button("Change…").click();
  await settle();
  const dialog = target.querySelector('[role="dialog"]')!;
  expect(p.refusedWrites).toHaveBeenCalledWith([{ entity: "confluence:98400", op: "create_page" }]);
  expect(dialog.querySelector('[role="alert"]')?.textContent).toBe(reason);
  expect([...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Save")?.disabled).toBe(true);
});

test("Monitoring: a field left with a new value saves both settings", async () => {
  const p = await open("monitoring");
  const slow = target.querySelector<HTMLInputElement>("#tf-mon-slow")!;
  expect(slow.value).toBe("2000");
  slow.value = "1500";
  slow.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.setMonitoringSettings).toHaveBeenCalledWith({ sample_retention_days: 30, response_time_warn_ms: 1500 });
});

test("Backup: the last backup, the schedule and keep, and Back up now", async () => {
  const p = await open("backup");
  expect(rowValue("Last backup")).toBe(`today 02:00, 48 MB, in ${DIR}`);
  const every = target.querySelector<HTMLSelectElement>("#tf-bak-every")!;
  expect([...every.options].map((o) => o.textContent)).toEqual(["Every day at 02:00", "Never"]);
  every.value = "never";
  every.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.setBackupSchedule).toHaveBeenLastCalledWith({ enabled: false, hour: 2, minute: 0, keep: 14 });

  const keep = target.querySelector<HTMLInputElement>("#tf-bak-keep")!;
  keep.value = "7";
  keep.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.setBackupSchedule).toHaveBeenLastCalledWith({ enabled: false, hour: 2, minute: 0, keep: 7 });

  button("Back up now").click();
  await settle();
  expect(p.backupNow).toHaveBeenCalledOnce();
  expect(p.backupStatus).toHaveBeenCalledTimes(2);
});

test("Share ticks map to the export's parts, and a part that does not exist is not drawn", async () => {
  const p = await open("backup");
  const ticks = () => [...target.querySelectorAll(".tf-stg-checks label")].map((el) => el.textContent?.trim());
  // No saved smart list, no ticket view and no link set: no tick for any.
  expect(ticks()).toEqual(["Links", "Contexts", "Assets and routes", "Sources, without passwords", "Notes", "Time"]);
  const notes = [...target.querySelectorAll<HTMLLabelElement>(".tf-stg-checks label")].find((el) =>
    el.textContent?.includes("Notes"),
  )!;
  notes.querySelector("input")!.click();
  flushSync();
  button("Export…").click();
  await settle();
  expect(p.shareExport).toHaveBeenCalledWith({
    links: true,
    contexts: true,
    assets: true,
    sources: true,
    smart_lists: false,
    ticket_views: false,
    link_sets: false,
    notes: true,
    time: false,
  });
  expect(toasts.items.some((toast) => toast.text === `Saved knobas-share-new.knobas to ${DIR}`)).toBe(true);
});

test("a saved smart list draws its tick, on by default", async () => {
  const p = await open("backup", sectionPorts({ smartLists: vi.fn(() => Promise.resolve([{ saved: true }])) }));
  expect([...target.querySelectorAll(".tf-stg-checks label")].map((el) => el.textContent?.trim())).toContain("Smart lists");
  button("Export…").click();
  await settle();
  expect(p.shareExport).toHaveBeenCalledWith(expect.objectContaining({ smart_lists: true }));
});

test("a ticket view draws its tick, on by default (#812)", async () => {
  const p = await open("backup", sectionPorts({ ticketViews: vi.fn(() => Promise.resolve([{ id: "my-work" }])) }));
  expect([...target.querySelectorAll(".tf-stg-checks label")].map((el) => el.textContent?.trim())).toContain("Ticket views");
  button("Export…").click();
  await settle();
  expect(p.shareExport).toHaveBeenCalledWith(expect.objectContaining({ ticket_views: true }));
});

test("a link set draws its tick after Links, on by default, and switching it off sends it off (#870)", async () => {
  const p = await open("backup", sectionPorts({ linkSets: vi.fn(() => Promise.resolve([{ id: "payout-stack" }])) }));
  const labels = () => [...target.querySelectorAll<HTMLLabelElement>(".tf-stg-checks label")];
  expect(labels().map((el) => el.textContent?.trim()).slice(0, 2)).toEqual(["Links", "Link sets"]);
  button("Export…").click();
  await settle();
  expect(p.shareExport).toHaveBeenLastCalledWith(expect.objectContaining({ link_sets: true }));
  labels()
    .find((el) => el.textContent?.trim() === "Link sets")!
    .querySelector("input")!
    .click();
  flushSync();
  button("Export…").click();
  await settle();
  expect(p.shareExport).toHaveBeenLastCalledWith(expect.objectContaining({ link_sets: false }));
});

/** The file names the open dialog lists, one pick each. */
const listedFiles = () =>
  [...target.querySelectorAll<HTMLInputElement>('[role="dialog"] [role="radiogroup"] input[type="radio"]')].map((el) => el.value);

test("Restore from a file… lists the backups and Import… the share exports, each restoring that file", async () => {
  const p = await open("backup");
  button("Restore from a file…").click();
  await settle();
  expect(listedFiles()).toEqual(["knobas-20261015-020000.knobas"]);
  button("Restore").click();
  await settle();
  expect(p.restoreBackup).toHaveBeenCalledWith("knobas-20261015-020000.knobas");
  expect(target.querySelector('[role="dialog"]')).toBeNull();

  button("Import…").click();
  await settle();
  expect(target.querySelector('[role="dialog"] h2')?.textContent).toBe("Import a share export");
  expect(listedFiles()).toEqual(["knobas-share-20261012-101500.knobas"]);
  button("Import").click();
  await settle();
  expect(p.restoreBackup).toHaveBeenLastCalledWith("knobas-share-20261012-101500.knobas");
});

test("a restore puts the sources it brought in the health store the status bar follows (#2059)", async () => {
  // A restore lands only in a knobas holding no data, so no source before it.
  let listed: SourceSummary[] = [];
  const brought: SourceSummary = {
    id: "jira",
    adapter_kind: "jira",
    display_name: "Jira",
    base_url: "https://jira.tidewater.example",
    enabled: true,
    sync_interval_secs: 300,
    config: {},
    health: { source_id: "jira", state: "unknown", checked_at: null, detail: null, secret_expires_at: null },
    last_run: null,
    next_run_at: null,
    item_count: 0,
    auth_kind: "UserPassword",
    kinds: [],
    roles: [],
    role_misses: [],
  };
  const store = createHealth({ credentialHealth: () => Promise.resolve([]), listen: () => Promise.resolve(() => {}) });
  const p = sectionPorts({
    restoreBackup: vi.fn(() => {
      listed = [brought];
      return Promise.resolve();
    }),
  });
  view = mount(TfSettings, {
    target,
    props: {
      rest: ["backup"],
      job: null,
      ready: true,
      ports: { ...sourcePorts, listSources: () => Promise.resolve(listed) },
      sectionPorts: p,
      health: store,
      layers: createLayers(),
      now: () => NOW,
      theme: createTheme(),
    },
  });
  await settle();
  expect(store.all).toEqual([]);
  button("Restore from a file…").click();
  await settle();
  button("Restore").click();
  await settle();
  expect(store.all.map((row) => row.source_id)).toEqual(["jira"]);
});

test("a restore into a knobas that holds data is refused: the dialog stays and says why", async () => {
  const refusal = {
    code: "conflict",
    message: "the database already holds knobas data. Merge-restore is not built yet -- restore into an empty knobas instead",
  };
  const p = await open("backup", sectionPorts({ restoreBackup: vi.fn(() => Promise.reject(refusal)) }));
  // Opening the dialog writes nothing; only a file's own button does.
  button("Restore from a file…").click();
  await settle();
  expect(p.restoreBackup).not.toHaveBeenCalled();
  // The toast store is the app's one, so count only what this press adds.
  const before = toasts.items.length;
  button("Restore").click();
  await settle();
  expect(p.restoreBackup).toHaveBeenCalledOnce();
  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
  expect(toasts.items.slice(before).map((toast) => [toast.tone, toast.text])).toEqual([
    ["err", `Restore refused: ${refusal.message}`],
  ]);
});

/** The section's status with one more archive in the folder, as a later read finds it. */
function statusWith(file: string, share: boolean): BackupStatus {
  const first = status();
  return { ...first, archives: [...first.archives, { file, bytes: 1024, share }] };
}

test("Import… and Restore from a file… read the folder again, so a file put there since the section opened is listed (#1470)", async () => {
  const backupStatus = vi
    .fn<SectionPorts["backupStatus"]>()
    .mockResolvedValueOnce(status())
    .mockResolvedValueOnce(statusWith("knobas-share-20261015-120000.knobas", true))
    .mockResolvedValue(statusWith("knobas-20261015-140000.knobas", false));
  const p = await open("backup", sectionPorts({ backupStatus }));
  expect(p.backupStatus).toHaveBeenCalledOnce();

  button("Import…").click();
  await settle();
  expect(p.backupStatus).toHaveBeenCalledTimes(2);
  expect(listedFiles()).toEqual(["knobas-share-20261012-101500.knobas", "knobas-share-20261015-120000.knobas"]);
  button("Cancel").click();
  flushSync();

  button("Restore from a file…").click();
  await settle();
  expect(p.backupStatus).toHaveBeenCalledTimes(3);
  expect(listedFiles()).toEqual(["knobas-20261015-020000.knobas", "knobas-20261015-140000.knobas"]);
});

test("the dialog lists nothing until the folder is read again, and a failed read is the section's failure line (#1470)", async () => {
  let answer: (next: BackupStatus) => void = () => {};
  const backupStatus = vi
    .fn<SectionPorts["backupStatus"]>()
    .mockResolvedValueOnce(status())
    .mockReturnValueOnce(new Promise((resolve) => (answer = resolve)))
    .mockRejectedValueOnce({ code: "io", message: "permission denied" });
  await open("backup", sectionPorts({ backupStatus }));

  button("Import…").click();
  await settle();
  expect(listedFiles()).toEqual([]);
  expect(target.querySelector('[role="dialog"] .tf-stg-note')?.textContent).toBe("Reading the folder…");
  answer(status());
  await settle();
  expect(listedFiles()).toEqual(["knobas-share-20261012-101500.knobas"]);
  button("Cancel").click();
  flushSync();

  button("Restore from a file…").click();
  await settle();
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  expect(body().querySelector(".tf-stg-errt")?.textContent).toBe("Could not read the backup settings: permission denied");
  expect(button("Try again")).toBeTruthy();
});

test("coming back to the window reads the folder again; leaving the section stops listening (#1470)", async () => {
  const backupStatus = vi
    .fn<SectionPorts["backupStatus"]>()
    .mockResolvedValueOnce(status())
    .mockResolvedValue({ ...status(), last: { taken_at: new Date(2026, 9, 15, 14, 0).toISOString(), file: "knobas-20261015-140000.knobas", bytes: 50 * 1024 * 1024 } });
  const p = await open("backup", sectionPorts({ backupStatus }));
  expect(p.backupStatus).toHaveBeenCalledOnce();

  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(p.backupStatus).toHaveBeenCalledTimes(2);
  expect(rowValue("Last backup")).toBe(`today 14:00, 50 MB, in ${DIR}`);

  unmount(view!);
  view = null;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(p.backupStatus).toHaveBeenCalledTimes(2);
});

test("Open the backup folder: beside Export… and Import…, and in both dialogs (#1471)", async () => {
  const p = await open("backup");
  const opens = () => [...target.querySelectorAll("button")].filter((el) => el.textContent?.trim() === "Open the backup folder");
  expect(opens()).toHaveLength(1);
  expect(opens()[0]!.closest(".tf-stg-sec")?.querySelector("h2")?.textContent).toBe("Share with a colleague");
  opens()[0]!.click();
  await settle();
  expect(p.openBackupFolder).toHaveBeenCalledOnce();
  expect(p.openBackupFolder, "no path: the command opens the running profile's folder (#1704)").toHaveBeenLastCalledWith();

  for (const [press, calls] of [["Restore from a file…", 2], ["Import…", 3]] as const) {
    button(press).click();
    await settle();
    const inDialog = opens().filter((el) => el.closest('[role="dialog"]'));
    expect(inDialog, press).toHaveLength(1);
    inDialog[0]!.click();
    await settle();
    expect(p.openBackupFolder).toHaveBeenCalledTimes(calls);
    button("Cancel").click();
    flushSync();
  }
});

test("Open the backup folder refused: the platform's message in a toast (#1471)", async () => {
  await open("backup", sectionPorts({ openBackupFolder: vi.fn(() => Promise.reject(new Error("not allowed by scope"))) }));
  const before = toasts.items.length;
  button("Open the backup folder").click();
  await settle();
  expect(toasts.items.slice(before).map((toast) => [toast.tone, toast.text])).toEqual([
    ["err", "Could not open the backup folder: not allowed by scope"],
  ]);
});

test("Keyboard lists the fixed shortcuts", async () => {
  await open("keyboard");
  const rows = [...target.querySelectorAll(".tf-stg-keys tr")];
  expect(rows).toHaveLength(12);
  expect([...rows[0]!.querySelectorAll("td")].map((td) => td.textContent?.trim())).toEqual([
    "⌘K",
    "Anywhere",
    "Search everything",
  ]);
  expect([...rows[3]!.querySelectorAll("td:first-child kbd")].map((kbd) => kbd.textContent), "history, #1086").toEqual([
    "⌘[",
    "⌘]",
    "⌘←",
    "⌘→",
  ]);
  expect([...rows[4]!.querySelectorAll("td")].map((td) => td.textContent?.trim()), "refresh, #2156").toEqual([
    "⌘R",
    "A detail or a role view",
    "Refresh, or Sync now",
  ]);
  expect([...rows[11]!.querySelectorAll("td:first-child kbd")].map((kbd) => kbd.textContent)).toEqual(["Enter", "Shift", "Enter"]);
});

test("Logs and database: size, items and version; the folder and the re-index", async () => {
  const p = await open("logs");
  expect(rowValue("Database")).toBe("1.2 GB, 18 400 items");
  expect(rowValue("Version")).toBe("knobas 1.6.0");
  button("Open the log folder").click();
  button("Re-index search").click();
  await settle();
  expect(p.openLogs).toHaveBeenCalledOnce();
  expect(p.reindexFts).toHaveBeenCalledOnce();
});

test("no section reads before the database answers", async () => {
  const p = sectionPorts();
  view = mount(TfSettings, {
    target,
    props: { rest: ["backup"], job: null, ready: false, ports: sourcePorts, sectionPorts: p, layers: createLayers() },
  });
  await settle();
  expect(p.backupStatus).not.toHaveBeenCalled();
  expect(body().querySelector("h1")?.textContent).toBe("Backup and export");
});

// A page keep-alive holds hidden (#2249): `open` mounts Settings inside it.
const page = $state({ shown: true });
const hiddenable = pageContext(() => page.shown);

test("a hidden Settings page does not read the backup folder on focus, and reads it once when shown again (#2249)", async () => {
  const p = await open("backup");
  expect(p.backupStatus).toHaveBeenCalledOnce();
  try {
    page.shown = false;
    flushSync();
    window.dispatchEvent(new Event("focus"));
    window.dispatchEvent(new Event("focus"));
    await settle();
    expect(p.backupStatus).toHaveBeenCalledOnce();

    page.shown = true;
    flushSync();
    await settle();
    expect(p.backupStatus).toHaveBeenCalledTimes(2);
  } finally {
    page.shown = true;
  }
});

/**
 * #2284: a Settings page's actions sit on the right of its title line, as every
 * other page's do (the Controls section's header), never in a row of their own
 * under it. Section rows (a form's Save, a table's Edit…) stay in their section.
 */
const headerActions = () => [...target.querySelectorAll<HTMLElement>(".tf-stg-h .tf-acts .tf-btn")].map((b) => b.textContent!.trim());
const loneRows = () => body().querySelectorAll(":scope > .tf-stg-acts").length;

test.each([
  ["notifications", ["Send a test desktop notification"]],
  ["backup", ["Back up now", "Restore from a file…"]],
  ["logs", ["Open the log folder", "Re-index search"]],
])("Settings › %s puts its actions on the title line", async (section, actions) => {
  await open(section, sectionPorts(), section === "notifications" ? { notifications: notificationsStub([]) } : {});
  expect(headerActions()).toEqual(actions);
  expect(loneRows()).toBe(0);
});

test("a section that could not read puts Try again on the title line", async () => {
  await open("backup", sectionPorts({ backupStatus: vi.fn(() => Promise.reject(new Error("locked"))) }));
  expect(body().textContent).toContain("Could not read the backup settings");
  expect(headerActions()).toEqual(["Try again"]);
  expect(loneRows()).toBe(0);
});

const footerOf = () =>
  [...target.querySelectorAll<HTMLElement>('[role="dialog"] footer .tf-btn')].map((b) => [b.textContent!.trim(), b.className]);

test("Standup's Change… and Backup's Restore from a file… draw Cancel as a ghost (#2284)", async () => {
  await open("standup");
  button("Change…").click();
  await settle();
  expect(footerOf()).toEqual([
    ["Cancel", "tf-btn tf-ghost"],
    ["Save", "tf-btn tf-primary"],
  ]);
  unmount(view!);
  view = null;
  await open("backup");
  button("Restore from a file…").click();
  await settle();
  expect(footerOf()).toEqual([
    ["Open the backup folder", "tf-btn tf-first"],
    ["Cancel", "tf-btn tf-ghost"],
    ["Restore", "tf-btn tf-primary"],
  ]);
});

/** The radio for one listed file in the open dialog. */
const pickFor = (file: string) =>
  target.querySelector<HTMLInputElement>(`[role="dialog"] [role="radiogroup"] input[type="radio"][value="${file}"]`)!;

test("Restore from a file… is a pick and a footer primary: the first file is picked, another can be (#2343)", async () => {
  const p = await open("backup", sectionPorts({ backupStatus: vi.fn(() => Promise.resolve(statusWith("knobas-20261014-020000.knobas", false))) }));
  button("Restore from a file…").click();
  await settle();
  expect(target.querySelector('[role="dialog"] [role="radiogroup"]')?.getAttribute("aria-label")).toBe("Backups");
  expect(listedFiles()).toEqual(["knobas-20261015-020000.knobas", "knobas-20261014-020000.knobas"]);
  expect(pickFor("knobas-20261015-020000.knobas").checked).toBe(true);
  expect(
    [...target.querySelectorAll('[role="dialog"] button')].filter((b) => b.textContent!.trim() === "Restore"),
    "one Restore, the footer's, none per row",
  ).toHaveLength(1);
  pickFor("knobas-20261014-020000.knobas").click();
  flushSync();
  button("Restore").click();
  await settle();
  expect(p.restoreBackup).toHaveBeenCalledExactlyOnceWith("knobas-20261014-020000.knobas");
});

test("Import… names its picks Share exports and its primary Import (#2343)", async () => {
  await open("backup");
  button("Import…").click();
  await settle();
  expect(target.querySelector('[role="dialog"] [role="radiogroup"]')?.getAttribute("aria-label")).toBe("Share exports");
  expect(footerOf().at(-1)).toEqual(["Import", "tf-btn tf-primary"]);
});

test("with nothing in the folder the primary is there and disabled (#2343)", async () => {
  await open("backup", sectionPorts({ backupStatus: vi.fn(() => Promise.resolve({ ...status(), archives: [] })) }));
  button("Restore from a file…").click();
  await settle();
  expect(body().textContent).toContain("No backups there yet.");
  expect(button("Restore").disabled).toBe(true);
});

test("Standup puts a failed read's Try again on the title line (#2284)", async () => {
  await open("standup", sectionPorts({ standupPublishTarget: vi.fn(() => Promise.reject(new Error("locked"))) }));
  expect(headerActions()).toEqual(["Try again"]);
  expect(loneRows()).toBe(0);
});

test.each(["time", "notifications", "standup", "monitoring", "backup", "appearance", "performance", "renderers", "keyboard", "logs", "checkouts"])(
  "Settings › %s draws no action row of its own under the title (#2284)",
  async (section) => {
    await open(section, sectionPorts(), section === "notifications" ? { notifications: notificationsStub([]) } : {});
    expect(body().querySelector("h1")).not.toBeNull();
    expect(loneRows()).toBe(0);
  },
);

/**
 * #2343: "Small: everything inside a page body" (Controls). Every button in a
 * Settings page body is small, except the title line's actions, which are the
 * header's, and a dialog's, which have their own footer rule.
 */
const bodyButtonsAtDefault = () =>
  [...body().querySelectorAll<HTMLElement>(".tf-btn")]
    .filter((b) => !b.closest(".tf-stg-h") && !b.closest('[role="dialog"]') && !b.classList.contains("tf-sm"))
    .map((b) => b.textContent!.trim());

// Renderers and Checkouts draw their bodies in their own files' fixtures, and are checked there.
test.each(["time", "notifications", "standup", "monitoring", "backup", "appearance", "performance", "keyboard", "logs", "notes"])(
  "Settings › %s draws its body buttons small (#2343)",
  async (section) => {
    await open(section, sectionPorts(), section === "notifications" ? { notifications: notificationsStub([]) } : {});
    expect(body().querySelector("h1")).not.toBeNull();
    expect(bodyButtonsAtDefault()).toEqual([]);
  },
);
