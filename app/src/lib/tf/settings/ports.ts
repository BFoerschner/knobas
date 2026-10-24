/**
 * The commands Settings reads and writes, injectable so a component test
 * needs no Tauri bridge: `SettingsPorts` for Sources and a source's page
 * (#830), `SectionPorts` for the other sections (#831). Every one of them
 * already existed, but for `watch_sync`, which #1384 added for the run in
 * flight on a source's page.
 */
import { Channel } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

import { appStatus, type AppStatus } from "../../ipc/app";
import { monitoringSettings, setMonitoringSettings, type MonitoringSettings } from "../../ipc/assets";
import {
  backupNow,
  backupStatus,
  openBackupFolder,
  restoreBackup,
  setBackupSchedule,
  shareExport,
  type BackupRecord,
  type BackupSchedule,
  type BackupStatus,
  type ShareParts,
} from "../../ipc/backup";
import {
  getEntity,
  linkSets,
  notify,
  setStandupPublishTarget,
  standupPublishTarget,
  type EntityDetail,
  type LinkSetRow,
  type PublishTarget,
} from "../../ipc/entity";
import {
  hideSavedView,
  search,
  smartLists,
  ticketViews,
  type SmartListSummary,
  type TicketViewRow,
} from "../../ipc/search";
import { passiveAttribution, setPassiveAttribution } from "../../ipc/time";
import {
  addSource,
  backfillSource,
  dbStats,
  reindexFts,
  deleteSource,
  installAdapter,
  previewInstallAdapter,
  listInstalledAdapters,
  checkAdapterUpdates,
  previewUpdateAdapter,
  updateAdapter,
  uninstallAdapter,
  listAdapters,
  listSources,
  listSyncRuns,
  setSourceSecret,
  syncAll,
  syncNow,
  syncStatus,
  testSource,
  updateSource,
  watchSync,
  type ConnectionReport,
  type DbStats,
  type InstalledAdapter,
  type AdapterPreview,
  type AdapterStatus,
  type UpdatePreview,
  type Removal,
  type CredentialHealth,
  type NewSource,
  type SecretInput,
  type SourceDescriptor,
  type SourceDraft,
  type SourcePatch,
  type SourceSummary,
  type SourceSyncStatus,
  type SyncProgress,
  type SyncRunRow,
} from "../../ipc/sources";

export interface SettingsPorts {
  listSources: () => Promise<SourceSummary[]>;
  listAdapters: () => Promise<SourceDescriptor[]>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  listSyncRuns: (sourceId: string, limit: number) => Promise<SyncRunRow[]>;
  updateSource: (id: string, patch: SourcePatch) => Promise<SourceSummary>;
  backfillSource: (sourceId: string) => Promise<number>;
  deleteSource: (id: string, purgeItems: boolean) => Promise<void>;
  setSourceSecret: (id: string, secret: SecretInput) => Promise<CredentialHealth>;
  testSource: (draft: SourceDraft) => Promise<ConnectionReport>;
  syncNow: (sourceId: string) => Promise<number>;
  /**
   * Watch the source's run in flight (#1384): `onmessage` hears its progress
   * and its ending until `stop` aborts; resolves with its id, or `null` when
   * none is in flight. The channel is built here, so a component test needs
   * no Tauri bridge.
   */
  watchSync: (
    sourceId: string,
    onmessage: (message: SyncProgress) => void,
    stop: AbortSignal,
  ) => Promise<number | null>;
  syncAll: () => Promise<number[]>;
  /** *Add a source* (#903): the secret and the row in one call, as the rooms `AddSource` sent it. */
  addSource: (input: NewSource) => Promise<SourceSummary>;
  /** *Install an adapter* in Add a source (#1202): what it would install, shown before the confirm (#2031). */
  previewInstallAdapter: (manifest: string) => Promise<AdapterPreview>;
  /** The install once confirmed, held to the artifact hash its preview showed. */
  installAdapter: (manifest: string, sha256: string) => Promise<InstalledAdapter>;
  /** *Installed adapters* under Sources (#1209, #1210). */
  listInstalledAdapters: () => Promise<AdapterStatus[]>;
  checkAdapterUpdates: () => Promise<AdapterStatus[]>;
  /** What an *Update* switches to, beside what is installed (#2031). */
  previewUpdateAdapter: (kind: string) => Promise<UpdatePreview>;
  updateAdapter: (kind: string, sha256: string) => Promise<AdapterStatus>;
  uninstallAdapter: (kind: string, removals: Removal[]) => Promise<void>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
  /** Every ticket view (#812): a Jira source's page lists its Jira views (#878). */
  ticketViews: () => Promise<TicketViewRow[]>;
  /** *Hide* or *Show* a Jira view (#878). */
  hideSavedView: (id: string, hidden: boolean) => Promise<void>;
  /** Whether credentials live in memory for this session only (#1585): a source's credential row. */
  appStatus: () => Promise<Pick<AppStatus, "secrets_in_memory">>;
}

/**
 * The real commands, each looked up when it is called rather than when this
 * module loads, so a test that mocks `ipc/sources` with only the functions it
 * needs can still mount the shell.
 */
