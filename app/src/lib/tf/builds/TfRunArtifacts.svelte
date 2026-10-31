<!--
  The run page's Artifacts tab (#856, R-9.11 Artifacts, D8): the run's
  artifacts as a tree (`run_artifacts`, one folder per read).

  - A folder folds, says how many files it holds and offers *Download
    folder* (a zip). The top-level folders start open; a deeper one is read
    when it is opened.
  - A file shows its size, *Preview* for a text file (`artifact_preview`,
    drawn under the tree) and *Download*.
  - *Download all as zip*. Every download streams through knobas with the
    stored credential into the downloads folder (`download_run_file`).
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { ArtifactEntry, ArtifactPreview, DownloadTarget } from "../../ipc/entity";
  import type { RunPorts } from "./pipeline-ports";
  import { downloadAndSay, filesText, previewable, sizeText } from "./run-reads";

  let {
    runId,
    sourceName,
    ports,
    rereads = 0,
  }: {
    runId: string;
    /** The source's configured name, for whose sign-in a download uses. */
    sourceName: string;
    ports: Pick<RunPorts, "artifacts" | "preview" | "download">;
    /** Bumped when the page reads the run again (#1450): the folders read are read again, open ones kept open. */
    rereads?: number;
  } = $props();

  /** How many top-level folders start open. */
  const OPEN_TOP = 10;
  /** The deepest indent class (`tf-ad6`). */
  const DEPTH_MAX = 6;

  type Folder = ArtifactEntry[] | "reading" | { failed: string };
  let folders = $state<Record<string, Folder>>({});
  let open = $state<Record<string, boolean>>({});
  let preview = $state<{ path: string; answer: ArtifactPreview | "reading" | { failed: string } } | null>(null);
  let token = 0;

  function readFolder(path: string) {
    if (untrack(() => folders[path]) !== undefined) return;
    folders[path] = "reading";
    const mine = token;
    ports.artifacts(runId, path).then(
      (entries) => {
        if (mine !== token) return;
        folders[path] = entries ?? [];
        if (path === "") {
          for (const entry of (entries ?? []).filter((e) => e.folder).slice(0, OPEN_TOP)) toggle(entry.path, true);
        }
      },
      (cause: unknown) => {
        if (mine === token) folders[path] = { failed: ipcErrorMessage(cause) };
      },
    );
  }

  $effect(() => {
    void runId;
    ++token;
    untrack(() => {
      folders = {};
      open = {};
      preview = null;
      readFolder("");
    });
  });

  /** A read again of every folder read so far; a failed one keeps what it had, and only the latest one lands. */
  let first = true;
  let again = 0;
  $effect(() => {
    void rereads;
    if (first) return void (first = false);
    untrack(() => {
      const mine = token;
      const seq = ++again;
      for (const path of Object.keys(folders).filter((p) => Array.isArray(folders[p]))) {
        ports.artifacts(runId, path).then(
          (entries) => {
            if (mine === token && seq === again) folders[path] = entries ?? [];
          },
          () => {},
        );
      }
    });
  });

  function toggle(path: string, to = !(open[path] ?? false)) {
    open[path] = to;
    if (to) readFolder(path);
  }

  const root = $derived(folders[""]);
  const any = $derived(Array.isArray(root) && root.length > 0);

  const download = (target: DownloadTarget, what: string) => downloadAndSay(ports.download, runId, target, what);

  function showPreview(path: string) {
    preview = { path, answer: "reading" };
    const mine = token;
    ports.preview(runId, path).then(
      (answer) => {
        if (mine === token && preview?.path === path) preview = { path, answer: answer ?? { text: null, truncated: false } };
      },
      (cause: unknown) => {
        if (mine === token && preview?.path === path) preview = { path, answer: { failed: ipcErrorMessage(cause) } };
      },
    );
  }
</script>

