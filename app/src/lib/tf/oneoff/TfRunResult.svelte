<!--
  A run's result that is not lines (ADR-0060, section 16), drawn as the file viewer draws one: a page sanitized in a
  frame that runs no script and asks for nothing (`TfHtmlPage`), an image whole from a blob, a PDF with scripting off
  (`TfPdf`), JSON, code, a diff or text as numbered lines with the app's highlighter (`TfLines`), and bytes as a hex
  dump of their first 64 KiB. The bytes are read once from the backend; nothing of them is run.
-->
<script lang="ts">
  import type { OneOffResultKind } from "../../ipc/oneoff";
  import { folderRenderFrame } from "../../ipc/app";
  import { hexDump, size } from "../../render/oneoff";
  import { renderPort } from "../../render/port";
  import TfHtmlPage from "../render/TfHtmlPage.svelte";
  import TfPdf from "../render/TfPdf.svelte";
  import TfLines from "../code/TfLines.svelte";

  let {
    kind,
    bytes,
    name,
    bytesSize,
    room = ".tf-oo-stage",
    frame = (doc: string) => folderRenderFrame(doc),
  }: {
    kind: OneOffResultKind;
    /** Reads the bytes. */
    bytes: () => Promise<Uint8Array>;
    name: string;
    bytesSize: number;
    room?: string;
    frame?: (document: string) => Promise<string>;
  } = $props();

  let read = $state<Uint8Array | null>(null);
  let failed = $state<string | null>(null);
  $effect(() => {
    let live = true;
    read = null;
    failed = null;
    if (kind.kind === "pdf") return;
    bytes().then(
      (b) => live && (read = b),
      (e: unknown) => live && (failed = (e as { message?: string } | null)?.message ?? String(e)),
    );
    return () => (live = false);
  });

  const text = $derived(read === null ? "" : new TextDecoder().decode(read));
  let url = $state<string | null>(null);
  $effect(() => {
    if (kind.kind !== "image" || read === null) return;
    const made = URL.createObjectURL(new Blob([read as BlobPart], { type: kind.mime }));
    url = made;
    return () => URL.revokeObjectURL(made);
  });
  const path = $derived(
    kind.kind === "json" ? "out.json" : kind.kind === "code" ? `out.${kind.ext}` : kind.kind === "diff" ? "out.diff" : kind.kind === "markdown" ? "out.md" : "out.txt",
  );
  const render = renderPort();
</script>

{#if failed !== null}
  <div class="tf-oo-quiet tf-err"><p><b>Its result could not be read.</b></p><p>{failed}</p></div>
{:else if kind.kind === "pdf"}
  <div class="tf-oo-pdf"><TfPdf read={bytes} size={bytesSize} /></div>
{:else if read === null}
  <div class="tf-oo-quiet"><p>Reading {name}, {size(bytesSize)}…</p></div>
{:else if kind.kind === "image"}
  <div class="tf-oo-image">{#if url !== null}<img src={url} alt="{name}, what the command made" />{/if}</div>
{:else if kind.kind === "page"}
  <div class="tf-oo-page"><TfHtmlPage html={text} title="{name}, drawn without its scripts" {frame} {room} /></div>
{:else if kind.kind === "bytes"}
  <div class="tf-oo-lines"><TfLines text={hexDump(read)} path="out.txt" {render} find="overlay" /></div>
{:else}
  <div class="tf-oo-lines"><TfLines {text} {path} {render} find="overlay" /></div>
{/if}
