<!--
  A split-flap cell.

  Spec §2 (Rec 08-24): **flaps are for values that change while you watch** —
  the sync countdown, a health transition, a count that moves. A static reading
  is plain mono type; a board where everything flaps is a board where nothing
  reads as news.

  The mockup drove these through a global `FL` map and a `settleFlaps()` sweep
  after each full-page re-render (`signal-miller.html:2199-2220`). That does not
  carry over — it exists only because the mockup threw its DOM away every
  frame. Here the component remembers its own previous value, which is the
  whole of the state a flap has.
-->
<script lang="ts">
  /** How long the two leaves take, matching `.flap .leaf`'s keyframes. */
  const FLIP_MS = 360;

  let {
    value,
    tone = "plain",
    width,
    big = false,
    label,
  }: {
    value: string;
    tone?: "plain" | "amber" | "fail" | "ok" | "dim";
    width?: "s" | "t" | "m" | "b" | "r";
    big?: boolean;
    /** What the value *is*, for a reader who cannot see the board. */
    label?: string;
  } = $props();

  /**
   * The value on its way out, or `null` between flips.
   *
   * The only state a flap has. Both halves always render `value` itself:
   * mirroring it into a second rune would make the effect below depend on its
   * own write, which cancels the timer it just set and leaves a leaf frozen
   * over the cell for ever.
   */
  let leaving = $state<string | null>(null);
  /**
   * The last value a flip was run for. Plain, not a rune, so the effect has
   * exactly one dependency: `value`.
   */
  // svelte-ignore state_referenced_locally
  // Reading `value` once here is the point: a freshly mounted flap has nothing
  // to flip *from*, so its first render must be still. Making this reactive
  // would animate every cell on every room switch.
  let flipped = value;

  // Read once, on mount: the leaves are decoration, and re-reading the query
  // on every tick would cost a layout for a value that effectively never
  // changes mid-session.
  const reduced =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  $effect(() => {
    const next = value;
    if (next === flipped) return;
    const previous = flipped;
    flipped = next;
    if (reduced) return;
    leaving = previous;
    // Cleared rather than left behind: `.leaf` is absolutely positioned over
    // the cell, so a leaf that outlived its animation would cover the value.
    const timer = setTimeout(() => {
      leaving = null;
    }, FLIP_MS);
    return () => clearTimeout(timer);
  });

  const classes = $derived(
    ["flap", tone === "plain" ? "" : tone, width ? `w-${width}` : "", big ? "big" : ""]
      .filter(Boolean)
      .join(" "),
  );
</script>

<span class={classes} aria-label={label} aria-live="polite">
  <span class="ft">{value}</span>
  <span class="fb">{value}</span>
  {#if leaving}
    <!--
      Two leaves, and they carry different values: the top one falls away
      showing what is leaving, the bottom one swings up showing what has
      arrived. `aria-hidden`, because the halves above already say the value
      and a screen reader announcing it three times is noise.
    -->
    <span class="leaf top" aria-hidden="true">{leaving}</span>
    <span class="leaf bot" aria-hidden="true">{value}</span>
  {/if}
</span>
