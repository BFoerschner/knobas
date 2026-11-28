/**
 * What a pipeline page's header draws from before its own reads answer (#2618): nothing in it may move once drawn
 * (Björn, 2026-11-17).
 *
 * - **The source's word** (*TeamCity build configuration*) comes from the sources and the adapters' descriptors, which
 *   each pipeline page read on its mount. Opened again, the page drew its line without the word, the settings' repository
 *   after it, and the word landing moved the repository and the chain's sign 119 px right. The last answers are kept
 *   here for the session, so a page mounted again draws its word in its first frame.
 * - **Its *Refresh pipeline*** came with the pipeline's item (`get_entity`, 300 ms late on the fixture) and moved the
 *   header's five other actions 138 px left. The item's `refreshes` is its adapter's declaration for its kind
 *   (`KindInfo::refreshes`, #2146), which the TypeScript mirror leaves out. So the header draws the button from
 *   {@link expectedSubject} until the item answers: what this source's last pipeline item said, else *Refresh*, which
 *   every adapter with pipelines declares (TeamCity since #2564, Gitea since ADR-0047, the WASM fixture). One whose
 *   pipelines don't refresh fails nothing: pressed before its item answers, `refresh_entity` says *not offered* and the
 *   press runs the source's *Sync now* (`tf/refresh.svelte.ts`); its item landing trades the two buttons for one *Sync
 *   now* once, and its next pipeline page draws that from the first frame.
 */
import type { PipelineRow } from "../../ipc/entity";
import type { SourceDescriptor, SourceSummary } from "../../ipc/sources";
import type { RefreshSubject } from "../refresh.svelte";

/** The sources and descriptors as a pipeline page last read them; `null` until one has. */
const kept: { adapters: SourceDescriptor[] | null; sources: SourceSummary[] | null } = { adapters: null, sources: null };

/** What each source's pipeline item last said of `refreshes`, by source id. */
const refreshes = new Map<string, boolean>();

/** The descriptors a pipeline page last read, or `null`. */
export function keptAdapters(): SourceDescriptor[] | null {
  return kept.adapters;
}

/** The sources a pipeline page last read, or `null`. */
export function keptSources(): SourceSummary[] | null {
  return kept.sources;
}

/** Keep what a pipeline page read. */
export function keepAdapters(rows: SourceDescriptor[]): void {
  kept.adapters = rows;
}

/** Keep what a pipeline page read. */
export function keepSources(rows: SourceSummary[]): void {
  kept.sources = rows;
}

/** Keep what a pipeline's item said: the next pipeline of its source draws the same button from its first frame. */
export function keepRefreshes(sourceId: string, value: boolean): void {
  refreshes.set(sourceId, value);
}

/**
 * The header's *Refresh* subject before the pipeline's item answers: the pipeline's own row, its source as the sources
 * read says it (or the row's name for it, switched on, before that read), and `refreshes` as this source's last pipeline
 * item said, else `true`.
 */
export function expectedSubject(pipeline: PipelineRow, sources: readonly SourceSummary[]): RefreshSubject & { row: { title: string } } {
  const source = sources.find((s) => s.id === pipeline.source_id);
  return {
    refreshes: refreshes.get(pipeline.source_id) ?? true,
    row: { entity_id: pipeline.entity_id, title: pipeline.name },
    source: { id: pipeline.source_id, display_name: source?.display_name ?? pipeline.source_name, enabled: source?.enabled ?? true },
  };
}

/** Forget it all: a new session, which is what each test is. */
export function forgetPipelineFacts(): void {
  kept.adapters = null;
  kept.sources = null;
  refreshes.clear();
}
