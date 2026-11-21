// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * Why a copy is updated by hand.
 */
export type ByHandWhy = "translocated" | "package" | "development";

/**
 * Which versions knobas offers.
 */
export type Channel = "releases" | "pre_releases";

/**
 * Why the last check found nothing to say.
 */
export type CheckFailed = { kind: CheckFailedKind, message: string, at: string, };

/**
 * The three ways a check fails.
 */
export type CheckFailedKind = "offline" | "rate_limited" | "bad_answer";

/**
 * How this copy was installed: which decides whether it can replace itself.
 */
export type InstallKind = "nsis" | "msi" | "app" | "app_image" | "deb" | "rpm";

/**
 * The last update on this computer.
 */
export type LastUpdate = { from: string, to: string, at: string, installed: boolean, backup: string | null, };

/**
 * A newer version than this one.
 */
export type Offer = { version: string, pre_release: boolean, published_at: string | null, 
/**
 * This copy's artifact's size.
 */
bytes: number | null, 
/**
 * Whether it migrates the database, from the manifest's `knobas.schema`.
 */
changes_schema: boolean | null, page_url: string, };

/**
 * Where the updater is.
 */
export type Phase = { "phase": "idle" } | { "phase": "checking" } | { "phase": "downloading", received: number, total: number | null, } | { "phase": "ready" } | { "phase": "installing", step: Step, } | { "phase": "failed", stage: Stage, message: string, 
/**
 * How far a download got.
 */
received: number | null, };

/**
 * The channel's newest version.
 */
export type Seen = { version: string, pre_release: boolean, published_at: string | null, page_url: string, };

/**
 * Where a failure happened.
 */
export type Stage = "download" | "signature" | "backup" | "install";

/**
 * Whether this copy replaces itself, and how.
 */
export type Standing = { "standing": "in_place", kind: InstallKind, } | { "standing": "by_hand", why: ByHandWhy, kind: InstallKind | null, };

/**
 * The steps of an install, each said on the page.
 */
export type Step = "checking" | "backing_up" | "replacing" | "closing";

/**
 * A release's notes.
 */
export type UpdateNotes = { version: string, 
/**
 * The notes' markdown, or `None` when GitHub could not be asked.
 */
markdown: string | null, page_url: string, };

/**
 * What the page and the cog read: everything known, from memory.
 */
export type UpdateState = { 
/**
 * The running version, `3.2.9-ux27`.
 */
current: string, current_is_pre_release: boolean, 
/**
 * The greatest migration this build embeds.
 */
current_schema: number, 
/**
 * Whether this build trusts any signing key. Until Björn's two keys are committed it trusts
 * none, and the page says updates aren't set up: nothing is checked or downloaded.
 */
set_up: boolean, 
/**
 * *Check for updates*.
 */
check: boolean, channel: Channel, standing: Standing, 
/**
 * False in a debug build, the demo profile and under `KNOBAS_UPDATE=off`.
 */
checks_itself: boolean, checked_at: string | null, next_check_at: string | null, check_failed: CheckFailed | null, 
/**
 * The channel's newest version, also when it is not newer.
 */
newest: Seen | null, 
/**
 * Set only when `newest` is newer than `current`.
 */
offer: Offer | null, phase: Phase, 
/**
 * Whether the cog carries the dot.
 */
marked: boolean, last: LastUpdate | null, 
/**
 * The update that went in on this start: the toast says so once.
 */
just_updated: boolean, };
