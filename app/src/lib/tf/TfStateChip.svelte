<!--
  **A state's chip** (design system, Words): the source's word in sentence case (`state-word.ts`) on its tone. Tones
  carry state and only state, so a chip with no tone is the plain neutral one.
-->
<script lang="ts">
  import { stateWord } from "./state-word";

  let {
    text,
    tone = null,
    title,
    class: extra = "",
  }: {
    /** The state as the source or the app words it. */
    text: string | null | undefined;
    /** `ok`, `warn`, `err`, `info` or `muted`, or nothing for the neutral chip. */
    tone?: string | null | undefined;
    title?: string | undefined;
    class?: string;
  } = $props();
</script>

<span class="tf-chip {tone ? `tf-${tone}` : ''} {extra}" {title}>{stateWord(text ?? "")}</span>
