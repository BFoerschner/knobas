<!--
  The attachment preview (#2415): one attachment of the ticket page, opened
  from its Attachments row, a `[^name]` link or an inline image, in the app's
  file viewer (`TfFileViewer`). This says what the viewer shows:

  - an image, from the page's previews;
  - a PDF (`isPdf`), its bytes from the same previews;
  - text (`isText`): its lines, the first 256 KB of them;
  - anything else, *No preview*;
  - a file over the 5 MiB cap, or a read that failed, why, in the words the
    inline images' placeholders use, with *Try again* for a failed read.

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
  import TfFileViewer, { type ViewerContent } from "../render/TfFileViewer.svelte";
  import { missingWords } from "./attachment-image";
  import { dataBytes, isImage, isPdf, isText, type Previews } from "./attachment-previews.svelte";
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

  /** The most of a text file the viewer draws, as an artifact's preview does. */
  const TEXT_HEAD = 256 * 1024;

  const shows = $derived(isImage(file) ? "image" : isPdf(file) ? "pdf" : isText(file) ? "text" : "none");
  const preview = $derived(previews.of(file.id));
  const failure = $derived(previews.failure);

  const content = $derived.by((): ViewerContent => {
    if (shows === "none") return { kind: "none" };
    if (preview.state === "too-large" || preview.state === "failed") {
      return { kind: "missing", why: missingWords(file.name, source, preview), retry: preview.state === "failed" ? () => previews.retry() : null };
    }
    if (preview.state !== "ready") return { kind: "reading" };
    if (shows === "image") return { kind: "image", src: preview.src };
    const bytes = dataBytes(preview.src);
    if (shows === "text") {
      const head = bytes.subarray(0, TEXT_HEAD);
      return { kind: "text", text: new TextDecoder().decode(head, { stream: bytes.length > TEXT_HEAD }), truncated: bytes.length > TEXT_HEAD };
    }
    return { kind: "pdf", read: () => Promise.resolve(bytes), size: bytes.length };
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

<TfFileViewer
  name={file.name}
  line={attachmentLine(file) || null}
  kind={attachmentKind(file)}
  {content}
  download={() => void save()}
  downloadWaits={failure !== null ? `Waits until ${failure.source} can be read` : null}
  {layers}
  {onclose}
/>
