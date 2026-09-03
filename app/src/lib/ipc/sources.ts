/**
 * Sources, secrets, credential health, sync and diagnostics — one function per
 * `#[tauri::command]` in `crates/knobas-app/src/commands/sources.rs`.
 *
 * Hand-written, as in M0: `tauri-specta` is still an RC. Argument names are
 * camelCase because Tauri renames command *arguments*; **struct fields keep
 * their Rust snake_case spelling**, which is why the interfaces below are
 * snake_case and the functions are not.
 *
 * There is deliberately no function here that reads a secret back. There is no
 * command behind one either — `sources_crud::nothing_in_the_ipc_surface_reads_a_secret_back`
 * scans the Rust side to keep it that way.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
// Declared once, by the modules that own them. `KindInfo` is `entity.ts`'s and
// `IpcErrorCode` is the barrel's; re-declaring either here would put a second
// copy of a frozen shape in the tree, and the barrel's `export *` would refuse
// to re-export both. Type-only, so nothing is imported at run time and the
// cycle through `./index` is erased.
import type { KindInfo } from "./entity";
import type { IpcErrorCode } from "./index";

/** `knobas_source::AuthMethod` — PascalCase, unchanged since M0. */
export type AuthMethod = "UserPassword" | "Pat" | "ApiToken" | "OAuth";

/**
 * `knobas_source::SourceDescriptor` — what the Add-source form is generated
 * from. A *template* has `id === adapter_kind`; a configured instance's
 * descriptor carries the instance id instead.
 */
export interface SourceDescriptor {
  id: string;
  adapter_kind: string;
  name: string;
  capabilities: string[];
  adapter_version: string;
  auth_methods: AuthMethod[];
  write_ops: string[];
  entity_kinds: KindInfo[];
  /** JSON Schema. The form is generated from it — never hand-built per adapter. */
  config_schema: unknown;
  /**
   * Where this adapter's records keep what knobas reads but the contract does
   * not normalize — a status, a priority, an assignee, requested reviewers, a
   * merged flag, a project (#277). One entry per entity kind that has any of
   * them; a kind that declares nothing is a miss, never a guess.
   */
  payload_paths: KindPaths[];
}

/**
 * `knobas_core::payload::PayloadPath` — the object keys to walk, in order.
 * `["fields", "status", "name"]`.
 */
export type PayloadPath = string[];

/** `knobas_core::payload::ListPath` — where a list of strings lives. */
export interface ListPath {
  /** Where the array is. */
  at: PayloadPath;
  /** Where the string is inside each element; empty means the element itself. */
  entry: PayloadPath;
}

/**
 * `knobas_core::payload::KindPaths` — one entity kind's declared paths.
 *
 * Every path field is a list of *candidates*, most specific first: one
 * adapter's alternative spellings of its own field, never knobas guessing.
 */
export interface KindPaths {
  /** One of the descriptor's `entity_kinds`. */
  kind: string;
  status_name: PayloadPath[];
  priority: PayloadPath[];
  assignee: PayloadPath[];
  reviewers: ListPath[];
  /** A boolean in the payload — a merge timestamp is a different fact. */
  merged: PayloadPath[];
  project_key: PayloadPath[];
  project_name: PayloadPath[];
  /** Status names this source considers blocked-like, in its own spelling. */
  blocked_statuses: string[];
}

/** What one sync run did — `knobas_sync::SyncReport`. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
  /**
   * Rows a **full** sync tombstoned because the run did not see them
   * (hard-delete reconciliation). Counts only kinds that declared
   * `full_sync_exhaustive` and emitted something, so it is 0 for an
   * incremental run, for a kind whose full sync is a bounded window, and for a
   * kind that emitted nothing.
   */
  swept: number;
  /** Opaque to the frontend; hand it back to resume an incremental sync. */
  cursor: string;
}

/**
 * Register the mock source if it is not registered yet, then run a full sync
 * from it. Idempotent: calling it twice leaves one source and the same items.
 */
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}

