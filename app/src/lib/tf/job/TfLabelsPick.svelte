<!--
  The labels picker (ADR-0054, section 25): a field over the names on offer, each with a tick where the ticket carries
  it. A tick is one write, one name added or removed, drawn at once; the picker stays open, so `Esc` means only
  "close". Where the source lets a new name be made, the typed words are the last row: *Add "sepa-retry"*.

  Placed, keyed and stacked as the people picker is (`TfPeoplePick.svelte`).
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import { HOLD_MS } from "../still-press";

  import Marked from "../filter/Marked.svelte";
  import { heldOrder } from "../filter/ranked.svelte";
  import { inFloatHost, placeFloat } from "../floats";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";

  let {
    open = $bindable(false),
    opener,
    label,
    carried,
    offered,
    create,
    failed = null,
    asking = false,
    ontoggle,
    layers = sharedLayers,
  }: {
    open: boolean;
    opener: HTMLElement | null;
    label: string;
    /** The names the ticket carries now, echoes included. */
    carried: readonly string[];
    /** The names on offer. */
    offered: readonly string[];
    /** A name the source has not seen may be added. */
    create: boolean;
    failed?: string | null;
    asking?: boolean;
    /** Add `name` (`true`) or remove it. */
    ontoggle: (name: string, add: boolean) => void;
    layers?: Layers | undefined;
  } = $props();

  const uid = $props.id();
  let typed = $state("");
  let at = $state(0);
  let box: HTMLElement | null = null;
  let field: HTMLInputElement | null = $state(null);
  /** When it opened, and how long an `Enter` with nothing typed is taken as the opener's second press: a dialog's hold (`still-press.ts`). */
  let openedAt = -Infinity;
  const ARRIVAL_MS = HOLD_MS;

  /** Every name: those carried first in their order, then those on offer. Fixed while open, so a tick moves no row. */
  let names = $state<string[]>([]);
  $effect.pre(() => {
    if (!open) return;
    const offeredNow = offered;
    untrack(() => {
      const next = names.length === 0 ? [...carried] : [...names];
      for (const name of offeredNow) if (!next.includes(name)) next.push(name);
      names = next;
    });
  });

  /** The most rows drawn at once; typing narrows the rest into reach. */
  const DRAWN_MOST = 60;
  /** The app ranks the names by what was typed (#2535), the closest first. */
  const nameOf = (name: string) => name;
  const order = heldOrder<string>("labels-pick");
  $effect(() => order.ask(typed, names, nameOf));
  const hits = $derived(order.rows(typed, names, nameOf).slice(0, DRAWN_MOST));
  const drawn = $derived(hits);
  const marking = $derived(order.typed(typed, names));
  /** `Enter` before the ranking answered what was typed picks from that answer, once it lands. */
  let entering = $state(false);
  $effect(() => {
    if (!entering || !order.settled(typed, names)) return;
    entering = false;
    pickAt();
  });

  function pickAt() {
    const hit = hits[at];
    if (hit !== undefined) toggle(hit);
    else if (adds) addFresh();
  }
  const fresh = $derived(typed.trim());
  const adds = $derived(create && fresh !== "" && !names.some((name) => name.toLowerCase() === fresh.toLowerCase()));
  const count = $derived(hits.length + (adds ? 1 : 0));
  const optionId = (index: number) => `${uid}-opt-${index}`;

  function search(text: string) {
    typed = text;
    at = 0;
  }

  async function move(by: 1 | -1) {
    at = Math.min(Math.max(at + by, 0), Math.max(count - 1, 0));
    await tick();
    document.getElementById(optionId(at))?.scrollIntoView?.({ block: "nearest" });
  }

  function toggle(name: string) {
    ontoggle(name, !carried.includes(name));
  }

  function addFresh() {
    const name = fresh;
    names = [...names, name];
    ontoggle(name, true);
    search("");
  }

  function close() {
    open = false;
    untrack(() => opener)?.focus({ preventScroll: true });
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      openedAt = -Infinity;
      event.preventDefault();
      void move(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      // `Enter` on arrival (the second of a double press on the opener) picks nothing: the press rule.
      if (typed === "" && performance.now() - openedAt < ARRIVAL_MS) return;
      if (order.settled(typed, names)) pickAt();
      else entering = true;
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Tab") {
      close();
    }
  }

  function place(node: HTMLElement) {
    return untrack(() => {
      box = node;
      const unhost = inFloatHost(node);
      const spot = () => {
        if (opener === null) return;
        const spotted = placeFloat(opener.getBoundingClientRect(), { width: node.offsetWidth, height: node.offsetHeight }, { width: window.innerWidth, height: window.innerHeight }, "start");
        node.style.left = `${spotted.left}px`;
        node.style.top = `${spotted.top}px`;
      };
      spot();
      window.addEventListener("resize", spot);
      window.addEventListener("scroll", spot, true);
      return () => {
        box = null;
        unhost();
        window.removeEventListener("resize", spot);
        window.removeEventListener("scroll", spot, true);
      };
    });
  }

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "dialog", close }));
    const away = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Node && (box?.contains(target) || opener?.contains(target))) return;
      open = false;
    };
    window.addEventListener("mousedown", away, true);
    openedAt = performance.now();
    queueMicrotask(() => field?.focus({ preventScroll: true }));
    return () => {
      untrack(off);
      window.removeEventListener("mousedown", away, true);
      names = [];
      typed = "";
      at = 0;
    };
  });
</script>

{#if open}
  <div class="tf-fpick" role="dialog" aria-label={label} {@attach place}>
    <input
      bind:this={field}
      type="text"
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      aria-autocomplete="list"
      aria-expanded="true"
      aria-controls="{uid}-opts"
      aria-activedescendant={count > 0 ? optionId(at) : undefined}
      aria-label="{label}: type a name"
      placeholder={create ? "Type a label, or a new one" : "Type a label"}
      value={typed}
      oninput={(event) => search(event.currentTarget.value)}
      {onkeydown}
    />
    <div class="tf-fpick-opts" id="{uid}-opts" role="listbox" aria-multiselectable="true" aria-label={label}>
      {#each drawn as name, index (name)}
        <button
          type="button"
          role="option"
          tabindex="-1"
          id={optionId(index)}
          class="tf-fpick-opt"
          class:tf-at={index === at}
          aria-selected={carried.includes(name)}
          onmousedown={(event) => event.preventDefault()}
          onclick={() => toggle(name)}
        >
          <span class="tf-fpick-tick" aria-hidden="true">{carried.includes(name) ? "✓" : ""}</span>
          <span class="tf-fpick-name"><Marked text={name} typed={marking} pairs={order.marks(name)} /></span>
        </button>
      {/each}
      {#if adds}
        <button
          type="button"
          role="option"
          tabindex="-1"
          id={optionId(hits.length)}
          class="tf-fpick-opt"
          class:tf-at={at === hits.length}
          aria-selected="false"
          onmousedown={(event) => event.preventDefault()}
          onclick={addFresh}
        >
          <span class="tf-fpick-tick" aria-hidden="true">+</span>
          <span class="tf-fpick-name">Add “{fresh}”</span>
        </button>
      {/if}
      {#if count === 0}
        <p class="tf-fpick-none">{asking ? "Reading the labels…" : `No label matches ${fresh}.`}</p>
      {/if}
    </div>
    {#if failed !== null}<p class="tf-fpick-foot tf-err">The labels could not be read: {failed}</p>{/if}
  </div>
{/if}
