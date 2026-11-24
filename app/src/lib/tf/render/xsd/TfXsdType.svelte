<!-- A type as a row's type cell says it (section 10): its name to its page, *its own*, or a name not found here. -->
<script lang="ts">
  import type { TypeCell } from "../../../render/xsd-answer";
  import TfXsdName from "./TfXsdName.svelte";

  let { type, ongo }: { type: TypeCell; ongo: (key: string, label: string) => void } = $props();
</script>

{#if type.missing !== null}<span class="tf-xsd-lostname">{type.missing}</span> <span class="tf-xsd-lost">not found here</span>{:else}{#if type.name !== null}<TfXsdName name={type.name} {ongo} />{/if}{#if type.note !== null}{type.name !== null ? " " : ""}<span class="tf-xsd-own">{type.note}</span>{/if}{/if}
