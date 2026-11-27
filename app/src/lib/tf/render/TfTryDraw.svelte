<!--
  What *Try it on* draws on Settings › Renderers (#2562): the person's example (pasted text, or a file chosen anywhere
  on this disk) drawn as the Code page draws a file, so what they try is what they get in a repository.

  - **The same choice.** What takes the file and what it opens with are `openWith`'s, over the registry and the default
    renderers; *Drawn by* on the panel picks one, else the default does, and the header's *Render as ▾* switches as on
    the Code page (and *Make default* writes the same default). A pipeline runs where the panel names one, or where a
    default names one for the file's name (*As the Code page does*); its bar is the Code page's, and a change runs at once.
  - **The same drawing.** The leaves are Code's own: Source's lines, Markdown, Mermaid, D2 and Graphviz, a page in the
    frame that runs no script, a command renderer's output, an image, a PDF, the XML and JSON view, the API explorer and
    the schema explorer (a set of one file: nothing else is beside it). A JAR, which Code reads through the archive's
    backend reads, is not drawn here, and says so.
  - **The same sizes.** A file is read up to the 50 MiB Code hands a renderer; past it the read is refused and said. A
    pipeline runs under its own caps (`try_piped`).
  - **Nothing is kept**: the bytes stay in this component, the run in the backend's memory.

  Waiting, a refusal and the drawing all sit on one stage that holds its room.