/**
 * Whether a source's credential works — `knobas_sync::AuthState`.
 *
 * `unauthorized` is the one the UI must act on: it is what puts *Re-enter
 * password* on the row rather than a shrug, and the scheduler never retries it
 * on its own (interfaces §8 P7). `missing_secret` is configured-but-no-secret,
 * which is a different prompt from a secret that was rejected.
 */
export type AuthState =
  | "ok"
  | "unauthorized"
  | "unreachable"
  | "missing_secret"
  | "unknown";

/**
 * One source's credential health — `knobas_sync::CredentialHealth`, the
 * payload of `EVENTS.sourceHealth` and the return of `credentialHealth()`.
 */
export interface CredentialHealth {
  source_id: string;
  state: AuthState;
  /** RFC 3339, or null if it has never been checked. */
  checked_at: string | null;
  /** One line for the sources view. Never a secret. */
  detail: string | null;
  /** RFC 3339 — feeds the PAT expiry countdown. Null if the source will not say. */
  secret_expires_at: string | null;
}

/** How a run ended — `knobas_sync::SyncOutcome`. */
export type SyncOutcome = "ok" | "unauthorized" | "unreachable" | "error";

/**
 * Why a run happened — `knobas_sync::SyncTrigger`, the `trigger` of a
 * `knobas.sync_run` row.
 *
 * The diagnostics list labels a run with it: a scheduled poll, the user's
 * *Sync now*, the initial sync the first-run wizard drew a progress bar for,
 * or a backfill.
 *
 * `backfill` is the one that carries information the others do not. It is a
 * deliberate cursor-less run that widens the payload of items an incremental
 * would never re-fetch, and it is forbidden to reconcile -- so unlike every
 * other cursor-less run it *cannot* have tombstoned anything. When a
 * suspicious `swept` count needs explaining, this spelling is what rules a
 * run out.
 */
export type SyncTrigger = "schedule" | "manual" | "first_run" | "backfill";

/** One row of the sync log — `knobas_sync::run_log::SyncRunRow`. */
export interface SyncRunRow {
  id: number;
  source_id: string;
  trigger: SyncTrigger;
  started_at: string;
  /** null while the run is still going. */
  finished_at: string | null;
  outcome: SyncOutcome | null;
  upserted: number;
  deleted: number;
  /** Rows the full-sync sweep tombstoned. */
  swept: number;
  error: string | null;
  cursor_after: string | null;
}

/**
 * The coarse state one source's syncing is in — `knobas_sync::SourceSyncStatus`,
 * the payload of `EVENTS.syncState` and the return of `syncStatus()`.
 *
 * Coarse by rule: a transition each, at most a handful per run. Per-item
 * progress is {@link SyncProgress} on a channel and nowhere else.
 */
export interface SourceSyncStatus {
  source_id: string;
  running: boolean;
  /**
   * The run this status is about: the one in flight, or — once `running` is
   * false — the last one to finish, which is the run `last_outcome` describes.
   * null only for a source that has never run.
   */
  run_id: number | null;
  /** RFC 3339, while `running`. */
  started_at: string | null;
  /** RFC 3339, of the last run that finished. */
  last_finished_at: string | null;
  last_outcome: SyncOutcome | null;
  /**
   * When the scheduler will run it next: last finish + interval, clamped up by
   * `backoff_until`. Derived, never stored (P7). null while a run is in flight,
   * and for a source that is disabled or needs a human.
   */
  next_run_at: string | null;
  /** Held off until this, after a failure. Never set by a 401 (P7). */
  backoff_until: string | null;
}

/** Where a run is — `knobas_sync::SyncPhase`. */
export type SyncPhase = "started" | "fetching" | "writing" | "finished" | "failed";

/** One progress message — `knobas_sync::SyncProgress`. */
export interface SyncProgress {
  /** `knobas.sync_run.id`, so two runs on two channels stay distinguishable. */
  run_id: number;
  source_id: string;
  phase: SyncPhase;
  items: number;
  elapsed_ms: number;
  message: string | null;
}