export const realSettingsPorts: SettingsPorts = {
  listSources: () => listSources(),
  listAdapters: () => listAdapters(),
  syncStatus: () => syncStatus(),
  listSyncRuns: (sourceId, limit) => listSyncRuns(sourceId, limit),
  updateSource: (id, patch) => updateSource(id, patch),
  backfillSource: (sourceId) => backfillSource(sourceId),
  deleteSource: (id, purgeItems) => deleteSource(id, purgeItems),
  setSourceSecret: (id, secret) => setSourceSecret(id, secret),
  testSource: (draft) => testSource(draft),
  syncNow: (sourceId) => syncNow(sourceId),
  watchSync: (sourceId, onmessage, stop) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onmessage;
    // Tauri keeps a channel's callback registered for as long as the window
    // lives, so a page that left would otherwise stay reachable through it.
    // The backend sink stays enrolled until the run ends; what it sends after
    // this lands nowhere.
    stop.addEventListener("abort", () => (channel.onmessage = () => {}), { once: true });
    return watchSync(sourceId, channel);
  },
  syncAll: () => syncAll(),
  addSource: (input) => addSource(input),
  previewInstallAdapter: (manifest) => previewInstallAdapter(manifest),
  installAdapter: (manifest, sha256) => installAdapter(manifest, sha256),
  listInstalledAdapters: () => listInstalledAdapters(),
  checkAdapterUpdates: () => checkAdapterUpdates(),
  previewUpdateAdapter: (kind) => previewUpdateAdapter(kind),
  updateAdapter: (kind, sha256) => updateAdapter(kind, sha256),
  uninstallAdapter: (kind, removals) => uninstallAdapter(kind, removals),
  listen: (event, handler) => tauriListen(event, handler),
  ticketViews: () => ticketViews(),
  hideSavedView: (id, hidden) => hideSavedView(id, hidden),
  appStatus: () => appStatus(),
};

/**
 * The commands the other Settings sections read and write (#831): Time,
 * Notifications' test, Standup, Monitoring, Backup and export, and Logs and
 * database. Every one exists already, the rooms UI's settings sections call
 * them: this ticket adds no command. Notifications' switches go through the
 * one store `inbox/notify.svelte.ts` keeps, not through here.
 */
export interface SectionPorts {
  passiveAttribution: () => Promise<boolean>;
  setPassiveAttribution: (enabled: boolean) => Promise<boolean>;
  notify: (draft: { title: string; body: string; address: string }) => Promise<void>;
  standupPublishTarget: () => Promise<PublishTarget | null>;
  setStandupPublishTarget: (target: PublishTarget) => Promise<PublishTarget>;
  getEntity: (entityId: string) => Promise<Pick<EntityDetail, "row">>;
  search: typeof search;
  listSources: () => Promise<SourceSummary[]>;
  listAdapters: () => Promise<SourceDescriptor[]>;
  monitoringSettings: () => Promise<MonitoringSettings>;
  setMonitoringSettings: (settings: MonitoringSettings) => Promise<MonitoringSettings>;
  backupStatus: () => Promise<BackupStatus>;
  backupNow: () => Promise<BackupRecord>;
  setBackupSchedule: (schedule: BackupSchedule) => Promise<BackupStatus>;
  shareExport: (parts: ShareParts) => Promise<BackupRecord>;
  restoreBackup: (file: string) => Promise<void>;
  smartLists: () => Promise<Pick<SmartListSummary, "saved">[]>;
  /** Whether any ticket view exists, for the share export's tick (#812). */
  ticketViews: () => Promise<Pick<TicketViewRow, "id">[]>;
  /** Whether any link set exists, for the share export's tick (#870). */
  linkSets: () => Promise<Pick<LinkSetRow, "id">[]>;
  appStatus: () => Promise<Pick<AppStatus, "app_version">>;
  dbStats: () => Promise<DbStats>;
  reindexFts: () => Promise<void>;
  openLogs: () => Promise<void>;
  /**
   * Open the profile's backup folder (#1471): the share exports and backups
   * the Restore and Import dialogs list live there, under the hidden
   * `~/Library`. No path: the command opens the running profile's (#1704).
   */
  openBackupFolder: () => Promise<void>;
}

/** The real commands, each looked up when it is called (see above). */
export const realSectionPorts: SectionPorts = {
  passiveAttribution: () => passiveAttribution(),
  setPassiveAttribution: (enabled) => setPassiveAttribution(enabled),
  notify: (draft) => notify(draft),
  standupPublishTarget: () => standupPublishTarget(),
  setStandupPublishTarget: (target) => setStandupPublishTarget(target),
  getEntity: (entityId) => getEntity(entityId),
  search: (query) => search(query),
  listSources: () => listSources(),
  listAdapters: () => listAdapters(),
  monitoringSettings: () => monitoringSettings(),
  setMonitoringSettings: (settings) => setMonitoringSettings(settings),
  backupStatus: () => backupStatus(),
  backupNow: () => backupNow(),
  setBackupSchedule: (schedule) => setBackupSchedule(schedule),
  shareExport: (parts) => shareExport(parts),
  restoreBackup: (file) => restoreBackup(file),
  smartLists: () => smartLists(),
  ticketViews: () => ticketViews(),
  linkSets: () => linkSets(),
  appStatus: () => appStatus(),
  dbStats: () => dbStats(),
  reindexFts: () => reindexFts(),
  // The rooms UI's `LogsSection` (#747): Tauri's path API and the opener,
  // whose `open-path` permission is scoped to `$APPLOG`; loaded on the press.
  openLogs: async () => {
    const { appLogDir } = await import("@tauri-apps/api/path");
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(await appLogDir());
  },
  // The running profile's folder, chosen in Rust (#1704).
  openBackupFolder: () => openBackupFolder(),
};
