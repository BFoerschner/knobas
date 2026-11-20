/**
 * App lifecycle and status — `crates/knobas-app/src/commands/app.rs`.
 *
 * The types are generated from the Rust (`./generated/app.ts`, contract §2.6
 * as amended by #1325): a field is explained in its Rust doc comment and
 * nowhere else. The `invoke` functions stay hand-written, pinned to the Rust
 * by a test in that module which `include_str!`s this file.
 */
import { invoke } from "./invoke";
import type { ViewBlock } from "./generated/entity";
import type {
  AppStatus,
  CommandStep,
  CommandStepDraft,
  FolderMarks,
  FolderRenderer,
  FolderRendererDraft,
  FolderRenderView,
  Ask,
  InstalledFont,
  MadeStore,
  MadeTree,
  JavaRuntime,
  NamedPipeline,
  NamedPipelineDraft,
  RenderContent,
  RendererDefault,
  RenderRun,
  UserRenderer,
  UserRendererDraft,
} from "./generated/app";

export type {
  AppStatus,
  CommandOutput,
  CommandStep,
  CommandStepDraft,
  DbState,
  Ask,
  DrawnKind,
  FolderDrawn,
  FolderFailure,
  FolderMarks,
  FolderOutput,
  FolderRenderer,
  FolderRendererDraft,
  FolderRenderMoved,
  FolderRenderView,
  FolderRunState,
  MadeFile,
  MadeLeftOut,
  MadeLeftOutWhy,
  MadePlace,
  MadeStore,
  MadeTree,
  ReadFrom,
  InstalledFont,
  JavaRuntime,
  FoundJava,
  NamedPipeline,
  NamedPipelineDraft,
  RenderContent,
  RendererDefault,
  RendererInput,
  RenderRun,
  UserRenderer,
  UserRendererDraft,
} from "./generated/app";

/**
 * Liveness probe.
 *
 * Answers as soon as the webview can call, which is now *before* the database
 * is up: a `"pong"` says the backend is running and nothing about PostgreSQL.
 * Ask {@link appStatus} for that.
 */
export function ping(): Promise<string> {
  return invoke<string>("ping");
}

/** The current lifecycle state, plus the counts the shell shows beside it. */
export function appStatus(): Promise<AppStatus> {
  return invoke<AppStatus>("app_status");
}

/**
 * Arm event emission: the backend replays the current `db:state` (gotcha 9).
 *
 * Call it **after** registering the `db:state` listener, never before — that
 * ordering is the entire reason this command exists.
 */
export function frontendReady(): Promise<void> {
  return invoke<void>("frontend_ready");
}

/**
 * Start the database again after a failure — the boot screen's *Retry*.
 *
 * Idempotent on the backend: a second call while a bring-up is already running
 * starts nothing, which is the right answer to "start the database" when it is
 * already starting.
 */
export function retryDatabase(): Promise<void> {
  return invoke<void>("retry_database");
}

/**
 * Record that the §14a first-run wizard finished.
 *
 * `AppStatus.first_run` is `source_count === 0 && !completed` — two facts,
 * because they are different ones. Without this, a person who finished the
 * wizard and later removed their only source would be shown it again.
 *
 * The flag lives in `knobas.setting`, not in the webview's storage: §14 makes
 * the database what an export carries, and a flag outside it would not survive
 * the round trip.
 *
 * Idempotent — finishing twice is the same as finishing once.
 */
export function completeFirstRun(): Promise<void> {
  return invoke<void>("complete_first_run");
}

/**
 * *Pages kept open* (#2247): how many pages the shell keeps mounted and hidden
 * besides the one in front, 0 to 15, 5 until set.
 */
export function pagesKeptOpen(): Promise<number> {
  return invoke<number>("pages_kept_open");
}

/**
 * Store *Pages kept open*, and get back what is now stored (held to 15), so
 * Settings › Performance draws what the database holds.
 */
export function setPagesKeptOpen(count: number): Promise<number> {
  return invoke<number>("set_pages_kept_open", { count });
}

