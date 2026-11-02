<!--
  The app's file viewer (#2415): one file in a dialog, whatever opened it. A
  ticket's attachment (`TfAttachmentPreview`) and a run's artifact
  (`TfRunArtifacts`) both open here.

  The title line is the file's name and its line (who, when, how big), and on
  the right one cluster of controls centred on that block: *Download* (glyph
  and word, the default size), a rule, and the Close. The body is the stage, a
  recessed surface the file sits on:

  - an image, whole, scaled to fit;
  - a PDF, in the file viewer's PDF component (`TfPdf`);
  - text, numbered lines, with a note when only the head was read;
  - anything else: its kind and *No preview*, pointing at *Download*;
  - why the bytes are missing, with *Try again* where the caller can ask again.

  The caller reads the bytes and says what it has (`content`); this draws.
-->
<script lang="ts" module>
  /** What the viewer has to show for its file. */
  export type ViewerContent =
    | { kind: "reading" }
    | { kind: "image"; src: string }
    | { kind: "pdf"; read: () => Promise<Uint8Array>; size: number }
    | { kind: "text"; text: string; truncated: boolean }
    | { kind: "none" }
    | { kind: "missing"; why: string; retry: (() => void) | null };
</script>

<script lang="ts">
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfPdf from "./TfPdf.svelte";

  let {
    name,
    line = null,
    kind,
    content,
    download,
    downloadWaits = null,
    layers = sharedLayers,
    onclose,
  }: {
    /** The file's name: the dialog's title. */
    name: string;
    /** Who, when and how big, under the name; `null` where nothing is known. */
    line?: string | null;
    /** The file's kind as a short word (`PDF`, `CSV`), for a file with no preview. */
    kind: string;
    content: ViewerContent;
    /** Saves the file; the caller says how it went. */
    download: () => void;
    /** Why *Download* waits (its title), `null` while it can run. */
    downloadWaits?: string | null;
    layers?: Layers;
    onclose: () => void;
  } = $props();
</script>

{#snippet downloadButton()}
  <button
    type="button"
    class="tf-btn"
    disabled={downloadWaits !== null}
    title={downloadWaits ?? `Save ${name} to Downloads`}
    onclick={download}
  >
    <svg class="tf-vglyph" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.5v7.5M4.75 6.75 8 10l3.25-3.25M3 13.5h10" /></svg>
    Download
  </button>
{/snippet}

<TfDialog title={name} subtitle={line} {onclose} {layers} className="tf-viewer" closeButton>
  {#snippet actions()}{@render downloadButton()}{/snippet}
  {#snippet body()}
    <div class="tf-vstage" data-shows={content.kind}>
      {#if content.kind === "image"}
        <img src={content.src} alt={name} />
      {:else if content.kind === "pdf"}
        <TfPdf read={content.read} size={content.size} />
      {:else if content.kind === "text"}
        <pre class="tf-code">{#each content.text.replace(/\r?\n$/, "").split("\n") as text, i (i)}<div class="tf-l"><span>{i + 1}</span><span>{text || " "}</span></div>{/each}</pre>
        {#if content.truncated}<p class="tf-vnote">The first 256 KB. Download it for the rest.</p>{/if}
      {:else if content.kind === "none"}
        <div class="tf-vempty">
          <span class="tf-tkkind" aria-hidden="true">{kind}</span>
          <p>No preview for this kind of file. Download it to open it.</p>
        </div>
      {:else if content.kind === "missing"}
        <div class="tf-vempty">
          <p>{content.why}</p>
          {#if content.retry !== null}
            <button type="button" class="tf-btn tf-sm" onclick={content.retry}>Try again</button>
          {/if}
        </div>
      {:else}
        <p class="tf-vempty tf-dim">Reading {name}…</p>
      {/if}
    </div>
  {/snippet}
</TfDialog>
