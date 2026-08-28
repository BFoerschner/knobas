/**
 * Backup export and restore — `crates/knobas-app/src/commands/backup.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a field or a command name added on one side only
 * fails `cargo test`, not merely `svelte-check`.
 *
 * A backup is everything in the `knobas` schema, notes included; the synced
 * mirror is left out because it re-syncs (design §16.12, ratified).
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

/** One archive on disk — `backup::ArchiveFile`. */
export interface ArchiveFile {
  file: string;
  bytes: number;
}

/** Everything the backup settings dialog draws — `backup::BackupStatus`. */
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
