<!--
  Settings › Renderers, the pipelines half (#2265, spec #2234 step 5): the person's command steps and their named
  pipelines.

  A command step is a name, a command (an argument list under ADR-0033: no shell, the input on stdin, the output
  on stdout) and a timeout; a pipeline's *Add step* menu lists it under *Your commands*. A named pipeline is a name
  and its steps, edited with the file viewer's bar; the default renderers table above names it per file pattern,
  by id, so editing it here changes every pattern that uses it. Each list is saved whole, and the backend's answer
  is held in the app's copy (`render/pipelines.svelte.ts`) the file viewer reads.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CommandStep, CommandStepDraft, NamedPipeline, NamedPipelineDraft } from "../../ipc/app";
  import type { PipeStep } from "../../ipc/entity";
  import { stepName } from "../../render/pipeline";
  import type { RendererDefaults } from "../../render/defaults.svelte";
  import type { CommandSteps, NamedPipelines } from "../../render/pipelines.svelte";
  import { push } from "../../shell/toasts.svelte";
  import TfFilePipeline from "../render/TfFilePipeline.svelte";

  let { pipelines, stepCommands, defaults }: { pipelines: NamedPipelines; stepCommands: CommandSteps; defaults: RendererDefaults } = $props();

  $effect(() => {
    void pipelines.load().catch((cause) => push({ text: `Could not read the pipelines: ${ipcErrorMessage(cause)}`, tone: "err" }));
    void stepCommands.load().catch((cause) => push({ text: `Could not read the command steps: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  // Command steps.
  let editingStep = $state<number | "new" | null>(null);
  let stepForm = $state({ name: "", command: "", timeout: "10" });
  let stepRefusal = $state<string | null>(null);

  const stepDraft = (s: CommandStep): CommandStepDraft => ({ id: s.id, name: s.name, command: s.command, timeout_secs: s.timeout_secs });

  function editStep(at: number | "new") {
    const s = at === "new" ? null : stepCommands.list[at]!;
    stepForm = { name: s?.name ?? "", command: s?.command ?? "", timeout: String(s?.timeout_secs ?? 10) };
    stepRefusal = null;
    editingStep = at;
  }

  async function storeSteps(list: CommandStepDraft[]): Promise<boolean> {
    try {
      await stepCommands.save(list);
      stepRefusal = null;
      return true;
    } catch (cause) {
      stepRefusal = ipcErrorMessage(cause);
      return false;
    }
  }

  async function saveStep() {
    if (editingStep === null) return;
    const timeout = Number.parseInt(stepForm.timeout, 10);
    const draft: CommandStepDraft = {
      id: editingStep === "new" ? null : stepCommands.list[editingStep]!.id,
      name: stepForm.name,
      command: stepForm.command,
      timeout_secs: Number.isFinite(timeout) ? timeout : null,
    };
    const list = stepCommands.list.map(stepDraft);
    if (editingStep === "new") list.push(draft);
    else list[editingStep] = draft;
    if (await storeSteps(list)) editingStep = null;
  }

  // Named pipelines.
  let editingPipe = $state<number | "new" | null>(null);
  let pipeForm = $state<{ name: string; steps: PipeStep[] }>({ name: "", steps: [] });
  let pipeRefusal = $state<string | null>(null);

  const pipeDraft = (p: NamedPipeline): NamedPipelineDraft => ({ id: p.id, name: p.name, steps: p.steps });
  /** The patterns whose default names pipeline `id`. */
  const usedBy = (id: string) => defaults.list.filter((d) => d.pipeline === id).map((d) => d.pattern);

  function editPipe(at: number | "new") {
    const p = at === "new" ? null : pipelines.list[at]!;
    pipeForm = { name: p?.name ?? "", steps: p ? [...p.steps] : [] };
    pipeRefusal = null;
    editingPipe = at;
  }

  async function storePipes(list: NamedPipelineDraft[]): Promise<boolean> {
    try {
      await pipelines.save(list);
      pipeRefusal = null;
      return true;
    } catch (cause) {
      pipeRefusal = ipcErrorMessage(cause);
      return false;
    }
  }

  async function savePipe() {
    if (editingPipe === null) return;
    const draft: NamedPipelineDraft = {
      id: editingPipe === "new" ? null : pipelines.list[editingPipe]!.id,
      name: pipeForm.name,
      steps: $state.snapshot(pipeForm.steps),
    };
    const list = pipelines.list.map(pipeDraft);
    if (editingPipe === "new") list.push(draft);
    else list[editingPipe] = draft;
    if (await storePipes(list)) editingPipe = null;
  }

  /** A file name the bar's menu reads extensions from: the first pattern that uses the pipeline, as a name. */
  const sampleName = (at: number | "new") => {
    const id = at === "new" ? null : (pipelines.list[at]?.id ?? null);
    const pattern = id === null ? undefined : usedBy(id)[0];
    return (pattern ?? "file").replace(/[*?]/g, "file");
  };
</script>

<div class="tf-stg-sec">
  <h2>Command steps</h2>
  <p class="tf-stg-p">
    Your own programs as pipeline steps: the step's input goes to the command on stdin and its output comes back on stdout. The
    command is an argument list, never run through a shell.
  </p>
  {#if stepCommands.list.length === 0}
    <p class="tf-dim">No command steps yet.</p>
  {:else}
    <table class="tf-tbl tf-rnd-steps">
      <thead><tr><th>Step</th><th>Command</th><th>Timeout</th><th><span class="tf-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each stepCommands.list as s, at (s.id)}
          <tr>
            <td><b>{s.name}</b></td>
            <td class="tf-small"><span class="tf-mono">{s.command}</span></td>
            <td class="tf-small">{s.timeout_secs} s</td>
            <td class="tf-right">
              <button class="tf-btn tf-sm tf-ghost" aria-label="Edit {s.name}" onclick={() => editStep(at)}>Edit</button>
              <button class="tf-btn tf-sm tf-ghost" aria-label="Remove {s.name}" onclick={() => void storeSteps(stepCommands.list.filter((_, i) => i !== at).map(stepDraft))}
                >Remove</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if editingStep === null}
    <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn" onclick={() => editStep("new")}>Add a command step…</button></div>
    {#if stepRefusal !== null}<p class="tf-stg-errt" role="alert">{stepRefusal}</p>{/if}
  {:else}
    <div class="tf-stg-sec">
      <h2>{editingStep === "new" ? "New command step" : `Edit ${stepCommands.list[editingStep]?.name ?? ""}`}</h2>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-pst-name">Step name</label>
        <div class="tf-stg-c"><input id="tf-pst-name" class="tf-stg-in" bind:value={stepForm.name} placeholder="Decode protobuf" /></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-pst-cmd">Step command<small>Quotes group words. No shell: <span class="tf-mono">|</span> and <span class="tf-mono">$VAR</span> are plain text.</small></label>
        <div class="tf-stg-c"><input id="tf-pst-cmd" class="tf-stg-in tf-mono" bind:value={stepForm.command} placeholder="protoc --decode_raw" /></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-pst-time">Step timeout<small>1 to 120 seconds; the command is stopped after it.</small></label>
        <div class="tf-stg-c tf-stg-inl"><input id="tf-pst-time" type="number" min="1" max="120" class="tf-stg-in tf-stg-n70" bind:value={stepForm.timeout} /><span>seconds</span></div>
      </div>
      {#if stepRefusal !== null}<p class="tf-stg-errt" role="alert">{stepRefusal}</p>{/if}
      <div class="tf-stg-acts">
        <button class="tf-btn tf-primary" onclick={() => void saveStep()}>Save step</button>
        <button class="tf-btn tf-ghost" onclick={() => (editingStep = null)}>Cancel</button>
      </div>
    </div>
  {/if}
</div>

<div class="tf-stg-sec">
  <h2>Pipelines</h2>
  <p class="tf-stg-p">
    Named pipelines, saved from the file viewer or here. A default above opens a pattern's files through one; editing it here changes
    every pattern that uses it.
  </p>
  {#if pipelines.list.length === 0}
    <p class="tf-dim">No pipelines yet: open a file, add a pipeline and save it as a default.</p>
  {:else}
    <table class="tf-tbl tf-rnd-pipes">
      <thead><tr><th>Pipeline</th><th>Steps</th><th>Default for</th><th><span class="tf-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each pipelines.list as p, at (p.id)}
          <tr>
            <td><b>{p.name}</b></td>
            <td class="tf-small">{p.steps.map((s) => stepName(s, stepCommands.list)).join(" → ")}</td>
            <td class="tf-small tf-mono">{usedBy(p.id).join(" ")}</td>
            <td class="tf-right">
              <button class="tf-btn tf-sm tf-ghost" aria-label="Edit {p.name}" onclick={() => editPipe(at)}>Edit</button>
              <button class="tf-btn tf-sm tf-ghost" aria-label="Remove {p.name}" onclick={() => void storePipes(pipelines.list.filter((_, i) => i !== at).map(pipeDraft))}
                >Remove</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if editingPipe === null}
    <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn" onclick={() => editPipe("new")}>Add a pipeline…</button></div>
    {#if pipeRefusal !== null}<p class="tf-stg-errt" role="alert">{pipeRefusal}</p>{/if}
  {:else}
    <div class="tf-stg-sec">
      <h2>{editingPipe === "new" ? "New pipeline" : `Edit ${pipelines.list[editingPipe]?.name ?? ""}`}</h2>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ppl-name">Pipeline name</label>
        <div class="tf-stg-c"><input id="tf-ppl-name" class="tf-stg-in" bind:value={pipeForm.name} placeholder="Gzipped JSON" /></div>
      </div>
      <TfFilePipeline
        steps={pipeForm.steps}
        path={sampleName(editingPipe)}
        shownAs="Renderer"
        commands={stepCommands.list}
        onchange={(next) => (pipeForm.steps = next)}
        onreset={() => editPipe(editingPipe ?? "new")}
      />
      {#if pipeRefusal !== null}<p class="tf-stg-errt" role="alert">{pipeRefusal}</p>{/if}
      <div class="tf-stg-acts">
        <button class="tf-btn tf-primary" onclick={() => void savePipe()}>Save pipeline</button>
        <button class="tf-btn tf-ghost" onclick={() => (editingPipe = null)}>Cancel</button>
      </div>
    </div>
  {/if}
</div>
