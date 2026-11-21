<!--
  One colour of a theme in Settings › Appearance › Code colours (ADR-0051): a swatch, and for a theme of one's own a button that opens a
  small panel under it: the hex field, the platform's own colour well, the button that leaves the colour out (*Use ink*
  for a role; *Auto* for a window colour on the Theme page, which is then derived again) and *Reset to ⟨from⟩'s*.
  Every change is written at once; the theme is kept 300 ms after the last (`themes.svelte.ts`). `Esc` closes the
  panel and leaves focus on the swatch. A built-in's swatches are not buttons.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { layers } from "../layers.svelte";
  import { cssVars } from "../theme/paint";
  import { colour } from "../theme/themes";

  let {
    label,
    value,
    fallback,
    editable,
    inkable = true,
    leave = "Use ink",
    resetTo,
    resetName,
    onchange,
  }: {
    /** What the swatch colours: *Keyword*, *Added line*. */
    label: string;
    /** `#RRGGBB`, or `undefined` for ink (a role) or knobas's own (a diff colour). */
    value: string | undefined;
    /** What draws when `value` is `undefined`, for the swatch and the well. */
    fallback: string;
    editable: boolean;
    inkable?: boolean;
    /** The name of the button that leaves the colour out: *Use ink* for a role, *Auto* for a window colour. */
    leave?: string;
    /** The colour the theme it came from has here, `undefined` for ink, `null` when there is none to go back to. */
    resetTo?: string | undefined | null;
    resetName?: string;
    onchange?: (next: string | undefined) => void;
  } = $props();

  let open = $state(false);
  let swatch = $state<HTMLButtonElement | null>(null);
  let typed = $state("");
  // A role left out draws in ink; a window colour left out is derived, and its swatch shows what was derived.
  const ink = $derived(value === undefined && inkable && leave === "Use ink");

  function close(focus = true): void {
    open = false;
    if (focus) void tick().then(() => swatch?.focus());
  }

  let holder = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "panel", close }));
    // A press anywhere else closes it, focus going where the press put it.
    const away = (event: PointerEvent) => {
      if (holder && !holder.contains(event.target as Node)) close(false);
    };
    window.addEventListener("pointerdown", away, true);
    return () => {
      off();
      window.removeEventListener("pointerdown", away, true);
    };
  });

  function toggle(): void {
    open = !open;
    typed = value ?? "";
  }

  function fromField(): void {
    const c = colour(typed.startsWith("#") ? typed : `#${typed}`);
    if (c !== null) onchange?.(c);
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Enter" && (event.target as HTMLElement).tagName === "INPUT") {
      fromField();
    }
  }
</script>

{#if editable}
  <span class="tf-thm-swp" bind:this={holder}>
    <button
      type="button"
      class="tf-thm-sw"
      class:tf-ink={ink}
      aria-label="Change {label}"
      aria-expanded={open}
      bind:this={swatch}
      onclick={toggle}
      {@attach cssVars(() => (ink ? {} : { background: value ?? fallback }))}
    ></button>
    {#if open}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div class="tf-thm-pop" role="group" aria-label={label} onkeydown={keydown}>
        <label class="tf-thm-popf"
          ><span>Hex</span><input
            class="tf-stg-in"
            aria-label="{label} as hex"
            spellcheck="false"
            autocomplete="off"
            bind:value={typed}
            oninput={fromField}
            aria-invalid={typed !== "" && colour(typed.startsWith("#") ? typed : `#${typed}`) === null}
          /></label
        >
        <label class="tf-thm-popf"
          ><span>Pick</span><input type="color" aria-label="Pick {label}" value={value ?? fallback} oninput={(event) => onchange?.((event.currentTarget as HTMLInputElement).value.toUpperCase())} /></label
        >
        <div class="tf-thm-popa">
          {#if inkable}
            <button type="button" class="tf-btn tf-sm" disabled={value === undefined} onclick={() => ((typed = ""), onchange?.(undefined))}>{leave}</button>
          {/if}
          {#if resetTo !== null && resetName}
            <button type="button" class="tf-btn tf-sm" disabled={resetTo === value} onclick={() => ((typed = resetTo ?? ""), onchange?.(resetTo))}>Reset to {resetName}'s</button>
          {/if}
        </div>
      </div>
    {/if}
  </span>
{:else}
  <span class="tf-thm-sw" class:tf-ink={ink} role="img" aria-label="{label}: {value ?? (inkable ? 'ink' : fallback)}" {@attach cssVars(() => (ink ? {} : { background: value ?? fallback }))}></span>
{/if}
