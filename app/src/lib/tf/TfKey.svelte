<!--
  **A code value** (design system, Type): a ticket key, branch, sha or repository name, in mono, one word that never
  breaks at its hyphen. A key links with no underline until pointed at. `lead` is 600, for a key that leads a row, a card
  or a pane; in a sentence it is 400. `chip` draws it as a neutral chip, which names the job without yellow.
-->
<script lang="ts" module>
  /** A ticket key as a place's name reads it: `PAY-231`, or `payout-service#143`. A breadcrumb asks this of its parts. */
  export function isKey(name: string): boolean {
    return /^[A-Z][A-Z0-9_]*-\d+$/.test(name) || /^[\w.-]+#\d+$/.test(name);
  }
</script>

<script lang="ts">
  let {
    value,
    href,
    lead = false,
    chip = false,
    title,
    class: extra = "",
  }: {
    value: string;
    href?: string | undefined;
    lead?: boolean;
    chip?: boolean;
    title?: string | undefined;
    class?: string;
  } = $props();

  const classes = $derived(`tf-mono tf-key ${lead ? "tf-key-lead" : ""} ${chip ? "tf-chip" : ""} ${extra}`.replace(/\s+/g, " ").trim());
</script>

{#if href !== undefined}
  <a class={classes} {href} {title}>{value}</a>
{:else}
  <span class={classes} {title}>{value}</span>
{/if}
