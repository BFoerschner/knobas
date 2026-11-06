<!--
  Settings › Renderers, the pipelines half (#2265, spec #2234 step 5): the person's command steps and their named
  pipelines.

  A command step is a name, a command (an argument list under ADR-0033: no shell, the input on stdin, the output
  on stdout) and a timeout; a pipeline's *Add step* menu lists it under *Your commands*. A named pipeline is a name
  and its steps, edited with the file viewer's bar; the default renderers table above names it per file pattern,
  by id, so editing it here changes every pattern that uses it. Each list is saved whole, and the backend's answer
  is held in the app's copy (`render/pipelines.svelte.ts`) the file viewer reads. Each is added and edited in `TfDialog`
  (F-618), as a renderer is.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CommandStep, CommandStepDraft, NamedPipeline, NamedPipelineDraft } from "../../ipc/app";
  import type { PipeStep } from "../../ipc/entity";
  import { stepName } from "../../render/pipeline";
  import type { RendererDefaults } from "../../render/defaults.svelte";
  import type { CommandSteps, NamedPipelines } from "../../render/pipelines.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { secondsLabel } from "../../time/format";
  import { putBack, removalToast } from "./removed";
  import TfFilePipeline from "../render/TfFilePipeline.svelte";
  import TfDialog from "./TfDialog.svelte";
  import { needsWords, secondsWhy } from "./form-words";

  let { pipelines, stepCommands, defaults }: { pipelines: NamedPipelines; stepCommands: CommandSteps; defaults: RendererDefaults } = $props();

  $effect(() => {
    void pipelines.load().catch((cause) => push({ text: `Could not read the pipelines: ${ipcErrorMessage(cause)}`, tone: "err" }));
    void stepCommands.load().catch((cause) => push({ text: `Could not read the command steps: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  // The removal rule: gone at once, with *Undo* on one toast however many went.
  const stepRemoved = removalToast<CommandStepDraft>("command steps", (removed) => void storeSteps(putBack(stepCommands.list.map(stepDraft), removed)));
  const pipeRemoved = removalToast<NamedPipelineDraft>("pipelines", (removed) => void storePipes(putBack(pipelines.list.map(pipeDraft), removed)));

  async function removeStep(at: number) {
    const gone = stepCommands.list[at]!;
    if (await storeSteps(stepCommands.list.filter((_, i) => i !== at).map(stepDraft))) stepRemoved({ name: gone.name, at, item: stepDraft(gone) });
  }

  async function removePipe(at: number) {
    const gone = pipelines.list[at]!;
    if (await storePipes(pipelines.list.filter((_, i) => i !== at).map(pipeDraft))) pipeRemoved({ name: gone.name, at, item: pipeDraft(gone) });
  }

  // Command steps.
  let editingStep = $state<number | "new" | null>(null);
  let stepForm = $state({ name: "", command: "", timeout: "10" });
  let stepRefusal = $state<string | null>(null);
  let savingStep = $state(false);
  const stepWhy = $derived(
    needsWords([...(stepForm.name.trim() === "" ? ["the step's name"] : []), ...(stepForm.command.trim() === "" ? ["the command"] : [])]) ?? secondsWhy(stepForm.timeout),
  );

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
    if (editingStep === null || stepWhy !== null || savingStep) return;
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
    savingStep = true;
    if (await storeSteps(list)) {
      push({ text: `${editingStep === "new" ? "Added" : "Saved"} ${stepForm.name.trim()}` });
      editingStep = null;
    }
    savingStep = false;
  }

  // Named pipelines.
  let editingPipe = $state<number | "new" | null>(null);
  let pipeForm = $state<{ name: string; steps: PipeStep[] }>({ name: "", steps: [] });
  let pipeRefusal = $state<string | null>(null);
  let savingPipe = $state(false);
  const pipeWhy = $derived(needsWords([...(pipeForm.name.trim() === "" ? ["a name"] : []), ...(pipeForm.steps.length === 0 ? ["a step"] : [])]));

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
    if (editingPipe === null || pipeWhy !== null || savingPipe) return;
    const draft: NamedPipelineDraft = {
      id: editingPipe === "new" ? null : pipelines.list[editingPipe]!.id,
      name: pipeForm.name,
      steps: $state.snapshot(pipeForm.steps),
    };
    const list = pipelines.list.map(pipeDraft);
    if (editingPipe === "new") list.push(draft);
    else list[editingPipe] = draft;
    savingPipe = true;
    if (await storePipes(list)) {
      push({ text: `${editingPipe === "new" ? "Added" : "Saved"} ${pipeForm.name.trim()}` });
      editingPipe = null;
    }
    savingPipe = false;
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
            <td class="tf-small">{secondsLabel(s.timeout_secs)}</td>
            <td class="tf-right">
              <button class="tf-btn tf-sm tf-ghost" aria-label="Edit {s.name}…" onclick={() => editStep(at)}>Edit…</button>
              <button class="tf-btn tf-sm tf-ghost tf-danger" aria-label="Remove {s.name}" onclick={() => void removeStep(at)}
                >Remove</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn tf-sm" onclick={() => editStep("new")}>Add a command step…</button></div>
  {#if stepRefusal !== null && editingStep === null}<p class="tf-stg-errt" role="alert">{stepRefusal}</p>{/if}
</div>

{#if editingStep !== null}
  <TfDialog title={editingStep === "new" ? "Add a command step" : `Edit ${stepCommands.list[editingStep]?.name ?? ""}`} why={stepWhy} onclose={() => (editingStep = null)}>
    {#snippet body()}
      <form
        class="tf-formgrid tf-sf-grid"
        id="tf-pst-form"
        onsubmit={(event) => {
          event.preventDefault();
          void saveStep();
        }}
      >
        <label for="tf-pst-name">Step name <small class="tf-req">required</small></label>
        <input id="tf-pst-name" aria-label="Step name" autocomplete="off" required bind:value={stepForm.name} placeholder="Decode protobuf" />
        <label for="tf-pst-cmd">Step command <small class="tf-req">required</small></label>
        <div class="tf-sf-c">
          <input id="tf-pst-cmd" aria-label="Step command" class="tf-mono" autocomplete="off" spellcheck="false" required bind:value={stepForm.command} placeholder="protoc --decode_raw" />
          <small class="tf-sf-help">Quotes group words. No shell: <span class="tf-mono">|</span> and <span class="tf-mono">$VAR</span> are plain text.</small>
        </div>
        <label for="tf-pst-time">Step timeout</label>
        <div class="tf-sf-c">
          <div class="tf-rnd-secs"><input id="tf-pst-time" class="tf-rnd-secn" type="number" min="1" max="120" bind:value={stepForm.timeout} /><span>seconds</span></div>
          <small class="tf-sf-help">1 to 120; the command is stopped after it.</small>
        </div>
      </form>
      {#if stepRefusal !== null}<p class="tf-dlg-note tf-stg-errt" role="alert">{stepRefusal}</p>{/if}
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (editingStep = null)}>Cancel</button>
      <button class="tf-btn tf-primary" type="submit" form="tf-pst-form" disabled={stepWhy !== null || savingStep}>{editingStep === "new" ? "Add step" : "Save step"}</button>
    {/snippet}
  </TfDialog>
{/if}

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
              <button class="tf-btn tf-sm tf-ghost" aria-label="Edit {p.name}…" onclick={() => editPipe(at)}>Edit…</button>
              <button class="tf-btn tf-sm tf-ghost tf-danger" aria-label="Remove {p.name}" onclick={() => void removePipe(at)}
                >Remove</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn tf-sm" onclick={() => editPipe("new")}>Add a pipeline…</button></div>
  {#if pipeRefusal !== null && editingPipe === null}<p class="tf-stg-errt" role="alert">{pipeRefusal}</p>{/if}
</div>

{#if editingPipe !== null}
  {@const opened = editingPipe}
  <TfDialog className="tf-dwide" title={opened === "new" ? "Add a pipeline" : `Edit ${pipelines.list[opened]?.name ?? ""}`} why={pipeWhy} onclose={() => (editingPipe = null)}>
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-ppl-form"
        onsubmit={(event) => {
          event.preventDefault();
          void savePipe();
        }}
      >
        <label for="tf-ppl-name">Pipeline name <small class="tf-req">required</small></label>
        <input id="tf-ppl-name" aria-label="Pipeline name" autocomplete="off" required bind:value={pipeForm.name} placeholder="Gzipped JSON" />
      </form>
      <TfFilePipeline
        steps={pipeForm.steps}
        path={sampleName(opened)}
        shownAs="Renderer"
        commands={stepCommands.list}
        onchange={(next) => (pipeForm.steps = next)}
        onreset={() => editPipe(opened)}
      />
      {#if pipeRefusal !== null}<p class="tf-dlg-note tf-stg-errt" role="alert">{pipeRefusal}</p>{/if}
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (editingPipe = null)}>Cancel</button>
      <button class="tf-btn tf-primary" type="submit" form="tf-ppl-form" disabled={pipeWhy !== null || savingPipe}>{opened === "new" ? "Add pipeline" : "Save pipeline"}</button>
    {/snippet}
  </TfDialog>
{/if}
