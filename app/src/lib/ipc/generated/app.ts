// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * What the shell needs to decide what to draw.
 */
export type AppStatus = { db: DbState, 
/**
 * No source configured and the first run was never completed (§14a).
 */
first_run: boolean, 
/**
 * The `--demo` profile: its own data dir, port and keychain suffix (P13).
 */
demo: boolean, source_count: number, app_version: string, 
/**
 * Credentials live in this process's memory, not the OS keychain, and
 * are gone when it exits: `KNOBAS_SECRET_STORE=memory`, the headless-QA
 * store (#1585). A source's page says so instead of naming the keychain.
 */
secrets_in_memory: boolean, };

/**
 * What a renderer writes on stdout: SVG or HTML (drawn sanitized, in a
 * sandboxed frame), or an image's bytes.
 */
export type CommandOutput = "svg" | "html" | "image";

/**
 * How far the database has got.
 *
 * Interfaces §2.1. Tagged on `state`, so TypeScript reads it as a
 * discriminated union and `detail` and `message` are reachable only after
 * the check that identifies which state this is.
 *
 * - `starting`: coming up.
 * - `migrating`: up, and running migrations.
 * - `ready`: up, migrated, and answering queries.
 * - `failed`: it will not come up.
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/app.ts`) carries a type's
 * and a field's doc but not a variant's (#1325).
 */
export type DbState = { "state": "starting", 
/**
 * A line for the boot screen -- "downloading PostgreSQL 18.6", which
 * on a first run is tens of seconds.
 */
detail: string | null, } | { "state": "migrating" } | { "state": "ready" } | { "state": "failed", 
/**
 * Why, shown to the user as text.
 */
message: string, };

/**
 * What goes on stdin: text, or bytes in base64.
 */
export type RenderContent = { "kind": "text", text: string, } | { "kind": "bytes", base64: string, };

/**
 * How a run ended. A run that failed is an answer, not an error: the viewer
 * draws the source and what the command said.
 *
 * - `drawn`: exit 0. `body` is the SVG or HTML as text, or for an image a
 *   `data:` URL of the type its first bytes name.
 * - `failed`: a non-zero exit (`status` is `None` for a signal), with stderr.
 * - `timed_out`: stopped at its timeout, with the stderr it had written.
 * - `too_large`: stopped when stdout passed the cap.
 * - `not_started`: the program could not be started (not on `PATH`).
 */
export type RenderRun = { "outcome": "drawn", output: CommandOutput, body: string, } | { "outcome": "failed", status: number | null, stderr: string, } | { "outcome": "timed_out", seconds: number, stderr: string, } | { "outcome": "too_large", cap_bytes: number, } | { "outcome": "not_started", reason: string, };

/**
 * One default renderer per file pattern (#2263): files `pattern` takes open
 * with `renderer`. Which of several matching patterns decides, and what
 * happens when the renderer is gone, is the frontend's
 * (`app/src/lib/render/defaults.ts`): the backend runs no built-in renderer.
 */
export type RendererDefault = { 
/**
 * A file glob (`*.md`, `docs/*.mmd`), matched without regard to case.
 */
pattern: string, 
/**
 * The renderer's registry id: `source`, `markdown`, `command:<saved id>`.
 */
renderer: string, };

/**
 * What a renderer reads on stdin: the file's text, or its bytes.
 */
export type RendererInput = "text" | "bytes";

/**
 * One saved command renderer (#2262), Settings › Renderers.
 */
export type UserRenderer = { 
/**
 * Made from the name when first saved, kept through a rename.
 */
id: string, 
/**
 * What the switcher shows.
 */
name: string, 
/**
 * The program and its arguments, as typed: quotes group words, no shell.
 */
command: string, 
/**
 * File globs it accepts (`*.puml`, `docs/*.uml`).
 */
patterns: Array<string>, 
/**
 * Markdown fence languages it accepts, lowercase.
 */
fences: Array<string>, input: RendererInput, output: CommandOutput, 
/**
 * Killed after this many seconds.
 */
timeout_secs: number, };

/**
 * A renderer as Settings › Renderers sends it: a new one has no id yet, and
 * no timeout is the default.
 */
export type UserRendererDraft = { id: string | null, name: string, command: string, patterns: Array<string>, fences: Array<string>, input: RendererInput, output: CommandOutput, timeout_secs: number | null, };
