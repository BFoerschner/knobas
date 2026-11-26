<!--
  *Try it* on a folder of files renderer (ADR-0046): what the run made, as a
  list under the form. The made line (how many files and bytes, how long, and
  what was left out), then the generated tree with every folder open, in the
  tree's own row classes inside a box that scrolls, windowed past 120 rows. A
  press on a file opens it in the app's file viewer (an image, a PDF, text,
  else no preview), its bytes read by the token. No link leaves the page: an
  unsaved try would be taken back, and the address would name a renderer that
  is gone.
-->
<script lang="ts">
  import type { FolderDrawn, MadeLeftOutWhy, MadeTree } from "../../ipc/app";
  import { ipcErrorMessage } from "../../ipc";
  import { sizeLabel, tookLabel } from "../../render/folder";
  import { ipcMadePorts, type MadePorts } from "../../render/made-ports";
  import { Windowed } from "../../shell/windowed.svelte";
  import { thousands } from "../counted";
  import TfFileViewer, { type ViewerContent } from "./TfFileViewer.svelte";

  let { drawn, ports = ipcMadePorts }: { drawn: FolderDrawn; ports?: MadePorts } = $props();

  let tree = $state.raw<MadeTree | null>(null);
  let failed = $state<string | null>(null);
  $effect(() => {
    const token = drawn.token;
    let live = true;
    tree = null;
    failed = null;
    ports.tree(token).then(
      (read) => {
        if (live) tree = read;
      },
      (cause: unknown) => {
        if (live) failed = ipcErrorMessage(cause);
      },
    );
    return () => {
      live = false;
    };
  });

  /** Every folder open: a folder's row before its files, each indented by its depth. */
  const rows = $derived.by(() => {
    if (tree === null) return [];
    const out: { path: string; name: string; dir: boolean; depth: number }[] = [];
    const seen = new Set<string>();
    for (const file of tree.files) {
      const parts = file.path.split("/");
      for (let i = 1; i < parts.length; i++) {
        const dir = parts.slice(0, i).join("/");
        if (seen.has(dir)) continue;
        seen.add(dir);
        out.push({ path: dir, name: parts[i - 1]!, dir: true, depth: i - 1 });
      }
      out.push({ path: file.path, name: parts.at(-1)!, dir: false, depth: parts.length - 1 });
    }
    return out;
  });
  const win = new Windowed<string>({ count: () => rows.length, key: (i) => rows[i]!.path, estimate: 24, from: 121 });

  let leftOpen = $state(false);
  const WHY: Record<MadeLeftOutWhy, string> = {
    link: "a link, not followed",
    special: "not a file",
    git: "a .git folder",
    name: "a name Windows does not take",
    not_utf8: "a name that is not text",
    case_twin: "the same name as another but for case",
    too_deep: "too deep",
  };

  /** The file the viewer shows, and what it has of it. */
  let viewing = $state<{ path: string; content: ViewerContent } | null>(null);
  const ext = (path: string) => path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  const IMAGES: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", svg: "image/svg+xml" };
  let made: string | null = null;
  function view(path: string, size: number) {
    if (made !== null) URL.revokeObjectURL(made);
    made = null;
    viewing = { path, content: { kind: "reading" } };
    const read = () => ports.fileBytes(drawn.token, path);
    if (ext(path) === "pdf") {
      viewing = { path, content: { kind: "pdf", read, size } };
      return;
    }
    read().then(
      (bytes) => {
        if (viewing?.path !== path) return;
        const mime = IMAGES[ext(path)];
        if (mime !== undefined) {
          made = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
          viewing = { path, content: { kind: "image", src: made } };
        } else if (bytes.slice(0, 8000).includes(0)) viewing = { path, content: { kind: "none" } };
        else viewing = { path, content: { kind: "text", text: new TextDecoder().decode(bytes.slice(0, 1_000_000)), truncated: bytes.length > 1_000_000 } };
      },
      (cause: unknown) => {
        if (viewing?.path === path) viewing = { path, content: { kind: "missing", why: ipcErrorMessage(cause), retry: () => view(path, size) } };
      },
    );
  }
  $effect(() => () => {
    if (made !== null) URL.revokeObjectURL(made);
  });
  const sizeOf = (path: string) => tree?.files.find((f) => f.path === path)?.size ?? 0;
</script>

<div class="tf-madelist">
  <p class="tf-madelist-line" role="status">
    {#if drawn.kind.kind === "folder"}
      Made {thousands(drawn.kind.files)} {drawn.kind.files === 1 ? "file" : "files"}, {sizeLabel(drawn.kind.bytes)}, in {tookLabel(drawn.took_ms)}.
      {#if drawn.kind.left_out > 0}<button type="button" class="tf-linkish" aria-expanded={leftOpen} onclick={() => (leftOpen = !leftOpen)}
          >{thousands(drawn.kind.left_out)} left out</button
        >{/if}
    {/if}
  </p>
  {#if leftOpen && tree !== null}
    <ul class="tf-madebar-left">
      {#each tree.left_out as one, i (i)}<li><span class="tf-mono" title={one.path}>{one.path}</span><span>{WHY[one.why]}</span></li>{/each}
    </ul>
    {#if tree.left_out_total > tree.left_out.length}<p class="tf-small">and {thousands(tree.left_out_total - tree.left_out.length)} more</p>{/if}
  {/if}
  {#if failed !== null}
    <div class="tf-stepfail" role="alert"><b>Could not read what it made.</b><span>{failed}</span></div>
  {:else}
    <nav class="tf-ftree tf-madelist-tree" aria-label="What it made" aria-busy={tree === null}>
      <div class="tf-ftrows" {@attach win.list}>
        {#each win.slice(rows) as row (row.path)}
          {#if row.dir}
            <span class="tf-fsub tf-fdirrow tf-d{Math.min(row.depth, 8)}" {@attach win.row(row.path)}
              ><svg class="tf-ficon" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4.5h4l1.5 1.5h5.5v1.5M2.5 4.5v8h10l1.5-5h-10l-1.5 5" /></svg>{row.name}</span
            >
          {:else}
            <button type="button" class="tf-fdir tf-madelist-file tf-d{Math.min(row.depth, 8)}" onclick={() => view(row.path, sizeOf(row.path))} {@attach win.row(row.path)}
              ><svg class="tf-ficon" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2h5l3 3v9H4z M9 2v3h3" /></svg>{row.name}</button
            >
          {/if}
        {/each}
      </div>
    </nav>
  {/if}
  <p class="tf-stg-p">Add it, then open the folder on the Code page to browse these as knobas draws them.</p>
</div>

{#if viewing !== null}
  <TfFileViewer
    name={viewing.path.slice(viewing.path.lastIndexOf("/") + 1)}
    line={viewing.path}
    kind={ext(viewing.path).toUpperCase()}
    content={viewing.content}
    download={() => {}}
    downloadWaits="Kept in the generated folder: open it on the Code page once the renderer is added"
    onclose={() => (viewing = null)}
  />
{/if}
