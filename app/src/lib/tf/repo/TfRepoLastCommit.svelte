<!--
  The last commit at the branch the tabs read at, above the README on the
  repository's landing (#2257, mockups/code-area `.lastc`): who made it, an
  avatar of their initials and their name (#2311), its message, its sha (to
  its diff), when, and *N commits* to Commits -- the section's count, every
  commit the branch has where the source counts them, else *History*. It is
  the first row of the source's `commits` section, the read Commits makes, so
  the app's read cache answers the two together.

  A skeleton while the first read is out; a re-read (another branch, a
  refresh) keeps the commit drawn until it answers. The strip is an extra
  on the landing: a read that fails, or a repository with no commit, draws
  nothing rather than an error above the README.
-->
<script lang="ts">
  import type { ViewRow } from "../../ipc/entity";
  import { untrack } from "svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { onViewFresh } from "./view-fresh.svelte";
  import { commitHash, repoHash, viewRows } from "./repo";
  import { authorOf, initials } from "./file-tree";

  let {
    ports,
    repoId,
    at,
    refreshed = 0,
  }: {
    /** `codeFacts` says whose the commit is; without it the strip names the author as the source does. */
    ports: Pick<RepoPorts, "entityView"> & Partial<Pick<RepoPorts, "codeFacts" | "listen">>;
    repoId: string;
    /** The entity the commits are read at: the repository, or the branch picked. */
    at: string;
    /** The page's refreshes (#2135): each reads the commit again over what the strip drew. */
    refreshed?: number;
  } = $props();

  /** `undefined` before the first answer; `null` where there is none to draw. */
  let last = $state<ViewRow | null | undefined>(undefined);
  /** How many commits the Commits tab counts, `null` where the source said not. */
  let total = $state<number | null>(null);
  const by = $derived(last ? authorOf(last) : null);
  /** Whether that commit is the reader's own, `code_facts`' word: it reads *You*, as the Commits tab says. */
  let mine = $state(false);

  function read(where: string, alive: () => boolean, again: boolean): Promise<unknown> {
    return ports.entityView(where, "commits").then(
      (view) => {
        if (!alive()) return;
        last = viewRows(view?.sections[0])[0] ?? null;
        total = view?.sections[0]?.count == null ? null : Number(view.sections[0].count);
        const id = last?.entity_id ?? null;
        if (id === null) {
          mine = false;
          return;
        }
        return ports.codeFacts?.([id]).then(
          (facts) => {
            if (alive() && last?.entity_id === id) mine = facts.some((entry) => entry.entity_id === id && entry.mine);
          },
          () => {},
        );
      },
      () => {
        if (alive() && !again) last = null;
      },
    );
  }

  // Kept by the app and read again behind its answer (PR #2479): a new tip is drawn over the strip.
  onViewFresh(
    () => at,
    "commits",
    () => {
      const where = at;
      void read(where, () => at === where, true);
    },
    untrack(() => ports.listen),
  );

  $effect(() => {
    const where = at;
    let live = true;
    read(where, () => live, false);
    return () => {
      live = false;
    };
  });

  onRefreshed(
    () => refreshed,
    () => {
      const where = at;
      return read(where, () => at === where, true);
    },
  );
</script>

{#if last === undefined}
  <div class="tf-lastc tf-skel" aria-busy="true">
    <span class="tf-skel-bar"></span><span class="tf-skel-bar"></span>
  </div>
{:else if last !== null}
  <!-- The mockup's order: who, the message, the sha, when, then the way to the history. -->
  <div class="tf-lastc">
    {#if by?.author}
      <span class="tf-av" class:tf-me={mine} aria-hidden="true">{initials(by.author)}</span><b class="tf-lastc-by">{mine ? "You" : by.author}</b>
    {/if}
    <span class="tf-lastc-msg">{last.title}</span>
    {#if last.entity_id !== null}
      <a class="tf-mono" href={commitHash(repoId, last.entity_id)}>{last.chip ?? ""}</a>
    {:else if last.chip !== null}
      <span class="tf-mono">{last.chip}</span>
    {/if}
    {#if by?.when}<span class="tf-dim tf-small">{by.when}</span>{/if}
    <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}
      >{total === null ? "History" : `${total.toLocaleString("en-US")} ${total === 1 ? "commit" : "commits"}`}</a
    >
  </div>
{/if}
