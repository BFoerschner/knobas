<!--
  The last commit at the branch the tabs read at, above the README on the
  repository's landing (#2257, mockups/code-area `.lastc`): its message, its
  sha (to its diff), who and when as the source says it, and *History*
  to Commits. It is the first row of the source's `commits` section, the
  read Commits makes, so the app's read cache answers the two together.

  A skeleton while the first read is out; a re-read (another branch, a
  refresh) keeps the commit drawn until it answers. The strip is an extra
  on the landing: a read that fails, or a repository with no commit, draws
  nothing rather than an error above the README.
-->
<script lang="ts">
  import type { ViewRow } from "../../ipc/entity";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { commitHash, repoHash, viewRows } from "./repo";

  let {
    ports,
    repoId,
    at,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView">;
    repoId: string;
    /** The entity the commits are read at: the repository, or the branch picked. */
    at: string;
    /** The page's refreshes (#2135): each reads the commit again over what the strip drew. */
    refreshed?: number;
  } = $props();

  /** `undefined` before the first answer; `null` where there is none to draw. */
  let last = $state<ViewRow | null | undefined>(undefined);

  function read(where: string, alive: () => boolean, again: boolean): Promise<unknown> {
    return ports.entityView(where, "commits").then(
      (view) => {
        if (alive()) last = viewRows(view?.sections[0])[0] ?? null;
      },
      () => {
        if (alive() && !again) last = null;
      },
    );
  }

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
  <!-- The mockup's order: the message, then the sha, then who and when, then the way to the history. -->
  <div class="tf-lastc">
    <span class="tf-lastc-msg">{last.title}</span>
    {#if last.entity_id !== null}
      <a class="tf-mono" href={commitHash(repoId, last.entity_id)}>{last.chip ?? ""}</a>
    {:else if last.chip !== null}
      <span class="tf-mono">{last.chip}</span>
    {/if}
    {#if last.meta !== null}<span class="tf-dim tf-small">{last.meta}</span>{/if}
    <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}>History</a>
  </div>
{/if}
