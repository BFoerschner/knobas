<!--
  The Run… dialog (#852, R-9.7): one form for every way of starting a run
  of a pipeline, prototype-6's `openRunDialog`.

  - **Start from**: *Defaults*, *#N settings* when opened from a run, or a
    saved preset; picking one fills the form. With a preset picked,
    *Remove preset* (#996) deletes it (`delete_run_preset`).
  - **Branch**: a combobox with type-ahead over the branches of the
    repository the pipeline builds and those its runs built, the default
    marked *default* and the job's *your job's branch*; arrow keys and
    `Enter` pick, and any typed name is allowed.
  - **Parameters**: a field per declared parameter, typed by its parsed spec
    (a select for a fixed list, text otherwise, with the source's hint),
    marked *changed* when it differs from its default; *Add a parameter*
    adds a free name and value row.
  - **<source> options**: clean checkout, put first in the queue.
  - Where the source's credential may run and may not send the overrides or
    the queue's top (#2055, `refused_writes` asked by the page), those are
    drawn disabled saying why, and the run is sent without them.
  - **Save these settings as a preset named …**, knobas-local.
  - A warn line when the source's sign-in is failing, and the footer's line
    saying what will run.

  *Run* hands the write to `onrun`; the page queues it and draws the run as
  *queued* at once (R-9.8).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import type { PipelineRow, PipelineSettings, RunPreset, RunRow } from "../../ipc/entity";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { defaultBranch } from "./pipeline";
  import type { PipelinePorts } from "./pipeline-ports";
  import {
    branchChoices,
    fieldsOf,
    matchingBranches,
    overridesOf,
    runPayload,
    summaryOf,
    valuesFrom,
    type ExtraRow,
    type RunRequest,
    type StartValue,
  } from "./run-dialog";

  let {
    pipeline,
    settings,
    runs,
    from = null,
    jobBranch = null,
    secretNoun = null,
    cannotOverride = null,
    cannotPutFirst = null,
    ports,
    onclose,
    onrun,
  }: {
    pipeline: PipelineRow;
    settings: PipelineSettings | null;
    /** The runs on the page, for the branches they built. */
    runs: readonly RunRow[];
    /** The run *Run again…* was pressed on. */
    from?: RunRow | null;
    jobBranch?: string | null;
    /** The credential the reader has to re-enter (its noun: *password*, *token*), or `null` while sign-in works. */
    secretNoun?: string | null;
    /** Why the run cannot carry parameter overrides (#2055), or `null` where it may. */
    cannotOverride?: string | null;
    /** Why the run cannot go first in the queue (#2055), or `null` where it may. */
    cannotPutFirst?: string | null;
    ports: Pick<PipelinePorts, "presets" | "deletePreset" | "repoBranches" | "runParameters">;
    onclose: () => void;
    onrun: (request: RunRequest) => void;
  } = $props();

  const uid = $props.id();
  const fields = $derived(fieldsOf(settings));
  const fallback = $derived(defaultBranch(settings));
  const repoId = $derived(settings?.repositories.find((repo) => repo.entity_id !== null)?.entity_id ?? null);

  // -- the reads: presets, the repository's branches, the run's settings ------

  let presets = $state<RunPreset[]>([]);
  let repoBranches = $state<string[]>([]);
  let runValues = $state<StartValue[]>([]);

  onMount(() => {
    void ports.presets(pipeline.entity_id).then((rows) => (presets = rows), () => {});
    if (repoId !== null) void ports.repoBranches(repoId).then((names) => (repoBranches = names), () => {});
    if (from !== null) {
      void ports.runParameters(from.entity_id).then(
        (values) => {
          runValues = values;
          if (start === "from") pick("from");
        },
        () => {},
      );
    }
  });

  // -- the form ----------------------------------------------------------------

  let start = $state<string>(untrack(() => (from !== null ? "from" : "")));
  let branch = $state(untrack(() => from?.branch ?? jobBranch ?? defaultBranch(settings) ?? ""));
  let values = $state<Record<string, string>>(untrack(() => valuesFrom(fieldsOf(settings), [])));
  let extra = $state<ExtraRow[]>([]);
  let clean = $state(false);
  let first = $state(false);
  let save = $state(false);
  let presetName = $state("My run");

  /**
   * *Start from*: fill the form from the defaults, the run, or a preset. The
   * added rows go: a preset brings back its own, its overrides of no
   * declared parameter, and a preset saved on the default branch (`branch`
   * none) moves the branch back to it.
   */
  function pick(choice: string) {
    start = choice;
    extra = [];
    if (choice === "") {
      values = valuesFrom(fields, []);
      return;
    }
    if (choice === "from") {
      values = valuesFrom(fields, runValues);
      if (from?.branch) branch = from.branch;
      return;
    }
    const preset = presets.find((p) => `preset:${p.name}` === choice);
    if (preset === undefined) return;
    values = valuesFrom(fields, preset.parameters);
    extra = preset.parameters
      .filter((p) => !fields.some((field) => field.name === p.name))
      .map((p) => ({ name: p.name, value: p.value }));
    branch = preset.branch ?? fallback ?? "";
    clean = preset.clean_checkout;
    first = preset.first_in_queue;
  }

  const pickedPreset = $derived(presets.find((p) => `preset:${p.name}` === start) ?? null);

  /**
   * *Remove preset* (#996): drop the picked preset, then *Start from* shows
   * *Defaults* while the form keeps the removed preset's settings, so *Save
   * these settings as a preset* can bring it back. A refusal re-reads the list,
   * so a preset another window already removed leaves it too; any refusal but
   * `not_found` also says it failed, as a failed save does.
   */
  async function removePreset() {
    const gone = pickedPreset;
    if (gone === null) return;
    try {
      await ports.deletePreset(pipeline.entity_id, gone.name);
      presets = presets.filter((p) => p.name !== gone.name);
    } catch (cause) {
      if (!isIpcError(cause) || cause.code !== "not_found") {
        push({ text: `Could not remove the preset: ${ipcErrorMessage(cause)}`, tone: "err" });
      }
      presets = await ports.presets(pipeline.entity_id).catch(() => presets);
    }
    if (!presets.some((p) => `preset:${p.name}` === start)) start = "";
  }

  /** What the run carries: no override, and not the queue's top, where the credential may not send them. */
  const overrides = $derived(cannotOverride !== null ? [] : overridesOf(fields, values, extra));
  const firstInQueue = $derived(first && cannotPutFirst === null);
  const shownBranch = $derived(branch.trim() || fallback || "the default branch");
  const summary = $derived(summaryOf(shownBranch, overrides));

  // -- the branch combobox -----------------------------------------------------

  const choices = $derived(
    branchChoices({ defaultBranch: fallback, repository: repoBranches, runs: runs.map((run) => run.branch), jobBranch }),
  );
  let open = $state(false);
  let active = $state(0);
  const shown = $derived(matchingBranches(choices, branch));
  const listId = `tf-run-branches-${uid}`;

  function choose(name: string) {
    branch = name;
    open = false;
  }

  function onBranchKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      open = true;
      active = Math.min(active + 1, shown.length - 1);
      event.preventDefault();
    } else if (event.key === "ArrowUp") {
      active = Math.max(active - 1, 0);
      event.preventDefault();
    } else if (event.key === "Enter" && open && shown[active] !== undefined) {
      choose(shown[active]!.name);
      event.preventDefault();
    } else if (event.key === "Escape" && open) {
      // The list closes first; the dialog stays.
      open = false;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  // -- Run -----------------------------------------------------------------------

  const title = $derived(
    from !== null ? `Run ${pipeline.name} again, from ${from.number !== null ? `#${from.number}` : "this run"}` : `Run ${pipeline.name}`,
  );
  const fromLabel = $derived(from !== null && from.number !== null ? `#${from.number} settings` : "This run’s settings");

  function run() {
    const options = { clean_checkout: clean, first_in_queue: firstInQueue };
    const payload = runPayload(pipeline.entity_id, branch, fallback, overrides, options);
    const sent = "RunBuild" in payload ? payload.RunBuild.branch : null;
    onrun({
      payload,
      summary,
      preset:
        save && presetName.trim() !== ""
          ? { pipeline_id: pipeline.entity_id, name: presetName.trim(), branch: sent, parameters: overrides, ...options }
          : null,
    });
  }
</script>

<TfDialog {title} {onclose} className="tf-run-dialog">
  {#snippet body()}
    <div class="tf-formgrid">
      <label for={`tf-run-start-${uid}`}>Start from</label>
      <div class="tf-run-start">
        <select id={`tf-run-start-${uid}`} value={start} onchange={(event) => pick(event.currentTarget.value)}>
          <option value="">Defaults</option>
          {#if from !== null}<option value="from">{fromLabel}</option>{/if}
          {#each presets as preset (preset.name)}
            <option value={`preset:${preset.name}`}>Preset: {preset.name}</option>
          {/each}
        </select>
        {#if pickedPreset !== null}
          <button type="button" class="tf-linkish tf-run-remove" onclick={removePreset}>Remove preset</button>
        {/if}
      </div>
      <label for={`tf-run-branch-${uid}`}>Branch</label>
      <div class="tf-combo">
        <input
          id={`tf-run-branch-${uid}`}
          type="text"
          role="combobox"
          autocomplete="off"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && shown[active] ? `${listId}-${active}` : undefined}
          bind:value={branch}
          onfocus={() => ((open = true), (active = 0))}
          onblur={() => (open = false)}
          oninput={() => ((open = true), (active = 0))}
          onkeydown={onBranchKey}
        />
        {#if open && shown.length > 0}
          <div class="tf-opts" id={listId} role="listbox" aria-label="Branches">
            {#each shown as choice, at (choice.name)}
              <div
                id={`${listId}-${at}`}
                role="option"
                tabindex="-1"
                aria-selected={at === active}
                class:tf-act={at === active}
                onmousedown={(event) => {
                  event.preventDefault();
                  choose(choice.name);
                }}
              >
                <span>{choice.name}</span>{#if choice.note !== null}<span class="tf-dim">{choice.note}</span>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    {#if fields.length > 0 || cannotOverride !== null}
      <div class="tf-pane-h">Parameters</div>
    {/if}
    {#if cannotOverride !== null}
      <p class="tf-dim tf-run-refused">{cannotOverride}</p>
      {#if fields.length > 0}
        <div class="tf-prm">
          {#each fields as field (field.name)}
            {@const id = `tf-run-p-${uid}-${field.name}`}
            <label class="tf-k" for={id} title={field.label !== null ? field.name : undefined}>{field.label ?? field.name}</label>
            {#if field.kind.type === "select"}
              <select {id} aria-label={field.label ?? field.name} disabled value={field.default}>
                {#each field.kind.options as option (option.value)}
                  <option value={option.value}>{option.label ?? option.value}</option>
                {/each}
              </select>
            {:else}
              <input {id} aria-label={field.label ?? field.name} type={field.secret ? "password" : "text"} disabled value={field.default} />
            {/if}
            <span class="tf-chg"></span>
          {/each}
        </div>
      {/if}
    {:else if fields.length > 0}
      <div class="tf-prm">
        {#each fields as field (field.name)}
          {@const id = `tf-run-p-${uid}-${field.name}`}
          <label class="tf-k" for={id} title={field.label !== null ? field.name : undefined}>{field.label ?? field.name}</label>
          {#if field.kind.type === "select"}
            <select {id} aria-label={field.label ?? field.name} bind:value={values[field.name]}>
              {#each field.kind.options as option (option.value)}
                <option value={option.value}>{option.label ?? option.value}</option>
              {/each}
            </select>
          {:else}
            <input
              {id}
              aria-label={field.label ?? field.name}
              type={field.secret ? "password" : "text"}
              placeholder={field.secret ? "(hidden)" : (field.hint ?? "")}
              bind:value={values[field.name]}
            />
          {/if}
          <span class="tf-chg">{values[field.name] !== field.default ? "changed" : ""}</span>
        {/each}
      </div>
    {/if}
    {#if extra.length > 0 && cannotOverride === null}
      <div class="tf-prm">
        {#each extra as row, at (at)}
          <input class="tf-k" type="text" placeholder="name" aria-label={`Name of added parameter ${at + 1}`} bind:value={row.name} />
          <input type="text" placeholder="value" aria-label={`Value of added parameter ${at + 1}`} bind:value={row.value} />
          <span></span>
        {/each}
      </div>
    {/if}
    <button
      type="button"
      class="tf-linkish tf-run-add"
      {...refusedAttrs(cannotOverride)}
      onclick={() => {
        if (cannotOverride === null) extra = [...extra, { name: "", value: "" }];
      }}>Add a parameter</button
    >

    <details class="tf-run-opts">
      <summary>{pipeline.source_name} options</summary>
      <div>
        <label class="tf-ck"><input type="checkbox" bind:checked={clean} /> Clean checkout</label>
        {#if cannotPutFirst !== null}
          <label class="tf-ck" title={cannotPutFirst}><input type="checkbox" disabled checked={false} /> Put first in the queue</label>
        {:else}
          <label class="tf-ck"><input type="checkbox" bind:checked={first} /> Put first in the queue</label>
        {/if}
      </div>
    </details>
    <label class="tf-ck tf-run-save"
      ><input type="checkbox" bind:checked={save} /> Save these settings as a preset named
      <input type="text" aria-label="Preset name" bind:value={presetName} /></label
    >
    {#if secretNoun !== null}
      <div class="tf-banner tf-warn tf-dbanner">
        {pipeline.source_name} sign-in is failing. The run waits in knobas’s write queue and starts once you re-enter the {secretNoun}.
      </div>
    {/if}
  {/snippet}
  {#snippet footer()}
    <span class="tf-runsum">{summary}</span>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" onclick={run}>Run</button>
  {/snippet}
</TfDialog>
