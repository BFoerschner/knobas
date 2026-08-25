<!--
  A source's two-letter monogram: `JI`, `GT`, `TC`.

  `tone="err"` is the 401 state the top strip highlights (spec §2). The dot the
  stylesheet draws is `.sync .mg::after`, so a monogram outside a `.sync` group
  carries no status lamp — which is right: a monogram in a row is an
  identifier, not a health reading.
-->
<script lang="ts">
  let {
    text,
    tone = "ok",
    label,
  }: {
    text: string;
    tone?: "ok" | "err";
    /** The source's display name, so the letters are not the only clue. */
    label?: string;
  } = $props();
</script>

<span class="mg {tone === 'err' ? 'err' : ''}" title={label} aria-label={label}>{text}</span>
