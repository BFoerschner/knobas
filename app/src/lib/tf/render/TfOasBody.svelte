<!--
  An answer's body in the API explorer (#2533): drawn by knobas's file renderers and run through its file pipelines,
  as a file in Code is.

  - **One line of controls**: *Render as* (the JSON view, the XML view, or the text as written), *Pipeline*, and
    *Expand* (in the column) or *Back to the explorer* (expanded).
  - **Render as** starts at what the content type names, else what the text is (`bodyKind`); a pipeline's run renames
    the answer (`answer-200.json.gz` → `answer-200.json`), so the view follows what the steps made.
  - **Pipeline** is the file pipeline bar (`TfFilePipeline`): jq, the decompressors, base64, the person's command
    steps. Its steps run in the backend over the kept answer's bytes (`api_answer_piped`), never over a copy the page
    holds, and what they make stays in memory, as the answer does (ADR-0057). *Save to Downloads* saves the answer as it
    came, or what the pipeline made of it. A value looked at in the view (Space on a JSON string, an element's text)
    runs over the text drawn (`text_piped`), as an example made from a schema does.
  - **A JSON answer is read indented** (`api_answer_body`'s `pretty`): an API's JSON often comes on one line, and the
    view lays out only XML. *Text* reads it as it came.
  - **In the column it is compact**: the JSON view with its jq query, the XML view with its XPath query, folding and
    *Levels*, and no rail. **Expanded** it is the whole view, the rail with its results and findings, over the
    explorer's pane.
  - **HTML is never rendered**: it is drawn as its text, whatever the person picks; no view runs its scripts.

  `view` is what the person set (the steps, the pick), shared by the column's body and the expanded one.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { apiAnswerBody, apiAnswerPiped, apiAnswerPipedSave, apiSaveAnswer, jsonQueryOpen, textPiped, textPipedSave } from "../../ipc/api";
  import { ipcErrorMessage } from "../../ipc";
  import type { PipeFailure, PipeStep } from "../../ipc/entity";
  import { inputOf, sized } from "../../render/pipeline";
  import { commandSteps } from "../../render/pipelines.svelte";
  import { renderPort } from "../../render/port";
  import { push } from "../../shell/toasts.svelte";
  import TfFilePipeline from "./TfFilePipeline.svelte";
  import { docViewModule } from "./doc/doc-module";
  import { answerName, bodyKind, sizeWords, type BodyKind, type BodyView } from "./oas-answer";

  let {
    answer,
    view,
    full = false,
    onexpand = null,
    oncollapse = null,
  }: {
    /** The answer: its send's id (what the kept body is read by), its status and its content type. */
    answer: { id: number; status: number; content_type: string; bytes: number };
    /** The steps and the pick, the person's. */
    view: BodyView;
    /** Drawn over the explorer's pane, with the view's rail; else compact, in the answer's column. */
    full?: boolean;
    /** *Expand*, in the column. */
    onexpand?: (() => void) | null;
    /** *Back to the explorer*, expanded. */
    oncollapse?: (() => void) | null;
  } = $props();

  /** The most of a body, or of what a pipeline made of it, that is drawn. */
  const DRAWN = 8 * 1024 * 1024;
  const KINDS: readonly { id: BodyKind; label: string }[] = [
    { id: "json", label: "JSON view" },
    { id: "xml", label: "XML view" },
    { id: "text", label: "Text" },
  ];

  const name = $derived(answerName(answer.status, answer.content_type));
  const html = $derived(/html/i.test(answer.content_type));
  /** The answer's own bytes are drawn: text of a size the page holds. Anything else waits for a pipeline. */
  const drawable = $derived(answer.bytes <= DRAWN && !/^(image|audio|video)\/|octet-stream|zip|pdf|zstd|x-xz|bzip/i.test(answer.content_type));
  const chain = $derived(view.steps ?? []);
  /**
   * A JSON answer is read indented by Rust (as written, it is often one line, which the view does not lay out): where
   * the type says JSON and the person has not asked for the text as written.
   */
  const pretty = $derived(/json/i.test(answer.content_type) && !/ndjson|jsonl|json-seq/i.test(answer.content_type) && view.as !== "text");

  /** What is drawn: the body's text, or a run's (its name), or why there is none. */
  type Shown = { key: string; text: string; name: string } | { key: string; none: string } | { key: string; failure: PipeFailure };
  let shown = $state.raw<Shown | null>(null);
  let running = $state(false);
  let refused = $state<string | null>(null);

  $effect(() => {
    const id = answer.id;
    const steps = $state.snapshot(chain) as PipeStep[];
    const indent = steps.length === 0 && pretty;
    const key = JSON.stringify([id, steps, indent]);
    if (untrack(() => shown?.key) === key) return;
    let live = true;
    refused = null;
    if (steps.length === 0 && !untrack(() => drawable)) {
      shown = { key, none: `${answer.content_type || "A body"} of ${sizeWords(answer.bytes)}, not drawn here. A pipeline can make text of it.` };
      return;
    }
    running = true;
    const read =
      steps.length === 0
        ? apiAnswerBody(id, indent).then((bytes): Shown => ({ key, text: new TextDecoder().decode(bytes), name: untrack(() => name) }))
        : apiAnswerPiped(id, untrack(() => name), steps).then((run): Shown => {
            if (run.outcome === "failed") return { key, failure: run.failure };
            if (run.output.kind === "bytes") return { key, none: `The pipeline made ${sized(run.output.size)} that are not text. Add a step that makes text of them.` };
            if (run.output.text.length > DRAWN) return { key, none: `The pipeline made ${sizeWords(run.output.text.length)} of text, more than knobas draws here. Save it to Downloads.` };
            return { key, text: run.output.text, name: run.name };
          });
    read.then(
      (next) => {
        if (!live) return;
        running = false;
        shown = next;
      },
      (cause: unknown) => {
        if (!live) return;
        running = false;
        shown = null;
        refused = ipcErrorMessage(cause);
      },
    );
    return () => {
      live = false;
    };
  });

  const text = $derived(shown !== null && "text" in shown ? shown : null);
  /** The view the text is drawn by: the person's pick, else what it is; HTML is only ever its text. */
  const kind = $derived<BodyKind>(text === null ? "text" : html ? "text" : (view.as ?? bodyKind(answer.content_type, text.name, text.text)));

  async function copy() {
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text.text);
      push({ text: "Copied the answer", subject: "copy-answer" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function save() {
    const steps = $state.snapshot(chain) as PipeStep[];
    try {
      const path = steps.length === 0 ? await apiSaveAnswer(answer.id, name) : await apiAnswerPipedSave(answer.id, name, steps, text?.name ?? name);
      push({ text: `Saved to ${path}`, subject: "save-answer" });
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<div class="tf-oas-bodybar">
  <span class="tf-oas-k">Render as</span>
  <span class="tf-seg" role="group" aria-label="Render as">
    {#each KINDS as k (k.id)}
      <button type="button" aria-pressed={kind === k.id} disabled={html && k.id !== "text"} title={html && k.id !== "text" ? "An HTML answer is only ever drawn as its text." : undefined} onclick={() => (view.as = k.id)}>{k.label}</button>
    {/each}
  </span>
  {#if view.steps === null}<button type="button" class="tf-btn tf-sm" onclick={() => (view.steps = [])}>Pipeline</button>{/if}
  <span class="tf-trail">
    {#if full}
      <!-- Expanded, the view takes the pane's height: what is done with the answer stands up here. -->
      {@render actions()}
      {#if oncollapse !== null}<button type="button" class="tf-btn tf-sm" data-oas-collapse onclick={() => oncollapse?.()}>Back to the explorer</button>{/if}
    {:else if onexpand !== null}
      <button type="button" class="tf-btn tf-sm tf-ghost" title="The answer over the explorer, with the view's rail" onclick={() => onexpand?.()}>Expand</button>
    {/if}
  </span>
</div>

{#if view.steps !== null}
  <TfFilePipeline
    steps={view.steps}
    path={name}
    shownAs={KINDS.find((k) => k.id === kind)?.label ?? "Text"}
    commands={commandSteps.list}
    failedAt={shown !== null && "failure" in shown ? shown.failure.index : null}
    {running}
    onchange={(next) => (view.steps = next)}
    onreset={() => (view.steps = [])}
    onclose={() => (view.steps = null)}
  />
{/if}

{#if refused !== null}
  <p class="tf-oas-fnote" role="alert">{refused === "The answer is no longer kept." ? refused : `The answer could not be read: ${refused}`}</p>
{:else if shown !== null && "failure" in shown}
  {@const f = shown.failure}
  <div class="tf-stepfail" role="alert">
    <b>Step {f.index + 1}, {f.name}, failed.</b>
    <div class="tf-sfcols">
      <div><b>What it said</b><pre>{f.error}</pre></div>
      <div><b>Its input, {inputOf(chain, f.index, commandSteps.list)}, {sized(f.input.size)}</b><pre>{f.input.head}</pre></div>
    </div>
  </div>
{:else if shown !== null && "none" in shown}
  <p class="tf-oas-fnote">{shown.none}</p>
{:else if text !== null}
  {#if kind === "text"}
    <pre class="tf-oas-ansbody tf-oas-wrap" class:tf-oas-full={full}>{text.text}</pre>
  {:else}
    {@const doc = text}
    <div class="tf-oas-docbox" class:tf-oas-full={full}>
      {#await docViewModule() then { default: TfDocView }}
        {#key `${doc.key}\n${kind}`}
          <TfDocView
            text={doc.text}
            path={doc.name}
            repo="api-answer"
            kind={kind === "json" ? "json" : "xml"}
            compact={!full}
            render={renderPort()}
            jq={kind === "json" ? { open: () => jsonQueryOpen({ text: doc.text }) } : null}
            onpipeline={kind === "json" ? (filter) => (view.steps = [...(view.steps ?? []), { step: "jq", filter }]) : null}
            values={{ run: (steps) => textPiped(doc.text, doc.name, steps), save: (steps, as) => textPipedSave(doc.text, doc.name, steps, as) }}
          />
        {/key}
      {/await}
    </div>
  {/if}
{:else}
  <div class="tf-oas-docbox" class:tf-oas-full={full} aria-busy="true"></div>
{/if}

{#if !full}<div class="tf-oas-send">{@render actions()}</div>{/if}

{#snippet actions()}
  {#if text !== null}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void copy()}>Copy</button>{/if}
  <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void save()}>Save to Downloads</button>
{/snippet}
