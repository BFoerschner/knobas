<!-- How often (section 7): the one loud token, heavy where the element must stand. Its title says it in words. -->
<script lang="ts">
  import type { Occurs } from "../../../render/xsd-answer";
  import { occText, occWords } from "../../../render/xsd-view";

  let { occ }: { occ: Occurs } = $props();
</script>

<span class="tf-xsd-occ" class:tf-xsd-may={occ.min === 0} title={occWords(occ)}>{occText(occ)}</span>
