<!--
  One dialog for the Day tab's three block forms (#827): *Edit* a block (from,
  to, target, *Delete block*), *Assign…* a stretch to a target, and *+ Add a
  block*. One form because they are one question, what this time was on, and
  two copies is how they come to disagree about which halves a target has.

  The target is an entity id or a label, chosen outright: `TimerTarget` is
  exactly one of the two on the wire, and a field that guessed would be the one
  place that rule was a heuristic (the rooms UI's day review, #279).

  A refusal stays in the dialog beside the fields it is about, and the dialog
  stays open. It registers on the shell's layers, so `Esc` closes it first.
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
  import { onMount } from "svelte";

  import type { Layers } from "../layers.svelte";

  let {
    mode,
    from,
    to,
    target,
    span,
    refusal,
    layers,
    onsave,
    ondelete,
    onclose,
  }: {
    mode: "edit" | "assign" | "add";
    from: string;
    to: string;
    /** The block's own target on *Edit*; `null` starts the field empty. */
    target: TimerTarget | null;
    /** `"12:35 to 13:10"`, said on *Assign…*, whose times do not move. */
    span: string | null;
    refusal: string | null;
    layers: Layers;
    onsave: (form: BlockForm) => void;
    ondelete: () => void;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // The form starts from the props once; what the reader types is theirs.
  let form = $state({
    from,
    to,
    kind: target?.kind ?? ("entity" as TimerTarget["kind"]),
    value: target === null ? "" : target.kind === "entity" ? target.entity_id : target.label,
  });

  const TITLE = { edit: "Edit the block", assign: "Assign the time", add: "Add a block" } as const;
  const SAVE = { edit: "Save", assign: "Assign", add: "Add" } as const;

  let field = $state<HTMLInputElement | null>(null);
  onMount(() => {
    field?.focus();
    return layers.add({ kind: "dialog", close: onclose });
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const value = form.value.trim();
    onsave({
      from: form.from,
      to: form.to,
      target: form.kind === "entity" ? { kind: "entity", entity_id: value } : { kind: "label", label: value },
    });
  }
</script>

<div class="tf-modal" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <div class="tf-dialog" role="dialog" aria-modal="true" aria-labelledby="tf-tm-dialog-title">
  <form class="tf-dform" onsubmit={submit}>
    <header>
      <h2 id="tf-tm-dialog-title">{TITLE[mode]}</h2>
      {#if mode === "assign" && span !== null}<p class="tf-dsub">{span}</p>{/if}
    </header>
    <div class="tf-body">
      <div class="tf-formgrid">
        {#if mode !== "assign"}
          <label for="tf-tm-from">From</label>
          <input id="tf-tm-from" type="time" bind:value={form.from} />
          <label for="tf-tm-to">To</label>
          <input id="tf-tm-to" type="time" bind:value={form.to} />
        {/if}
        <label for="tf-tm-on">On</label>
        <div class="tf-tm-on">
          <select aria-label="What kind of target" bind:value={form.kind}>
            <option value="entity">A ticket or item</option>
            <option value="label">A label</option>
          </select>
          <input
            id="tf-tm-on"
            type="text"
            bind:this={field}
            bind:value={form.value}
            placeholder={form.kind === "entity" ? "jira:PAY-231" : "What it was"}
          />
        </div>
      </div>
      {#if refusal !== null}
        <p class="tf-tm-refusal" role="alert">{refusal}</p>
      {/if}
    </div>
    <footer>
      {#if mode === "edit"}
        <button type="button" class="tf-btn tf-ghost tf-danger tf-first" onclick={ondelete}>Delete block</button>
      {/if}
      <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
      <button type="submit" class="tf-btn tf-primary">{SAVE[mode]}</button>
    </footer>
  </form>
  </div>
</div>
