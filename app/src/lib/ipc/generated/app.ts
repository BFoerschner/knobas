// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { PipeStep } from "./entity";

/**
 * What the shell needs to decide what to draw.
 */
export type AppStatus = { db: DbState, 
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
 * What the boot screen tells someone whose machine blocks knobas from its
 * own database (ux28): what knobas tried, and what an administrator can
 * allow.
 */
export type Blocked = { 
/**
 * The cause in plain words, for the line under the heading.
 */
cause: string, 
/**
 * Each address or socket path tried and refused: `tcp 127.0.0.1:54321`.
 */
tried: Array<string>, 
/**
 * The programs to allow, by their full paths: `knobas-app` and the
 * `postgres` it starts.
 */
allow: Array<string>, };

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
message: string, 
/**
 * What the boot screen's *Copy details* copies (ux28): the step
 * that failed, what the data folder held, every connection tried
 * with its OS error, the machine and its security products. Absent
 * for a failure that has no start behind it (a server that died
 * after `ready`).
 */
details?: string, 
/**
 * Set when the operating system refused knobas its own server for
 * permissions (Windows' WSAEACCES, 10013): a firewall rule or
 * security software. The boot screen then says so in plain words.
 */
blocked?: Blocked, 
/**
 * What the server wrote to its log when it failed to start, each
 * line from its severity label on (the Windows pass on ux28): the
 * cause PostgreSQL names before its `FATAL` summary. The boot screen
 * draws the last few under the message. Absent when the server
 * wrote nothing or no start failed.
 */
said?: Array<string>, };

/**
 * What kind of output a run made, as the Code page draws it.
 */
export type DrawnKind = { "kind": "image", mime: string, } | { "kind": "text" } | { "kind": "markdown" } | { "kind": "html" } | { "kind": "undrawable" } | { "kind": "folder", files: number, bytes: number, left_out: number, };

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
export type FolderFailure = { "why": "failed", status: number | null, stderr: string, } | { "why": "timed_out", seconds: number, stderr: string, } | { "why": "too_large", cap_bytes: number, } | { "why": "not_started", reason: string, } | { "why": "no_output_file", found: number, stderr: string, } | { "why": "copy", reason: string, } | { "why": "too_big", files: number, bytes: number, cap_files: number, cap_bytes: number, from: ReadFrom, } | { "why": "no_folder" } | { "why": "stopped" } | { "why": "no_output", stderr: string, } | { "why": "output_too_big", files: number, bytes: number, cap_files: number, cap_bytes: number, } | { "why": "keep", reason: string, };

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
export type FolderOutput = "detect" | "image" | "text" | "markdown" | "html" | "folder";

/**
 * One pick: the renderer a folder of a repository gets, wherever a folder renderer is chosen for it (`docs/decisions/
 * 2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md` ruling 10). A pick whose renderer is
 * gone is passed over and left in place, so a renderer saved again under its id takes its folders back.
 */
export type FolderPick = { 
/**
 * The repository's entity id.
 */
repo: string, 
/**
 * The folder's path from the top; `""` the top.
 */
folder: string, 
/**
 * The saved renderer's id.
 */
renderer: string, };

/**
 * The event's payload: a slot's run moved; the page showing it asks again.
 */
export type FolderRenderMoved = { entity_id: string, renderer: string, folder: string, 
/**
 * The run's state now; `None` once it ended.
 */
running: FolderRunState | null, 
/**
 * The slot's lane (ux26): `""` for the Files tab and Settings, a side of a comparison otherwise.
 */
lane: string, };

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
 * `{path}` and one of `{out}` and `{outdir}` the only placeholders.
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
export type FolderRunState = { "state": "queued" } | { "state": "copying", done: number, total: number, } | { "state": "running", elapsed_ms: number, } | { "state": "keeping", done: number, total: number, };

export type FoundJava = { path: string, 
/**
 * As `java -version` names it (`21.0.4`).
 */
version: string, 
/**
 * The release (`21`; `8` for `1.8.0_392`).
 */
release: number, };

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
monospace: boolean, 
/**
 * The family's upright regular face is classed as a serif face (its
 * `OS/2` table's family class, else its PANOSE serif style): *Reading*
 * lists these first (Settings › Fonts, 2026-11-17).
 */
