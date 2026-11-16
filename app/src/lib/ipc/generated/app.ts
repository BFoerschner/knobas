// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { PipeStep } from "./entity";

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
 * What an ask may start.
 */
export type Ask = "look" | "open" | "again";

/**
 * What a renderer writes on stdout: SVG or HTML (drawn sanitized, in a
 * sandboxed frame), or an image's bytes.
 */
export type CommandOutput = "svg" | "html" | "image";

/**
 * One saved command step, Settings › Renderers.
 */
export type CommandStep = { 
/**
 * Made from the name when first saved, kept through a rename.
 */
id: string, 
/**
 * What the pipeline bar and a failure call it.
 */
name: string, 
/**
 * The program and its arguments, as typed: quotes group words, no shell.
 */
command: string, 
/**
 * Killed after this many seconds.
 */
timeout_secs: number, };

/**
 * A command step as Settings sends it: a new one has no id yet, and no
 * timeout is the default.
 */
export type CommandStepDraft = { id: string | null, name: string, command: string, timeout_secs: number | null, };

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
 * What kind of output a run made, as the Code page draws it.
 */
export type DrawnKind = { "kind": "image", mime: string, } | { "kind": "text" } | { "kind": "markdown" } | { "kind": "html" } | { "kind": "undrawable" };

/**
 * What a run drew.
 */
export type FolderDrawn = { 
/**
 * What `folder_render_output` reads the bytes by.
 */
token: string, kind: DrawnKind, size: number, 
/**
 * The commit the folder was copied at, where known.
 */
commit: string | null, from: ReadFrom, 
/**
 * How long the copy and the run took, in milliseconds.
 */
took_ms: number, 
/**
 * When it was drawn (RFC 3339).
 */
drawn_at: string, };

/**
 * Why nothing was drawn. An answer the page draws, not an error.
 */
export type FolderFailure = { "why": "failed", status: number | null, stderr: string, } | { "why": "timed_out", seconds: number, stderr: string, } | { "why": "too_large", cap_bytes: number, } | { "why": "not_started", reason: string, } | { "why": "no_output_file", found: number, stderr: string, } | { "why": "copy", reason: string, } | { "why": "too_big", files: number, bytes: number, cap_files: number, cap_bytes: number, from: ReadFrom, } | { "why": "no_folder" } | { "why": "stopped" };

/**
 * The folders of a tree that a renderer holding globs takes, per renderer:
 * what the Code page's tree marks without reading each folder's listing.
 * Renderers with no `holds` glob are left out (the page matches them by
 * path alone).
 */
export type FolderMarks = { renderer: string, 
/**
 * Folder paths from the top (`""` the top itself), in tree order.
 */
folders: Array<string>, };

/**
 * What a folder renderer makes. `detect` reads it from the bytes (and the
 * output file's extension, for Markdown).
 */
export type FolderOutput = "detect" | "image" | "text" | "markdown" | "html";

/**
 * The event's payload: a slot's run moved; the page showing it asks again.
 */
export type FolderRenderMoved = { entity_id: string, renderer: string, folder: string, 
/**
 * The run's state now; `None` once it ended.
 */
running: FolderRunState | null, };

/**
 * The answer to a page's ask.
 */
export type FolderRenderView = { renderer: string, folder: string, 
/**
 * What to show: the drawing of the folder as it is now, or the last one
 * of an earlier state (`current` false).
 */
drawn: FolderDrawn | null, current: boolean, running: FolderRunState | null, 
/**
 * The last run of the folder as it is now failed (or the copy cannot be
 * made); `None` while one runs.
 */
failure: FolderFailure | null, 
/**
 * Where the copy is, or would be, written from.
 */
from: ReadFrom | null, 
/**
 * The commit the folder is read at now.
 */
commit: string | null, };

/**
 * One saved folder renderer, Settings › Renderers › *Folder renderers*.
 */
export type FolderRenderer = { 
/**
 * Made from the name when first saved, kept through a rename.
 */
id: string, name: string, 
/**
 * The program and its arguments, as typed: quotes group words, no shell,
 * `{path}` and `{out}` the only placeholders.
 */
command: string, 
/**
 * File globs one of the folder's own files must match (`*.lcproj`);
 * empty takes any folder.
 */
holds: Array<string>, 
/**
 * Globs the folder's path must match (`docs/*`, `flows`); a glob with no
 * `/` is matched against the folder's name. Empty takes any folder.
 */
folders: Array<string>, output: FolderOutput, 
/**
 * Killed after this many seconds (the copy is not counted).
 */
timeout_secs: number, };

/**
 * A folder renderer as Settings sends it: a new one has no id yet, and no
 * timeout is the default.
 */
export type FolderRendererDraft = { id: string | null, name: string, command: string, holds: Array<string>, folders: Array<string>, output: FolderOutput, timeout_secs: number | null, };

/**
 * Where a run is.
 */
export type FolderRunState = { "state": "queued" } | { "state": "copying", done: number, total: number, } | { "state": "running", elapsed_ms: number, };

/**
 * One installed font family.
 */
export type InstalledFont = { 
/**
 * The family name, as `font-family` takes it.
 */
family: string, 
/**
 * A face of the family says it is fixed pitch (its `post` table's
 * `isFixedPitch`), or draws `i`, `M`, `0`, `.` and a space at one
 * advance: the picker lists these first.
 */
monospace: boolean, };

/**
 * One named pipeline: steps a default per file pattern names by id.
 */
export type NamedPipeline = { 
/**
 * Made from the name when first saved, kept through a rename.
 */
id: string, name: string, steps: Array<PipeStep>, };

/**
 * A named pipeline as the viewer or Settings sends it: a new one has no id.
 */
export type NamedPipelineDraft = { id: string | null, name: string, steps: Array<PipeStep>, };

/**
 * Where the copy was written from.
 */
export type ReadFrom = "clone" | "forge";

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
 * Empty when the row only sets a pipeline: the pipeline's output picks
 * its renderer by its own name.
 */
renderer: string, 
/**
 * A named pipeline's id (#2265, `file_pipeline::saved`): files the
 * pattern takes open through it. By reference, so editing the pipeline
 * changes every pattern that names it. A row stored before the field
 * existed reads as none.
 */
pipeline: string | null, };

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
timeout_secs: number, 
/**
 * Whether a diff of a file it draws offers a *Rendered* mode: the
 * file drawn by it twice, before and after side by side (#2279). A
 * row saved before the field existed reads as `false`.
 */
diff: boolean, };

/**
 * A renderer as Settings › Renderers sends it: a new one has no id yet, and
 * no timeout is the default.
 */
export type UserRendererDraft = { id: string | null, name: string, command: string, patterns: Array<string>, fences: Array<string>, input: RendererInput, output: CommandOutput, timeout_secs: number | null, diff: boolean, };
