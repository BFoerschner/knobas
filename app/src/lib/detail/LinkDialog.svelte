<!--
  *Link to…* — spec §5a's write, from any detail view.

  Three fields, in the order the decision is made: **what** to link to, **how**
  it relates, and **why**. Only the first has to be answered — the relation
  defaults to `related` so a quick link costs no extra decisions (#40's story
  4), and the note is optional.

  ## The target picker is the launcher

  Not "like" it: it drives the same `Session`, so it gets the same 90 ms
  debounce, the same request sequencing (a slow answer that lands after a
  faster, newer one is dropped) and the same selection model. Linking is as
  fast as finding because it *is* finding.

  ## The Esc ladder

  One rung per press, the launcher's own rule: a non-empty search clears first,
  and only an already-empty box lets the key through to `Modal`, which closes
  the dialog and hands focus back to the control that opened it. The shell's
  global handler never sees it either way — `Modal` stops it — so one keystroke
  never unwinds two ladders.
-->
<script lang="ts">
  import type { SearchHit } from "../ipc/search";
  import { launcherHome, search } from "../ipc/search";
  import { createLink } from "../ipc/entity";
  import { Session } from "../launcher/session.svelte";
  import Modal from "../shell/Modal.svelte";
  import Monogram from "../shell/Monogram.svelte";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { linkFailureMessage } from "./links.svelte";
  import { DEFAULT_RELATION, RELATIONS, drawn } from "./relations";

  let {
    fromId,
    fromTitle,
    onclose,
    oncreated,
  }: {
    /** The entity the link is drawn from — the one whose detail is open. */
    fromId: string;
    /** Its title, so the dialog says what is being linked. */
    fromTitle: string;
    onclose: () => void;
    /** A link was written. The detail re-reads; this dialog is done. */
    oncreated: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, like the launcher's: a `Session` owns a debounce timer and a
  // request counter, and swapping its ports mid-life would drop both.
  const session = new Session({ search, launcherHome });

  let target = $state<SearchHit | null>(null);
  let relation = $state(DEFAULT_RELATION);
  let note = $state("");
  /** What the last attempt refused with, shown in the dialog rather than as a toast. */
  let failure = $state<string | null>(null);
  let writing = $state(false);

  /** Unique per instance, so two stacked dialogs cannot share a list id. */
  const listId = `relations-${Math.random().toString(36).slice(2, 9)}`;

  /** Only `hit` rows are pickable: the picker is in results mode or empty. */
  const hits = $derived(session.rows.filter((row) => row.kind === "hit"));

  function labelOf(kind: string): string {
    return kindSingular(kind, kindRegistry.info(kind));
  }

  function monogramOf(kind: string): string {
    return kindMonogram(kind, kindRegistry.info(kind));
  }

  function pick(hit: SearchHit) {
    target = hit;
    failure = null;
    session.set("");
  }

  /**
   * The picker's keyboard, which is the launcher's.
   *
   * `Escape` is handled here rather than left to `Modal` so the ladder has its
   * first rung: a search somebody typed is a step, and one keypress must undo
   * one step.
   */
  function onpickerkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        session.move(1);
        return;
      case "ArrowUp":
        event.preventDefault();
        session.move(-1);
        return;
      case "Enter": {
        event.preventDefault();
        const row = session.current;
        if (row?.kind === "hit") pick(row.hit);
        return;
      }
      case "Escape":
        if (session.raw === "") return;
        // Rung 1: clear what was typed. `Modal`'s handler, the rung above,
        // must not also close the dialog on this press.
        event.preventDefault();
        event.stopPropagation();
        session.set("");
        return;
      default:
    }
  }

  /** `Enter` in the relation or note field is the same as pressing *Link*. */
  function onfieldkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void submit();
  }

  async function submit() {
    if (!target || writing) return;
    writing = true;
    failure = null;
    try {
      // The ends come from `drawn`, not from this dialog: a relation offered
      // as the inverse reading of another is one row asked for from the other
      // side, and which end it starts at is a fact about the vocabulary.
      const row = drawn(relation, fromId, target.entity_id);
      await createLink(row.fromId, row.toId, row.relation, note);
      oncreated();
    } catch (rejection) {
      // Inline, and in the dialog's own words: "already linked" is an ordinary
      // outcome, and a toast over a dialog the reader is still working in is
      // an answer in the wrong place.
      failure = linkFailureMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<Modal title="Link to…" subtitle={fromTitle} {onclose}>
  {#snippet body()}
    <div class="fields">
      <div class="fld">
        <label class="l" for="{listId}-target">What to link to</label>
        {#if target}
          <div class="picked">
            <Monogram text={monogramOf(target.kind)} label={labelOf(target.kind)} />
            <span class="t">{target.title}</span>
            <span class="k muted">{labelOf(target.kind)}</span>
            <button
              class="btn sm"
              id="{listId}-target"
              onclick={() => {
                target = null;
                failure = null;
              }}
            >
              Change
            </button>
          </div>
        {:else}
          <input
            class="inp"
            id="{listId}-target"
            type="text"
            autocomplete="off"
            placeholder="Search everything"
            value={session.raw}
            oninput={(event) => session.type(event.currentTarget.value)}
            onkeydown={onpickerkeydown}
          />
          {#if session.error}
            <p class="fail">{session.error}</p>
          {:else if session.raw.trim() !== "" && hits.length === 0 && !session.pending}
            <p class="hint">Nothing matches. Only things knobas has already synced can be linked.</p>
          {/if}
          <div class="hits" role="listbox" aria-label="Link targets">
            {#each session.rows as row, index (row.id)}
              {#if row.kind === "hit"}
                <button
                  class="row g3 hit"
                  class:sel={index === session.selected}
                  role="option"
                  aria-selected={index === session.selected}
                  onclick={() => pick(row.hit)}
                  onmouseenter={() => (session.selected = index)}
                >
                  <Monogram text={monogramOf(row.hit.kind)} label={labelOf(row.hit.kind)} />
                  <!-- Raw source text, rendered as text (gotcha 7). -->
                  <span class="t">{row.hit.title}</span>
                  <span class="r">{labelOf(row.hit.kind)}</span>
                </button>
              {/if}
            {/each}
          </div>
        {/if}
      </div>

      <div class="fld">
        <label class="l" for="{listId}-relation">How it relates</label>
        <input
          class="inp"
          id="{listId}-relation"
          type="text"
          list={listId}
          autocomplete="off"
          bind:value={relation}
          onkeydown={onfieldkeydown}
        />
        <!--
          A datalist, so the curated list is a suggestion and not a cage: §5a
          lets a relation be anything the user types.
        -->
        <datalist id={listId}>
          {#each RELATIONS as option (option.id)}
            <option value={option.id}>{option.forward}</option>
          {/each}
        </datalist>
        <div class="chips">
          {#each RELATIONS as option (option.id)}
            <button
              class="pill"
              class:on={relation === option.id}
              onclick={() => (relation = option.id)}
            >
              {option.id}
            </button>
          {/each}
        </div>
      </div>

      <div class="fld">
        <label class="l" for="{listId}-note">Why (optional)</label>
        <input
          class="inp"
          id="{listId}-note"
          type="text"
          autocomplete="off"
          placeholder="What this link records"
          bind:value={note}
          onkeydown={onfieldkeydown}
        />
      </div>

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn pri" disabled={!target || writing} onclick={() => void submit()}>
      {writing ? "Linking…" : "Link"}
    </button>
  {/snippet}
</Modal>

<style>
  .fields {
    display: grid;
    gap: 12px;
  }

  .fld .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-bottom: 4px;
  }

  .fld .chips {
    margin-top: 6px;
  }

  .picked {
    display: grid;
    grid-template-columns: 34px 1fr auto auto;
    align-items: center;
    column-gap: 8px;
    height: var(--row);
    padding: 0 8px;
    border: 1px solid var(--hair2);
    border-radius: 2px;
    background: var(--bg);
    font-size: 12px;
  }

  .picked > * {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The result list is bounded: a dialog that grows past the viewport is one
     whose footer buttons cannot be reached. */
  .hits {
    max-height: 210px;
    overflow: auto;
    margin-top: 6px;
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .hits:empty {
    display: none;
  }

  .hit:last-child {
    border-bottom: 0;
  }

  .hint {
    margin-top: 6px;
    color: var(--muted);
    font-size: 12px;
  }

  .fail {
    color: var(--fail);
    font-size: 12px;
    line-height: 1.5;
  }
</style>