/**
 * *Smooth scrolling on 120 Hz displays*, Settings › Performance: whether the
 * window draws at the display's rate rather than WebKit's 60. On until set.
 * `null` where the platform has no such cap (Windows, Linux), so no switch is drawn.
 */
export function smoothScrolling(): Promise<boolean | null> {
  return invoke<boolean | null>("smooth_scrolling");
}

/** Store *Smooth scrolling on 120 Hz displays*, applied to the window at once; answers what is now stored. */
export function setSmoothScrolling(on: boolean): Promise<boolean | null> {
  return invoke<boolean | null>("set_smooth_scrolling", { on });
}

/**
 * The font families installed on this machine, monospace ones first, for
 * Settings › Appearance › *Code font*. Read once a run; needs no database.
 */
export function installedFonts(): Promise<InstalledFont[]> {
  return invoke<InstalledFont[]>("installed_fonts");
}

/** *Run pipelines on diffs* (#2270): on until set in Settings › Renderers. */
export function diffPipelines(): Promise<boolean> {
  return invoke<boolean>("diff_pipelines");
}

/** Store *Run pipelines on diffs*, and get back what is now stored. */
export function setDiffPipelines(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_diff_pipelines", { on });
}

/** The command renderers saved in Settings › Renderers (#2262), in order. */
export function userRenderers(): Promise<UserRenderer[]> {
  return invoke<UserRenderer[]>("user_renderers");
}

/** Save the whole list of command renderers; answers it as stored (ids, lowercase fences, held timeouts). */
export function setUserRenderers(renderers: UserRendererDraft[]): Promise<UserRenderer[]> {
  return invoke<UserRenderer[]>("set_user_renderers", { renderers });
}

/**
 * Run saved renderer `id` with `content` on its stdin (ADR-0033). A failure,
 * a timeout or too much output is an answer (`RenderRun`), not a rejection.
 */
export function runRenderer(id: string, content: RenderContent): Promise<RenderRun> {
  return invoke<RenderRun>("run_renderer", { id, content });
}

/** The folder renderers saved in Settings › Renderers (ADR-0044), in order. */
export function folderRenderers(): Promise<FolderRenderer[]> {
  return invoke<FolderRenderer[]>("folder_renderers");
}

/** Save the whole list of folder renderers; answers it as stored (ids, trimmed globs, held timeouts). */
export function setFolderRenderers(renderers: FolderRendererDraft[]): Promise<FolderRenderer[]> {
  return invoke<FolderRenderer[]>("set_folder_renderers", { renderers });
}

/** Where a folder render is asked for: the repository or branch, the renderer, the folder and its reads. */
export interface FolderAsk {
  entityId: string;
  renderer: string;
  folder: string;
  /** The commit the page shows; `null` reads at the mirror's tip. */
  commit: string | null;
  /** The section the source lists every path in, for a repository with no clone. */
  paths: string | null;
  ask: Ask;
}

/**
 * Saved folder renderer `renderer` on `folder` (ADR-0044): what is kept and
 * the run's state, at once. `ask` says whether a run may start; the run is
 * told on `folder-render:moved`.
 */
export function folderRender(at: FolderAsk): Promise<FolderRenderView> {
  return invoke<FolderRenderView>("folder_render", {
    entityId: at.entityId,
    renderer: at.renderer,
    folder: at.folder,
    commit: at.commit,
    paths: at.paths,
    ask: at.ask,
  });
}

/** Stop a folder render's run, waiting or going. */
export function folderRenderStop(entityId: string, renderer: string, folder: string): Promise<void> {
  return invoke<void>("folder_render_stop", { entityId, renderer, folder });
}

/** The page showing a folder render left: a run still waiting for its place is dropped. */
export function folderRenderLeave(entityId: string, renderer: string, folder: string): Promise<void> {
  return invoke<void>("folder_render_leave", { entityId, renderer, folder });
}

