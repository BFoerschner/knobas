<!--
  The generated config form (spec §3a) in the ticket-first look, over the
  form model (`sources/schema-form`). One row per field, the control chosen by
  `control.kind`; no per-adapter branch (#903, #904).

  `rows` draws prototype-6's settings rows (`.tf-stg-row`: the label with its
  help under it, then the control), for a source's page. `grid` draws the
  dialog's two-column `.tf-formgrid`, for *Add a source*.

  Values are the caller's and are written in place, so the caller
  validates the whole draft on every keystroke. An empty text field says *None set*: a field the sync fills (a Jira field id, #1021) then
  reads as not set rather than as a blank box.
-->
<script lang="ts">
  import type { SchemaField } from "../../sources/schema-form";

  let {
    fields,
    values,
    errors = {},
    disabled = false,
    idPrefix,
    layout,
  }: {
    fields: SchemaField[];
    values: Record<string, unknown>;
    errors?: Record<string, string>;
    disabled?: boolean;
    /** Namespaces the element ids, so two forms on one page stay wired. */
    idPrefix: string;
    layout: "rows" | "grid";
  } = $props();

  const controlId = (key: string) => `${idPrefix}-${key}`;
  const helpId = (key: string) => `${idPrefix}-${key}-help`;
  const errorId = (key: string) => `${idPrefix}-${key}-err`;

  function describedBy(field: SchemaField): string | undefined {
    const ids = [field.help ? helpId(field.key) : null, errors[field.key] ? errorId(field.key) : null].filter(
      (id): id is string => id !== null,
    );
    return ids.length > 0 ? ids.join(" ") : undefined;
  }

  function listText(value: unknown): string {
    if (Array.isArray(value)) return value.map((entry) => String(entry)).join("\n");
    return typeof value === "string" ? value : "";
  }

  /**
   * A list field is as tall as its lines (#1404), so a value added below the
   * fourth (a Jira view's *Sync WEB too*) is in sight: never fewer rows than
   * the three it always had, and past twelve it scrolls inside after all.
   */
  function listRows(value: unknown): number {
    return Math.min(12, Math.max(3, listText(value).split("\n").length));
  }

  function text(value: unknown): string {
    return typeof value === "string" ? value : value === null || value === undefined ? "" : String(value);
  }

  /** What a field adds under its control: the list and JSON hints. */
  function hint(field: SchemaField): string | null {
    if (field.control.kind === "list") return "One per line.";
    if (field.control.kind === "json") return "A shape the generated form does not cover. Enter it as JSON.";
    return null;
  }
</script>

{#snippet control(field: SchemaField)}
  {@const cls = layout === "rows" ? "tf-stg-in" : ""}
  {#if field.control.kind === "select"}
    <select
      class={cls}
      id={controlId(field.key)}
      {disabled}
      required={field.required}
      aria-invalid={errors[field.key] ? "true" : undefined}
      aria-describedby={describedBy(field)}
      value={text(values[field.key])}
      onchange={(event) => (values[field.key] = event.currentTarget.value)}
    >
      {#each field.control.options as option (option)}
        <option value={option}>{option}</option>
      {/each}
    </select>
  {:else if field.control.kind === "toggle"}
    <input
      type="checkbox"
      id={controlId(field.key)}
      {disabled}
      aria-describedby={describedBy(field)}
      checked={values[field.key] === true}
      onchange={(event) => (values[field.key] = event.currentTarget.checked)}
    />
  {:else if field.control.kind === "number"}
    <input
      class="{cls} {layout === 'rows' ? 'tf-stg-n90' : ''}"
      type="number"
      id={controlId(field.key)}
      {disabled}
      required={field.required}
      min={field.control.min ?? undefined}
      max={field.control.max ?? undefined}
      step={field.control.integer ? 1 : "any"}
      aria-invalid={errors[field.key] ? "true" : undefined}
      aria-describedby={describedBy(field)}
      value={text(values[field.key])}
      oninput={(event) => (values[field.key] = event.currentTarget.value)}
    />
  {:else if field.control.kind === "list" || field.control.kind === "json"}
    <textarea
      class="{cls} tf-sf-area {field.control.kind === 'json' ? 'tf-sf-mono' : ''}"
      id={controlId(field.key)}
      rows={field.control.kind === "json" ? 4 : listRows(values[field.key])}
      {disabled}
      required={field.required}
      aria-invalid={errors[field.key] ? "true" : undefined}
      aria-describedby={describedBy(field)}
      value={field.control.kind === "list" ? listText(values[field.key]) : text(values[field.key])}
      oninput={(event) =>
        (values[field.key] =
          field.control.kind === "list" ? event.currentTarget.value.split("\n") : event.currentTarget.value)}
    ></textarea>
  {:else}
    <input
      class={cls}
      type="text"
      id={controlId(field.key)}
      placeholder="None set"
      {disabled}
      required={field.required}
      aria-invalid={errors[field.key] ? "true" : undefined}
      aria-describedby={describedBy(field)}
      value={text(values[field.key])}
      oninput={(event) => (values[field.key] = event.currentTarget.value)}
    />
  {/if}
{/snippet}

{#snippet notes(field: SchemaField, withHelp: boolean)}
  {@const extra = hint(field)}
  {#if extra}<small class="tf-sf-help">{extra}</small>{/if}
  {#if withHelp && field.help}
    <!-- Text: a `description` is a sentence the adapter author wrote. -->
    <small class="tf-sf-help" id={helpId(field.key)}>{field.help}</small>
  {/if}
  {#if errors[field.key]}<small class="tf-sf-help tf-stg-errt" id={errorId(field.key)}>{errors[field.key]}</small>{/if}
{/snippet}

{#if layout === "rows"}
  {#each fields as field (field.key)}
    <div class="tf-stg-row tf-sf-row">
      <div class="tf-stg-l">
        <label for={controlId(field.key)}>{field.label}</label>
        {#if field.help}<small id={helpId(field.key)}>{field.help}</small>{/if}
      </div>
      <div class="tf-stg-c tf-sf-c">
        {@render control(field)}
        {@render notes(field, false)}
      </div>
    </div>
  {/each}
{:else if fields.length === 0}
  <p class="tf-dlg-note">This adapter needs no configuration.</p>
{:else}
  <div class="tf-formgrid tf-sf-grid">
    {#each fields as field (field.key)}
      <label for={controlId(field.key)}>{field.label}</label>
      <div class="tf-sf-c">
        {@render control(field)}
        {@render notes(field, true)}
      </div>
    {/each}
  </div>
{/if}
