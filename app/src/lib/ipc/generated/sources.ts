// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * One parameter of a [`RunBuild`](WriteOp::RunBuild): its full name
 * (`env.SEPA_RETRIES`) and the value this run takes for it. A struct rather
 * than a pair so the wire reads `{"name": .., "value": ..}`, the shape a
 * source's own parameter lists have.
 */
export type BuildParameter = { name: string, value: string, };

/**
 * Which side of a diff a [`ReviewComment`]'s `line` counts on: the file as it
 * was (`old`, a removed or unchanged line's old number) or as it becomes
 * (`new`).
 */
export type ReviewSide = "old" | "new";

/**
 * How a run was started, in the shell's words (#961).
 */
export type Trigger = "change" | "schedule" | "person" | "chain";

/**
 * What the queue will do about a write next.
 *
 * Stored as lowercase text in `knobas.write_queue.state`, whose
 * `write_queue_state_chk` (migration 0005) allows exactly these
 * spellings -- and `ALL` is what the test that pins the two together
 * walks.
 */
export type WriteState = "pending" | "held" | "refused" | "sent" | "discarded";
