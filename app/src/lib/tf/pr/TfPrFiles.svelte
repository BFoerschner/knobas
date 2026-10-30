<!--
  *Files changed* (#842, R-8.17), prototype-6's `renderPR(n, 'diff')`: a
  file list on the left with a filter, each file with `+N −N` and a tick once
  viewed (a click scrolls to it); on the right one diff per file in the
  source's hunks, `⋯` between them.

  - *Viewed* per file collapses it. It is knobas-local, kept by pull request
    and the blob the file holds (`pr_viewed_files`, `set_pr_file_viewed`,
    #2276): a push that changes a file brings that file back, and the others
    stay viewed.
  - A thread takes a reply and, where the source can, a resolve (#2277),
    each queued as a write and drawn *queued* under the thread until the
    read after it shows it; a resolved thread is drawn collapsed.
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
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { LineThread, SourceView } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import type { RenderPort } from "../../render/port";
  import type { WriteFollower } from "../../shell/follow-write";
  import { push } from "../../shell/toasts.svelte";
  import { echoOn, echoesOn, echoWrite, sameWords, settleEchoes, watchEchoes, type Echo, type EchoListen } from "../queued.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { diffOf } from "./files";

  let {
    prId,
    entityView,
    viewedFiles,
    setViewed,
    pending,
    submitWrite = null,
    source = null,
    listen,
    follower,
    render,
  }: {
    prId: string;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    /** The files viewed, by the blob each holds in the diff drawn (#2276). */
    viewedFiles: (prId: string, blobs: Record<string, string>) => Promise<string[]>;
    setViewed: (prId: string, path: string, viewed: boolean, blobs: Record<string, string>) => Promise<string[]>;
    /** The reader's pending line comments. */
    pending: PendingCommentStore;
    /** Queues a reply or a resolve (#2277); none offers neither. */
    submitWrite?: ((payload: WriteOpPayload) => Promise<QueuedWrite>) | null;
    /** The pull request's source, whose run's end reads the files again for a queued reply. */
    source?: string | null;
    /** Hears the refresh that follows a reply or a resolve landing; the bridge's when left out. */
    listen?: EchoListen | undefined;
    follower?: WriteFollower | undefined;
    /** The renderer port, for tests; the app's by default. */
    render?: RenderPort;
  } = $props();

  /** The surface the replies and resolves queued here are tagged with (#2277). */
  const SURFACE = "pr-files";

  /** `undefined` while the *Files* read has not answered. */
  let view = $state<SourceView | null | undefined>(undefined);
  let failed = $state<string | null>(null);
  let viewed = $state<string[]>([]);

  /** The blob each file holds in `answer`'s diff, by path (#2276): what *Viewed* is kept by. */
  const blobsOf = (answer: SourceView | null): Record<string, string> => {
    const out: Record<string, string> = {};
    for (const file of diffOf(answer)?.files ?? []) if (file.blob) out[file.path] = file.blob;
    return out;
  };

  /** The diff read (again, `again`): its head stamps new drafts (#2276), its blobs read *Viewed*. */
  function read(id: string, live: () => boolean, again = false) {
    entityView(id, "files").then(
      (answer) => {
        if (!live()) return;
        view = answer;
        const head = diffOf(answer)?.commentable?.head ?? null;
        if (head !== null) pending.seen(id, head);
        untrack(() => settleEchoes(id, (echo) => landed(answer, echo)));
        viewedFiles(id, blobsOf(answer)).then(
          (files) => {
            if (live()) viewed = files;
          },
          () => {},
        );
      },
      (error) => {
        if (live() && !again) failed = ipcErrorMessage(error);
      },
    );
  }

  $effect(() => {
    const id = prId;
    let live = true;
    view = undefined;
    failed = null;
    viewed = [];
    read(id, () => live);
    return () => {
      live = false;
    };
  });

  // A reply or resolve that landed is read back here (#2277): the refresh after it reads the files again.
  $effect(() => {
    return untrack(() =>
      watchEchoes(
        SURFACE,
        () => {
          const id = prId;
          if (view != null) read(id, () => prId === id, true);
        },
        { listen },
      ),
    );
  });

  const diff = $derived(view ? diffOf(view) : null);

  /** The thread of `answer` an echo is about, by the thread's id. */
  function threadOf(answer: SourceView | null, id: string): LineThread | null {
    const found = diffOf(answer);
    for (const file of found?.files ?? []) for (const thread of file.threads) if (thread.id === id) return thread;
    for (const g of found?.gone ?? []) if (g.thread.id === id) return g.thread;
    return null;
  }

  /** Whether `answer` shows `echo` landed: the reply's words in its thread, the thread resolved or open as asked. */
  function landed(answer: SourceView | null, echo: Echo): boolean {
    const [kind, id] = echo.kind.split("\n");
    if (id === undefined) return false;
    const thread = threadOf(answer, id);
    if (kind === "reply") return thread?.comments.some((c) => sameWords(c.body, echo.text)) ?? false;
    if (kind === "resolve") return thread !== null && (thread.resolved_by !== null) === (echo.text === "resolved");
    return false;
  }

  async function toggle(path: string, on: boolean) {
    try {
      viewed = await setViewed(prId, path, on, blobsOf(view ?? null));
    } catch (cause) {
      push({ text: `Could not mark ${path}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** Queue `payload` and echo it as `kind`; whether it was queued. */
  async function queue(payload: WriteOpPayload, kind: string, text: string, what: string, many: boolean): Promise<boolean> {
    if (submitWrite === null) return false;
    try {
      const write = await submitWrite(payload);
      echoWrite(prId, kind, text, { write, many, surface: SURFACE, source, following: { name: what.charAt(0).toUpperCase() + what.slice(1) }, ...(follower ? { follower } : {}) });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the ${what}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  const commentable = $derived(diff?.commentable ?? null);
  const review = $derived(
    commentable
      ? {
          entity: prId,
          pending,
          reply:
            commentable.reply && submitWrite
              ? (path: string, thread: LineThread, body: string) =>
                  thread.id === null
                    ? Promise.resolve(false)
                    : queue(
                        { ReplyToThread: { entity: prId, thread: thread.id, comment: { path, line: thread.line, side: thread.side, body } } },
                        `reply\n${thread.id}`,
                        body,
                        "reply",
                        true,
                      )
              : null,
          resolve:
            commentable.resolve && submitWrite
              ? (thread: LineThread, on: boolean) =>
                  thread.id === null
                    ? Promise.resolve(false)
                    : queue({ ResolveThread: { entity: prId, thread: thread.id, resolved: on } }, `resolve\n${thread.id}`, on ? "resolved" : "open", on ? "resolve" : "unresolve", false)
              : null,
          queued: (thread: LineThread) => (thread.id === null ? [] : echoesOn(prId, `reply\n${thread.id}`).map((echo) => echo.text)),
          resolved: (thread: LineThread) => {
            const asked = thread.id === null ? null : echoOn(prId, `resolve\n${thread.id}`);
            return asked === null ? thread.resolved_by !== null : asked.text === "resolved";
          },
        }
      : null,
  );
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
      {review}
      cut={diff?.truncated ?? false}
      {...render ? { render } : {}}
    />
  {/if}
{/if}
