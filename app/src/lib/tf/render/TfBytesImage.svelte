<!--
  An image's bytes drawn: a binary image in the file viewer (#2260, its bytes
  from the byte read, #2256), a text SVG drawn as an image, a command
  renderer's image (#2262). Drawn as an `<img>` from a blob (an SVG sanitized first,
  `render/image.ts`). The blob URL is let go when the file changes or the
  viewer closes. Past the byte cap nothing is read.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { imageType, typedImageBlob } from "../../render/image";
  import { BYTES_CAP } from "../../render/registry";

  let {
    read,
    path,
    size,
    type = imageType(path),
  }: {
    read: () => Promise<Uint8Array>;
    path: string;
    size: number;
    /** The image's media type; by default what `path`'s extension says. */
    type?: string;
  } = $props();

  let src = $state<string | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    const reading = read;
    const kind = type;
    let live = true;
    let made: string | null = null;
    src = null;
    const large = size > BYTES_CAP;
    failed = large ? `It is ${(size / 1024 / 1024).toFixed(1)} MiB; knobas reads a file's bytes up to ${BYTES_CAP / 1024 / 1024} MiB.` : null;
    if (large) return;
    reading().then(
      (bytes) => {
        if (!live) return;
        made = URL.createObjectURL(typedImageBlob(bytes, kind));
        src = made;
      },
      (error: unknown) => live && (failed = ipcErrorMessage(error)),
    );
    return () => {
      live = false;
      if (made !== null) URL.revokeObjectURL(made);
    };
  });
</script>

{#if failed !== null}
  <div class="tf-fimage"><div class="tf-stepfail" role="alert"><b>Could not draw this image.</b><pre>{failed}</pre></div></div>
{:else if src !== null}
  <div class="tf-fimage"><img {src} alt={path} /></div>
{:else}
  <div class="tf-empty"><p>Reading the image…</p></div>
{/if}
