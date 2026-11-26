<!--
  **The job scope** (design system, Layout): one control on every page that narrows to the job. *This job*, with how
  many it holds as a count badge, then *Mine* where a page has one, then *All*. Its default is *All* on every page, and
  a ticket card's row turns *This job* on. The key is not repeated here: the top bar names the job, and a page a card's
  row narrowed to another ticket says so in its bar (*Back to PAY-231*). The button's title names the ticket. It is as
  wide as its words on every page. A choice that counts passes `count`, with `null` while the read is out, which draws
  a dash, so the control does not change width when the number lands.
-->
<script lang="ts">
  let {
    scopes,
    scope,
    onpick,
  }: {
    scopes: readonly { id: string; label: string; count?: number | null | undefined; title?: string | undefined }[];
    /** The scope that is on. */
    scope: string | null;
    onpick: (id: string) => void;
  } = $props();
</script>

<div class="tf-seg" role="group" aria-label="Scope">
  {#each scopes as choice (choice.id)}
    <button type="button" aria-pressed={scope === choice.id} title={choice.title} onclick={() => onpick(choice.id)}
      >{choice.label}{#if choice.count !== undefined}{" "}<span class="tf-count">{choice.count ?? "–"}</span>{/if}</button
    >
  {/each}
</div>
