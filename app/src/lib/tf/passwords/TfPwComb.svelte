<!--
  The comb (ADR-0056): thirty ticks that count a hold down, one a second (a 60-second code keeps thirty, two seconds
  each). The ticker in `comb.ts` sets `--n` and the seconds text on these nodes directly, so a tick re-renders nothing;
  this component only registers them. Fixed width, tabular numerals, no transition: the same under reduced motion.
-->
<script lang="ts">
  import { count, seconds } from "./comb";
  import { HOLD_SECS } from "./hold";

  let {
    until,
    span = HOLD_SECS,
    size = "",
    words = seconds,
    ended,
    label,
  }: {
    /** When the count ends, in `Date.now()` milliseconds. */
    until: number;
    /** A full count's length in seconds. */
    span?: number;
    size?: "" | "sm" | "lg";
    words?: (secs: number) => string;
    ended?: () => void;
    /** The words around the seconds: `Hides itself in {left}.` */
    label?: [string, string];
  } = $props();

  let comb = $state<HTMLElement | null>(null);
  let left = $state<HTMLElement | null>(null);

  $effect(() => {
    if (comb === null) return;
    return count({ comb, left, until, span, words, ended });
  });
</script>

<span class="tf-pw-comb{size ? ` tf-pw-${size}` : ''}" aria-hidden="true" bind:this={comb}></span>{#if label}<span
    >{label[0]}<span class="tf-pw-left" bind:this={left}></span>{label[1]}</span
  >{/if}
