/**
 * The commands Settings › Sources reads and writes (#830), injectable so a
 * component test needs no Tauri bridge. Every one of them already exists:
 * this ticket adds no command.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import {
  backfillSource,
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
  type CredentialHealth,
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
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
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
  listen: (event, handler) => tauriListen(event, handler),
};
