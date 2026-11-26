<!--
  One fact's value on the ticket page's card, as a control where its source lets a person change it (ADR-0054, section
  25). The value reads as it always did; where the field opens it is a button with a small chevron, and where it does
  not it is the same box with the chevron's room kept unseen and the reason as its title, so nothing moves when the
  source's answer lands or a credential is refused.

  A press opens the field's control under it: the people picker for an assignee, a menu of the priorities, the labels
  picker. Every edit draws at once (the echo), with the write chip at the row's right end in the fixed words; the chip
  never adds a line, the value giving way with an ellipsis. A refused or held edit's note is the card's to draw, under
  the row.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import TfMenu from "../TfMenu.svelte";
  import type { Layers } from "../layers.svelte";
  import type { FieldEdits } from "./field-edits.svelte";
  import TfLabelsPick from "./TfLabelsPick.svelte";
  import TfPeoplePick from "./TfPeoplePick.svelte";

  let {
    edits,
    id,
    name,
    me = null,
    sourceName,
    labels = [],
    open = $bindable(false),
    layers,
    children,
  }: {
    edits: FieldEdits | null;
    /** The field: `assignee`, `priority`, `labels`. */
    id: string;
    /** The fact's name, the control's accessible name: *Assignee*. */
    name: string;
    /** The reader's account in the ticket's source, which leads the people picker. */
    me?: string | null;
    sourceName: string;
    /** The labels the read holds, for the labels picker. */
    labels?: readonly string[];
    /** The control stands open: the page's `a` opens the assignee's. */
    open?: boolean;
    layers?: Layers | undefined;
    children: Snippet;
  } = $props();

  let opener = $state<HTMLElement | null>(null);
  const control = $derived(edits?.control(id) ?? null);
  const field = $derived(control?.field ?? null);
  const can = $derived(control?.can ?? false);
  const title = $derived(control?.why ?? (can ? `Change the ${name.toLowerCase()}` : undefined));

  /** The labels as they stand with their echoes: a name added draws at once, a name removed goes at once. */
  const carried = $derived.by(() => {
    const now = new Set(labels);
    for (const echo of edits?.labelEchoes() ?? []) {
      const label = echo.text.slice(1);
      if (echo.text.startsWith("+")) now.add(label);
      else now.delete(label);
    }
    return [...now];
  });

  function press() {
    if (!can || field === null) return;
    if (field.kind.type === "search" || (field.kind.type === "labels" && field.kind.scope !== null)) edits?.ask(field);
    open = true;
  }

  $effect(() => {
    // Opened by a key (`a`): ask its list as a press does.
    if (open && field !== null && field.kind.type === "search") edits?.ask(field);
  });

  const held = $derived(field === null ? null : (edits?.options(field) ?? null));
  const choices = $derived(field?.kind.type === "choice" ? field.kind.options : []);
  const current = $derived.by(() => {
    if (field === null || field.value === null || field.value === undefined) return null;
    const value = String(field.value);
    return { value, shown: field.shown ?? choices.find((c) => c.value === value)?.label ?? value };
  });

  function assign(value: string | null, shown: string) {
    if ((current?.value ?? null) === value) return;
    edits?.edit(
      "assignee",
      { assignee: value === null ? "nobody" : { account: value } },
      shown,
      value === null ? "Unassign {key}" : `Assign {key} to ${shown}`,
    );
  }

  function setPriority(value: string, shown: string) {
    open = false;
    if (current?.value === value) return;
    edits?.edit("priority", { priority: value }, shown, `Set {key}’s priority to ${shown}`);
  }

  function toggleLabel(label: string, add: boolean) {
    edits?.edit(
      "labels",
      { labels: add ? { add: [label], remove: [] } : { add: [], remove: [label] } },
      `${add ? "+" : "-"}${label}`,
      add ? `Label {key} ${label}` : `Remove ${label} from {key}`,
    );
  }
</script>

{#if can}
  <button
    type="button"
    class="tf-fact-edit"
    aria-haspopup={field?.kind.type === "choice" && id !== "assignee" ? "menu" : "dialog"}
    aria-expanded={open}
    aria-label="{name}: change it"
    {title}
    bind:this={opener}
    onclick={press}>{@render children()}<span class="tf-chev" aria-hidden="true"></span></button
  >
{:else}
  <span class="tf-fact-edit" aria-disabled="true" {title}>{@render children()}<span class="tf-chev" aria-hidden="true"></span></span>
{/if}
{#if control?.chip}
  <span class="tf-chip tf-fwrite" class:tf-info={control.echo?.state === "queued"} title={control.echo?.state === "queued" ? "Waiting to be sent" : undefined}>{control.chip}</span>
{/if}

{#if field !== null && can}
  {#if id === "labels" && field.kind.type === "labels"}
    <TfLabelsPick
      bind:open
      {opener}
      label={name}
      {carried}
      offered={held?.options.map((option) => option.value) ?? []}
      create={field.kind.create}
      failed={held?.failed ?? null}
      asking={held?.asking ?? false}
      ontoggle={toggleLabel}
      {layers}
    />
  {:else if field.kind.type === "search" || (id === "assignee" && field.kind.type === "choice")}
    <TfPeoplePick
      bind:open
      {opener}
      label={name}
      {current}
      {me}
      person={field.kind.type === "search" ? field.kind.person : true}
      required={field.required}
      options={field.kind.type === "choice" ? field.kind.options : (held?.options ?? [])}
      complete={field.kind.type === "choice" ? true : (held?.complete ?? false)}
      asking={held?.asking ?? false}
      failed={held?.failed ?? null}
      {sourceName}
      scope={field.kind.type === "search" ? field.kind.scope : sourceName}
      onpick={assign}
      onsearch={(typed) => edits?.ask(field, typed)}
      {layers}
    />
  {:else if field.kind.type === "choice"}
    <TfMenu bind:open {opener} label={name} align="start" focusOn="[aria-checked=true]" {layers}>
      {#each field.kind.options as option (option.value)}
        <button type="button" role="menuitemradio" aria-checked={current?.value === option.value} onclick={() => setPriority(option.value, option.label)}
          ><span>{option.label}</span>{#if current?.value === option.value}<span aria-hidden="true">✓</span>{/if}</button
        >
      {/each}
    </TfMenu>
  {/if}
{/if}
