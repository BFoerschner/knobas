<!--
  One dialog for the Day tab's three block forms (#827): *Edit* a block (from,
  to, target, *Delete block*), *Assign…* a stretch to a target, and *+ Add a
  block*. One form because they are one question, what this time was on, and
  two copies is how they come to disagree about which halves a target has.

  The target is a ticket or a label, chosen outright: `TimerTarget` is exactly
  one of the two on the wire, and a field that guessed would be the one place
  that rule was a heuristic (the rooms UI's day review, #279).

  **A ticket is picked, never typed** (#1429, prototype-6's "Opens a ticket
  picker"). The picker is the launcher's `Session`, as *Link to…*
  (`job/TfLinkTo.svelte`) draws it: the board's recent tickets before anything
  is typed, the search's hits after. The block is written on the picked
  entity's id, so a key without its namespace or a namespace the source does
  not use cannot reach `create_block`. *A label* is the second mode, for time
  on no ticket.

  **The On row** (#1428): the kind switch is as wide as its words, and the
  field beside it takes the rest of the row (`time.css`).

  A refusal stays in the dialog beside the fields it is about, and the dialog
  stays open. It is a `TfDialog` (#2346), so `Esc` closes it first.
-->
<script lang="ts" module>
  import type { TimerTarget } from "../../ipc/time";

  /** What the reader settled on. The clocks are `HH:MM`. */
  export interface BlockForm {
    from: string;
    to: string;
    target: TimerTarget;
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";

  import { Session, type SessionPorts } from "../../launcher/session.svelte";
  import { targetReading } from "../../shell/timer";
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";

  let {
    mode,
    from,
    to,
    target,
    title = null,
    span,
    refusal,
    layers,
    ports,
    onsave,
    ondelete,
    onclose,
  }: {
    mode: "edit" | "assign" | "add";
    from: string;
    to: string;
    /** The block's own target on *Edit*; `null` starts with nothing picked. */
    target: TimerTarget | null;
    /** The title of the block's ticket on *Edit*, drawn beside its key. */
    title?: string | null;
    /** `"12:35 to 13:10"`, said on *Assign…*, whose times do not move. */
    span: string | null;
    refusal: string | null;
    layers: Layers;
    /** The launcher's reads, which the picker searches with. */
    ports: Pick<SessionPorts, "search" | "launcherHome">;
    onsave: (form: BlockForm) => void;
    ondelete: () => void;
    onclose: () => void;
  } = $props();

  /** A ticket the picker can name: the board's recents and the search's hits share these. */
  interface Ticket {
    entityId: string;
    title: string;
  }

  // svelte-ignore state_referenced_locally
  // The form starts from the props once; what the reader picks and types is theirs.
  let form = $state({
    from,
    to,
    kind: target?.kind ?? ("entity" as TimerTarget["kind"]),
    picked: target?.kind === "entity" ? ({ entityId: target.entity_id, title: title ?? "" } as Ticket) : null,
    label: target?.kind === "label" ? target.label : "",
  });

  // svelte-ignore state_referenced_locally
  // Read once: a `Session` owns a debounce timer and a request counter.
  const session = new Session({ search: ports.search, launcherHome: ports.launcherHome });
  /**
   * The highlighted choice, for the arrow keys and `Enter`. Its own index and
   * not `session.selected`, which counts the rows the ticket filter drops.
   */
  let cursor = $state(0);

  /**
   * What the picker offers: the recent tickets on an empty box, the hits on a
   * query, tickets only. Jira takes time on a ticket alone, and a block on
   * anything else is a row the Day tab says *needs a ticket*.
   */
  const choices = $derived(
    session.rows.flatMap((row): Ticket[] => {
      const item = row.kind === "hit" ? row.hit : row.kind === "recent" ? row.row : null;
      if (item === null || item.kind !== "ticket") return [];
      return [{ entityId: item.entity_id, title: item.title }];
    }),
  );

  const TITLE = { edit: "Edit the block", assign: "Assign the time", add: "Add a block" } as const;
  const SAVE = { edit: "Save", assign: "Assign", add: "Add" } as const;

  const keyOf = (entityId: string) => targetReading({ kind: "entity", entity_id: entityId });
  const ready = $derived(form.kind === "entity" ? form.picked !== null : form.label.trim() !== "");

  const uid = $props.id();
  let field = $state<HTMLInputElement | null>(null);
  onMount(() => {
    field?.focus();
    void session.loadHome();
    return () => session.dispose();
  });

  function pick(ticket: Ticket) {
    form.picked = ticket;
  }

  function change() {
    form.picked = null;
    cursor = 0;
    // The box is drawn again on the next tick; focus it there.
    void tick().then(() => field?.focus());
  }

  function onsearch(event: Event & { currentTarget: HTMLInputElement }) {
    cursor = 0;
    session.type(event.currentTarget.value);
  }

  function onsearchkey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const last = choices.length - 1;
      cursor = Math.min(Math.max(0, last), Math.max(0, cursor + (event.key === "ArrowDown" ? 1 : -1)));
    } else if (event.key === "Enter") {
      // A pick, never a submit: the form is not ready until something is picked.
      event.preventDefault();
      const choice = choices[cursor];
      if (choice !== undefined) pick(choice);
    }
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!ready) return;
    onsave({
      from: form.from,
      to: form.to,
      target:
        form.kind === "entity" && form.picked !== null
          ? { kind: "entity", entity_id: form.picked.entityId }
          : { kind: "label", label: form.label.trim() },
    });
  }
</script>

<!-- The form is the body; the footer's submit reaches it through `form=`, as `TfApplySet`'s does. -->
<TfDialog title={TITLE[mode]} subtitle={mode === "assign" ? span : null} {onclose} {layers}>
  {#snippet body()}
  <form id="tf-tm-form-{uid}" class="tf-dform" onsubmit={submit}>
      <div class="tf-formgrid">
        {#if mode !== "assign"}
          <label for="tf-tm-from">From</label>
          <input id="tf-tm-from" type="time" bind:value={form.from} />
          <label for="tf-tm-to">To</label>
          <input id="tf-tm-to" type="time" bind:value={form.to} />
        {/if}
        <span id="tf-tm-on-l" class="tf-tm-on-l">On</span>
        <div class="tf-tm-target" role="group" aria-labelledby="tf-tm-on-l">
          <div class="tf-tm-on">
            <div class="tf-seg" role="group" aria-label="What the time was on">
              <button type="button" aria-pressed={form.kind === "entity"} onclick={() => (form.kind = "entity")}
                >A ticket</button
              >
              <button type="button" aria-pressed={form.kind === "label"} onclick={() => (form.kind = "label")}
                >A label</button
              >
            </div>
            {#if form.kind === "label"}
              <input
                id="tf-tm-on"
                type="text"
                aria-label="What it was"
                placeholder="What it was"
                bind:this={field}
                bind:value={form.label}
              />
            {:else if form.picked !== null}
              <span class="tf-tm-picked"
                ><span class="tf-mono">{keyOf(form.picked.entityId)}</span>
                {form.picked.title}</span
              >
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={change}>Change</button>
            {:else}
              <input
                type="text"
                autocomplete="off"
                aria-label="Search for a ticket"
                placeholder="Search for a ticket"
                bind:this={field}
                value={session.raw}
                oninput={onsearch}
                onkeydown={onsearchkey}
              />
            {/if}
          </div>
          {#if form.kind === "entity" && form.picked === null}
            {#if session.error}<p class="tf-dim">{session.error}</p>{/if}
            {#if choices.length > 0}
              <div class="tf-picklist" role="listbox" aria-label="Tickets">
                {#each choices as choice, index (choice.entityId)}
                  <button type="button" role="option" aria-selected={index === cursor} onclick={() => pick(choice)}>
                    <span><span class="tf-mono">{keyOf(choice.entityId)}</span> {choice.title}</span>
                  </button>
                {/each}
              </div>
            {/if}
          {/if}
        </div>
      </div>
      {#if refusal !== null}
        <p class="tf-tm-refusal" role="alert">{refusal}</p>
      {/if}
  </form>
  {/snippet}
  {#snippet footer()}
      {#if mode === "edit"}
        <button type="button" class="tf-btn tf-ghost tf-danger tf-first" onclick={ondelete}>Delete block</button>
      {/if}
      <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
      <button type="submit" class="tf-btn tf-primary" form="tf-tm-form-{uid}" disabled={!ready}>{SAVE[mode]}</button>
  {/snippet}
</TfDialog>
