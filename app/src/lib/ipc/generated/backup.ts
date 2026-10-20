// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * One archive sitting in the backup directory.
 */
export type ArchiveFile = { file: string, bytes: number, 
/**
 * Whether this is a **share export** rather than a nightly backup
 * (#455). The two live in the same directory and restore through the
 * same button, and they are not the same thing: one is this profile
 * backed up, the other is a curated cut of it made on purpose to hand to
 * somebody, which retention never deletes. The settings view lists them
 * apart, and this is what it reads to do it.
 *
 * On the row rather than re-derived on the far side, because the naming
 * rule is [`policy`]'s: a frontend that told them apart by looking for
 * `knobas-share-` would be a second place to keep
 * [`policy::share_name`] right.
 */
share: boolean, };

/**
 * What one successful export produced.
 */
export type BackupRecord = { 
/**
 * RFC 3339, UTC.
 */
taken_at: string, 
/**
 * The file name inside `BackupStatus.directory`, not a path: the directory
 * moves with the profile, and a stored absolute path would outlive it.
 */
file: string, bytes: number, };

/**
 * The nightly export schedule (spec §14: "scheduled automatic exports as
 * backups", ratified nightly in #38).
 *
 * `Deserialize` because [`set_backup_schedule`](crate::commands::backup) takes
 * one from the settings dialog (issue #69); `Serialize` because it is stored as JSON in
 * `knobas.setting` and read back by [`backup_status`](crate::commands::backup).
 */
export type BackupSchedule = { 
/**
 * Whether the nightly export runs at all. *Export now* works either way.
 */
enabled: boolean, 
/**
 * Local hour, 0-23.
 */
hour: number, 
/**
 * Local minute, 0-59.
 */
minute: number, 
/**
 * How many archives to keep (ratified 2026-08-28, default seven). The
 * oldest beyond this are deleted after a successful export.
 */
keep: number, };

/**
 * Everything the backup settings dialog draws (issue #69).
 */
export type BackupStatus = { schedule: BackupSchedule, 
/**
 * Absolute path of the directory the archives live in.
 */
directory: string, last: BackupRecord | null, 
/**
 * RFC 3339, UTC; `null` when the schedule is off.
 */
next_due_at: string | null, 
/**
 * What is on disk right now, newest first.
 */
archives: Array<ArchiveFile>, };

/**
 * Which parts of the owned schema a share export carries.
 *
 * A backup is everything knobas owns; a **share export** is the same archive
 * restricted to the parts a colleague should get.
 *
 * `knobas.setting` is in no part, so a share export never carries the
 * sharer's schedule, first-run state or preferences: `backup::share`'s module
 * docs say why "the time settings" could not be one of them.
 *
 * The defaults are the spec's: links, assets, contexts and sources on; notes
 * and time off. The shell sends every toggle; the backend still decodes a
 * payload that leaves some out (`#[serde(default)]`), giving the ratified
 * answer for the rest; and a knobas built before a future part still
 * decodes a payload that names it.
 */
export type ShareParts = { 
/**
 * `knobas.link` and the entity rows its ends name.
 */
links: boolean, 
/**
 * `knobas.asset` and `knobas.route` -- the estate.
 */
assets: boolean, 
/**
 * `knobas.context` -- the rooms, and what each one is anchored to.
 */
contexts: boolean, 
/**
 * `knobas.note`. **Off by default**: a note is the private half of what
 * knobas holds.
 */
notes: boolean, 
/**
 * The timer, its blocks, the worklogs they became and the observations
 * behind them. **Off by default**, for the reason notes are: a colleague
 * reading a link map has no use for somebody's hours.
 */
time: boolean, 
/**
 * `knobas.source_config` -- which systems knobas talks to, and how. No
 * secret is in it: spec §14 puts every credential in the OS keychain and
 * nothing secret ever reaches Postgres.
 */
sources: boolean, 
/**
 * `knobas.smart_list` -- the launcher searches somebody saved (#506).
 * **On by default**, with links: a saved list is a query over the link
 * map, not a private note about it. The built-in lists are code and are
 * rows nowhere, so the dialog offers no toggle for it until a saved one
 * exists, and sends the part off while there is none: the archive of a
 * knobas nobody has saved a list on carries no list table at all (#507).
 *
 * The query crosses exactly as it was typed: nothing on either side of an
 * archive rewrites a stored query to a newer grammar.
 */
smart_lists: boolean, 
/**
 * `knobas.ticket_view` -- the ticket views somebody made (#812, story
 * 41). **On by default**, beside smart lists and for their reason: a view
 * is a query and a layout, not a private note. Like a saved list it is no
 * entity, so it brings no address book, and its text crosses exactly as
 * it was saved. The dialog offers its toggle only once a view exists and
 * sends it off while there is none, as with smart lists.
 */
ticket_views: boolean, 
/**
 * `knobas.link_set`, `link_set_member` and `link_set_rule` -- the link
 * sets somebody made (#870, R-7.11). **On by default**, beside ticket
 * views and for their reason: a set is a named group of items and the
 * rules that show it, not a private note. It brings no address book: a
 * member and a rule name an item by id with no foreign key (migration
 * `0039`), so it resolves once the colleague's own sources sync that
 * item. A pattern crosses as saved, its query and its reading together.
 * `knobas.link_set_hidden` stays behind: *Hide here* is one reader's
 * choice about one ticket, as a note is. The dialog offers its toggle
 * only once a set exists and sends it off while there is none, as with
 * smart lists.
 */
link_sets: boolean, };
