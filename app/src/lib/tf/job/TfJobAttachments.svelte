<!--
  The ticket page's Attachments section (#2404, #2405): the mirror's
  attachment list (`EntityDetail.attachments`), folded to one line by a
  native disclosure, closed by default. Its summary is the section heading, a
  count badge and a chevron that points right closed and down open. Open, one
  row a file, the whole row one button (#2415): a 56×40 preview for an image,
  else its kind in mono; its name, who attached it, when and how big. A click,
  Enter or Space opens the file's preview (`onopen`), which holds *Download*.
  The caller draws nothing when there are no attachments.

  Opening the panel asks for its images' previews (`previews.fetchImages`),
  never the page's opening. A preview that cannot be fetched draws a dashed
  placeholder, and the panel leads with the stale strip and *Try again*. An
  image over the 5 MiB cap draws its kind.
-->
<script lang="ts">
  import { attachmentKind, attachmentLine } from "./attachments";
  import { isImage, type Previews, type PreviewsFailure } from "./attachment-previews.svelte";

  let {
    previews,
    onopen,
  }: {
    /** The page's previews, which hold the list. */
    previews: Previews;
    /** Opens one file's preview. */
    onopen: (attachmentId: string) => void;
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
          <button type="button" class="tf-r" title="Preview {file.name}" onclick={() => onopen(file.id)}>
            {#if preview?.state === "ready"}
              <img class="tf-tkthumb" src={preview.src} alt="" />
            {:else if preview?.state === "failed"}
              <span class="tf-tkthumb tf-tknone" title={preview.message} aria-hidden="true">–</span>
            {:else}
              <span class="tf-tkkind" aria-hidden="true">{attachmentKind(file)}</span>
            {/if}
            <span class="tf-t">{file.name}<span class="tf-rsub">{attachmentLine(file)}</span></span>
          </button>
        {/each}
      </div>
    </div>
  </details>
</section>
