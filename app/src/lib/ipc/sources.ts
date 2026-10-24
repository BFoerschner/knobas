/**
 * Sources, secrets, credential health, sync and diagnostics — one function per
 * `#[tauri::command]` in `crates/knobas-app/src/commands/sources.rs`.
 *
 * The types are generated from the Rust (`./generated/sources.ts`, contract
 * §2.6 as amended by #1325): a field is explained in its Rust doc comment and
 * nowhere else. The `invoke` functions stay hand-written, pinned to the Rust
 * by `commands::sources`'s barrel test. Argument names are camelCase because
 * Tauri renames command *arguments*; **struct fields keep their Rust
 * snake_case spelling**, which is why the types are snake_case and the
 * functions are not.
 *
 * There is deliberately no function here that reads a secret back. There is no
 * command behind one either — `sources_crud::nothing_in_the_ipc_surface_reads_a_secret_back`
 * scans the Rust side to keep it that way.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AdapterPreview,
  AdapterStatus,
  ConnectionReport,
  CredentialHealth,
  DbStats,
  InstalledAdapter,
  NewSource,
  QueueCounts,
  QueuedWrite,
  Removal,
  SecretInput,
  SourceDescriptor,
  SourceDraft,
  SourcePatch,
  SourceSummary,
  SourceSyncStatus,
  SyncProgress,
  SyncReport,
  SyncRunRow,
  UpdatePreview,
  WriteOpPayload,
} from "./generated/sources";

export type {
  AdapterLimits,
  AdapterPreview,
  AdapterSource,
  AdapterStatus,
  AuthMethod,
  AuthState,
  BuildParameter,
  CloneFrom,
  ConnectionReport,
  CredentialHealth,
  DbStats,
  Frame,
  InstalledAdapter,
  IpcErrorCode,
  KindPaths,
  ListPath,
  MergeStyle,
  MonitorKind,
  NewSource,
  ParentPath,
  PayloadPath,
  ProjectPath,
  PullRequestState,
  QueueCounts,
  QueuedWrite,
  Removal,
  ReviewComment,
  ReviewSide,
  ReviewVerdict,
  RoleClaim,
  RoleMiss,
  SecretAccount,
  SecretInput,
  SourceCount,
  SourceDescriptor,
  SourceDraft,
  SourcePatch,
  SourceSummary,
  SourceSyncStatus,
  StatePath,
  StateValue,
  SyncOutcome,
  SyncPhase,
  SyncProgress,
  SyncReport,
  SyncRunRow,
  SyncTrigger,
  Trigger,
  TriggerPath,
  TriggerValue,
  UpdateCheck,
  UpdatePreview,
  UpdateStyle,
  WaitReason,
  WriteOpPayload,
  WriteState,
} from "./generated/sources";

/**
 * Register the mock source if it is not registered yet, then run a full sync
 * from it. Idempotent: calling it twice leaves one source and the same items.
 */
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}

// -- §2.2: sources, secrets, credential health --------------------------------

/**
 * One descriptor template per compiled-in adapter kind (`id === adapter_kind`).
 *
 * Answers before the database is up — nothing is instantiated and no keychain
 * is touched — so the Add-source form is drawable on a cold start.
 */
export function listAdapters(): Promise<SourceDescriptor[]> {
  return invoke<SourceDescriptor[]>("list_adapters");
}

/**
 * What installing from `manifest` would install (#2031), every check run
 * and nothing written: a GitHub repository URL (its latest release), a
 * manifest URL, or a local `knobas-adapter.json`. A refused install rejects
 * with `invalid` and the check's reason; a kind already taken with
 * `conflict`; a release nobody answers for with `unreachable`.
 */
export function previewInstallAdapter(manifest: string): Promise<AdapterPreview> {
  return invoke<AdapterPreview>("preview_install_adapter", { manifest });
}

/**
 * Install an external adapter (#1202, #1209) once the reader confirmed its
 * preview: `sha256` is the preview's `confirm_sha256`, over the artifact,
 * hosts and limits it showed, and a release it does not match rejects with
 * `invalid`. Refuses as the preview refuses.
 */
export function installAdapter(manifest: string, sha256: string): Promise<InstalledAdapter> {
  return invoke<InstalledAdapter>("install_adapter", { manifest, sha256 });
}

export function listInstalledAdapters(): Promise<AdapterStatus[]> {
  return invoke<AdapterStatus[]>("list_installed_adapters");
}

/** Check every installed adapter for a newer release; changes no install. */
export function checkAdapterUpdates(): Promise<AdapterStatus[]> {
  return invoke<AdapterStatus[]>("check_adapter_updates");
}

