/**
 * The commands Settings reads and writes, injectable so a component test
 * needs no Tauri bridge: `SettingsPorts` for Sources and a source's page
 * (#830), `SectionPorts` for the other sections (#831). Every one of them
 * already exists: neither ticket adds a command.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { appStatus, type AppStatus } from "../../ipc/app";
import { monitoringSettings, setMonitoringSettings, type MonitoringSettings } from "../../ipc/assets";
import {
  backupNow,
  backupStatus,
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
  hideJiraView,
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
  listAdapters,
  listSources,
  listSyncRuns,
  setSourceSecret,
  syncAll,
  syncNow,
  syncStatus,
  testSource,
  updateSource,
  type ConnectionReport,
  type DbStats,
  type CredentialHealth,
  type NewSource,
  type SecretInput,
  type SourceDescriptor,
  type SourceDraft,
  type SourcePatch,
  type SourceSummary,
  type SourceSyncStatus,
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
  syncAll: () => Promise<number[]>;
  /** *Add a source* (#903): the secret and the row in one call, as `AddSource` sends it. */
  addSource: (input: NewSource) => Promise<SourceSummary>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
  /** Every ticket view (#812): a Jira source's page lists its Jira views (#878). */
  ticketViews: () => Promise<TicketViewRow[]>;
  /** *Hide* or *Show* a Jira view (#878). */
  hideJiraView: (id: string, hidden: boolean) => Promise<void>;
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
  syncAll: () => syncAll(),
  addSource: (input) => addSource(input),
  listen: (event, handler) => tauriListen(event, handler),
  ticketViews: () => ticketViews(),
  hideJiraView: (id, hidden) => hideJiraView(id, hidden),
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
};
