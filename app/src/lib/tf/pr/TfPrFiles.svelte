<!--
  *Files changed* (#842, R-8.17), prototype-6's `renderPR(n, 'diff')`: a
  file list on the left with a filter, each file with `+N −N` and a tick once
  viewed (a click scrolls to it); on the right one diff per file in the
  source's hunks, `⋯` between them.

  - *Viewed* per file collapses it. It is knobas-local, kept by pull request
    and head commit (`pr_viewed_files`, `set_pr_file_viewed`): a push to the
    head brings every file back.
  - Each thread the source holds sits under its line; an outdated one, or one
    whose line is not drawn, is kept apart at the end of its file, closed
    (`files.ts`). A thread on a file the diff no longer holds is listed at the
    end.
  - Where the source takes comments (`commentable`), each line offers a `+` on
    hover or focus that opens a box; *Add to review* keeps the comment
    **pending** (`detail/view/pending-comments`, this viewer's) with a warn
    edge and a *pending* chip, until *Submit review* sends it.

  The diff is the *Files* section, read once per pull request when the tab
  opens (the whole view defers it, ADR-0020 rule 3). The one diff component
  draws it (`diff/TfDiffView`, #2266), with *Viewed* and, where the source
  takes comments, the review passed in.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { SourceView } from "../../ipc/entity";
  import type { PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import type { RenderPort } from "../../render/port";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { diffOf } from "./files";

  let {
    prId,
    entityView,
    viewedFiles,
    setViewed,
    pending,
    render,
  }: {
    prId: string;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    viewedFiles: (prId: string) => Promise<string[]>;
    setViewed: (prId: string, path: string, viewed: boolean) => Promise<string[]>;
    /** The reader's pending line comments. */
    pending: PendingCommentStore;
    /** The renderer port, for tests; the app's by default. */
    render?: RenderPort;
  } = $props();

  /** `undefined` while the *Files* read has not answered. */
  let view = $state<SourceView | null | undefined>(undefined);
  let failed = $state<string | null>(null);
  let viewed = $state<string[]>([]);

  $effect(() => {
    const id = prId;
    let live = true;
    view = undefined;
    failed = null;
    viewed = [];
    entityView(id, "files").then(
      (answer) => {
        if (live) view = answer;
      },
      (error) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    viewedFiles(id).then(
      (files) => {
        if (live) viewed = files;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  const diff = $derived(view ? diffOf(view) : null);

  async function toggle(path: string, on: boolean) {
    try {
      viewed = await setViewed(prId, path, on);
    } catch (cause) {
      push({ text: `Could not mark ${path}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

{#if failed !== null}
  <p class="tf-cfail">Could not read the changed files: {failed}</p>
{:else if view === null}
  <div class="tf-empty">This source draws no changed files for a pull request.</div>
{:else if view !== undefined}
  {#if (diff?.files.length ?? 0) === 0 && (diff?.gone.length ?? 0) === 0}
    <div class="tf-empty">This pull request changes no files.</div>
  {:else}
    <TfDiffView
      files={diff?.files ?? []}
      gone={diff?.gone ?? []}
      viewed={{ paths: viewed, set: (path, on) => void toggle(path, on) }}
      review={diff?.commentable ? { entity: prId, pending } : null}
      cut={diff?.truncated ?? false}
      {...render ? { render } : {}}
    />
  {/if}
{/if}