/**
 * The latest release beside what is installed (#2031), every check run and
 * nothing switched. A refusal rejects with `invalid` and is recorded on the
 * row as an update's is.
 */
export function previewUpdateAdapter(kind: string): Promise<UpdatePreview> {
  return invoke<UpdatePreview>("preview_update_adapter", { kind });
}

/**
 * Update to the latest release after every static check, held to the
 * release its preview showed (`sha256`, the release's `confirm_sha256`). A refusal rejects with
 * `invalid` and its reason, and the installed version stays.
 */
export function updateAdapter(kind: string, sha256: string): Promise<AdapterStatus> {
  return invoke<AdapterStatus>("update_adapter", { kind, sha256 });
}

/**
 * Uninstall, with one answer per source of the adapter (`invalid` when one
 * is missing): each source removed as `deleteSource` removes it.
 */
export function uninstallAdapter(kind: string, removals: Removal[]): Promise<void> {
  return invoke<void>("uninstall_adapter", { kind, removals });
}

export function listSources(): Promise<SourceSummary[]> {
  return invoke<SourceSummary[]>("list_sources");
}

export function addSource(input: NewSource): Promise<SourceSummary> {
  return invoke<SourceSummary>("add_source", { input });
}

export function updateSource(id: string, patch: SourcePatch): Promise<SourceSummary> {
  return invoke<SourceSummary>("update_source", { id, patch });
}

export function deleteSource(id: string, purgeItems: boolean): Promise<void> {
  return invoke<void>("delete_source", { id, purgeItems });
}

/**
 * Store a credential for a saved source and test it. Write-only: nothing reads
 * one back.
 */
export function setSourceSecret(id: string, secret: SecretInput): Promise<CredentialHealth> {
  return invoke<CredentialHealth>("set_source_secret", { id, secret });
}

/** *Test connection*. Writes nothing — not to Postgres, not to the keychain. */
export function testSource(draft: SourceDraft): Promise<ConnectionReport> {
  return invoke<ConnectionReport>("test_source", { draft });
}

export function credentialHealth(): Promise<CredentialHealth[]> {
  return invoke<CredentialHealth[]>("credential_health");
}

// -- §2.3: sync, status, the run log, diagnostics ------------------------------

/**
 * Start a sync of one configured source; resolves with its `sync_run.id` as
 * soon as the run is recorded, **not** when it finishes — watch
 * `EVENTS.syncState` for that. Triggering a source that is already syncing
 * returns the run already in flight.
 *
 * Use {@link syncNowWithProgress} when you are drawing per-item progress.
 * There are two functions rather than one optional argument because
 * `Option<Channel<_>>` is not a valid Tauri command argument (`Channel` has no
 * `Deserialize` impl); the Rust side splits for the same reason, and the
 * evidence is in `crates/knobas-app/tests/it/ipc.rs`.
 */
export function syncNow(sourceId: string): Promise<number> {
  return invoke<number>("sync_now", { sourceId });
}

/**
 * {@link syncNow}, reporting per-item progress on `progress`.
 *
 * The channel is required. A caller that only needs to know a run started
 * should call {@link syncNow} and listen to `EVENTS.syncState`: per-item
 * progress goes on the channel and nowhere else, and events carry coarse state
 * only.
 *
 * **This is the first-run wizard's command**, and it asks the scheduler for
 * *the source's first sync* rather than for a sync (ADR-0005). Adding a source
 * already wakes the scheduler, so on a fast source that first sync can be over
 * before this call lands; when it is, the run that already happened comes back
 * with its ending on the channel instead of a second run being started over a
 * corpus that is already mirrored. Calling it again — the wizard's *Retry* —
 * gets a run. Whatever happens, an ending arrives on the channel: there is no
 * interleaving in which it is safe to add a timeout here.
 */
export function syncNowWithProgress(
  sourceId: string,
  progress: Channel<SyncProgress>,
): Promise<number> {
  return invoke<number>("sync_now_with_progress", { sourceId, progress });
}

/**
 * Watch the source's run in flight on `progress`, whoever started it (#1384).
 * Resolves with that run's id, after which the channel hears the run's
 * remaining progress and its ending; resolves with `null` on an idle source,
 * which starts nothing and sends nothing.
 *
 * This is how a page that did not start a run draws it: per-item progress
 * goes on a channel and nowhere else, so a page that opens on a scheduled run
 * joins it here. There is no detach: the channel is enrolled until the run
 * ends, and a page that leaves stops listening to it.
 */
export function watchSync(sourceId: string, progress: Channel<SyncProgress>): Promise<number | null> {
  return invoke<number | null>("watch_sync", { sourceId, progress });
}

