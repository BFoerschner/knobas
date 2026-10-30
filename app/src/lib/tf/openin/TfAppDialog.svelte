<!--
  *Add an application* and *Edit an application* (#844, R-8.23), prototype-6's
  `openAppDialog`: a name, a command, a command for a file, and (#2274) a
  command for a commit, a range and a pull request, each optional: an
  application is offered for the objects it has a command for; the placeholders
  as buttons that insert at the cursor; *It will run*, filled at a checkout on
  this disk; the warnings; *Try it on <repo>*; and, editing, *Remove*.

  Saving writes the whole list (`set_checkout_applications`), so every menu
  redraws from the answer. *Try it* saves the draft too and opens it by the id
  the save gave it (`open_checkout` with `application`): the program is always
  a saved template (#1643, ADR-0016). From then on the dialog edits that entry,
  and closing it unsaved puts the list back as it was before the first try.
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
  let commitCommand = $state(editing?.commit_command ?? "");
  let rangeCommand = $state(editing?.range_command ?? "");
  let prCommand = $state(editing?.pull_request_command ?? "");
  let error = $state<string | null>(null);
  let busy = $state(false);
  /** The entry's place in the saved list: `index`, or where a *Try it* saved a new one. */
  let entryAt: number | null = (() => index)();
  /** The list and its default before the first *Try it*, put back if the dialog closes unsaved. */
  let beforeTry: { list: CheckoutApplication[]; default: string | null } | null = null;
  /** The save a *Try it* has in flight, which putting the list back waits for. */
  let trying: Promise<void> | null = null;
  /** Set when the dialog closes: a try whose save lands after it starts nothing. */
  let closed = false;

  let commandField = $state<HTMLInputElement | null>(null);
  let fileField = $state<HTMLInputElement | null>(null);
  let commitField = $state<HTMLInputElement | null>(null);
  let rangeField = $state<HTMLInputElement | null>(null);
  let prField = $state<HTMLInputElement | null>(null);
  type Field = "command" | "file" | "commit" | "range" | "pr";
  /** The field a placeholder button inserts into: the one last focused. */
  let last = $state<Field>("command");

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

  const FIELDS: Record<Field, { el: () => HTMLInputElement | null; get: () => string; set: (value: string) => void }> = {
    command: { el: () => commandField, get: () => command, set: (value) => (command = value) },
    file: { el: () => fileField, get: () => fileCommand, set: (value) => (fileCommand = value) },
    commit: { el: () => commitField, get: () => commitCommand, set: (value) => (commitCommand = value) },
    range: { el: () => rangeField, get: () => rangeCommand, set: (value) => (rangeCommand = value) },
    pr: { el: () => prField, get: () => prCommand, set: (value) => (prCommand = value) },
  };

  function insert(placeholder: string) {
    const target = FIELDS[last];
    const field = target.el();
    const next = insertAt(target.get(), field?.selectionStart ?? null, placeholder);
    target.set(next.value);
    queueMicrotask(() => {
      field?.focus();
      field?.setSelectionRange(next.cursor, next.cursor);
    });
  }

  /** Save `list`, keeping the default where it still names an entry; rejects with the refusal. */
  async function store(list: CheckoutApplication[]) {
    const current = apps.settings?.default_application ?? null;
    await apps.save(list, list.some((app) => app.id === current) ? current : null);
  }

  async function write(list: CheckoutApplication[], said: string) {
    busy = true;
    error = null;
    try {
      await store(list);
      beforeTry = null;
      push({ text: said });
      onclose();
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
    }
  }

  /** The saved list with the dialog's entry in its place, or at the end for a new one. */
  function withEntry(): CheckoutApplication[] {
    const list = [...(apps.settings?.applications ?? [])];
    const entry: CheckoutApplication = {
      id: entryAt !== null ? (list[entryAt]?.id ?? "") : "",
      name: name.trim(),
      command: command.trim(),
      file_command: fileCommand.trim() || null,
      ...(commitCommand.trim() ? { commit_command: commitCommand.trim() } : {}),
      ...(rangeCommand.trim() ? { range_command: rangeCommand.trim() } : {}),
      ...(prCommand.trim() ? { pull_request_command: prCommand.trim() } : {}),
    };
    if (entryAt !== null && entryAt < list.length) list[entryAt] = entry;
    else list.push(entry);
    return list;
  }

  function save() {
    if (!ready) return;
    void write(withEntry(), `${editing ? "Saved" : "Added"} ${name.trim()}. It is in every Open in… menu.`);
  }

  function remove() {
    if (index === null) return;
    const list = (apps.settings?.applications ?? []).filter((_, at) => at !== index);
    void write(list, "Removed the application");
  }

  async function tryIt() {
    if (sample === null || !ready) return;
    const was = { list: [...(apps.settings?.applications ?? [])], default: apps.settings?.default_application ?? null };
    const list = withEntry();
    const first = beforeTry === null;
    // Before the save, so a dialog closed while it is in flight puts the list back.
    beforeTry ??= was;
    busy = true;
    error = null;
    trying = store(list);
    try {
      await trying;
    } catch (cause) {
      error = ipcErrorMessage(cause);
      if (first) beforeTry = null;
      return;
    } finally {
      busy = false;
      trying = null;
    }
    entryAt ??= list.length - 1;
    if (closed) return;
    const id = apps.settings?.applications[entryAt]?.id;
    if (id === undefined) error = `${name.trim()} was saved, but the list came back without it.`;
    else await apps.run(sample.entityId, name.trim(), { application: id });
  }

  /** Close unsaved: a list a *Try it* changed goes back to what it was. */
  function dismiss() {
    closed = true;
    const restore = beforeTry;
    beforeTry = null;
    if (restore !== null) {
      const tried = name.trim();
      (trying ?? Promise.resolve())
        .catch(() => {})
        .then(() => apps.save(restore.list, restore.default))
        .catch((cause: unknown) => {
          push({ text: `Could not take back the try of ${tried}: ${ipcErrorMessage(cause)}`, tone: "err" });
        });
    }
    onclose();
  }
</script>

<TfDialog title={editing ? "Edit an application" : "Add an application"} onclose={dismiss} {layers} className="tf-app-dialog">
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
      <label for="tf-app-commit">For a commit</label>
      <input
        id="tf-app-commit"
        class="tf-mono"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="optional, e.g. tig -C {'{path}'} show {'{sha}'}"
        bind:value={commitCommand}
        bind:this={commitField}
        onfocus={() => (last = "commit")}
      />
      <label for="tf-app-range">For a range</label>
      <input
        id="tf-app-range"
        class="tf-mono"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="optional, e.g. git -C {'{path}'} difftool -d {'{base}'} {'{head}'}"
        bind:value={rangeCommand}
        bind:this={rangeField}
        onfocus={() => (last = "range")}
      />
      <label for="tf-app-pr">For a pull request</label>
      <input
        id="tf-app-pr"
        class="tf-mono"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="optional, e.g. MyGitViewer --pr {'{pr}'} {'{path}'}"
        bind:value={prCommand}
        bind:this={prField}
        onfocus={() => (last = "pr")}
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
      The paths come only from this disk: the checkout knobas found or the path you set. A sha, a branch or a pull
      request number is passed only when it is a plain git name, and the URL is built from the source’s address; titles
      and other synced values are never passed to a program. Paths with spaces are quoted for you. The command runs as you, with no
      shell, so pipes and <span class="tf-mono">&amp;&amp;</span> don’t work; point it at a script instead.
    </p>
  {/snippet}
  {#snippet footer()}
    {#if editing}
      <button type="button" class="tf-btn tf-ghost tf-remove" disabled={busy} onclick={remove}>Remove</button>
    {/if}
    {#if sample !== null}
      <button type="button" class="tf-btn" disabled={!ready} onclick={() => void tryIt()}>Try it on {sample.name}</button>
    {/if}
    <button type="button" class="tf-btn tf-ghost" onclick={dismiss}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-app-form" disabled={!ready}>{editing ? "Save" : "Add application"}</button>
  {/snippet}
</TfDialog>
