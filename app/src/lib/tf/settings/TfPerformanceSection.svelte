<!--
  Settings › Performance (#2247, spec #2234 step 3): *Pages kept open*, over
  `pages_kept_open` and `set_pages_kept_open`. The field saves when it is
  left, and what the backend stored goes to the page host at once
  (`kept-pages.svelte.ts`), so a lower number drops the surplus hidden pages
  with no restart.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { KEPT_PAGES_MAX, KEPT_PAGES_MIN, keptPages, type KeptPagesSetting } from "../kept-pages.svelte";
  import type { SectionPorts } from "./ports";
  import { boundedNumber } from "./sections";

  let {
    ports,
    kept = keptPages,
  }: {
    ports: Pick<SectionPorts, "pagesKeptOpen" | "setPagesKeptOpen">;
    /** The page host's setting; the window's when omitted. */
    kept?: Pick<KeptPagesSetting, "set">;
  } = $props();

  /** `null` until the read answers. */
  let stored = $state<number | null>(null);

  $effect(() => {
    void ports
      .pagesKeptOpen()
      .then((read) => {
        stored = read;
        kept.set(read);
      })
      .catch((cause) => push({ text: `Could not read Pages kept open: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function save(input: HTMLInputElement) {
    if (stored === null) return;
    const value = boundedNumber(input.value, KEPT_PAGES_MIN, KEPT_PAGES_MAX, stored);
    if (value !== stored) {
      try {
        stored = await ports.setPagesKeptOpen(value);
        kept.set(stored);
      } catch (cause) {
        push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
      }
    }
    input.value = String(stored);
  }
</script>

<header class="tf-stg-h"><h1>Performance</h1><p>How much knobas keeps ready, against the memory it uses.</p></header>
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
        onchange={(event) => void save(event.currentTarget)}
      />
      <span>pages</span>
    </div>
  </div>
{/if}
