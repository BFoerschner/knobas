<!--
  The ticket page's Attachments section (#2404, #2405): the mirror's
  attachment list (`EntityDetail.attachments`), folded to one line by a
  native disclosure, closed by default. Its summary is the section heading, a
  count badge and a chevron that points right closed and down open. Open, one
  row a file: a 56×40 preview for an image, else its kind in mono; its name,
  who attached it, when and how big; *Open*, which hands the file's address to
  the browser (which has the source's session), and *Download*, which saves it
  to the Downloads folder through the stored credential. The caller draws
  nothing when there are no attachments.

  Opening the panel asks for its images' previews (`previews.fetchImages`),
  never the page's opening. A preview that cannot be fetched draws a dashed
  placeholder, and the panel leads with the stale strip and *Try again*;
  *Download* waits while it shows, *Open* does not. An image over the 5 MiB
  cap draws its kind, and keeps both buttons.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { downloadAttachment as realDownload, type Attachment } from "../../ipc/entity";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";
  import { savedText } from "../builds/run-reads";
  import { attachmentKind, attachmentLine } from "./attachments";
  import { isImage, type Previews, type PreviewsFailure } from "./attachment-previews.svelte";

  let {
    previews,
    openExternal = realOpenExternal,
    download = realDownload,
  }: {
    /** The page's previews, which hold the list. */
    previews: Previews;
    /** Hands an address to the browser; the shell's opener, which refuses anything but http and https. */
    openExternal?: (url: string) => Promise<void>;
    /** Saves one attachment into the Downloads folder and answers its path. */
    download?: (entityId: string, attachmentId: string) => Promise<string>;
  } = $props();

  const files = $derived(previews.files);
  const failure = $derived(previews.failure);

  /** The stale strip's words: why previews and downloads wait. */
  function stale(failed: PreviewsFailure): string {
    const why =
      failed.reason === "forbidden"
        ? `${failed.source} answers not permitted`
        : failed.reason === "refused"
          ? `${failed.source} refuses the stored credential`
          : `${failed.source} cannot be reached`;
    return `${why}, so previews and downloads wait. The names and links are the mirror's.`;
  }

  function open(event: MouseEvent, url: string): void {
    event.preventDefault();
    openExternal(url).catch((error: unknown) => push({ text: error instanceof Error ? error.message : String(error) }));
  }

  async function save(file: Attachment): Promise<void> {
    try {
      push({ text: savedText(await download(previews.entityId, file.id)) });
    } catch (error: unknown) {
      push({ text: `Could not download ${file.name}: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

<section aria-labelledby="tf-tkatt">
  <details class="tf-tkfold" ontoggle={(event) => event.currentTarget.open && previews.fetchImages()}>
    <summary>
      <h2 id="tf-tkatt">Attachments</h2>
      <span class="tf-count">{files.length}</span>
      <svg class="tf-tkmark" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.5 3.5" /></svg>
    </summary>
    <div class="tf-panel tf-tkatt">
      {#if failure !== null}
        <div class="tf-stale">
          <span>{stale(failure)}</span>
          <button type="button" class="tf-btn tf-sm" onclick={() => previews.retry()}>Try again</button>
        </div>
      {/if}
      <div class="tf-rows">
        {#each files as file (file.id)}
          {@const preview = isImage(file) ? previews.of(file.id) : null}
          <div class="tf-r">
            {#if preview?.state === "ready"}
              <img class="tf-tkthumb" src={preview.src} alt="" />
            {:else if preview?.state === "failed"}
              <span class="tf-tkthumb tf-tknone" title={preview.message} aria-hidden="true">–</span>
            {:else}
              <span class="tf-tkkind" aria-hidden="true">{attachmentKind(file)}</span>
            {/if}
            <span class="tf-t">{file.name}<span class="tf-rsub">{attachmentLine(file)}</span></span>
            <span class="tf-rend">
              <a class="tf-btn tf-sm tf-ghost" href={file.content_url} title="Open {file.name} in the browser" onclick={(event) => open(event, file.content_url)}>Open</a>
              <button
                type="button"
                class="tf-btn tf-sm tf-ghost"
                disabled={failure !== null}
                title={failure !== null ? `Waits until ${failure.source} can be read` : `Save ${file.name} to Downloads`}
                onclick={() => void save(file)}>Download</button
              >
            </span>
          </div>
        {/each}
      </div>
    </div>
  </details>
</section>
