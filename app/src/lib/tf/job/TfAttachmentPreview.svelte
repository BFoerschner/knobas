<!--
  The attachment preview (#2415): one attachment of the ticket page, opened
  from its Attachments row, a `[^name]` link or an inline image. The page's
  `TfDialog`, a header of the file's name, its line, *Download* and ×; the
  body by kind:

  - an image, whole, scaled to fit;
  - a PDF (`isPdf`), in the file viewer's PDF component over the same bytes;
  - anything else, its kind and *No preview*, with *Download*;
  - a file over the 5 MiB cap, or a read that failed, says why in the words
    the inline images' placeholders use, with *Try again* for a failed read.

  The bytes are the page's previews (`attachment-previews.svelte.ts`): asked
  once, kept while the page is open, never in the mirror. *Download* waits
  while the panel's stale strip shows, as the row's did.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { downloadAttachment as realDownload, type Attachment } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { savedText } from "../builds/run-reads";
  import type { Layers } from "../layers.svelte";
  import TfPdf from "../render/TfPdf.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { missingWords } from "./attachment-image";
  import { dataBytes, isImage, isPdf, type Previews } from "./attachment-previews.svelte";
  import { attachmentKind, attachmentLine } from "./attachments";

  let {
    file,
    previews,
    source,
    layers,
    download = realDownload,
    onclose,
  }: {
    file: Attachment;
    /** The page's previews, which hold the bytes. */
    previews: Previews;
    /** The source's configured name, for why a read failed. */
    source: string;
    layers: Layers;
    /** Saves one attachment into the Downloads folder and answers its path. */
    download?: (entityId: string, attachmentId: string) => Promise<string>;
    onclose: () => void;
  } = $props();

  const shows = $derived(isImage(file) ? "image" : isPdf(file) ? "pdf" : "none");
  const preview = $derived(previews.of(file.id));
  const failure = $derived(previews.failure);
  const pdfBytes = $derived(shows === "pdf" && preview.state === "ready" ? dataBytes(preview.src) : null);
  const readPdf = $derived.by(() => {
    const bytes = pdfBytes;
    return () => (bytes === null ? Promise.reject(new Error("not read")) : Promise.resolve(bytes));
  });

  $effect(() => {
    if (shows !== "none") previews.fetch(file.id);
  });

  async function save(): Promise<void> {
    try {
      push({ text: savedText(await download(previews.entityId, file.id)) });
    } catch (error: unknown) {
      push({ text: `Could not download ${file.name}: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#snippet downloadButton(className: string)}
  <button
    type="button"
    class={className}
    disabled={failure !== null}
    title={failure !== null ? `Waits until ${failure.source} can be read` : `Save ${file.name} to Downloads`}
    onclick={() => void save()}>Download</button
  >
{/snippet}

<TfDialog title={file.name} subtitle={attachmentLine(file)} {onclose} {layers} className="tf-attdlg" closeButton>
  {#snippet actions()}{@render downloadButton("tf-btn")}{/snippet}
  {#snippet body()}
    <div class="tf-attview">
      {#if shows === "none"}
        <span class="tf-tkkind">{attachmentKind(file)}</span>
        <p>No preview for this kind of file.</p>
        {@render downloadButton("tf-btn tf-sm")}
      {:else if preview.state === "ready" && shows === "image"}
        <img src={preview.src} alt={file.name} />
      {:else if pdfBytes !== null}
        <TfPdf read={readPdf} size={pdfBytes.length} />
      {:else if preview.state === "too-large" || preview.state === "failed"}
        <p>{missingWords(file.name, source, preview)}</p>
        {#if preview.state === "failed"}
          <button type="button" class="tf-btn tf-sm" onclick={() => previews.retry()}>Try again</button>
        {/if}
      {:else}
        <p class="tf-dim">Reading {file.name}…</p>
      {/if}
    </div>
  {/snippet}
</TfDialog>
