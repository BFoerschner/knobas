<!--
  *Files N* (ADR-0060, section 17): what a command wrote to `{outdir}`, on the left a row a file with its kind, its
  path in the code face and its size, and on the right the picked file drawn as a result is. The first is picked on
  arrival; `↑` and `↓` step. What was left out is said under the picked file.
-->
<script lang="ts">
  import type { OneOffResult, OneOffResultKind } from "../../ipc/oneoff";
  import { size, sniff } from "../../render/oneoff";
  import TfRunResult from "./TfRunResult.svelte";

  let {
    result,
    bytes,
    picked = $bindable(0),
  }: {
    result: Extract<OneOffResult, { from: "files" }>;
    bytes: (file: string) => Promise<Uint8Array>;
    picked?: number;
  } = $props();

  /** The most rows drawn: a folder of more says how many. */
  const SHOWN = 300;
  const file = $derived(result.files[picked] ?? null);
  let kind = $state<OneOffResultKind | null>(null);
  let held = $state<Uint8Array | null>(null);
  $effect(() => {
    const f = file;
    kind = null;
    held = null;
    if (f === null) return;
    let live = true;
    bytes(f.path).then(
      (b) => {
        if (!live) return;
        held = b;
        kind = sniff(b);
      },
      () => live && (kind = { kind: "bytes" }),
    );
    return () => (live = false);
  });
  const label = (path: string) => (path.split(".").pop() ?? "").toUpperCase().slice(0, 4) || "FILE";

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      picked = Math.min(result.files.length - 1, picked + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      picked = Math.max(0, picked - 1);
    }
  }
</script>

<div class="tf-oo-files">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex: the list takes focus so the arrows step through it -->
  <div class="tf-oo-flist" tabindex="0" role="listbox" aria-label="The files it wrote" {onkeydown}>
    {#each result.files.slice(0, SHOWN) as f, i (f.path)}
      <button type="button" role="option" aria-selected={i === picked} aria-current={i === picked} onclick={() => (picked = i)}
        ><span class="tf-lab">{label(f.path)}</span><span class="tf-oo-fname" title={f.path}>{f.path}</span><span class="tf-oo-fsize">{size(f.size)}</span></button
      >
    {/each}
    {#if result.files.length > SHOWN}<p class="tf-oo-fnote">{result.files.length - SHOWN} more, in Download.</p>{/if}
  </div>
  <div class="tf-oo-fone">
    {#if file !== null}
      <div class="tf-oo-fhead"><b>{file.path}</b><span class="tf-dim">{size(file.size)}</span></div>
      <div class="tf-oo-fbody">
        {#if kind !== null && held !== null}
          {#key file.path}
            <TfRunResult {kind} bytes={() => Promise.resolve(held!)} name={file.path} bytesSize={file.size} room=".tf-oo-fbody" />
          {/key}
        {/if}
      </div>
      {#if result.left_out_total > 0}
        <div class="tf-oo-fnote">
          Left out: {result.left_out.map((l) => `${l.path} (${l.why === "link" ? "a link" : l.why === "git" ? "git's own" : l.why === "name" ? "a name Windows does not take" : l.why})`).join(", ")}{result.left_out_total > result.left_out.length ? `, and ${result.left_out_total - result.left_out.length} more` : ""}. Links, .git folders and names Windows does not take are never kept.
        </div>
      {/if}
    {/if}
  </div>
</div>