/** What `folder_render_output` answers: an `ArrayBuffer` from the bridge, a `Uint8Array` from the dev fake. */
type RawBytes = ArrayBuffer | ArrayLike<number>;

/** The bytes a folder render drew, by its token: raw from the bridge, as `entity_file_bytes` answers. */
export async function folderRenderOutput(token: string): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("folder_render_output", { token });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/** Keep a sanitized HTML document for a folder render's frame; answers the token the frame loads it by. */
export function folderRenderFrame(document: string): Promise<string> {
  return invoke<string>("folder_render_frame", { document });
}

/** Which folders of a tree the folder renderers holding a file glob take, per renderer. */
export function folderRenderMarks(entityId: string, commit: string | null, paths: string | null): Promise<FolderMarks[]> {
  return invoke<FolderMarks[]>("folder_render_marks", { entityId, commit, paths });
}

/** A generated folder's whole tree by its token (ADR-0046); `NotFound` for a token the backend no longer holds. */
export function folderRenderTree(token: string): Promise<MadeTree> {
  return invoke<MadeTree>("folder_render_tree", { token });
}

/** A generated file, as `entity_local_file` answers a file; `path` from the generated folder's top. */
export function folderRenderFile(token: string, path: string): Promise<ViewBlock> {
  return invoke<ViewBlock>("folder_render_file", { token, path });
}

/** A generated file's bytes, raw, as `entity_file_bytes` answers. */
export async function folderRenderFileBytes(token: string, path: string): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("folder_render_file_bytes", { token, path });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/** Where generated folders are kept, and how much. */
export function folderRenderStore(): Promise<MadeStore> {
  return invoke<MadeStore>("folder_render_store");
}

/** Keep generated folders in `path`, or `null` for the default; what was kept at the old place is removed. */
export function setFolderRenderStore(path: string | null): Promise<MadeStore> {
  return invoke<MadeStore>("set_folder_render_store", { path });
}

/** Remove every generated folder. */
export function clearFolderRenderStore(): Promise<MadeStore> {
  return invoke<MadeStore>("clear_folder_render_store");
}

/** The default renderer per file pattern (#2263), in order. */
export function rendererDefaults(): Promise<RendererDefault[]> {
  return invoke<RendererDefault[]>("renderer_defaults");
}

/** Save the whole list of default renderers per file pattern; answers it as stored (trimmed). */
export function setRendererDefaults(defaults: RendererDefault[]): Promise<RendererDefault[]> {
  return invoke<RendererDefault[]>("set_renderer_defaults", { defaults });
}

/** The command steps saved in Settings › Renderers (#2265), in order. */
export function commandSteps(): Promise<CommandStep[]> {
  return invoke<CommandStep[]>("command_steps");
}

/** Save the whole list of command steps; answers it as stored (ids, held timeouts). */
export function setCommandSteps(steps: CommandStepDraft[]): Promise<CommandStep[]> {
  return invoke<CommandStep[]>("set_command_steps", { steps });
}

/** The named pipelines (#2265), in order. */
export function namedPipelines(): Promise<NamedPipeline[]> {
  return invoke<NamedPipeline[]>("named_pipelines");
}

/** Save the whole list of named pipelines; answers it as stored (ids made from new names). */
export function setNamedPipelines(pipelines: NamedPipelineDraft[]): Promise<NamedPipeline[]> {
  return invoke<NamedPipeline[]>("set_named_pipelines", { pipelines });
}

/**
 * What *Decompile* in the JAR view stands at — `knobas_app::commands::app::java_runtime` (ux19): the Java found here,
 * the bundled decompiler, why not where it cannot run. Looked for once a run; `again` looks again.
 */
export function javaRuntime(again: boolean): Promise<JavaRuntime> {
  return invoke<JavaRuntime>("java_runtime", { again });
}

/** Settings' Java for *Decompile* — `set_java_path`: an absolute path to a `java` or a Java home, `null` to look by itself. */
export function setJavaPath(path: string | null): Promise<JavaRuntime> {
  return invoke<JavaRuntime>("set_java_path", { path });
}
