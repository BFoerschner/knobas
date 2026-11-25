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
 * A field a named pipeline is kept for: how its names end, in which files,
 * and the renderer its output is drawn by.
 */
export type KeptField = { 
/**
 * A file pattern, as a default per pattern has it: `*.xml`.
 */
files: string, 
/**
 * How the value's names end, `/` between them and `@` for an attribute
 * (`MIME_EMBEDDED/MIME_DATA`), or for JSON `.` and `[]`
 * (`attachments[].content`). Positions are never part of it.
 */
field: string, 
/**
 * The renderer the output is drawn by: `image`, `pdf`, `xml`, `text`, `hex`.
 */
end: string, };

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
id: string, name: string, steps: Array<PipeStep>, 
/**
 * The fields it is kept for; none for a pipeline only a file pattern names.
 */
fields?: Array<KeptField>, };

/**
 * A named pipeline as the viewer or Settings sends it: a new one has no id.
 */
export type NamedPipelineDraft = { id: string | null, name: string, steps: Array<PipeStep>, fields?: Array<KeptField>, };

/**
 * One saved command, a row of Settings › Commands.
 */
export type OneOff = { 
/**
 * Stable: what a run, a key and the last runs name it by.
 */
id: string, name: string, command: string, stdin: OneOffStdin, 
/**
 * A file's command: the files it takes, globs, empty for any.
 */
files: Array<string>, 
/**
 * A folder's command: the folders it takes by name or path.
 */
folders: Array<string>, 
/**
 * A folder's command: one of the folder's own files must match.
 */
has: Array<string>, 
/**
 * Every kind: a file at the repository's top must match.
 */
repo_holds: Array<string>, 
/**
 * Only in these repositories (step 10), by entity id; empty for every one.
 */
repos: Array<string>, shows: OneOffShows, start_in: OneOffStartIn, 
/**
 * Folders put in front of `PATH`; `{root}` may stand in one.
 */
path: Array<string>, env: Array<OneOffEnv>, 
/**
 * Ask the program for colours (`FORCE_COLOR` and friends), else `NO_COLOR`.
 */
colours: boolean, timeout_secs: number, 
/**
 * On Windows: hand every path as WSL sees it (step 9).
 */
wsl_paths: boolean, };

/**
 * What crosses on the run's channel: the new lines, the lines that changed,
 * and the counts, the last one carrying how it ended.
 */
export type OneOffBatch = { run: string, lines: Array<OneOffLine>, changed: Array<OneOffLine>, 
/**
 * Lines printed, kept or not.
 */
total: number, 
/**
 * Lines in the middle no longer kept, and the first line after them.
 */
dropped: number, tail_from: number, 
/**
 * Bytes printed.
 */
bytes: number, findings: number, elapsed_ms: number, ended?: OneOffEnded | null, 
/**
 * What it made that is not its lines, once it ended.
 */
result?: OneOffResult | null, };

/**
 * One of the eight colours a program can name.
 */
export type OneOffColour = "k" | "r" | "g" | "y" | "b" | "m" | "c" | "w";

/**
 * A command as the page sends it: a new one has no id yet.
 */
export type OneOffDraft = { id?: string | null, name: string, command: string, stdin: OneOffStdin, files: Array<string>, folders: Array<string>, has: Array<string>, repo_holds: Array<string>, repos: Array<string>, shows: OneOffShows, start_in: OneOffStartIn, path: Array<string>, env: Array<OneOffEnv>, colours?: boolean | null, 
/**
 * Blank or 0 is the default, 120 s.
 */
timeout_secs?: number | null, wsl_paths: boolean, };

/**
 * How a run ended.
 */
export type OneOffEnded = { "how": "exit", code: number, } | { "how": "signal", signal: number, } | { "how": "timed_out", seconds: number, } | { "how": "stopped" } | { "how": "not_started", reason: string, };

/**
 * One `NAME=value` line of a command's environment.
 */
export type OneOffEnv = { name: string, value: string, };

/**
 * What a program said about a place: a linter's error, a compiler's warning.
 */
export type OneOffFinding = { 
/**
 * `error`, `warning`, `note`, or empty where the program wrote none.
 */
severity: string, said: string, rule: string, };

/**
 * What a command is offered on, read from its placeholders.
 */
export type OneOffKind = "file" | "folder" | "repository" | "commit" | "range";

/**
 * One line of a run's output.
 */
export type OneOffLine = { 
/**
 * In the order the lines began, from 0.
 */
n: number, stream: OneOffStream, text: string, runs?: Array<OneOffRun>, places?: Array<OneOffPlace>, 
/**
 * Web addresses, opened in the browser.
 */
urls?: Array<OneOffUrl>, 
/**
 * The place the line is about, with what was said there.
 */
finding?: OneOffFinding, 
/**
 * Bytes past the line's cap, not kept.
 */
cut?: number, 
/**
 * Still being written: no newline yet.
 */
open?: boolean, };

/**
 * One command the repository's file holds, as the copy dialog shows it.
 */
