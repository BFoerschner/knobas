<!--
  One colour of a theme in Settings › Appearance › Code colours (ADR-0051): a swatch, and when `editable` a button that opens a
  small panel under it (its row's hex value opens it too, at the hex field, #2516): its own picker, a square of
  saturation and brightness over a strip of hue (`TfColourPicker.svelte`), the hex field, the platform's own colour well, the button that leaves the colour out (*Use ink*
  for a role; *Auto* for a window colour on the Theme page, which is then derived again) and *Reset to ⟨from⟩'s*.
  Every change is written at once; the theme is kept 300 ms after the last (`themes.svelte.ts`). `Esc` closes the
  panel and leaves focus on the swatch. Both editors make every swatch a button, a built-in's too: its first change
  goes to a copy (#2516).
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { layers } from "../layers.svelte";
  import TfColourPicker from "./TfColourPicker.svelte";
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
    /** `preview`: a colour picker's drag, handed on once a frame; its end, and every other change, without it. */
    onchange?: (next: string | undefined, preview?: boolean) => void;
  } = $props();

  let open = $state(false);
  let swatch = $state<HTMLButtonElement | null>(null);
  /** The panel's Hex field. */
  let field = $state<HTMLInputElement | null>(null);
  /** What opened the panel when it was not the swatch (its row's hex value, #2516): where focus goes back on `Esc`. */
  let opener: HTMLElement | null = null;
  let typed = $state("");
  // A role left out draws in ink; a window colour left out is derived, and its swatch shows what was derived.
  const ink = $derived(value === undefined && inkable && leave === "Use ink");

  function close(focus = true): void {
    open = false;
    const back = opener ?? swatch;
    opener = null;
    if (focus) void tick().then(() => back?.focus());
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
    opener = null;
    typed = value ?? "";
  }

  /** Opens the panel from `from`, its row's hex value (#2516), with focus in the Hex field; `Esc` goes back to `from`. */
  export function openFrom(from: HTMLElement): void {
    open = true;
    opener = from;
    typed = value ?? "";
    void tick().then(() => field?.focus());
  }

  function fromField(): void {
    const c = colour(typed.startsWith("#") ? typed : `#${typed}`);
    if (c !== null) onchange?.(c);
  }

  /**
   * The system's picker sends an `input` for every move of a drag, faster than the screen draws (about 170 a second
   * on Björn's PC), and each one handed on re-reads the whole editor: its colours, figures and preview. So a drag hands
   * on one colour a frame, the last one picked, as a preview (the editor's preview follows, the window waits), and its
   * end (`change`) at once, which colours the window (the themes decisions' risk 9; the ux30 end gate, 2026-11-23: in
   * knobas's WebView2 window a whole-window restyle a frame painted a pick 120 ms late at 4x and 249 ms at 6x).
   * WebKit (the Mac's and Linux's windows) sends a `change` with every pick, in the task of its `input`, and nothing
   * when the panel closes; Chromium sends `change` once, as the picker closes. So a `change` of the value an `input`
   * gave in the same task is a move, and such a drag rests when the theme is kept, 300 ms after the last pick.
   */
  let picked: string | null = null;
  let pickFrame = 0;
  /** A drag's last pick, which the swatch draws while its editor holds `value` until the drag ends; dropped when `value` changes. */
  let dragged = $state<string | null>(null);
  $effect.pre(() => {
    void value;
    untrack(() => (dragged = null));
  });
  /** The value the last `input` gave, until the task it came in ends. */
  let moved: string | null = null;
  function pick(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value.toUpperCase();
    picked = value;
    if (event.type === "change" && moved !== value) return handPick(false);
    if (event.type === "input") {
      moved = value;
      setTimeout(() => {
        if (moved === value) moved = null;
      });
    }
    if (pickFrame === 0) pickFrame = requestAnimationFrame(() => handPick(true));
  }
  /** The panel's own picker (`TfColourPicker.svelte`, #2516): a drag's moves once a frame as a preview, its end and each key at once. */
  function slide(next: string, end: boolean): void {
    picked = next;
    if (end) return handPick(false);
    if (pickFrame === 0) pickFrame = requestAnimationFrame(() => handPick(true));
  }
  function handPick(preview: boolean): void {
    if (pickFrame !== 0) cancelAnimationFrame(pickFrame);
    pickFrame = 0;
    const next = picked;
    picked = null;
    dragged = preview ? next : null;
    // The panel's Hex field is where the moving colour reads as text, while the role list waits for it to rest.
    if (next !== null) typed = next;
    if (next !== null) onchange?.(next, preview);
  }
  $effect(() => () => {
    if (pickFrame !== 0) cancelAnimationFrame(pickFrame);
  });

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
      {@attach cssVars(() => (ink && dragged === null ? {} : { background: dragged ?? value ?? fallback }))}
    ></button>
    {#if open}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div class="tf-thm-pop" role="group" aria-label={label} onkeydown={keydown}>
        <TfColourPicker {label} value={dragged ?? value ?? fallback} onpick={slide} />
        <label class="tf-thm-popf"
          ><span>Hex</span><input
            class="tf-stg-in"
            aria-label="{label} as hex"
            bind:this={field}
            spellcheck="false"
            autocomplete="off"
            bind:value={typed}
            oninput={fromField}
            aria-invalid={typed !== "" && colour(typed.startsWith("#") ? typed : `#${typed}`) === null}
          /></label
        >
        <label class="tf-thm-popf"
          ><span>System</span><input type="color" aria-label="Pick {label}" value={value ?? fallback} oninput={pick} onchange={pick} /></label
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
