<!-- Words with code in them: a fact, a note, a line seen while reading. -->
<script lang="ts">
  // Its own stylesheet: the XML view draws it too, before any schema file has loaded the explorer's.
  import "../../css/xsd.css";
  import type { Run } from "../../../render/xsd-answer";

  /** The code face's class: the explorer's chip, or the XML view's plain code (`tf-xv-code`) in a document's words. */
  let { runs, cls = "tf-xsd-code" }: { runs: readonly Run[]; cls?: string } = $props();
</script>

{#each runs as run, i (i)}{#if run.code}<code class={cls}>{run.text}</code>{:else}{run.text}{/if}{/each}
