<!--
  The people picker (ADR-0054, the 2026-11-21 ruling's sections 18 and 25): a field over a list, standing in the float
  host under the value it changes, as a `TfMenu` stands under its opener. `ValuePicker` is the pattern for the field
  and its keys, but it is a form's inline control with toggles; this one picks one value and closes.

  It opens at once with *Me*, *Unassigned* (unless the field is required) and the current assignee; the source's people
  join below them in name order when the list lands, and no row drawn moves. Typing ranks every row by `fuzzyRank` over
  the name and the account, the matched letters marked, with no request a key; where the source cut its list short, the
  page asks the source 150 ms after the last key and what it adds joins at the end.

  `↑`/`↓` move, `Enter` picks, `Esc` clears the words and, with none, closes. A pick closes it and focuses the opener.
  On the layer stack as a dialog is, so the page's keys wait while it stands.

  The same picker serves a `search` field that is not a person (an adapter's own searched list): with `person` false it
  leads with nothing and names its rows by label alone.
-->
<script lang="ts" module>
  /** How long after the last key the source is asked, where its list is not whole. */
  export const ASK_AFTER_MS = 150;

  /** The first `length` letters of marked runs: a row's name, out of the name and account it was ranked on. */
  export function cut(runs: readonly { text: string; hit: boolean }[], length: number): { text: string; hit: boolean }[] {
    const kept: { text: string; hit: boolean }[] = [];
    let left = length;
    for (const run of runs) {
      if (left <= 0) break;
      kept.push({ text: run.text.slice(0, left), hit: run.hit });
      left -= run.text.length;
    }
    return kept;
  }

  /** Two letters for a person's avatar: the first of the first and last words. */
  export function initials(name: string): string {
    const words = name.replace(/^Me \((.*)\)$/, "$1").trim().split(/\s+/).filter(Boolean);
    if (words.length === 0) return "?";
    const first = words[0]![0] ?? "";
    const last = words.length > 1 ? (words[words.length - 1]![0] ?? "") : (words[0]![1] ?? "");
    return `${first}${last}`.toUpperCase();
  }
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import { HOLD_MS } from "../still-press";

  import { fuzzyRank } from "../filter/fuzzy";
  import { inFloatHost, placeFloat } from "../floats";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import type { FieldOption } from "../../ipc/entity";

  /** The most rows drawn at once; typing narrows the rest into reach. */
  const DRAWN_MOST = 60;

  let {
    open = $bindable(false),
    opener,
    label,
    current,
    me = null,
    person = true,
    required = false,
    options,
    complete,
    asking = false,
    failed = null,
    sourceName,
    scope,
    onpick,
    onsearch,
    layers = sharedLayers,
  }: {
    open: boolean;
    opener: HTMLElement | null;
    /** The field's name, the picker's accessible name: *Assignee*. */
    label: string;
    /** What the field holds now: its value and how it reads, or `null` for none. */
    current: { value: string; shown: string } | null;
    /** The reader's own account in this source, where knobas knows it. */
    me?: string | null;
    /** The options are people: lead with *Me* and *Unassigned*. */
    person?: boolean;
    /** It may not be cleared: no *Unassigned*. */
    required?: boolean;
    /** What the window holds of the list. */
    options: readonly FieldOption[];
    /** The list is every choice there is. */
    complete: boolean;
    /** The list is being read. */
    asking?: boolean;
    /** Why the list could not be read. */
    failed?: string | null;
    /** The source's name, for the line under the list. */
    sourceName: string;
    /** The list's scope in the source's words (*PAY*), for the line under the list. */
    scope: string;
    /** A value picked: its account, or `null` for nobody. */
    onpick: (value: string | null, shown: string) => void;
    /** Words typed into a list that is not whole: the page asks the source for them. */
    onsearch?: (typed: string) => void;
    layers?: Layers | undefined;
  } = $props();

  const uid = $props.id();
  let typed = $state("");
  let at = $state(0);
  let box: HTMLElement | null = null;
  let field: HTMLInputElement | null = $state(null);
  /** When it opened, and how long an `Enter` with nothing typed is taken as the opener's second press: a dialog's hold (`still-press.ts`). */
  let openedAt = 0;
  const ARRIVAL_MS = HOLD_MS;
  let asked: ReturnType<typeof setTimeout> | undefined;

  /** One row: an option, or *Unassigned* (`value: null`). */
  interface Row {
    value: string | null;
    label: string;
    hint: string | null;
  }

  /**
   * The rows in the order they stand: the leading three, then the source's people in name order. The leading rows
   * come from what the picker knew as it opened, so the list landing appends and moves nothing drawn.
   */
  const rows = $derived.by((): Row[] => {
    const lead: Row[] = [];
    const seen = new Set<string>();
    const named = (value: string) => options.find((option) => option.value === value);
    if (person && me !== null) {
      const mine = named(me);
      lead.push({ value: me, label: `Me (${mine?.label ?? (current?.value === me ? current.shown : me)})`, hint: me });
      seen.add(me);
    }
    if (person && !required) lead.push({ value: null, label: "Unassigned", hint: null });
    if (current !== null && !seen.has(current.value)) {
      lead.push({ value: current.value, label: current.shown, hint: current.shown === current.value ? null : current.value });
      seen.add(current.value);
    }
    const rest = options
      .filter((option) => !seen.has(option.value))
      .map((option) => ({ value: option.value, label: option.label, hint: option.hint ?? null }));
    if (complete) rest.sort((a, b) => a.label.localeCompare(b.label));
    return [...lead, ...rest];
  });

  const hits = $derived(fuzzyRank(rows, typed, (row) => (row.hint === null ? row.label : `${row.label} ${row.hint}`)));
  const drawn = $derived(hits.slice(0, DRAWN_MOST));
  const optionId = (index: number) => `${uid}-opt-${index}`;

  /** The one line under the list: how many of how many, where the list is read, or why it is not. */
  const foot = $derived.by(() => {
    if (failed !== null) return `${sourceName}'s list could not be read: ${failed}`;
    if (asking && options.length === 0) return `Asking ${sourceName}…`;
    const people = person ? "people who can be assigned" : "choices";
    const total = complete ? `${options.length}` : `${options.length}+`;
    return typed.trim() === "" ? `${total} ${people} in ${scope}` : `${hits.length} of ${total} ${people} in ${scope}`;
  });

  function search(text: string) {
    typed = text;
    at = 0;
    clearTimeout(asked);
    if (!complete && onsearch !== undefined && text.trim() !== "") {
      const words = text.trim();
      asked = setTimeout(() => onsearch?.(words), ASK_AFTER_MS);
    }
  }

  async function move(by: 1 | -1) {
    at = Math.min(Math.max(at + by, 0), Math.max(drawn.length - 1, 0));
    await tick();
    document.getElementById(optionId(at))?.scrollIntoView?.({ block: "nearest" });
  }

  function close() {
    clearTimeout(asked);
    open = false;
    untrack(() => opener)?.focus({ preventScroll: true });
  }

  function pick(row: Row) {
    onpick(row.value, row.value === null ? "Unassigned" : row.label.replace(/^Me \((.*)\)$/, "$1"));
    close();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      openedAt = 0;
      event.preventDefault();
      void move(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      // `Enter` on arrival (the second of a double press on the opener) picks nothing: the press rule.
      if (typed === "" && performance.now() - openedAt < ARRIVAL_MS) return;
      const hit = drawn[at];
      if (hit !== undefined) pick(hit.option);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (typed !== "") search("");
      else close();
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
        const size = { width: node.offsetWidth, height: node.offsetHeight };
        const view = { width: window.innerWidth, height: window.innerHeight };
        const spotted = placeFloat(opener.getBoundingClientRect(), size, view, "start");
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
      clearTimeout(asked);
      open = false;
    };
    window.addEventListener("mousedown", away, true);
    openedAt = performance.now();
    queueMicrotask(() => field?.focus({ preventScroll: true }));
    return () => {
      untrack(off);
      window.removeEventListener("mousedown", away, true);
      clearTimeout(asked);
    };
  });

  // Each opening starts with no words, at the first row.
  $effect.pre(() => {
    if (open) untrack(() => search(""));
  });
</script>

{#if open}
  <div class="tf-pick" role="dialog" aria-label={label} {@attach place}>
    <input
      bind:this={field}
      type="text"
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      aria-autocomplete="list"
      aria-expanded="true"
      aria-controls="{uid}-opts"
      aria-activedescendant={drawn.length > 0 ? optionId(at) : undefined}
      aria-label="{label}: type a name"
      placeholder={person ? "Type a name" : "Type to search"}
      value={typed}
      oninput={(event) => search(event.currentTarget.value)}
      {onkeydown}
    />
    <div class="tf-pick-opts" id="{uid}-opts" role="listbox" aria-label={label}>
      {#each drawn as { option: row, runs }, index (row.value ?? "")}
        <button
          type="button"
          role="option"
          tabindex="-1"
          id={optionId(index)}
          class="tf-pick-opt"
          class:tf-at={index === at}
          aria-selected={(row.value ?? null) === (current?.value ?? null)}
          onmousedown={(event) => event.preventDefault()}
          onclick={() => pick(row)}
        >
          {#if person}<span class="tf-av" aria-hidden="true">{row.value === null ? "–" : initials(row.label)}</span>{/if}
          <span class="tf-pick-name">{#each cut(runs, row.label.length) as run, i (i)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}</span>
          {#if row.hint !== null && row.hint !== row.label}<small>{row.hint}</small>{/if}
        </button>
      {/each}
      {#if drawn.length === 0}
        <p class="tf-pick-none">No one matches {typed.trim()}.</p>
      {/if}
    </div>
    <p class="tf-pick-foot" class:tf-err={failed !== null}>{foot}</p>
  </div>
{/if}
