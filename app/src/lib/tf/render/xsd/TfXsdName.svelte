<!--
  A name as the schema explorer writes it (the XSD design, sections 7 and 16): its prefix dim where it comes from
  another file, and a button that goes to its page where it is a declaration.
-->
<script lang="ts">
  import type { Name } from "../../../render/xsd-answer";

  let { name, ongo = null, cls = "" }: { name: Name; ongo?: ((key: string, label: string) => void) | null; cls?: string } = $props();
  const label = $derived(`${name.prefix}${name.local}`);
</script>

{#if name.key !== null && ongo !== null}<button type="button" class="tf-oas-ref {cls}" title="Go to {label}" onclick={() => ongo?.(name.key!, label)}>{#if name.prefix !== ""}<span class="tf-xsd-pre">{name.prefix}</span>{/if}{name.local}</button>{:else}<span class={cls}>{#if name.prefix !== ""}<span class="tf-xsd-pre">{name.prefix}</span>{/if}{name.local}</span>{/if}