export type OneOffOffered = { name: string, command: string, kind?: OneOffKind | null, 
/**
 * What Save would refuse; such a command cannot be copied.
 */
refused?: string | null, 
/**
 * The person's copy of it, by id, where they made one.
 */
copied?: string | null, 
/**
 * The copy's command line, where it differs from the file's now.
 */
theirs?: string | null, files: Array<string>, folders: Array<string>, has: Array<string>, repo_holds: Array<string>, stdin: OneOffStdin, shows: OneOffShows, start_in: OneOffStartIn, timeout_secs?: number | null, };

/**
 * A file of the checkout a line names, with a line and maybe a column.
 */
export type OneOffPlace = { from: number, to: number, 
/**
 * From the repository's top, with `/`.
 */
path: string, line: number | null, col: number | null, };

/**
 * What a repository's `.knobas/commands.toml` offers, as its copy dialog
 * shows it.
 */
export type OneOffRepoCommands = { 
/**
 * The file's path on disk, where the checkout holds one.
 */
file?: string | null, offered: Array<OneOffOffered>, 
/**
 * Why the file could not be read.
 */
failed?: string | null, };

/**
 * A run's result, as the dialog is told it.
 */
export type OneOffResult = { "from": "one", kind: OneOffResultKind, size: number, 
/**
 * Its name: the file's under `{out}`, `stdout` for the stdout.
 */
name: string, 
/**
 * The label overruled by the bytes, where it was.
 */
overruled?: string | null, } | { "from": "files", files: Array<MadeFile>, bytes: number, left_out: Array<MadeLeftOut>, left_out_total: number, } | { "from": "refused", why: string, };

/**
 * What a result is drawn as.
 */
export type OneOffResultKind = { "kind": "image", mime: string, } | { "kind": "pdf" } | { "kind": "page" } | { "kind": "json" } | { "kind": "markdown" } | { "kind": "code", ext: string, } | { "kind": "diff" } | { "kind": "text" } | { "kind": "bytes" };

/**
 * A run of one style in a line, `from` to `to` in UTF-16 units.
 */
export type OneOffRun = { from: number, to: number, style: OneOffStyle, };

/**
 * One run, as the list keeps it.
 */
export type OneOffRunRow = { id: string, command: string, name: string, entity_id: string, target: OneOffTarget, 
/**
 * The words it was run with.
 */
argv: Array<string>, 
/**
 * The folder it started in.
 */
cwd: string, 
/**
 * The checkout's branch and commit when it started.
 */
branch?: string | null, head?: string | null, 
/**
 * Unix milliseconds.
 */
started_at: number, 
/**
 * `None` while it runs.
 */
duration_ms?: number | null, ended?: OneOffEnded | null, lines: number, findings: number, 
/**
 * Whether what it printed is still in memory.
 */
kept: boolean, };

/**
 * *Where programs are found* in Settings › Commands.
 */
export type OneOffShell = { 
/**
 * *Use my shell's PATH*.
 */
on: boolean, 
/**
 * Whether the row is drawn: not on Windows.
 */
offered: boolean, read?: OneOffShellPath | null, };

/**
 * What the login shell answered.
 */
export type OneOffShellPath = { 
/**
 * The shell asked, `$SHELL`.
 */
shell: string, 
/**
 * Its `PATH`, or `None` where it gave none.
 */
path?: string | null, 
/**
 * How many folders that `PATH` holds.
 */
folders: number, 
/**
 * Unix milliseconds.
 */
read_at: number, 
/**
 * Why there is no `PATH`: the shell failed, printed nothing or took too
 * long.
 */
failed?: string | null, };

/**
 * How a run's result is drawn (section 16): as its bytes say, or as named.
 */
export type OneOffShows = "auto" | "text" | "problems" | "code" | "diff" | "json" | "markdown" | "page" | "image" | "pdf";

/**
 * Where a run starts: the repository's top, or the folder it runs on.
 */
export type OneOffStartIn = "root" | "target";

/**
 * What a start answers.
 */
export type OneOffStarted = { run: string, argv: Array<string>, cwd: string, branch?: string | null, head?: string | null, started_at: number, 
/**
 * The same command on the same thing was running: this is that run.
 */
already: boolean, };

/**
 * What a command reads on stdin (step 8): nothing, the file as the page shows
 * it, or the selected lines.
 */
export type OneOffStdin = "none" | "file" | "selection";

/**
 * Which pipe a line came from.
 */
export type OneOffStream = "out" | "err";

/**
 * How a run of characters is drawn.
 */
export type OneOffStyle = { fg?: OneOffColour, bg?: OneOffColour, bold?: boolean, dim?: boolean, italic?: boolean, underline?: boolean, };

/**
 * What a command is run on.
 */
export type OneOffTarget = { "kind": "file", path: string, lines?: [number, number] | null, at?: string | null, } | { "kind": "folder", path: string, } | { "kind": "repository" } | { "kind": "commit", sha: string, } | { "kind": "range", base: string, head: string, path?: string | null, } | { "kind": "files", paths: Array<string>, };

/**
 * A web address in a line: opened in the browser, nothing else.
 */
export type OneOffUrl = { from: number, to: number, href: string, };

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
