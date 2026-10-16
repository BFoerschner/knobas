<!--
  *Add an application* and *Edit an application* (#844, R-8.23), prototype-6's
  `openAppDialog`: a name, a command, a command for a file; the placeholders
  as buttons that insert at the cursor; *It will run*, filled at a checkout on
  this disk; the warnings; *Try it on <repo>*; and, editing, *Remove*.

  Saving writes the whole list (`set_checkout_applications`), so every menu
  redraws from the answer. *Try it* runs the command as typed and saves
  nothing (`open_checkout` with `command`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutApplication } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { Apps } from "./apps.svelte";
  import { fill, insertAt, PLACEHOLDERS, quoteIfSpace, warnings, type Sample } from "./openin";

  let {
    apps,
    index = null,
    sample,
    layers = sharedLayers,
    onclose,
  }: {
    apps: Apps;
    /** The application being edited, by its place in the list; `null` adds one. */
    index?: number | null;
    /** `null` where there is no checkout to try it on (Settings). */
    sample: Sample | null;
    layers?: Layers;
    onclose: () => void;
  } = $props();

  // Read once, when the dialog opens.
  const editing: CheckoutApplication | null = (() => (index !== null ? (apps.settings?.applications[index] ?? null) : null))();
  let name = $state(editing?.name ?? "");
  let command = $state(editing?.command ?? "");
  let fileCommand = $state(editing?.file_command ?? "");
  let error = $state<string | null>(null);
  let busy = $state(false);

  let commandField = $state<HTMLInputElement | null>(null);
  let fileField = $state<HTMLInputElement | null>(null);
  /** The field a placeholder button inserts into: the one last focused. */
  let last = $state<"command" | "file">("command");

  const checkout = $derived(sample?.path ?? "/Users/you/code/repository");
  const file = $derived(`${checkout}/${sample?.file ?? "README.md"}`);
  const preview = $derived.by(() => {
    const typed = command.trim();
    const forFile = fileCommand.trim();
    const head = typed
      ? fill(typed, { path: quoteIfSpace(checkout), file: null, line: null })
      : "(type a command)";
    return forFile ? `${head}\nfor a file: ${fill(forFile, { path: checkout, file, line: 42 })}` : head;
  });
  const warned = $derived(warnings(command.trim()));
  const ready = $derived(name.trim() !== "" && command.trim() !== "" && !busy);

  function insert(placeholder: string) {
    const field = last === "file" ? fileField : commandField;
    const value = last === "file" ? fileCommand : command;
    const next = insertAt(value, field?.selectionStart ?? null, placeholder);
    if (last === "file") fileCommand = next.value;
    else command = next.value;
    queueMicrotask(() => {
      field?.focus();
      field?.setSelectionRange(next.cursor, next.cursor);
    });
  }

  async function write(list: CheckoutApplication[], said: string) {
    busy = true;
    error = null;
    try {
      const current = apps.settings?.default_application ?? null;
      await apps.save(list, list.some((app) => app.id === current) ? current : null);
      push({ text: said });
      onclose();
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
    }
  }

  function save() {
    if (!ready) return;
    const list = [...(apps.settings?.applications ?? [])];
    const entry: CheckoutApplication = {
      id: editing?.id ?? "",
      name: name.trim(),
      command: command.trim(),
      file_command: fileCommand.trim() || null,
    };
    if (index !== null && editing !== null) list[index] = entry;
    else list.push(entry);
    void write(list, `${editing ? "Saved" : "Added"} ${entry.name}. It is in every Open in… menu.`);
  }

  function remove() {
    if (index === null) return;
    const list = (apps.settings?.applications ?? []).filter((_, at) => at !== index);
    void write(list, "Removed the application");
  }

  function tryIt() {
    if (sample === null || command.trim() === "") return;
    void apps.run(sample.entityId, name.trim() || "the application", { command: command.trim() });
  }
</script>

<TfDialog title={editing ? "Edit an application" : "Add an application"} {onclose} {layers} className="tf-app-dialog">
  {#snippet body()}
    <form
      class="tf-formgrid"
      id="tf-app-form"
      onsubmit={(event) => {
        event.preventDefault();
        save();
      }}
    >
      <label for="tf-app-name">Name</label>
      <input id="tf-app-name" type="text" autocomplete="off" placeholder="MyGitViewer" bind:value={name} />
      <label for="tf-app-command">Command</label>
      <input
        id="tf-app-command"
        class="tf-mono"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="MyGitViewer {'{path}'}"
        bind:value={command}
        bind:this={commandField}
        onfocus={() => (last = "command")}
      />
      <label for="tf-app-file">For a file</label>
      <input
        id="tf-app-file"
        class="tf-mono"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="optional, e.g. MyGitViewer --open {'{file}'}"
        bind:value={fileCommand}
        bind:this={fileField}
        onfocus={() => (last = "file")}
      />
    </form>
    <div class="tf-pane-h">Placeholders knobas fills in</div>
    <div class="tf-phs">
      {#each PLACEHOLDERS as placeholder (placeholder.name)}
        <button type="button" class="tf-phb" onclick={() => insert(placeholder.name)}
          ><span class="tf-mono">{placeholder.name}</span> {placeholder.says}</button
        >
      {/each}
    </div>
    <div class="tf-pane-h">It will run</div>
    <pre class="tf-log" aria-label="It will run">{preview}</pre>
    {#each warned as warning (warning)}
      <div class="tf-banner tf-warn tf-dbanner" role="status">{warning}</div>
    {/each}
    {#if error !== null}
      <div class="tf-banner tf-err tf-dbanner" role="alert">{error}</div>
    {/if}
    <p class="tf-dsub">
      The values come only from this disk: the checkout knobas found or the path you set. Branch names, titles and other
      synced values are never passed to a program. Paths with spaces are quoted for you. The command runs as you, with no
      shell, so pipes and <span class="tf-mono">&amp;&amp;</span> don’t work; point it at a script instead.
    </p>
  {/snippet}
  {#snippet footer()}
    {#if editing}
      <button type="button" class="tf-btn tf-ghost tf-remove" disabled={busy} onclick={remove}>Remove</button>
    {/if}
    {#if sample !== null}
      <button type="button" class="tf-btn" disabled={command.trim() === ""} onclick={tryIt}>Try it on {sample.name}</button>
    {/if}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-app-form" disabled={!ready}>{editing ? "Save" : "Add application"}</button>
  {/snippet}
</TfDialog>