serif: boolean, 
/**
 * The family can draw narrow: a face of it has a `wdth` variation axis,
 * or is a condensed member (its `OS/2` width class under normal), so
 * `font-stretch` narrows it as `"wdth"` narrows Archivo. *Headings* lists
 * these first and marks the rest "normal width only".
 */
narrows: boolean, 
/**
 * The upright regular face's x-height over its em (Archivo's is about
 * .52), for *Even out*: the size that draws its lower case as tall as
 * the bundled face's. `None` when the face does not say.
 */
x_height: number | null, 
/**
 * The upright regular face's version, as its `name` table's version
 * string gives it without the word *Version* and anything after a `;`
 * (`2407.024` for Cascadia Mono 2407.24, and for a Nerd Fonts patch of
 * it), so Settings › Fonts can tell an installed copy from the one knobas
 * brings (2026-11-21). `None` when the face does not say.
 */
version: string | null, };

/**
 * What the JAR view and Settings know about decompiling here.
 */
export type JavaRuntime = { 
/**
 * The Java found: its path and version; `null` for none.
 */
java: FoundJava | null, 
/**
 * The decompiler bundled with this build (`Vineflower 1.12.0`); `null` in a build without it.
 */
decompiler: string | null, 
/**
 * The Java release the decompiler needs (17).
 */
needs: number | null, 
/**
 * A Java was found, and the decompiler runs on it.
 */
can_decompile: boolean, 
/**
 * Why not, where there is a Java: `Java 11 found; Decompile needs Java 17`.
 */
reason: string | null, 
/**
 * The Java the person set in Settings, if any.
 */
set_path: string | null, };

/**
 * One generated file that differs.
 */
export type MadeChange = { path: string, change: MadeChangeKind, before_size: number | null, after_size: number | null, 
/**
 * A NUL in the first 8,000 bytes of either side, as git tells binary.
 */
binary: boolean, };

/**
 * How a generated file differs. No renames: a file that moved is one deleted and one added.
 */
export type MadeChangeKind = "added" | "deleted" | "modified";

/**
 * What two generated folders differ in (ux26, ADR-0049, ruling 7): the files only in one, and those in both whose bytes
 * differ, sorted by path bytes, at most [`COMPARE_FILES`]; the rest counted.
 */
export type MadeCompare = { files: Array<MadeChange>, 
/**
 * In both, byte for byte the same.
 */
same: number, 
/**
 * In both, the same but for line endings (only where line endings are set aside).
 */
eol_only: number, before_files: number, after_files: number, };

/**
 * One kept file: its path from the generated folder's top, and its size.
 */
export type MadeFile = { path: string, size: number, };

/**
 * One entry left out, its path made safe to show. `MadeLeftOut` in the
 * shell, beside the search's own `LeftOut`.
 */
export type MadeLeftOut = { 
/**
 * Lossy, control characters replaced, [`LEFT_OUT_CHARS`] at most.
 */
path: string, why: MadeLeftOutWhy, };

/**
 * Why an entry the tool wrote was not kept.
 */
export type MadeLeftOutWhy = "link" | "special" | "git" | "name" | "not_utf8" | "case_twin" | "too_deep";

/**
 * Where the store's base came from, for Settings' small line.
 */
export type MadePlace = "set" | "clones" | "profile";

/**
 * The store, as Settings shows it.
 */
export type MadeStore = { 
/**
 * The store's folder (`…/knobas-rendered`).
 */
dir: string, 
/**
 * The folder the person chose, as stored.
 */
set: string | null, 
/**
 * The base used when nothing is set: the clones root, else the profile's.
 */
default_dir: string, place: MadePlace, 
/**
 * Generated folders kept, and their bytes.
 */
folders: number, bytes: number, cap_bytes: number, };

/**
 * A generated folder's whole tree, as `folder_render_tree` answers it.
 */
export type MadeTree = { 
/**
 * Where it is kept, absolute: what *Copy path* copies on the landing.
 */
dir: string, 
/**
 * Sorted by path bytes.
 */
files: Array<MadeFile>, bytes: number, left_out: Array<MadeLeftOut>, left_out_total: number, };

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
export type ReadFrom = "clone" | "forge" | "working_tree";

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