-->
<script lang="ts" module>
  import { tryBytes, tryPiped, type TryOn } from "../../ipc/app";
  import type { PipeRun, PipeStep } from "../../ipc/entity";
  import { ipcCommands, type CommandPort } from "../../render/command";

  /** The reads a try makes: the backend's, a test's own. */
  export interface TryDrawPorts {
    bytes: (path: string) => Promise<Uint8Array>;
    piped: (on: TryOn, steps: PipeStep[]) => Promise<PipeRun>;
    commands: CommandPort;
    frame: (document: string) => Promise<string>;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";

  import { folderRenderFrame, type CommandStep, type NamedPipeline, type RendererDefault } from "../../ipc/app";
  import { ipcErrorMessage } from "../../ipc";
  import { commandIdOf } from "../../render/command";
  import { defaultPipeline, makeDefaultPattern, openWith, withDefault } from "../../render/defaults";
  import { pipedFile, sized, stepName } from "../../render/pipeline";
  import { allRenderers, type Renderer } from "../../render/registry";
  import { renderPort, type RenderPort } from "../../render/port";
  import { push } from "../../shell/toasts.svelte";
  import TfLines from "../code/TfLines.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { isText, tryName } from "../settings/try-on";
  import TfBytesImage from "./TfBytesImage.svelte";
  import TfCommandOutput from "./TfCommandOutput.svelte";
  import TfDiagram from "./TfDiagram.svelte";
  import TfFilePipeline from "./TfFilePipeline.svelte";
  import TfHtmlPage from "./TfHtmlPage.svelte";
  import TfPdf from "./TfPdf.svelte";
  import TfRendererSwitch from "./TfRendererSwitch.svelte";
  import { docViewModule } from "./doc/doc-module";
  import { explorerModule } from "./oas-module";
  import { xsdModule } from "./xsd/xsd-module";

  const realPorts: TryDrawPorts = { bytes: tryBytes, piped: tryPiped, commands: ipcCommands, frame: folderRenderFrame };

  let {
    on,
    renderer: wanted = "auto",
    pipeline = "auto",
    defaults,
    pipelines,
    commandSteps = [],
    savedefaults = null,
    ports = realPorts,
    render = renderPort(),
  }: {
    /** What the person pressed *Try it* on. */
    on: TryOn;
    /** *Drawn by*: a renderer's id, or `auto` for what the Code page opens the file with. */
    renderer?: string;
    /** *Through*: a named pipeline's id, `none`, or `auto` for the default's pipeline for the file's name. */
    pipeline?: string;
    defaults: readonly RendererDefault[];
    pipelines: readonly NamedPipeline[];
    commandSteps?: readonly CommandStep[];
    /** *Make default*: stores the defaults as the Code page does; `null` offers none. */
    savedefaults?: ((list: RendererDefault[]) => Promise<unknown>) | null;
    ports?: TryDrawPorts;
    render?: RenderPort;
  } = $props();

  const name = $derived(tryName(on));

  // -- the read ------------------------------------------------------------------------------------------------------

  type Read = { kind: "reading" } | { kind: "read"; bytes: Uint8Array; text: string | null } | { kind: "refused"; why: string };
  let read = $state.raw<Read>({ kind: "reading" });
  $effect(() => {
    const at = on;
    read = { kind: "reading" };
    if (at.kind === "text") {
      read = { kind: "read", bytes: new TextEncoder().encode(at.text), text: at.text };
      return;
    }
    let live = true;
    untrack(() => ports)
      .bytes(at.path)
      .then(
        (bytes) => live && (read = { kind: "read", bytes, text: isText(bytes) ? new TextDecoder().decode(bytes) : null }),
        (cause: unknown) => live && (read = { kind: "refused", why: ipcErrorMessage(cause) }),
      );
    return () => {
      live = false;
    };
  });

  // -- the pipeline --------------------------------------------------------------------------------------------------

  /** The steps the panel named: none, the default's for the name, or a named pipeline's. */
  const named = $derived.by((): { steps: PipeStep[]; name: string | null } => {
    if (pipeline === "none") return { steps: [], name: null };
    if (pipeline === "auto") {
      const found = defaultPipeline(name, defaults, pipelines);
      return found === null ? { steps: [], name: null } : { steps: found.pipeline.steps, name: found.pipeline.name };
    }
    const found = pipelines.find((p) => p.id === pipeline);
    return { steps: found?.steps ?? [], name: found?.name ?? null };
  });
  /** The bar's steps: the named ones, until the person changes them here. */
  let edited = $state<PipeStep[] | null>(null);
  $effect(() => {
    void on;
    void named;
    edited = null;
  });
  const steps = $derived(edited ?? named.steps);

  let ran = $state.raw<PipeRun | null>(null);
  let running = $state(false);
  let pipeRefused = $state<string | null>(null);
  $effect(() => {
    const chain = steps;
    const at = on;
    ran = null;
    pipeRefused = null;
    if (chain.length === 0) {
      running = false;
      return;
    }
    let live = true;
    running = true;
    untrack(() => ports)
      .piped(at, chain)
      .then(
        (answer) => {
          if (!live) return;
          running = false;
          ran = answer;
        },
        (cause: unknown) => {
          if (!live) return;
          running = false;
          pipeRefused = ipcErrorMessage(cause);
        },
      );
    return () => {
      live = false;
    };
  });
  const failure = $derived(ran !== null && ran.outcome === "failed" ? ran.failure : null);

  // -- what is drawn ---------------------------------------------------------------------------------------------------

  /** The file as drawn: the pipeline's output where it ran, else the example as read. */
  const drawn = $derived.by((): { path: string; text: string; binary: boolean; bytes: Uint8Array | null; size: number } | null => {
    if (read.kind !== "read") return null;
    if (steps.length > 0) {
      if (ran === null || ran.outcome !== "done") return null;
      return { ...pipedFile(ran), bytes: null };
    }
    return { path: name, text: read.text ?? "", binary: read.text === null, bytes: read.bytes, size: read.bytes.length };
  });
  const opened = $derived(drawn === null ? null : openWith(drawn, allRenderers(), defaults));
  /** The renderer picked in the header's switch, for this try. */
  let switched = $state<string | null>(null);
  $effect(() => {
    void on;
    void wanted;
    switched = null;
  });
  const shown = $derived.by((): Renderer | null => {
    if (opened === null) return null;
    const pick = switched ?? (wanted === "auto" ? null : wanted);
    return opened.offered.find((r) => r.id === pick) ?? opened.renderer;
  });
  /** *Drawn by* named a renderer that does not take this example. */
  const notTaken = $derived(opened !== null && wanted !== "auto" && switched === null && !opened.offered.some((r) => r.id === wanted) ? (allRenderers().find((r) => r.id === wanted)?.name ?? wanted) : null);
  const drawAs = $derived(shown?.id ?? null);
  const bytesRead = $derived(drawn?.bytes ? () => Promise.resolve(drawn.bytes!) : null);
  const image = $derived.by(() => {
    if (drawAs !== "image" || drawn === null || drawn.binary) return null;
    const bytes = new TextEncoder().encode(drawn.text);
    return { bytes, read: () => Promise.resolve(bytes) };
  });

  async function makeDefault(chosen: Renderer) {
    if (savedefaults === null || drawn === null || opened === null) return;
    try {
      await savedefaults(withDefault([...defaults], makeDefaultPattern(opened, drawn.path), chosen.id));
      switched = null;
    } catch (cause) {
      push({ text: `Could not make ${chosen.name} the default: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<div class="tf-try-out tf-fileview tf-try-view" data-shows={drawAs ?? read.kind}>
  <header>
    <span class="tf-p">{drawn?.path ?? name}</span>
    <span class="tf-dim tf-small tf-fmeta">{read.kind === "read" ? sized(read.bytes.length) : ""}{named.name !== null ? ` · through ${named.name}` : ""}</span>
    <span class="tf-trail">
      {#if opened !== null && shown !== null}
        <TfRendererSwitch {opened} {shown} pattern={drawn === null ? "" : makeDefaultPattern(opened, drawn.path)} onpick={(r) => (switched = r.id)} onmakedefault={(r) => void makeDefault(r)} />
      {/if}
    </span>
  </header>
  {#if steps.length > 0 && read.kind === "read"}
    <TfFilePipeline
      {steps}
      commands={commandSteps}
      path={name}
      shownAs={shown?.name ?? "Source"}
      failedAt={failure?.index ?? null}
      {running}
      onchange={(next) => (edited = next)}
      onreset={() => (edited = null)}
      edited={edited !== null}
    />
  {/if}
  <div class="tf-try-stage">
    {#if read.kind === "reading"}
      <p class="tf-empty tf-dim" role="status">Reading {name}…</p>
    {:else if read.kind === "refused"}
      <div class="tf-banner tf-err" role="alert">Could not read {name}: {read.why}</div>
    {:else if pipeRefused !== null}
      <div class="tf-banner tf-err" role="alert">The pipeline could not read {name}: {pipeRefused}</div>
    {:else if failure !== null}
      <div class="tf-stepfail" role="alert">
        <b>Step {failure.index + 1}, {failure.name}, failed.</b>
        <div class="tf-sfcols">
          <div><b>What it said</b><pre>{failure.error}</pre></div>
          <div><b>Its input, {sized(failure.input.size)}</b><pre>{failure.input.head}</pre></div>
        </div>
      </div>
    {:else if drawn === null}
      <p class="tf-empty tf-dim" role="status">Running {steps.map((s) => stepName(s, commandSteps)).join(" → ")}…</p>
    {:else}
      {#if notTaken !== null}<div class="tf-banner tf-info" role="status">{notTaken} does not take {drawn.path}: it is drawn as the Code page would draw it.</div>{/if}
      {#if bytesRead !== null && drawAs === "pdf"}
        <TfPdf read={bytesRead} size={drawn.size} {render} />
      {:else if drawn.binary && bytesRead !== null && drawAs === "image"}
        <TfBytesImage read={bytesRead} path={drawn.path} size={drawn.size} />
      {:else if drawAs === "jar"}
        <div class="tf-empty"><p>A JAR is drawn from a repository’s file on the Code page; here it is not drawn.</p></div>
      {:else if drawn.binary}
        <div class="tf-empty"><p>A binary file, not drawn.</p></div>
      {:else if image !== null}
        <TfBytesImage read={image.read} path={drawn.path} size={image.bytes.length} />
      {:else if drawAs === "openapi"}
        {#await explorerModule() then { default: TfOpenApi }}
          <TfOpenApi text={drawn.text} path={drawn.path} />
        {/await}
      {:else if drawAs === "xsd"}
        {#await xsdModule() then { default: TfXsd }}
          <TfXsd text={drawn.text} path={drawn.path} read={() => Promise.resolve(null)} paths={() => Promise.resolve([drawn.path])} {render} />
        {/await}
      {:else if drawAs === "xml" || drawAs === "json"}
        {#await docViewModule() then { default: TfDocView }}
          <TfDocView text={drawn.text} path={drawn.path} kind={drawAs === "json" ? "json" : "xml"} {render} />
        {/await}
      {:else if drawAs === "markdown"}
        <div class="tf-rendered"><TfMarkdown text={drawn.text} {render} /></div>
      {:else if drawAs === "html"}
        <TfHtmlPage html={drawn.text} title="{drawn.path}, drawn as a page" frame={(doc) => ports.frame(doc)} room=".tf-try-view" />
      {:else if drawAs === "mermaid" || drawAs === "d2" || drawAs === "graphviz"}
        <TfDiagram engine={drawAs} code={drawn.text} {render} />
      {:else if shown !== null && drawAs === shown.id && commandIdOf(shown) !== null}
        <div class="tf-rendered"><TfCommandOutput renderer={shown} text={drawn.text} commands={ports.commands} /></div>
      {:else}
        <TfLines text={drawn.text} path={drawn.path} {render} />
      {/if}
    {/if}
  </div>
</div>