/** One row of the sources view — `knobas_app::sources::SourceSummary`. */
export interface SourceSummary {
  id: string;
  adapter_kind: string;
  display_name: string;
  base_url: string;
  enabled: boolean;
  sync_interval_secs: number;
  config: unknown;
  health: CredentialHealth;
  last_run: SyncRunRow | null;
  /** null when the source is disabled, running, or needs a human. */
  next_run_at: string | null;
  item_count: number;
  /**
   * Which kind of credential this source authenticates with.
   *
   * `null` is a source that needs none — the compiled-in mock reaches nothing.
   * The same union `NewSource.auth_kind` submits, widened by `null`: the
   * backend's `AuthKind::None` and an `auth_kind` column this build cannot
   * read both arrive as `null`, and both mean "no credential kind to name".
   *
   * Never a secret. No command reads one back (contract §2.2).
   */
  auth_kind: AuthMethod | null;
  /** From the adapter's descriptor, so the view needs no per-adapter table. */
  kinds: KindInfo[];
}

/** Write-only. The backend has no way to send one back. */
export interface SecretInput {
  value: string;
}

/** What the Add-source form submits — `knobas_app::sources::NewSource`. */
export interface NewSource {
  id: string;
  adapter_kind: string;
  display_name: string;
  base_url: string;
  auth_kind: AuthMethod;
  config: unknown;
  secret: SecretInput;
  sync_interval_secs: number;
  enabled: boolean;
}

/**
 * What *Edit source* may change.
 *
 * No `id`, no `adapter_kind`: the instance id is the entity namespace, baked
 * into every entity id, link and activity row, and therefore immutable (P10).
 */
export interface SourcePatch {
  display_name?: string | null;
  base_url?: string | null;
  config?: unknown;
  sync_interval_secs?: number | null;
  enabled?: boolean | null;
}

/** An unsaved (or saved) source to test — `knobas_app::sources::SourceDraft`. */
export interface SourceDraft {
  /** Set, with `secret: null`, to re-test the stored credential. */
  source_id: string | null;
  adapter_kind: string;
  base_url: string;
  /**
   * Only meaningful for an **unsaved** draft. A draft naming a saved source is
   * tested against that source's *stored* configuration — otherwise *Test
   * connection* could pass against auth the scheduled run never attempts.
   */
  auth_kind: AuthMethod;
  config: unknown;
  secret: SecretInput | null;
}

/** What *Test connection* found — `knobas_app::sources::ConnectionReport`. */
export interface ConnectionReport {
  ok: boolean;
  account: string | null;
  server_version: string | null;
  secret_expires_at: string | null;
  /** One line for the form, or null when it connected. */
  error: string | null;
  /** The class to branch on: `unauthorized` turns *Test* into *Re-enter*. */
  code: IpcErrorCode | null;
  elapsed_ms: number;
}

/** `knobas_sync::stats::SourceCount`. */
export interface SourceCount {
  source_id: string;
  items: number;
  synced_at: string | null;
}

