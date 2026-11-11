<!--
  Settings › Performance (#2247, spec #2234 step 3): *Pages kept open*, over
  `pages_kept_open` and `set_pages_kept_open`. The field saves on `Enter` or
  when it is left, says *Saved*, and `Esc` puts the stored value back
  (`saved-field.svelte.ts`). What the backend stored goes to the page host at once
  (`kept-pages.svelte.ts`), so a lower number drops the surplus hidden pages
  with no restart.

  *Smooth scrolling on 120 Hz displays* (2026-11-11, `crates/knobas-app/src/frame_rate.rs`): a switch over
  `smooth_scrolling` and `set_smooth_scrolling`, drawn only where the backend offers it (macOS; it answers
  `null` elsewhere). The window changes at once.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { KEPT_PAGES_MAX, KEPT_PAGES_MIN, keptPages, type KeptPagesSetting } from "../kept-pages.svelte";
  import type { SectionPorts } from "./ports";
  import { SavedWord, savedField } from "./saved-field.svelte";
  import { boundedNumber, clampNote } from "./sections";

  let {
    ports,
    kept = keptPages,
  }: {
    ports: Pick<SectionPorts, "pagesKeptOpen" | "setPagesKeptOpen" | "smoothScrolling" | "setSmoothScrolling">;
    /** The page host's setting; the window's when omitted. */
    kept?: Pick<KeptPagesSetting, "set">;
  } = $props();

  /** `null` until the read answers, and for good where the platform has no switch. */
  let smoothOn = $state<boolean | null>(null);
  let smoothBusy = $state(false);

  $effect(() => {
    void ports
      .smoothScrolling()
      .then((on) => (smoothOn = on))
      .catch((cause) => push({ text: `Could not read Smooth scrolling: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function flipSmooth() {
    if (smoothOn === null || smoothBusy) return;
    smoothBusy = true;
    try {
      // What the backend stored, not what the click asked for.
      smoothOn = await ports.setSmoothScrolling(!smoothOn);
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      smoothBusy = false;
    }
  }

  /** `null` until the read answers. */
  let stored = $state<number | null>(null);
  /** Why the field holds something other than what was typed (F-613). */
  let why = $state<string | null>(null);

  $effect(() => {
    void ports
      .pagesKeptOpen()
      .then((read) => {
        stored = read;
        kept.set(read);
      })
      .catch((cause) => push({ text: `Could not read Pages kept open: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  const said = new SavedWord();

  /** Whether a new value was stored. */
  async function save(input: HTMLInputElement): Promise<boolean> {
    if (stored === null) return false;
    let done = false;
    const value = boundedNumber(input.value, KEPT_PAGES_MIN, KEPT_PAGES_MAX, stored);
    why = clampNote(input.value, KEPT_PAGES_MIN, KEPT_PAGES_MAX, value);
    if (value !== stored) {
      try {
        stored = await ports.setPagesKeptOpen(value);
        kept.set(stored);
        done = true;
      } catch (cause) {
        push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
      }
    }
    input.value = String(stored);
    return done;
  }
</script>

<header class="tf-stg-h"><h1>Performance</h1><p>How much knobas keeps ready and how smoothly it moves, against the memory and power they use.</p></header>
{#if stored !== null}
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-perf-kept"
      >Pages kept open<small
        >Pages you visited recently stay loaded in the background, so going back to one is instant. 0 keeps none;
        each one kept uses memory.</small
      ></label
    >
    <div class="tf-stg-c tf-stg-inl">
      <input
        id="tf-perf-kept"
        type="number"
        min={KEPT_PAGES_MIN}
        max={KEPT_PAGES_MAX}
        class="tf-stg-in tf-stg-n70"
        value={stored}
        aria-invalid={why !== null ? "true" : undefined}
        aria-describedby={why !== null ? "tf-perf-kept-why" : undefined}
        {@attach savedField({ saved: () => String(stored ?? ""), save, word: said, restored: () => (why = null) })}
      />
      <span>pages</span>
      <small class="tf-stg-saved" role="status">{said.shown ? "Saved" : ""}</small>
      {#if why !== null}<small class="tf-stg-why" id="tf-perf-kept-why" role="alert">{why}</small>{/if}
    </div>
  </div>
{/if}
{#if smoothOn !== null}
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-perf-smooth"
      >Smooth scrolling on 120 Hz displays<small
        >Lists and pages move at your display's full rate, up to 120 frames a second, instead of 60. It uses more power
        while something on screen moves.</small
      ></label
    >
    <div class="tf-stg-c tf-stg-inl">
      <button
        id="tf-perf-smooth"
        type="button"
        class="tf-stg-sw"
        role="switch"
        aria-checked={smoothOn}
        disabled={smoothBusy}
        onclick={flipSmooth}><i></i></button
      >
    </div>
  </div>
{/if}
