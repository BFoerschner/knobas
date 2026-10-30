<!--
  A surface that reads (#2246): what `waiting.ts`'s `waitPhase` decides,
  drawn. Skeleton rows on a first read, the content dimmed and inert while a
  read replaces it, the error in place of either when the read fails.

  ```svelte
  <TfWaiting phase={waitPhase({ drawn: rows !== null, reading, failed: failed !== null })} error={failed} shape="rows">
    <table class="tf-tbl">…</table>
  </TfWaiting>
  ```

  The content keeps one wrapper `div` whether dimmed or not, so a read that
  ends does not draw it anew: a list keeps its scroll and its rows.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { WaitPhase } from "./waiting";

  let {
    phase,
    error = null,
    shape = "rows",
    count = shape === "rows" ? 6 : 12,
    columns = 3,
    failClass = "",
    children,
  }: {
    phase: WaitPhase;
    /** The failed read's message, drawn when `phase` is `failed`. */
    error?: string | null;
    /** What the skeleton looks like: table rows for a list, lines for a file. */
    shape?: "rows" | "lines";
    /** How many skeleton rows or lines. */
    count?: number;
    /** A skeleton row's cells. */
    columns?: number;
    /** More classes on the error line (`tf-fpad` in the file view). */
    failClass?: string;
    children: Snippet;
  } = $props();
</script>

{#if phase === "failed"}
  <p class="tf-cfail {failClass}">{error}</p>
{:else if phase === "skeleton"}
  {#if shape === "rows"}
    <table class="tf-tbl tf-skel" aria-busy="true">
      <tbody>
        {#each { length: count }, n (n)}
          <tr>{#each { length: columns }, c (c)}<td><span class="tf-skel-bar"></span></td>{/each}</tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <div class="tf-skel tf-skel-lines" aria-busy="true">
      {#each { length: count }, n (n)}<span class="tf-skel-bar"></span>{/each}
    </div>
  {/if}
{:else}
  <div class="tf-waiting" class:tf-wait-dim={phase === "dimmed"} aria-busy={phase === "dimmed" ? "true" : undefined} inert={phase === "dimmed"}>
    {@render children()}
  </div>
{/if}
