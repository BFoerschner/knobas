<!--
  One tab body the repository page holds for the visit (#2248, `repo-tabs.ts`):
  hidden while another tab is shown, never unmounted for it. It tells the
  components inside it through `pageVisible()` that they are shown only while
  both the page and this tab are, and keeps the tab's scroll across a hide.
  The wrapper is the page host's `.tf-kept` (`display: contents`), so the
  shot steps skip a hidden tab as they skip a hidden page.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { pageVisible, providePageVisible } from "../kept-pages.svelte";
  import { keepScroll } from "../while-shown.svelte";

  let { shown, children }: { shown: boolean; children: Snippet } = $props();

  const page = pageVisible();
  const visible = () => page() && shown;
  providePageVisible(visible);

  let wrapper = $state<HTMLElement>();
  keepScroll(() => wrapper, () => shown);
</script>

<div class="tf-kept" hidden={!shown} bind:this={wrapper}>{@render children()}</div>
