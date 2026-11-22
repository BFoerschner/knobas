<!--
  A source's own field on the ticket page's card, under *More fields* (ADR-0054, step 10 of the 2026-11-21 ruling,
  section 25): the adapter named it and its kind, knobas draws the kind's control and sends the value back untouched,
  in `EditTicket.fields` under the adapter's id.

  - `text` and `number`: the value is a button; a press turns it into a one-line field, `Enter` saves, `Esc` cancels.
  - `choice`: a menu of the options, the current one ticked.
  - `search`: the people picker (or the same picker over a list that is not people).
  - `date`: a native date field; clearing it clears the field where it may be empty.
  - `flag`: a checkbox.

  A field the source lists read only, or a credential may not change, is text with the reason as its title. Every edit
  draws at once with its chip at the row's right end, as the named fields do.
-->
<script lang="ts">
  import type { TicketField } from "../../ipc/entity";
  import TfMenu from "../TfMenu.svelte";
  import { inlineEditor } from "../inline-editor";
  import type { Layers } from "../layers.svelte";
  import type { FieldEdits } from "./field-edits.svelte";
  import TfPeoplePick from "./TfPeoplePick.svelte";

  let {
    edits,
    field,
    sourceName,
    layers,
  }: {
    edits: FieldEdits | null;
    field: TicketField;
    sourceName: string;
    layers?: Layers | undefined;
  } = $props();

  let open = $state(false);
  let editing = $state(false);
  /** What the one-line field holds: a number field binds a number, or `null` when empty. */
  let draft = $state<string | number | null>("");
  let opener = $state<HTMLElement | null>(null);
  const control = $derived(edits?.control(field.id) ?? null);
  const can = $derived(control?.can ?? false);
  const echo = $derived(control?.echo ?? null);
  const name = $derived(field.label);
  const title = $derived(control?.why ?? (can ? `Change the ${name.toLowerCase()}` : undefined));
  const kind = $derived(field.kind);

  /** What the field holds, with an edit not yet read back laid over it, as the words it reads as. */
  const shown = $derived.by(() => {
    if (echo !== null) return echo.text;
    return words(field.value, field.shown ?? null);
  });

  function words(value: unknown, named: string | null): string {
    if (value === null || value === undefined || value === "") return "";
    if (named !== null) return named;
    if (kind.type === "choice") return kind.options.find((option) => option.value === value)?.label ?? String(value);
    return String(value);
  }

  /** A flag's or an empty value's words on the card. */
  function reads(text: string): string {
    if (kind.type === "flag") return text === "true" ? "Yes" : "No";
    return text === "" ? "None" : text;
  }

  /** Send `value` for the field: the echo's words are what the source's read will show. */
  function send(value: unknown, text: string) {
    const before = echo?.text ?? words(field.value, field.shown ?? null);
    if (text === before) return;
    const said = text === "" ? `Clear {key}’s ${name.toLowerCase()}` : `Set {key}’s ${name.toLowerCase()} to ${reads(text)}`;
    edits?.edit(field.id, { fields: { [field.id]: value } }, text, said);
  }

  function startEdit() {
    if (!can) return;
    draft = echo?.text ?? (field.value === null || field.value === undefined ? "" : String(field.value));
    editing = true;
  }

  function save() {
    editing = false;
    const typed = String(draft ?? "").trim();
    if (typed === "") {
      if (!field.required) send(null, "");
      return;
    }
    if (kind.type === "number") {
      const number = Number(typed);
      if (!Number.isFinite(number)) return;
      send(number, String(number));
    } else send(typed, typed);
  }

  function press() {
    if (!can) return;
    if (kind.type === "text" || kind.type === "number") return startEdit();
    if (kind.type === "search") edits?.ask(field);
    open = true;
  }

  const held = $derived(kind.type === "search" ? (edits?.options(field) ?? null) : null);
  const current = $derived.by(() => {
    if (field.value === null || field.value === undefined) return null;
    const value = String(field.value);
    return { value, shown: field.shown ?? value };
  });
</script>

{#if editing && can && (kind.type === "text" || kind.type === "number")}
  <form
    class="tf-ownedit"
    onsubmit={(event) => {
      event.preventDefault();
      save();
    }}
    {@attach inlineEditor(() => (editing = false), { send: save, back: () => opener, ...(layers ? { layers } : {}) })}
  >
    <input type={kind.type === "number" ? "number" : "text"} aria-label={name} bind:value={draft} />
  </form>
{:else if kind.type === "date" && can}
  <input
    class="tf-owndate"
    type="date"
    aria-label={name}
    {title}
    value={shown}
    onchange={(event) => {
      const day = event.currentTarget.value;
      if (day === "") {
        if (!field.required) send(null, "");
        return;
      }
      send(day, day);
    }}
  />
{:else if kind.type === "flag" && can}
  <label class="tf-ownflag" {title}>
    <input
      type="checkbox"
      checked={shown === "true"}
      onchange={(event) => {
        const on = event.currentTarget.checked;
        send(on, String(on));
      }}
    /><span>{reads(shown)}</span>
  </label>
{:else if can}
  <button
    type="button"
    class="tf-fact-edit"
    aria-haspopup={kind.type === "choice" ? "menu" : kind.type === "search" ? "dialog" : undefined}
    aria-expanded={kind.type === "choice" || kind.type === "search" ? open : undefined}
    aria-label="{name}: change it"
    {title}
    bind:this={opener}
    onclick={press}><span class:tf-dim={shown === ""}>{reads(shown)}</span><span class="tf-chev" aria-hidden="true"></span></button
  >
{:else}
  <span class="tf-fact-edit" aria-disabled="true" {title}><span class:tf-dim={shown === ""}>{reads(shown)}</span><span class="tf-chev" aria-hidden="true"></span></span>
{/if}
{#if control?.chip}
  <span class="tf-chip tf-fwrite" class:tf-info={echo?.state === "queued"} title={echo?.state === "queued" ? "Waiting to be sent" : undefined}>{control.chip}</span>
{/if}

{#if can && kind.type === "choice"}
  <TfMenu bind:open {opener} label={name} align="start" focusOn="[aria-checked=true]" {layers}>
    {#each kind.options as option (option.value)}
      {@const ticked = (echo?.text ?? String(field.value ?? "")) === option.value}
      <button
        type="button"
        role="menuitemradio"
        aria-checked={ticked}
        onclick={() => {
          open = false;
          send(option.value, option.value);
        }}><span>{option.label}</span>{#if ticked}<span aria-hidden="true">✓</span>{/if}</button
      >
    {/each}
  </TfMenu>
{:else if can && kind.type === "search"}
  <TfPeoplePick
    bind:open
    {opener}
    label={name}
    {current}
    person={kind.person}
    required={field.required}
    options={held?.options ?? []}
    complete={held?.complete ?? false}
    asking={held?.asking ?? false}
    failed={held?.failed ?? null}
    {sourceName}
    scope={kind.scope}
    onpick={(value, picked) => send(value, value === null ? "" : picked)}
    onsearch={(typed) => edits?.ask(field, typed)}
    {layers}
  />
{/if}