/**
 * **Backfill** one source: re-read it from the top and rewrite every mirrored
 * item, ignoring the position it has stored. Same shape as {@link syncNow} —
 * the run id resolves immediately and the run is watched on
 * `EVENTS.syncState` — and the same in-flight rule, so pressing it twice does
 * not start two full re-reads.
 *
 * What it is for: a *payload widening*. A scheduled sync re-fetches what
 * changed upstream, and widening an adapter's field list changes nothing
 * upstream, so an item nobody has touched keeps the narrower record for ever.
 * This is the only thing that reaches it.
 *
 * It is the longest run a source ever does, and it deliberately tombstones
 * nothing — the reasoning is on `knobas_sync::run_backfill`. That is also why
 * it appears in {@link listSyncRuns} under its own trigger, `"backfill"`,
 * rather than as a `"manual"` run: it is the one run whose `swept` is always
 * zero, so the log has to say which run it was.
 */
export function backfillSource(sourceId: string): Promise<number> {
  return invoke<number>("backfill_source", { sourceId });
}

/**
 * Start a sync for every enabled source that does not need a human, in id
 * order. Returns one run id per source it started.
 */
export function syncAll(): Promise<number[]> {
  return invoke<number[]>("sync_all");
}

/**
 * What every source is doing, and when it goes next.
 *
 * The authoritative read: `EVENTS.syncState` is a hint that something moved and
 * may be missed while the webview is still mounting. Call this on mount.
 */
export function syncStatus(): Promise<SourceSyncStatus[]> {
  return invoke<SourceSyncStatus[]>("sync_status");
}

/**
 * The per-source sync log the diagnostics view reads: errors, durations
 * (`finished_at - started_at`), item counts. `sourceId: null` spans every
 * source. `limit` is clamped to 500.
 */
export function listSyncRuns(sourceId: string | null, limit: number): Promise<SyncRunRow[]> {
  return invoke<SyncRunRow[]>("list_sync_runs", { sourceId, limit });
}

export function dbStats(): Promise<DbStats> {
  return invoke<DbStats>("db_stats");
}

/** Rebuild the FTS index. Concurrent — searches keep working meanwhile. */
export function reindexFts(): Promise<void> {
  return invoke<void>("reindex_fts");
}

// -- The write queue (issue #42) ----------------------------------------------

/**
 * Every write knobas still owes a source, newest first — pending, held and
 * refused alike.
 *
 * The authoritative read. There is no `write:*` event: every queue transition
 * already writes an activity line, so refresh on `EVENTS.activityNew` rather
 * than waiting for a second channel that would say the same thing.
 */
export function pendingWrites(): Promise<QueuedWrite[]> {
  return invoke<QueuedWrite[]>("pending_writes");
}

/** The counts the shell badge shows. */
export function writeQueueCounts(): Promise<QueueCounts> {
  return invoke<QueueCounts>("write_queue_counts");
}

/**
 * Flush the queue now — one source, or every source when `sourceId` is `null`.
 *
 * Impatience rather than necessity: the scheduler's own tick already does
 * this. It therefore does not fail for a source that is still down — the write
 * stays queued with its reason updated.
 *
 * **It cannot release a held write.** A flush decides nothing on the user's
 * behalf; {@link applyHeldWrite} is the only thing that moves one.
 */
export function flushWrites(sourceId: string | null): Promise<void> {
  return invoke<void>("flush_writes", { sourceId });
}

/**
 * *I know, and I still mean it*: release a held write and send it.
 *
 * The version the user was shown becomes the version the write is measured
 * against. A change arriving **after** they looked holds it again — what they
 * consented to overwrite is what they saw.
 *
 * Rejects with `conflict` if the write is no longer held.
 */
export function applyHeldWrite(id: number): Promise<void> {
  return invoke<void>("apply_held_write", { id });
}

/**
 * *Edit and send*: replace a held or refused write's payload with one the user
 * has just written.
 *
 * `payload` is the row's own {@link QueuedWrite.payload}, edited. It may not
 * name a different operation or a different target — that is a *new* write,
 * because a queued one holds a place in its entity's queue and a snapshot of
 * that entity. Both refusals arrive as `invalid`.
 */
export function amendWrite(id: number, payload: WriteOpPayload): Promise<void> {
  return invoke<void>("amend_write", { id, payload });
}

/**
 * Withdraw a write — cancelling a pending one and conceding a held one are the
 * same act on the same row. The row is kept, tombstoned.
 *
 * Rejects with `conflict` if there was nothing open left to withdraw.
 */
export function discardWrite(id: number): Promise<void> {
  return invoke<void>("discard_write", { id });
}
