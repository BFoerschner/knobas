<!--
  Settings › Renderers (#2262, spec #2234 step 5, ADR-0033): the command
  renderers, over `user_renderers` and `set_user_renderers`, and a test pane
  over `run_renderer`.

  A renderer is a name, a command (an argument list: quotes group words, no
  shell runs it), the file patterns and Markdown fence languages it accepts,
  whether it reads text or bytes, whether it writes SVG, HTML or an image,
  and a timeout. Every save writes the whole list and installs what the
  backend stored in the registry (`render/command.ts`), so the file viewer
  and Markdown use it at once.

  *Run pipelines on diffs* (#2270, `diff/diff-pipelines.svelte.ts`) is on
  by default; each diff's own toggle overrides it.

  *Default renderers* (#2263) is the global list from file pattern to the
  renderer a file opens with (`render/defaults.ts`), Source included: added
  here or from the file viewer's *Render as ▾*, changed or removed here. A
  row naming a renderer that is gone says the built-in default applies.

  The test pane runs a *saved* renderer on pasted text: the webview never
  sends a program (ADR-0016), so a renderer is saved before it is tried, as
  *Try it* does for *Open in…* (#1643).
-->
<script lang="ts" module>
  import { runRenderer, setUserRenderers, userRenderers, type UserRenderer, type UserRendererDraft } from "../../ipc/app";
  import type { CommandPort } from "../../render/command";

  export interface RendererPorts {
    userRenderers: () => Promise<UserRenderer[]>;
    setUserRenderers: (renderers: UserRendererDraft[]) => Promise<UserRenderer[]>;
    commands: CommandPort;
  }

  export const realRendererPorts: RendererPorts = {
    userRenderers: () => userRenderers(),
    setUserRenderers: (renderers) => setUserRenderers(renderers),
    commands: { run: (id, content) => runRenderer(id, content) },
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CommandOutput, RendererInput } from "../../ipc/app";
  import { COMMAND_PREFIX, installCommands } from "../../render/command";
  import { allRenderers, type Renderer } from "../../render/registry";
  import { withDefault } from "../../render/defaults";
  import { rendererDefaults, type RendererDefaults } from "../../render/defaults.svelte";
  import { push } from "../../shell/toasts.svelte";
  import TfCommandOutput from "../render/TfCommandOutput.svelte";
  import { diffPipelines, type DiffPipelinesSetting } from "../diff/diff-pipelines.svelte";

  let {
    ports = realRendererPorts,
    defaults = rendererDefaults,
    piping = diffPipelines,
  }: { ports?: RendererPorts; defaults?: RendererDefaults; piping?: DiffPipelinesSetting } = $props();

  /** `null` until the read answers. */
  let saved = $state<UserRenderer[] | null>(null);
  /** The renderer being edited: an index into `saved`, `"new"`, or none. */
  let editing = $state<number | "new" | null>(null);
  let form = $state({ name: "", command: "", patterns: "", fences: "", input: "text" as RendererInput, output: "svg" as CommandOutput, timeout: "10", diff: false });
  let refusal = $state<string | null>(null);

  /** The test pane: which saved renderer, the sample, and the run asked for. */
  let testId = $state("");
  let sample = $state("");
  let trying = $state<{ renderer: Renderer; text: string; at: number } | null>(null);

  /** Counts installs, so the renderers a default can name are read again after one. */
  let installs = $state(0);
  const choices = $derived((void installs, allRenderers()));
  const nameOf = (id: string) => choices.find((r) => r.id === id)?.name ?? null;
  /** The default being added. */
  let adding = $state({ pattern: "", renderer: "source" });
  let defaultsRefusal = $state<string | null>(null);

  $effect(() => {
    void defaults.load().catch((cause) => push({ text: `Could not read the default renderers: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  async function storeDefaults(list: { pattern: string; renderer: string }[]): Promise<boolean> {
    try {
      await defaults.save(list);
      defaultsRefusal = null;
      return true;
    } catch (cause) {
      defaultsRefusal = ipcErrorMessage(cause);
      return false;
    }
  }

  async function addDefault() {
    if (await storeDefaults(withDefault(defaults.list, adding.pattern.trim(), adding.renderer))) adding = { pattern: "", renderer: "source" };
  }

  $effect(() => {
    void ports
      .userRenderers()
      .then((read) => {
        saved = read;
        installCommands(read);
        installs += 1;
        testId = read[0]?.id ?? "";
      })
      .catch((cause) => push({ text: `Could not read the renderers: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  const words = (text: string) => text.split(/[\s,]+/).filter((word) => word !== "");
  const draftOf = (r: UserRenderer): UserRendererDraft => ({
    id: r.id,
    name: r.name,
    command: r.command,
    patterns: r.patterns,
    fences: r.fences,
    input: r.input,
    output: r.output,
    timeout_secs: r.timeout_secs,
    diff: r.diff,
  });

  function edit(at: number | "new") {
    const r = at === "new" ? null : saved![at]!;
    form = {
      name: r?.name ?? "",
      command: r?.command ?? "",
      patterns: r?.patterns.join(" ") ?? "",
      fences: r?.fences.join(" ") ?? "",
      input: r?.input ?? "text",
      output: r?.output ?? "svg",
      timeout: String(r?.timeout_secs ?? 10),
      diff: r?.diff ?? false,
    };
    refusal = null;
    editing = at;
  }

  async function store(list: UserRendererDraft[]): Promise<boolean> {
    try {
      const stored = await ports.setUserRenderers(list);
      saved = stored;
      installCommands(stored);
      installs += 1;
      if (!stored.some((r) => r.id === testId)) testId = stored[0]?.id ?? "";
      return true;
    } catch (cause) {
      refusal = ipcErrorMessage(cause);
      return false;
    }
  }

  async function saveForm() {
    if (saved === null || editing === null) return;
    const timeout = Number.parseInt(form.timeout, 10);
    const draft: UserRendererDraft = {
      id: editing === "new" ? null : saved[editing]!.id,
      name: form.name,
      command: form.command,
      patterns: words(form.patterns),
      fences: words(form.fences),
      input: form.input,
      output: form.output,
      timeout_secs: Number.isFinite(timeout) ? timeout : null,
      diff: form.diff,
    };
    const list = saved.map(draftOf);
    if (editing === "new") list.push(draft);
    else list[editing] = draft;
    if (await store(list)) {
      testId = editing === "new" ? (saved.at(-1)?.id ?? testId) : saved[editing]!.id;
      editing = null;
    }
  }

  async function remove(at: number) {
    if (saved === null) return;
    await store(saved.filter((_, i) => i !== at).map(draftOf));
  }

  function runTest() {
    const r = saved?.find((one) => one.id === testId);
    if (!r) return;
    trying = {
      renderer: { id: `${COMMAND_PREFIX}${r.id}`, name: r.name, patterns: r.patterns, fences: r.fences, input: r.input, output: r.output, diff: r.diff },
      text: sample,
      at: (trying?.at ?? 0) + 1,
    };
  }
</script>

<header class="tf-stg-h">
  <h1>Renderers</h1>
  <p>Your own programs that draw a file or a Markdown fence: SVG, HTML or an image from its text or bytes.</p>
</header>
<div class="tf-stg-row">
  <label class="tf-stg-l" for="tf-rnd-pipes"
    >Run pipelines on diffs<small
      >A diff runs a file's default pipeline on both sides and diffs the outputs, so two base64 blobs diff as decoded JSON. Each
      diff's own toggle overrides this.</small
    ></label
  >
  <div class="tf-stg-c tf-stg-inl">
    <input id="tf-rnd-pipes" type="checkbox" checked={piping.on} onchange={(event) => piping.set(event.currentTarget.checked)} /><span>On</span>
  </div>
</div>
{#if saved !== null}
  <div class="tf-stg-sec">
    <h2>Command renderers</h2>
    <p class="tf-stg-p">
      The content goes to the command on stdin and the drawing comes back on stdout. The command is an argument list, never run
      through a shell; its output is sanitized and drawn in a frame that can't reach knobas.
    </p>
    {#if saved.length === 0}
      <p class="tf-dim">No command renderers yet.</p>
    {:else}
      <table class="tf-tbl">
        <thead><tr><th>Renderer</th><th>Command</th><th>Files</th><th>Fences</th><th>Reads → writes</th><th><span class="tf-sr">Actions</span></th></tr></thead>
        <tbody>
          {#each saved as r, at (r.id)}
            <tr>
              <td><b>{r.name}</b></td>
              <td class="tf-small"><span class="tf-mono">{r.command}</span></td>
              <td class="tf-small tf-mono">{r.patterns.join(" ")}</td>
              <td class="tf-small tf-mono">{r.fences.join(" ")}</td>
              <td class="tf-small">{r.input} → {r.output.toUpperCase()}, {r.timeout_secs} s</td>
              <td class="tf-right">
                <button class="tf-btn tf-sm tf-ghost" onclick={() => edit(at)}>Edit</button>
                <button class="tf-btn tf-sm tf-ghost" onclick={() => void remove(at)}>Remove</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    {#if editing === null}
      <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn" onclick={() => edit("new")}>Add a renderer…</button></div>
    {:else}
      <div class="tf-stg-sec">
        <h2>{editing === "new" ? "New renderer" : `Edit ${saved[editing]?.name ?? ""}`}</h2>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-name">Name</label>
          <div class="tf-stg-c"><input id="tf-rnd-name" class="tf-stg-in" bind:value={form.name} placeholder="PlantUML" /></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-cmd">Command<small>Quotes group words. No shell: <span class="tf-mono">|</span>, <span class="tf-mono">;</span> and <span class="tf-mono">$VAR</span> are plain text.</small></label>
          <div class="tf-stg-c"><input id="tf-rnd-cmd" class="tf-stg-in tf-mono" bind:value={form.command} placeholder="plantuml -tsvg -pipe" /></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-pat">File patterns<small>Separated by spaces, such as <span class="tf-mono">*.puml docs/*.uml</span>.</small></label>
          <div class="tf-stg-c"><input id="tf-rnd-pat" class="tf-stg-in tf-mono" bind:value={form.patterns} /></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-fence">Markdown fence languages<small>A fenced block in one of these draws with this renderer.</small></label>
          <div class="tf-stg-c"><input id="tf-rnd-fence" class="tf-stg-in tf-mono" bind:value={form.fences} placeholder="plantuml puml" /></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-in">Reads</label>
          <div class="tf-stg-c">
            <select id="tf-rnd-in" class="tf-select" bind:value={form.input}><option value="text">Text</option><option value="bytes">Bytes</option></select>
          </div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-out">Writes</label>
          <div class="tf-stg-c">
            <select id="tf-rnd-out" class="tf-select" bind:value={form.output}
              ><option value="svg">SVG</option><option value="html">HTML</option><option value="image">An image</option></select
            >
          </div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-time">Timeout<small>1 to 120 seconds; the command is stopped after it.</small></label>
          <div class="tf-stg-c tf-stg-inl"><input id="tf-rnd-time" type="number" min="1" max="120" class="tf-stg-in tf-stg-n70" bind:value={form.timeout} /><span>seconds</span></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-rnd-diff">Diffs<small>A diff of a file it draws offers <i>Rendered</i>: before and after, each drawn by this command.</small></label>
          <div class="tf-stg-c tf-stg-inl"><input id="tf-rnd-diff" type="checkbox" bind:checked={form.diff} /><span>Draws diffs</span></div>
        </div>
        {#if refusal !== null}<p class="tf-stg-errt" role="alert">{refusal}</p>{/if}
        <div class="tf-stg-acts">
          <button class="tf-btn tf-primary" onclick={() => void saveForm()}>Save</button>
          <button class="tf-btn tf-ghost" onclick={() => (editing = null)}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>

  <div class="tf-stg-sec">
    <h2>Default renderers</h2>
    <p class="tf-stg-p">
      Which renderer a file opens with, by file pattern. Where two patterns take a file, the more specific one decides; a file no
      pattern takes opens with the first renderer that accepts it.
    </p>
    {#if defaults.list.length === 0}
      <p class="tf-dim">No defaults yet: every file opens with the first renderer that accepts it.</p>
    {:else}
      <table class="tf-tbl tf-rnd-defaults">
        <thead><tr><th>Files</th><th>Open with</th><th><span class="tf-sr">Actions</span></th></tr></thead>
        <tbody>
          {#each defaults.list as d, at (d.pattern)}
            <tr>
              <td class="tf-mono">{d.pattern}</td>
              <td>
                <select
                  class="tf-select"
                  aria-label="Open {d.pattern} with"
                  value={d.renderer}
                  onchange={(event) => void storeDefaults(defaults.list.map((row, i) => (i === at ? { ...row, renderer: event.currentTarget.value } : row)))}
                >
                  {#each choices as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
                  {#if nameOf(d.renderer) === null}<option value={d.renderer}>{d.renderer}</option>{/if}
                </select>
                {#if nameOf(d.renderer) === null}<span class="tf-small tf-dim">Not installed: the built-in default applies.</span>{/if}
              </td>
              <td class="tf-right">
                <button class="tf-btn tf-sm tf-ghost" onclick={() => void storeDefaults(defaults.list.filter((_, i) => i !== at))}>Remove</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-rnd-dpat">Add a default<small>A file pattern, such as <span class="tf-mono">*.md</span> or <span class="tf-mono">docs/*.mmd</span>.</small></label>
      <div class="tf-stg-c tf-stg-inl">
        <input id="tf-rnd-dpat" class="tf-stg-in tf-mono" bind:value={adding.pattern} placeholder="*.md" />
        <select class="tf-select" aria-label="Open with" bind:value={adding.renderer}>
          {#each choices as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
        </select>
        <button class="tf-btn" disabled={adding.pattern.trim() === ""} onclick={() => void addDefault()}>Add</button>
      </div>
    </div>
    {#if defaultsRefusal !== null}<p class="tf-stg-errt" role="alert">{defaultsRefusal}</p>{/if}
  </div>

  {#if saved.length > 0}
    <div class="tf-stg-sec">
      <h2>Test a renderer</h2>
      <p class="tf-stg-p">Paste sample text and run a saved renderer on it.</p>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-rnd-test">Renderer</label>
        <div class="tf-stg-c">
          <select id="tf-rnd-test" class="tf-select" bind:value={testId}>
            {#each saved as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
          </select>
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-rnd-sample">Sample</label>
        <div class="tf-stg-c"><textarea id="tf-rnd-sample" class="tf-stg-in tf-mono" rows="6" bind:value={sample}></textarea></div>
      </div>
      <div class="tf-stg-acts"><button class="tf-btn" onclick={runTest}>Run</button></div>
      {#if trying !== null}
        {#key trying.at}
          <div class="tf-stg-test"><TfCommandOutput renderer={trying.renderer} text={trying.text} commands={ports.commands} /></div>
        {/key}
      {/if}
    </div>
  {/if}
{/if}
