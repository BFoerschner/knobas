<!--
  The *Add source* form, generated (spec §3a).

  One `.form` grid, one row per {@link SchemaField}, and the control chosen by
  `control.kind`. There is no per-adapter branch anywhere in this file — an
  adapter that declares a new property gets a row without this component being
  touched, which is the whole claim §3a makes.

  ## Three attributes that are not decoration

  `for`/`id` pairs the label to the control. `aria-describedby` names the help
  text *and* the error, so an assistive reader gets the message with the field
  instead of finding unattached red text somewhere on the page. `required` is
  reflected onto the element so the browser, the assistive layer and
  `validate()` are all telling the reader the same story.

  ## Values

  Values are held by the caller and changed through `onchange`, rather than
  `bind:`-ed: `AddSource` needs to re-validate the whole draft on every
  keystroke, and a form that owned its own copy would give it a second one to
  keep in step.
-->
<script lang="ts">
  import type { SchemaField } from "./schema-form";

  let {
    fields,
    values,
    errors = {},
    disabled = false,
    idPrefix = "cfg",
  }: {
    fields: SchemaField[];
    values: Record<string, unknown>;
    errors?: Record<string, string>;
    disabled?: boolean;
    /** Namespaces the element ids, so two forms on one page stay wired. */
    idPrefix?: string;
  } = $props();

  const controlId = (key: string) => `${idPrefix}-${key}`;
  const helpId = (key: string) => `${idPrefix}-${key}-help`;
  const errorId = (key: string) => `${idPrefix}-${key}-err`;

  /** The ids this field's control is described by, as the attribute wants it. */
  function describedBy(field: SchemaField): string | undefined {
    const ids = [
      field.help ? helpId(field.key) : null,
      errors[field.key] ? errorId(field.key) : null,
    ].filter((id): id is string => id !== null);
    return ids.length > 0 ? ids.join(" ") : undefined;
  }

  /** A list control's textarea shows exactly what was typed. */
  function listText(value: unknown): string {
    if (Array.isArray(value)) return value.map((entry) => String(entry)).join("\n");
    return typeof value === "string" ? value : "";
  }

  function text(value: unknown): string {
    return typeof value === "string" ? value : value === null || value === undefined ? "" : String(value);
  }
</script>

{#if fields.length === 0}
  <p class="none">This adapter needs no configuration.</p>
{:else}
  <div class="form">
    {#each fields as field (field.key)}
      <label class="l" for={controlId(field.key)} data-field-label={field.key}>{field.label}</label>
      <div class="cell" data-field={field.key}>
        {#if field.control.kind === "select"}
          <select
            class="sel-inline"
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
            class="chkbox"
            type="checkbox"
            id={controlId(field.key)}
            {disabled}
            aria-describedby={describedBy(field)}
            checked={values[field.key] === true}
            onchange={(event) => (values[field.key] = event.currentTarget.checked)}
          />
        {:else if field.control.kind === "number"}
          <input
            class="inp"
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
        {:else if field.control.kind === "list"}
          <textarea
            class="inp area"
            id={controlId(field.key)}
            rows="3"
            {disabled}
            required={field.required}
            aria-invalid={errors[field.key] ? "true" : undefined}
            aria-describedby={describedBy(field)}
            value={listText(values[field.key])}
            oninput={(event) => (values[field.key] = event.currentTarget.value.split("\n"))}
          ></textarea>
        {:else if field.control.kind === "json"}
          <textarea
            class="inp area mono"
            id={controlId(field.key)}
            rows="4"
            {disabled}
            required={field.required}
            aria-invalid={errors[field.key] ? "true" : undefined}
            aria-describedby={describedBy(field)}
            value={text(values[field.key])}
            oninput={(event) => (values[field.key] = event.currentTarget.value)}
          ></textarea>
        {:else}
          <input
            class="inp"
            type="text"
            id={controlId(field.key)}
            {disabled}
            required={field.required}
            aria-invalid={errors[field.key] ? "true" : undefined}
            aria-describedby={describedBy(field)}
            value={text(values[field.key])}
            oninput={(event) => (values[field.key] = event.currentTarget.value)}
          />
        {/if}

        {#if field.control.kind === "list"}
          <span class="help" id="{idPrefix}-{field.key}-hint">One per line.</span>
        {/if}
        {#if field.control.kind === "json"}
          <span class="help" id="{idPrefix}-{field.key}-hint">
            This adapter declares a shape the generated form does not cover. Enter it as JSON.
          </span>
        {/if}
        {#if field.help}
          <!-- Text: a `description` is a sentence the adapter author wrote. -->
          <span class="help" id={helpId(field.key)}>{field.help}</span>
        {/if}
        {#if errors[field.key]}
          <span class="err-msg" id={errorId(field.key)}>{errors[field.key]}</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  /*
    `.form` in `app.css` is the mockup's `140px 1fr` grid and aligns its rows
    to the centre. A generated row can carry help and an error under its
    control, so this form's rows align to the top and the label gets the type's
    optical offset back.
  */
  .form {
    align-items: start;
  }

  .l {
    padding-top: 6px;
  }

  .cell {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .inp {
    width: 100%;
  }

  .area {
    resize: vertical;
    padding: 6px 8px;
    line-height: 1.45;
    min-height: 54px;
  }

  .chkbox {
    justify-self: start;
    accent-color: var(--text);
    width: 14px;
    height: 14px;
  }

  .help {
    font: 400 11px/1.45 var(--mono);
    color: var(--muted);
  }

  .err-msg {
    font: 400 11px/1.45 var(--mono);
    color: var(--fail);
  }

  .none {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
  }
</style>
