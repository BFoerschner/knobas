<!--
  **The one "nothing here" state** (design system, Layout): what every page draws for an address that names nothing,
  of any kind. Centred, one sentence with no id and no word of the app's insides, one action.
  The bar above it, when there is one, keeps the path to the kind (*Code › Pull request*), never an empty crumb.

  A read that failed for another reason is the same block with the reason and *Try again* (`onretry`).
-->
<script lang="ts">
  let {
    message,
    href,
    action,
    onretry,
  }: {
    message: string;
    /** Where the one action goes. */
    href?: string;
    action?: string;
    /** In place of a link: read again, after a read that failed. */
    onretry?: (() => void) | undefined;
  } = $props();
</script>

<div class="tf-empty tf-nohere">
  <p>{message}</p>
  {#if onretry}
    <button type="button" class="tf-btn tf-sm" onclick={onretry}>Try again</button>
  {:else if href !== undefined && action !== undefined}
    <a class="tf-btn tf-sm" {href}>{action}</a>
  {/if}
</div>