{#snippet tree(path: string, depth: number)}
  {@const folder = folders[path]}
  {#if folder === "reading"}
    <div class={`tf-arow tf-ad${Math.min(depth, DEPTH_MAX)}`}><span class="tf-nm tf-dim">…</span></div>
  {:else if folder !== undefined && !Array.isArray(folder)}
    <div class={`tf-arow tf-ad${Math.min(depth, DEPTH_MAX)}`}><span class="tf-nm tf-dim">Could not be read: {folder.failed}</span></div>
  {:else if folder !== undefined}
    {#each folder as entry (entry.path)}
      {@const indent = `tf-ad${Math.min(depth, DEPTH_MAX)}`}
      {#if entry.folder}
        {@const isOpen = open[entry.path] ?? false}
        <div class={`tf-arow tf-dir ${indent}`}>
          <button type="button" class="tf-fold" aria-expanded={isOpen} onclick={() => toggle(entry.path)}
            >{isOpen ? "▾" : "▸"} {entry.name}/</button
          >
          <span class="tf-sz">{filesText(entry) ?? ""}</span>
          <span class="tf-acts"
            ><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => download({ kind: "folder", path: entry.path }, `${entry.name}/`)}
              >Download folder</button
            ></span
          >
        </div>
        {#if isOpen}{@render tree(entry.path, depth + 1)}{/if}
      {:else}
        <div class={`tf-arow ${indent}`}>
          <span class="tf-nm">{entry.name}</span>
          <span class="tf-sz">{entry.size === null ? "" : sizeText(entry.size)}</span>
          <span class="tf-acts">
            {#if previewable(entry)}
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => showPreview(entry.path)}>Preview</button>
            {/if}
            <button type="button" class="tf-btn tf-sm" onclick={() => download({ kind: "file", path: entry.path }, entry.name)}>Download</button>
          </span>
        </div>
      {/if}
    {/each}
  {/if}
{/snippet}

{#if root === undefined || root === "reading"}
  <p class="tf-dim">Reading the artifacts…</p>
{:else if !Array.isArray(root)}
  <p class="tf-dim">The artifacts could not be read: {root.failed}</p>
{:else}
  <div class="tf-atree">
    {#if any}{@render tree("", 0)}{:else}<div class="tf-empty"><p>This run has published no artifacts yet.</p></div>{/if}
  </div>
  {#if any}
    <div class="tf-aall">
      <button type="button" class="tf-btn tf-sm" onclick={() => download({ kind: "all" }, "the artifacts")}>Download all as zip</button>
      <span class="tf-dim">Downloads go through knobas with the {sourceName} sign-in it already has, into ~/Downloads.</span>
    </div>
  {/if}
{/if}

{#if preview !== null}
  {@const answer = preview.answer}
  {@const name = preview.path.split("/").at(-1) ?? preview.path}
  <div class="tf-apreview">
    <div class="tf-fileview">
      <header>
        <span class="tf-p">{preview.path}</span>
        <span class="tf-trail">
          <button type="button" class="tf-btn tf-sm" onclick={() => preview && download({ kind: "file", path: preview.path }, name)}>Download</button>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (preview = null)}>Close</button>
        </span>
      </header>
      {#if answer === "reading"}
        <p class="tf-fpad tf-dim">Reading {name}…</p>
      {:else if "failed" in answer}
        <p class="tf-fpad tf-dim">{name} could not be read: {answer.failed}</p>
      {:else if answer.text === null}
        <p class="tf-fpad tf-dim">{name} is not text. Download it to open it.</p>
      {:else}
        <pre class="tf-code">{#each answer.text.split("\n") as text, i (i)}<div class="tf-l"><span>{i + 1}</span><span>{text || " "}</span></div>{/each}</pre>
        {#if answer.truncated}<p class="tf-fpad tf-dim">The first 256 KB. Download it for the rest.</p>{/if}
      {/if}
    </div>
  </div>
{/if}
