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
<script lang="ts" module>
  /** Where in the drawer *Edit set* was pressed: the group, the list's scroll and what the filter held. */
  export interface DrawerPlace {
    group: string;
    top: number;
    text: string;
  }
</script>

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
  import { heldByPointer, heldHeight, holdUnderPointer } from "../row-hold";

  let {
    jobKey,
    links,
    groups = [],
    hidden = [],
    alsoIn = () => [],
    place = null,
    layers,
    onclose,
    onunlink,
    onlinkto,
    onhide = () => {},
    onshow = () => {},
    onapplyset = () => {},
    onsaveset = () => {},
    oneditset = () => {},
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
    /** Back from *Edit set*: where the reader was, which the drawer opens at. */
    place?: DrawerPlace | null;
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
    /** *Edit set* pressed: the page is about to be left, to come back to with the drawer open, where it was. */
    oneditset?: (place: DrawerPlace) => void;
  } = $props();

  let field = $state<HTMLInputElement | null>(null);
  // svelte-ignore state_referenced_locally
  // Read once: the drawer opens at its place, and is the reader's after that.
  let text = $state(place?.text ?? "");

  const shown = $derived(filterLinks(links, text));

  /** The words a row drew, which its held place keeps. */
  type RowWords = { title: string; how: string; mono: boolean };
  /** A row a mouse press took away, its place held under the pointer until the pointer leaves it (F-138): its words stay, muted, with nothing to press. */
  let holding = $state.raw<{ list: string; at: number; id: string | number; height: number; words: RowWords } | null>(null);
  function hold(event: MouseEvent, list: string, at: number, id: string | number, words: RowWords) {
    const box = heldByPointer(event) ? (event.currentTarget as Element).closest(".tf-lk")?.getBoundingClientRect() : undefined;
    if (box !== undefined) holding = { list, at, id, height: box.height, words };
  }
  $effect(() => {
    if (holding === null) return;
    return holdUnderPointer(() => document.querySelector(".tf-lk-held"), () => (holding = null));
  });

  let body = $state<HTMLElement | null>(null);
  /** *Apply a link set…* or *Save as a link set…* was pressed, at this scroll, so the group the job's next read adds is its answer. A group added above the fold has moved the scroll by the time it's drawn. */
  let awaitingFrom: number | null = null;
  // svelte-ignore state_referenced_locally
  let knownGroups = new Set(groups.map((group) => group.key));
  /** The group a save or an apply brought into view, and where the list stood before it. */
  let broughtFrom: { group: string; top: number } | null = null;
  /** A set saved or applied draws its group below the fold, so the list brings it into view (F-550). Focus stays on the opener, in the header that doesn't scroll. `Undo` takes the group away, and the list goes back (F-558). */
  $effect(() => {
    const keys = groups.map((group) => group.key);
    const fresh = keys.find((key) => !knownGroups.has(key));
    knownGroups = new Set(keys);
    if (broughtFrom !== null && !keys.includes(broughtFrom.group)) {
      if (body !== null) body.scrollTop = broughtFrom.top;
      broughtFrom = null;
    }
    if (awaitingFrom === null || fresh === undefined) return;
    broughtFrom = { group: fresh, top: awaitingFrom };
    awaitingFrom = null;
    const heads = body?.querySelectorAll<HTMLElement>("[data-group]") ?? [];
    [...heads].find((head) => head.dataset.group === fresh)?.scrollIntoView?.({ block: "start" });
  });

  $effect(() => {
    const restoreTo = document.activeElement;
    const off = untrack(() => layers.add({ kind: "drawer", close: () => onclose() }));
    const back = untrack(() => place);
    const edit = back === null ? null : [...(body?.querySelectorAll<HTMLElement>("[data-group]") ?? [])].find((head) => head.dataset.group === back.group)?.querySelector<HTMLElement>(".tf-dedit");
    if (back !== null && body !== null) body.scrollTop = back.top;
    if (edit) edit.focus({ preventScroll: true });
    else field?.focus();
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
      <button type="button" class="tf-btn tf-sm" onclick={() => { awaitingFrom = body?.scrollTop ?? 0; onapplyset(); }}>Apply a link set…</button>
      <button type="button" class="tf-btn tf-sm" onclick={() => { awaitingFrom = body?.scrollTop ?? 0; onsaveset(); }}>Save as a link set…</button>
    </div>
  </div>
  <div class="tf-dbody" bind:this={body}>
    <h3>Linked on <TfKey value={jobKey} lead /> <span class="tf-count">{shown.length}</span></h3>
    {#snippet heldRow(list: string, at: number, present: boolean)}
      {#if holding !== null && holding.list === list && holding.at === at && !present}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="tf-lk tf-lk-held" aria-hidden="true" onmousedown={(event) => event.preventDefault()} {@attach heldHeight(holding.height)}>
          <div class="tf-lk-go"><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={holding.words.mono}>{holding.words.title}</span>{" "}<span class="tf-how">{holding.words.how}</span></span>{@render stateOf(null)}</div>
        </div>
      {/if}
    {/snippet}
    {#each shown as entry, at (entry.link.id)}
      {@const also = alsoIn(entry.other.entity_id)}
      {@const how = `${kindWord(entry.other.kind)}${also.length > 0 ? `, also in ${also.join(", ")}` : ""}`}
      {@render heldRow("links", at, shown.some((one) => one.link.id === holding?.id))}
      <div class="tf-lk" data-row="link:{entry.link.id}">
        <a class="tf-lk-go" href={itemHash(entry.other)} onclick={onclose}
          ><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={entry.other.kind === "repo"}><Marked text={entry.other.title} typed={text} /></span>{" "}<span class="tf-how">{how}</span></span
          >{@render stateOf(entry.other.state ?? null)}<span class="tf-chev" aria-hidden="true">›</span></a
        >
        <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={(event) => { if (entry.link.origin === "manual") hold(event, "links", shown.indexOf(entry), entry.link.id, { title: entry.other.title, how, mono: entry.other.kind === "repo" }); onunlink(entry); }} aria-label="Unlink {entry.other.title}">{entry.link.origin === "manual" ? "Unlink" : "Unlink…"}</button>
      </div>
    {:else}
      {#if text.trim() !== ""}
        <p class="tf-dnone">No links match. <button type="button" class="tf-linkish" onclick={() => (text = "")}>Clear the filter</button></p>
      {:else}
        <p class="tf-dnone">Nothing linked on the ticket itself.</p>
      {/if}
    {/each}
    {#if shown.length > 0}{@render heldRow("links", shown.length, shown.some((one) => one.link.id === holding?.id))}{/if}
    {#each groups as group (group.key)}
      {@const rows = filterRows(group.rows, text)}
      <h3 data-group={group.key}>
        From {group.label}, <TfKeyed text={group.via} lead /> <span class="tf-count">{rows.length}</span>
        {#if group.setId !== null}<a class="tf-linkish tf-dedit" href={group.href} onclick={() => { oneditset({ group: group.key, top: body?.scrollTop ?? 0, text }); onclose(); }}>Edit set</a>{/if}
      </h3>
      {#each rows as row, at (row.id)}
        {@render heldRow(group.key, at, rows.some((one) => one.id === holding?.id))}
        <div class="tf-lk" data-row="{group.key}:{row.id}">
          {#if row.href !== null}
            <a class="tf-lk-go" href={row.href} onclick={onclose}
              ><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={row.kind === kindWord("repo")}><Marked text={row.title} typed={text} /></span>{" "}<span class="tf-how">{row.kind}</span></span>{@render stateOf(row.state)}<span class="tf-chev" aria-hidden="true">›</span></a
            >
          {:else}
            <div class="tf-lk-go"><span class="tf-lk-t"><span class="tf-lk-name" class:tf-mono={row.kind === kindWord("repo")}><Marked text={row.title} typed={text} /></span>{" "}<span class="tf-how">{row.kind}</span></span>{@render stateOf(row.state)}</div>
          {/if}
          {#if group.setId !== null}
            <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={(event) => { hold(event, group.key, rows.indexOf(row), row.id, { title: row.title, how: row.kind, mono: row.kind === kindWord("repo") }); onhide(row); }} aria-label="Hide here: {row.title}">Hide here</button>
          {/if}
        </div>
      {/each}
      {@render heldRow(group.key, rows.length, rows.some((one) => one.id === holding?.id))}
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
