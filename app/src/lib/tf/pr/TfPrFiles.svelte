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

  Where the diff names its range (#2273, #2310):
  - **The commit picker** (`TfPrPicker`): all commits, one, a range, or *since
    my last review*. Any choice but all is a compare in the range's repository
    (`diff/pick.ts`), drawn in place. Line comments are offered on the whole
    diff only: the review goes to Gitea at the head, so a line of an older
    commit's diff would land on the head's line of the same number.
  - **Expanded context** (`diff/context.ts` `rangeContext`): each side read
    whole at its sha, bytes too for a rendered image diff (#2279). A new pick
    draws a new diff component, so no side read at one range is kept for another.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { entityFileBytes, type SourceView } from "../../ipc/entity";
  import type { PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import type { RenderPort } from "../../render/port";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { compareSection } from "../diff/address";
  import { rangeContext } from "../diff/context";
  import { pickedEnds, type PrPick } from "../diff/pick";
  import { diffOf } from "./files";
  import TfPrPicker from "./TfPrPicker.svelte";

  let {
    prId,
    entityView,
    viewedFiles,
    setViewed,
    pending,
    render,
    openAt = null,
    fileBytes = (entityId, path, commit) => entityFileBytes(entityId, path, commit),
  }: {
    prId: string;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    viewedFiles: (prId: string) => Promise<string[]>;
    setViewed: (prId: string, path: string, viewed: boolean) => Promise<string[]>;
    /** The reader's pending line comments. */
    pending: PendingCommentStore;
    /** The renderer port, for tests; the app's by default. */
    render?: RenderPort;
    /** *Open in…* per file (#2274): the repository and the head the files are read at. */
    openAt?: { entityId: string; at: string | null } | null;
    /** A file's bytes at a commit, for a rendered image diff (#2279); the IPC by default. */
    fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;
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
  const range = $derived(diff?.range ?? null);

  /** The commit picker's choice (#2273), and the compare it reads; all commits is the diff above. */
  let choice = $state<PrPick>({ kind: "all" });
  const ends = $derived(range === null ? null : pickedEnds(range, choice));
  const section = $derived(ends === null ? null : compareSection(ends.base, ends.head));
  /** `undefined` while the pick's compare has not answered. */
  let picked = $state<SourceView | null | undefined>(undefined);
  let pickFailed = $state<string | null>(null);

  $effect(() => {
    // A new pull request starts at all commits.
    void prId;
    choice = { kind: "all" };
  });

  $effect(() => {
    const repo = range?.repository;
    const asked = section;
    picked = undefined;
    pickFailed = null;
    if (!repo || asked === null) return;
    let live = true;
    entityView(repo, asked).then(
      (answer) => {
        if (live) picked = answer;
      },
      (error) => {
        if (live) pickFailed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  /** What is drawn: the pull request's diff, or the pick's compare once it answered. */
  const shown = $derived.by(() => {
    if (section === null) return diff;
    const block = picked?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
    return block?.type === "diff" ? { files: block.files, truncated: block.truncated, commentable: null, gone: [], range: block.range ?? null } : null;
  });
  const context = $derived(shown?.range ? rangeContext(entityView, shown.range, fileBytes) : null);

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
  {#if range !== null}
    <div class="tf-pickbar">
      <TfPrPicker {range} {choice} onchoose={(next) => (choice = next)} />
    </div>
  {/if}
  {#if pickFailed !== null}
    <p class="tf-cfail">Could not read the picked commits: {pickFailed}</p>
  {:else if shown === null}
    <!-- The pick's compare has not answered yet. -->
  {:else if shown.files.length === 0 && shown.gone.length === 0}
    <div class="tf-empty">{section === null ? "This pull request changes no files." : "Nothing changes in the picked commits."}</div>
  {:else}
    {#key section}
      <TfDiffView
        files={shown.files}
        gone={shown.gone}
        viewed={{ paths: viewed, set: (path, on) => void toggle(path, on) }}
        review={shown.commentable ? { entity: prId, pending } : null}
        {context}
        openAt={openAt && ends ? { entityId: openAt.entityId, at: ends.head } : openAt}
        cut={shown.truncated}
        {...render ? { render } : {}}
      />
    {/key}
  {/if}
{/if}
