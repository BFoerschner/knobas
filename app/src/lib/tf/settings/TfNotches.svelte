<!--
  Settings › Fonts' slider: a row of notches, one a step, the default taller, the chosen one a filled bar, and a ring
  on one notch where it means something (the size that evens a face out, the tallest code line spacing at which box
  drawing still joins). Every knob of the pane is one (2026-11-18 for the size; 2026-11-21 for line spacing, letter
  spacing and weight, `docs/decisions/2026-11-21-fonts-you-can-tune.md`, Fable's ruling 6).

  Keys, as the size slider always had them: the arrows step one notch, `Home` and `End` go to the ends, `0` goes home
  (`onHome` when given: code's line spacing goes back to *Joined*). With no notch chosen (a preset pressed), an arrow
  steps from `from`, the value drawn, to the next notch that way. A press applies at once: one custom property on
  `<html>`, one frame.
-->
<script lang="ts">
  let {
    label,
    notches,
    value,
    home,
    ring = null,
    from = null,
    ringTitle = null,
    text,
    onPick,
    onHome = null,
  }: {
    /** The slider's name for assistive tech. */
    label: string;
    notches: readonly number[];
    /** The chosen notch, or `null` when none is (a preset is). */
    value: number | null;
    /** The default notch, drawn taller; `null` when the default is no notch (code's *Joined*). */
    home: number | null;
    ring?: number | null;
    /** Where an arrow steps from when no notch is chosen. */
    from?: number | null;
    /** What the ringed notch's title says. */
    ringTitle?: string | null;
    /** A notch's value as words (`aria-valuetext`, titles). */
    text: (at: number) => string;
    onPick: (at: number) => void;
    onHome?: (() => void) | null;
  } = $props();

  function onKey(event: KeyboardEvent): void {
    const at = value === null ? -1 : notches.indexOf(value);
    let to: number | null = null;
    const up = event.key === "ArrowRight" || event.key === "ArrowUp";
    const down = event.key === "ArrowLeft" || event.key === "ArrowDown";
    if ((up || down) && at < 0) {
      // No notch chosen: the next one that way from what is drawn.
      const start = from ?? home ?? notches[0]!;
      to = up ? (notches.find((n) => n > start + 1e-9) ?? notches.at(-1)!) : ([...notches].reverse().find((n) => n < start - 1e-9) ?? notches[0]!);
    } else if (up) to = notches[Math.min(notches.length - 1, at + 1)]!;
    else if (down) to = notches[Math.max(0, at - 1)]!;
    else if (event.key === "Home") to = notches[0]!;
    else if (event.key === "End") to = notches.at(-1)!;
    else if (event.key === "0") {
      event.preventDefault();
      if (onHome) onHome();
      else if (home !== null) onPick(home);
      return;
    }
    if (to === null) return;
    event.preventDefault();
    onPick(to);
  }
</script>

<div
  class="tf-fr-scale"
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin={notches[0]}
  aria-valuemax={notches.at(-1)}
  aria-valuenow={value ?? from ?? home ?? notches[0]}
  aria-valuetext={value === null ? undefined : text(value)}
  onkeydown={onKey}
>
  {#each notches as at (at)}
    <button
      type="button"
      tabindex="-1"
      class="tf-fr-notch"
      class:tf-fr-home={at === home}
      class:tf-fr-even={at === ring}
      class:tf-fr-on={at === value}
      aria-hidden="true"
      title={at === ring && ringTitle ? `${text(at)}: ${ringTitle}` : text(at)}
      onclick={() => onPick(at)}><i></i></button
    >
  {/each}
</div>
