<!--
  The ticket page's Attachments section (#2404): the mirror's attachment list
  (`EntityDetail.attachments`), folded to one line by a native disclosure,
  closed by default. Its summary is the section heading, a count badge and a
  chevron that points right closed and down open. Open, one row a file: its
  kind in mono, its name, who attached it, when and how big, and *Open*,
  which hands the file's address to the browser (which has the source's
  session). The caller draws nothing when there are no attachments.

  Previews, *Download* and the unreachable state are #2405's; an image row
  shows its kind until then.
-->
<script lang="ts">
  import type { Attachment } from "../../ipc/entity";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";
  import { attachmentKind, attachmentLine } from "./attachments";

  let {
    files,
    openExternal = realOpenExternal,
  }: {
    files: readonly Attachment[];
    /** Hands an address to the browser; the shell's opener, which refuses anything but http and https. */
    openExternal?: (url: string) => Promise<void>;
  } = $props();

  function open(event: MouseEvent, url: string): void {
    event.preventDefault();
    openExternal(url).catch((error: unknown) => push({ text: error instanceof Error ? error.message : String(error) }));
  }
</script>

<section aria-labelledby="tf-tkatt">
  <details class="tf-tkfold">
    <summary>
      <h2 id="tf-tkatt">Attachments</h2>
      <span class="tf-count">{files.length}</span>
      <svg class="tf-tkmark" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.5 3.5" /></svg>
    </summary>
    <div class="tf-panel tf-tkatt">
      <div class="tf-rows">
        {#each files as file (file.id)}
          <div class="tf-r">
            <span class="tf-tkkind" aria-hidden="true">{attachmentKind(file)}</span>
            <span class="tf-t">{file.name}<span class="tf-rsub">{attachmentLine(file)}</span></span>
            <span class="tf-rend">
              <a class="tf-btn tf-sm tf-ghost" href={file.content_url} title="Open {file.name} in the browser" onclick={(event) => open(event, file.content_url)}>Open</a>
            </span>
          </div>
        {/each}
      </div>
    </div>
  </details>
</section>
