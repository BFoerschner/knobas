/**
 * Backup export and restore — `crates/knobas-app/src/commands/backup.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a field or a command name added on one side only
 * fails `cargo test`, not merely `svelte-check`.
 *
 * A backup is everything in the `knobas` schema, notes included; the synced
 * mirror is left out because it re-syncs (design §16.12, ratified).
 *
 * Called from the settings view's backup section (`#/settings`, issue #69,
 * landed in PR #102), which also drives the share export of M4.2 (#454).
 */
import { invoke } from "@tauri-apps/api/core";

/** The nightly export schedule — `backup::policy::BackupSchedule`. */
export interface BackupSchedule {
  /** Whether the nightly export runs. *Export now* works either way. */
  enabled: boolean;
  /** Local hour, 0–23. */
  hour: number;
  /** Local minute, 0–59. */
  minute: number;
  /** How many archives to keep; the oldest beyond this are deleted. */
  keep: number;
}

/** One successful export — `backup::BackupRecord`. */
export interface BackupRecord {
  /** RFC 3339, UTC. */
  taken_at: string;
  /** The file name inside {@link BackupStatus.directory}, not a path. */
  file: string;
  bytes: number;
}

/**
 * Which parts a share export carries — `backup::share::ShareParts`.
 *
 * A backup is everything knobas owns; a **share export** is the same archive
 * restricted to the parts a colleague should get. Links, assets, contexts and
 * sources are on by default; notes and time are off, because a colleague
 * reading a link map has no use for somebody's notes or hours.
 *
 * `knobas.setting` is in no part, so a share export never carries the sharer's
 * schedule, first-run state or preferences — see the Rust module's docs for
 * why "the time settings" could not be one of them.
 */
export interface ShareParts {
  /** Links, and the entity rows their ends name. */
  links: boolean;
  /** Assets and the routes they expose. */
  assets: boolean;
  /** Contexts, and what each is anchored to. */
  contexts: boolean;
  /** Off by default. */
  notes: boolean;
  /** The timer, its blocks, the worklogs and the observations. Off by default. */
  time: boolean;
  /** Source configurations. No secret is in one — credentials live in the OS keychain. */
  sources: boolean;
}

/** The ratified defaults, so a caller can start from them and toggle. */
export const shareDefaults: ShareParts = {
  links: true,
  assets: true,
  contexts: true,
  notes: false,
  time: false,
  sources: true,
};

/** One archive on disk — `backup::ArchiveFile`. */
export interface ArchiveFile {
  file: string;
  bytes: number;
  /**
   * Whether this is a share export rather than a nightly backup.
   *
   * The name carries it too (`knobas-share-…`), and this is what the section
   * lists on: the naming rule belongs to `backup::policy`, and a frontend
   * that parsed the file name would be a second place to keep it right.
   */
  share: boolean;
}

/** Everything the backup settings dialog (#69) draws — `backup::BackupStatus`. */
export interface BackupStatus {
  schedule: BackupSchedule;
  /** Absolute path of the directory the archives live in. */
  directory: string;
  last: BackupRecord | null;
  /** `null` when the schedule is off. */
  next_due_at: string | null;
  /** What is on disk right now, newest first. */
  archives: ArchiveFile[];
}

/** The schedule, the last export, and what is on disk. */
export function backupStatus(): Promise<BackupStatus> {
  return invoke<BackupStatus>("backup_status");
}

/** *Export now* — take a backup whatever the schedule says. */
export function backupNow(): Promise<BackupRecord> {
  return invoke<BackupRecord>("backup_now");
}

/**
 * *Share export* — an archive restricted to `parts`.
 *
 * Written into the same directory as the backups, under
 * `knobas-share-<stamp>.knobas`: retention never deletes it, it does not count
 * as the last backup, and {@link restoreBackup} reads it like any other
 * archive. Rejects with `invalid` when every part is switched off.
 */
export function shareExport(parts: ShareParts): Promise<BackupRecord> {
  return invoke<BackupRecord>("share_export", { parts });
}

/** Change the nightly schedule; answers with the redrawn status. */
export function setBackupSchedule(schedule: BackupSchedule): Promise<BackupStatus> {
  return invoke<BackupStatus>("set_backup_schedule", { schedule });
}

/**
 * Restore one of this profile's archives.
 *
 * Minimal by ratification: it restores into a knobas that holds no data yet.
 * Restoring over a populated database rejects with `conflict` — merging an
 * archive into an existing corpus, with a preview, is M4.
 */
export function restoreBackup(file: string): Promise<void> {
  return invoke<void>("restore_backup", { file });
}