/** `knobas_sync::stats::DbStats` — the diagnostics view's numbers. */
export interface DbStats {
  db_bytes: number;
  entity_count: number;
  item_count: number;
  per_source: SourceCount[];
  oldest_synced_at: string | null;
  newest_synced_at: string | null;
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
 * evidence is in `crates/knobas-app/tests/ipc.rs`.
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
 * What the queue will do about a write next —
 * `knobas_core::write_queue::WriteState`.
 *
 * The panel branches on this and so does the shell badge, so the three open
 * states are three different sentences, not three shades of "waiting":
 *
 * - `pending` — needs **patience**. It goes on its own when the source can
 *   take it.
 * - `held` — needs a **decision**. Its target changed after it was queued, and
 *   nothing moves it but the user. There is no timeout.
 * - `refused` — needs a **decision**. The source rejected the operation, and
 *   it is deliberately never retried.
 * - `sent` / `discarded` — settled. These never appear in
 *   {@link pendingWrites}.
 */
export type WriteState = "pending" | "held" | "refused" | "sent" | "discarded";

/**
 * Why a pending write has not gone yet —
 * `knobas_core::write_queue::WaitReason`.
 *
 * Only the two faults that pass on their own or with a human's help. A refusal
 * is not a reason to wait and has its own {@link WriteState}, so this is
 * `null` on every row that is not `pending` — and on a pending one that has
 * not been tried yet, which `attempted_at` is what distinguishes.
 */
export type WaitReason = "unreachable" | "unauthorized";

/**
 * The serialized `knobas_source::WriteOp` a queued write carries.
 *
 * Externally tagged, so the variant name is the object's one key. **It grows
 * per milestone** (ADR-0006), and this union grows with it — which means a
 * frontend written today will meet a payload it does not recognise. Handle
 * that by narrowing on the key rather than assuming `Comment`: an unrecognised
 * op can still be applied and discarded, and only *editing* needs to know
 * where the words are.
 */
export type WriteOpPayload =
  | { Comment: { entity: string; body: string } }
  | { Transition: { entity: string; status: string } }
  | {
      CreateTicket: {
        entity: string;
        title: string;
        body: string;
        ticket_type: string;
      };
    }
  | { CreateBranch: { entity: string; name: string; from_ref: string } }
  | {
      CreatePullRequest: {
        entity: string;
        title: string;
        body: string;
        head: string;
        base: string;
      };
    }
  | { Approve: { entity: string; body: string } }
  | { TriggerBuild: { entity: string } }
  | { RerunBuild: { entity: string } };

/**
 * One write knobas still owes a source —
 * `knobas_core::write_queue::QueuedWrite`.
 *
 * `CONTEXT.md`'s vocabulary: a row in `pending` is a **pending write**, one in
 * `held` is a **held write**.
 */
export interface QueuedWrite {
  id: number;
  source_id: string;
  /** The target, as an entity id. */
  entity_id: string;
  /** The stable snake_case operation name — `"comment"`. */
  op: string;
  /** The serialized write op; hand it back to {@link amendWrite} edited. */
  payload: WriteOpPayload;
  /**
   * The target as it stood when the write was queued.
   *
   * One half of the two versions a held write is shown with. Its shape is
   * per-op (`knobas_core::write_queue::project`); for `"comment"` it carries
   * `{op, live, text}`, where `text` is the item's indexed text — which every
   * adapter builds from its title, description and comment bodies, so a new
   * reply is exactly what changes it.
   */
  target_snapshot: unknown;
  state: WriteState;
  wait_reason: WaitReason | null;
  /** What the source said, in its own words. Untrusted: render it as text. */
  detail: string | null;
  queued_at: string;
  attempted_at: string | null;
  attempts: number;
  /**
   * The target as it stood when the write was held — the other half of the two
   * versions. `null` until a write is held; kept afterwards.
   */
  held_snapshot: unknown | null;
  settled_at: string | null;
  /**
   * Whether the user has this write's source turned on, as of this read —
   * derived by the backend, never stored (issue #204).
   *
   * A disabled source's items leave `sync.live_item` (migration 0012), so its
   * writes go `held` by the same mechanism as a withdrawn target's. The facts
   * are different and so are the remedies: a target that changed wants the
   * two-versions choice, a source that is off wants re-enabling — say which,
   * and never offer *Send mine anyway* for a source that cannot take it.
   * Flips on the next read once the source is back on; `true` for a source
   * that was never configured.
   */
  source_enabled: boolean;
}

/**
 * How many writes are in each open state —
 * `knobas_core::write_queue::QueueCounts`.
 *
 * Three numbers rather than one, deliberately: a single total would let
 * "3 waiting" absorb a write that needs a decision, which is the one thing the
 * shell badge exists to prevent.
 */
export interface QueueCounts {
  pending: number;
  held: number;
  refused: number;
}

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
