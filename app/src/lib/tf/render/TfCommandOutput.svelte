<!--
  A command renderer's run drawn (#2262, spec #2234 step 5, ADR-0033): the
  file viewer, a Markdown fence and Settings › Renderers' test pane draw it.

  The text goes to the backend (`run_renderer`, by the renderer's saved id);
  until it answers, the source shows plain under *Running*. SVG and HTML are
  sanitized and drawn in a frame with `sandbox=""` (`render/command.ts`): no
  script runs in it and its origin is opaque, so nothing in it reaches the
  app. In the app, HTML is served from the backend's `knobas-render` scheme
  under its own policy, so its styles apply (#2304, `render/frame.ts`). An image is the backend's data URL decoded and drawn from a blob,
  an SVG sanitized (`render/image.ts`). A failure,
  a timeout or too much output draws what the command said above the source,
  as the mockup's broken renderer does.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { RenderRun } from "../../ipc/app";
  import { commandIdOf, contentFor, failureLine, frameSource, ipcCommands, tauriBridge, type CommandPort, type FrameBridge } from "../../render/command";
  import { dataUrlImage } from "../../render/image";
  import type { Renderer } from "../../render/registry";
  import TfBytesImage from "./TfBytesImage.svelte";

  let {
    renderer,
    text,
    commands = ipcCommands,
    bridge = tauriBridge,
  }: {
    renderer: Renderer;
    text: string;
    /** Runs the renderer: the IPC, a fake in tests. */
    commands?: CommandPort;
    /** Where the frame's document is served from (#2304): the app's bridge, a stand-in in tests. */
    bridge?: FrameBridge;
  } = $props();

  let run = $state<RenderRun | null>(null);
  let refused = $state<string | null>(null);

  $effect(() => {
    const id = commandIdOf(renderer);
    const content = contentFor(renderer, text);
    run = null;
    refused = null;
    if (id === null) return;
    let live = true;
    commands
      .run(id, content)
      .then((answer) => {
        if (live) run = answer;
      })
      .catch((error: unknown) => {
        if (live) refused = ipcErrorMessage(error);
      });
    return () => {
      live = false;
    };
  });

  const failed = $derived(refused ?? (run === null ? null : failureLine(run)));
  const stderr = $derived(run !== null && (run.outcome === "failed" || run.outcome === "timed_out") ? run.stderr : "");
  const lines = $derived(text.split("\n"));
  const image = $derived(run !== null && run.outcome === "drawn" && run.output === "image" ? dataUrlImage(run.body) : null);
  const imageBytes = $derived(image === null ? null : () => Promise.resolve(image.bytes));
</script>

{#if run !== null && run.outcome === "drawn"}
  {#if run.output === "image"}
    {#if image !== null && imageBytes !== null}
      <TfBytesImage read={imageBytes} path={renderer.name} size={image.bytes.length} type={image.type} />
    {/if}
  {:else}
    {@const frame = frameSource(run.output, run.body, bridge)}
    <iframe class="tf-rframe" sandbox="" title={renderer.name} src={"src" in frame ? frame.src : undefined} srcdoc={"srcdoc" in frame ? frame.srcdoc : undefined}></iframe>
  {/if}
{:else}
  {#if failed !== null}
    <div class="tf-stepfail" role="alert">
      <b>{renderer.name} could not draw this.</b>
      <span>{failed}</span>
      {#if stderr !== ""}<pre>{stderr}</pre>{/if}
    </div>
  {:else}
    <div class="tf-banner tf-info" aria-live="polite">Running {renderer.name}…</div>
  {/if}
  <pre class="tf-code">{#each lines as line, i (i)}<div class="tf-l"><span>{i + 1}</span><span>{line || " "}</span></div>{/each}</pre>
{/if}
