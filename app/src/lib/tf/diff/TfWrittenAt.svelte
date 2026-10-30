<!--
  Where an outdated thread was written (#2318, spec #2234 story 84): "at
  <short sha>", linking that commit's diff, then the hunk the source kept
  with the thread, so its words read against the line as it was. Draws
  nothing for a thread that carries neither (one not outdated, or a source
  that names no commit). Without a repository the sha is plain text.
-->
<script lang="ts">
  import type { LineThread } from "../../ipc/entity";
  import { diffHash } from "./address";

  let { thread, repository = null }: { thread: LineThread; repository?: string | null } = $props();

  const SIGN = { context: " ", add: "+", del: "−" } as const;
  const commit = $derived(thread.commit ?? null);
</script>

{#if commit !== null || thread.hunk}
  <div class="tf-was">
    {#if commit !== null}
      <p class="tf-was-at">
        at {#if repository !== null}<a class="tf-mono" href={diffHash(repository, `${commit}^`, commit)} title="This commit's diff">{commit.slice(0, 7)}</a
          >{:else}<span class="tf-mono">{commit.slice(0, 7)}</span>{/if}
      </p>
    {/if}
    {#if thread.hunk}
      <div class="tf-was-hunk">
        {#each thread.hunk.lines as line, k (k)}
          <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
            <span class="tf-no">{line.old ?? ""}</span><span class="tf-no">{line.new ?? ""}</span><span>{SIGN[line.kind]} {line.text}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
