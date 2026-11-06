<!--
  The Connections drawer (#820, R-6.1, R-6.2): everything connected to the
  job, from the right, over a scrim, without moving the page.

  Top row: a filter field, then *Link to…*, *Apply a link set…* and *Save as
  a link set…* (#869), whose dialogs the job opens. Groups: **Linked on KEY**, the links drawn on
  the ticket, each with *Unlink* on hover or focus and "also in" the sets that
  hold it too; **From Payout stack, via epic PAY-200**, one per link set that
  applies and one for the parent (#867), a set's rows with *Hide here* and the
  group with *Edit set*; and **Hidden on KEY** with *Show again*.

  It draws in the host at the shell's root, as whatever floats does (design
  system, *Layers*), so its one root is the wrapper. It registers with the
  shell's layer stack while open, so `Esc` closes it (rung 2), as does a click
  on the scrim; focus goes to its filter and returns to what opened it, the
  *Connections* button. Its title line is a dialog's, and a row is the shape of
  the ticket card's rows: what it is under its name, its state's dot and word
  where it says one (F-036), then a chevron. A row's action stands in place of
  that state on hover and focus, as the app's row actions do.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { LinkEntry } from "../../ipc/entity";
  import Marked from "../filter/Marked.svelte";
  import { inFloatHost } from "../floats";
  import type { Layers } from "../layers.svelte";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { stateWord } from "../state-word";
  import { toneClass } from "../tone";
  import type { Tone } from "../../ipc/entity";
  import { filterLinks, filterRows, kindWord, type HiddenRow, type JobGroup, type GroupRow } from "./connections";
  import { itemHash } from "./job";
  import { filterKeys } from "../filter-keys";

  let {
    jobKey,
    links,
    groups = [],
    hidden = [],
    alsoIn = () => [],
    layers,
    onclose,
    onunlink,
    onlinkto,
    onhide = () => {},
    onshow = () => {},
    onapplyset = () => {},
    onsaveset = () => {},
  }: {
    jobKey: string;
    /** The links drawn on the ticket. */
    links: LinkEntry[];
    /** The link sets that apply, then the parent (#867): a group each. */
    groups?: JobGroup[];
    /** A set's rows hidden on this ticket (R-7.5). */
    hidden?: HiddenRow[];
    /** The names of the sets that also hold a linked entity. */
    alsoIn?: (entityId: string) => string[];
    layers: Layers;
    onclose: () => void;
    onunlink: (entry: LinkEntry) => void;
    onlinkto: () => void;
    /** *Hide here* on a set's row. */
    onhide?: (row: GroupRow) => void;
    onshow?: (id: string) => void;
    /** *Apply a link set…* and *Save as a link set…* (#869). */
    onapplyset?: () => void;
    onsaveset?: () => void;
  } = $props();

  let field = $state<HTMLInputElement | null>(null);
  let text = $state("");

  const shown = $derived(filterLinks(links, text));

  $effect(() => {
    const restoreTo = document.activeElement;
    const off = untrack(() => layers.add({ kind: "drawer", close: () => onclose() }));
    field?.focus();
    // It draws in the shell's host, so its page hiding would leave it over the next one: a move closes it, as a menu.
    const moved = () => onclose();
    window.addEventListener("hashchange", moved);
    return () => {
      window.removeEventListener("hashchange", moved);
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) restoreTo.focus();
    };
  });
</script>

<!-- A row's dot and word, as the ticket card's rows draw them; held empty so the column lines up down the drawer. -->
{#snippet stateOf(state: { text: string; tone: Tone | null } | null)}
  <span class="tf-lk-st" class:tf-asks={state?.tone === "err" || state?.tone === "warn"}
    >{#if state !== null}<span class="tf-dot {toneClass(state)}" aria-hidden="true"></span>{stateWord(state.text)}{/if}</span
  >
{/snippet}

<div class="tf-drawerhost" {@attach inFloatHost}>
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tf-scrim" onclick={onclose}></div>
<aside class="tf-drawer" aria-label="Connections">
  <header class="tf-dhead">
    <div class="tf-dtitle"><h2>Connections of <TfKey value={jobKey} /></h2></div>
    <button type="button" class="tf-iconbtn tf-dclose" aria-label="Close" onclick={onclose}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </header>
  <div class="tf-dfilter">
    <label class="tf-fsearch">
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      >
      <input bind:this={field} bind:value={text} {@attach filterKeys()} placeholder="Filter connections" aria-label="Filter connections" />
    </label>
    <div class="tf-dacts">
      <button type="button" class="tf-btn tf-sm" onclick={onlinkto}>Link to…</button>
      <button type="button" class="tf-btn tf-sm" onclick={onapplyset}>Apply a link set…</button>
      <button type="button" class="tf-btn tf-sm" onclick={onsaveset}>Save as a link set…</button>
    </div>
  </div>
  <div class="tf-dbody">
    <h3>Linked on <TfKey value={jobKey} lead /> <span class="tf-count">{shown.length}</span></h3>
    {#each shown as entry (entry.link.id)}
      {@const also = alsoIn(entry.other.entity_id)}
      <div class="tf-lk">
        <a class="tf-lk-go" href={itemHash(entry.other)} onclick={onclose}
          ><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={entry.other.kind === "repo"}><Marked text={entry.other.title} typed={text} /></span>{" "}<span class="tf-how">{kindWord(entry.other.kind)}{#if also.length > 0}, also in {also.join(", ")}{/if}</span></span
          >{@render stateOf(entry.other.state ?? null)}<span class="tf-chev" aria-hidden="true">›</span></a
        >
        <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={() => onunlink(entry)} aria-label="Unlink {entry.other.title}">{entry.link.origin === "manual" ? "Unlink" : "Unlink…"}</button>
      </div>
    {:else}
      {#if text.trim() !== ""}
        <p class="tf-dnone">No links match. <button type="button" class="tf-linkish" onclick={() => (text = "")}>Clear the filter</button></p>
      {:else}
        <p class="tf-dnone">Nothing linked on the ticket itself.</p>
      {/if}
    {/each}
    {#each groups as group (group.key)}
      {@const rows = filterRows(group.rows, text)}
      <h3>
        From {group.label}, <TfKeyed text={group.via} lead /> <span class="tf-count">{rows.length}</span>
        {#if group.setId !== null}<a class="tf-linkish tf-dedit" href={group.href} onclick={onclose}>Edit set</a>{/if}
      </h3>
      {#each rows as row (row.id)}
        <div class="tf-lk">
          {#if row.href !== null}
            <a class="tf-lk-go" href={row.href} onclick={onclose}
              ><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={row.kind === kindWord("repo")}><Marked text={row.title} typed={text} /></span>{" "}<span class="tf-how">{row.kind}</span></span>{@render stateOf(row.state)}<span class="tf-chev" aria-hidden="true">›</span></a
            >
          {:else}
            <div class="tf-lk-go"><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={row.kind === kindWord("repo")}><Marked text={row.title} typed={text} /></span>{" "}<span class="tf-how">{row.kind}</span></span>{@render stateOf(row.state)}</div>
          {/if}
          {#if group.setId !== null}
            <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={() => onhide(row)} aria-label="Hide here: {row.title}">Hide here</button>
          {/if}
        </div>
      {/each}
    {/each}
    {#if hidden.length > 0}
      <h3>Hidden on <TfKey value={jobKey} lead /> <span class="tf-count">{hidden.length}</span></h3>
      {#each hidden as row (row.id)}
        <div class="tf-lk">
          <div class="tf-lk-go tf-dim"><span class="tf-lk-t"><span class="tf-lk-name">{row.title}</span></span></div>
          <button type="button" class="tf-btn tf-sm tf-ghost tf-x tf-shown" onclick={() => onshow(row.id)} aria-label="Show again: {row.title}">Show again</button>
        </div>
      {/each}
    {/if}
    <p class="tf-dnote">Links and link sets stay in knobas and are never written to a source.</p>
  </div>
</aside>
</div>
