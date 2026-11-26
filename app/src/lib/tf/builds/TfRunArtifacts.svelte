<!--
  The run page's Artifacts tab (#856, R-9.11 Artifacts, D8): the run's
  artifacts as a tree (`run_artifacts`, one folder per read).

  - A folder folds, says how many files it holds and offers *Download
    folder* (a zip). The top-level folders start open; a deeper one is read
    when it is opened.
  - A file shows its size and *Download*; its name opens it in the app's
    file viewer (`TfFileViewer`, #2415): an image or a PDF read whole
    (`artifact_bytes`, #2417), up to the attachments' 5 MiB cap; the head of a
    text file (`artifact_preview`); else *No preview*. *Download* stays on its
    title line whichever it is.
  - *Download all as zip*. Every download streams through knobas with the
    stored credential into the downloads folder (`download_run_file`).
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import type { ArtifactEntry, ArtifactPreview, DownloadTarget } from "../../ipc/entity";
  import { missingWords } from "../job/attachment-image";
  import { dataBytes, PREVIEW_CAP } from "../job/attachment-previews.svelte";
  import { attachmentKind } from "../job/attachments";
  import TfFileViewer, { type ViewerContent } from "../render/TfFileViewer.svelte";
  import type { RunPorts } from "./pipeline-ports";
  import { downloadAndSay, drawnWhole, filesText, previewable, sizeText } from "./run-reads";
  import { Windowed } from "../../shell/windowed.svelte";

  let {
    runId,
    sourceName,
    ports,
    rereads = 0,
  }: {
    runId: string;
    /** The source's configured name, for whose sign-in a download uses. */
    sourceName: string;
    ports: Pick<RunPorts, "artifacts" | "preview" | "bytes" | "download">;
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
  /** What a read of the open file came to: a text head, a whole file to draw, over the cap, or why not. */
  type Answer = ArtifactPreview | { drawn: "image" | "pdf"; src: string } | "reading" | "too-large" | { failed: string };
  let preview = $state<{ entry: ArtifactEntry; answer: Answer } | null>(null);
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

  /** One drawn line of the tree: an entry, or a folder's own line while it reads or after it failed. */
  type Line = { key: string; depth: number } & ({ entry: ArtifactEntry } | { reading: true } | { failed: string });

  /** The tree as the lines it draws, open folders' entries under them (a folding list windows its visible rows). */
  const lines = $derived.by((): Line[] => {
    const out: Line[] = [];
    const walk = (path: string, depth: number) => {
      const folder = folders[path];
      if (folder === "reading") out.push({ key: `${path}\u0000reading`, depth, reading: true });
      else if (folder !== undefined && !Array.isArray(folder)) out.push({ key: `${path}\u0000failed`, depth, failed: folder.failed });
      else if (folder !== undefined)
        for (const entry of folder) {
          out.push({ key: entry.path, depth, entry });
          if (entry.folder && (open[entry.path] ?? false)) walk(entry.path, depth + 1);
        }
    };
    walk("", 0);
    return out;
  });
  /** A run of thousands of artifacts draws only the lines near the viewport (the extremes pass, 2026-11-13); a line is 33 px. */
  const win = new Windowed({ count: () => lines.length, key: (i) => lines[i]!.key, estimate: 33 });

  const root = $derived(folders[""]);
  const any = $derived(Array.isArray(root) && root.length > 0);

  const download = (target: DownloadTarget, what: string) => downloadAndSay(ports.download, runId, target, what);

  const NO_PREVIEW: Answer = { text: null, truncated: false };

  /** The source's own cap, for a file the listing gave no size. */
  const overCap = (cause: unknown) => isIpcError(cause) && cause.code === "invalid" && /\bpreviews an artifact up to\b/.test(cause.message);

  /** Open one file in the viewer: an image or PDF is read whole unless it is over the cap, a text file's head is read, any other file has no preview. */
  function showPreview(entry: ArtifactEntry) {
    const drawn = drawnWhole(entry);
    if (drawn !== null && entry.size !== null && entry.size > PREVIEW_CAP) {
      preview = { entry, answer: "too-large" };
      return;
    }
    if (drawn === null && !previewable(entry)) {
      preview = { entry, answer: NO_PREVIEW };
      return;
    }
    preview = { entry, answer: "reading" };
    const mine = token;
    const path = entry.path;
    const read: Promise<Answer> =
      drawn === null
        ? ports.preview(runId, path).then((head) => head ?? NO_PREVIEW)
        : ports.bytes(runId, path).then((bytes) => (bytes === null ? NO_PREVIEW : { drawn, src: `data:${bytes.mime};base64,${bytes.data}` }));
    read.then(
      (answer) => {
        if (mine === token && preview?.entry.path === path) preview = { entry, answer };
      },
      (cause: unknown) => {
        if (mine === token && preview?.entry.path === path) preview = { entry, answer: overCap(cause) ? "too-large" : { failed: ipcErrorMessage(cause) } };
      },
    );
  }

  /** What the viewer shows for the open file. */
  function viewerContent(open: NonNullable<typeof preview>): ViewerContent {
    const answer = open.answer;
    if (answer === "reading") return { kind: "reading" };
    if (answer === "too-large") return { kind: "missing", why: missingWords(open.entry.name, sourceName, { state: "too-large" }), retry: null };
    if ("failed" in answer) return { kind: "missing", why: `${open.entry.name} could not be read: ${answer.failed}`, retry: () => showPreview(open.entry) };
    if ("src" in answer) {
      if (answer.drawn === "image") return { kind: "image", src: answer.src };
      const bytes = dataBytes(answer.src);
      return { kind: "pdf", read: () => Promise.resolve(bytes), size: bytes.length };
    }
    if (answer.text === null) return { kind: "none" };
    return { kind: "text", text: answer.text, truncated: answer.truncated };
  }
</script>

{#snippet line(item: Line)}
  {@const indent = `tf-ad${Math.min(item.depth, DEPTH_MAX)}`}
  {#if "reading" in item}
    <div class={`tf-arow ${indent}`} {@attach win.row(item.key)}><span class="tf-nm tf-dim">…</span></div>
  {:else if "failed" in item}
    <div class={`tf-arow ${indent}`} {@attach win.row(item.key)}><span class="tf-nm tf-dim">Could not be read: {item.failed}</span></div>
  {:else if item.entry.folder}
    {@const entry = item.entry}
    {@const isOpen = open[entry.path] ?? false}
    <div class={`tf-arow tf-rowhost tf-dir ${indent}`} {@attach win.row(item.key)}>
      <button type="button" class="tf-fold" aria-expanded={isOpen} onclick={() => toggle(entry.path)}
        >{entry.name}/</button
      >
      <span class="tf-sz tf-rowval">{filesText(entry) ?? ""}</span>
      <span class="tf-rowend"><span class="tf-rowacts"
        ><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => download({ kind: "folder", path: entry.path }, `${entry.name}/`)}
          >Download folder</button
        ></span></span
      >
    </div>
  {:else}
    {@const entry = item.entry}
    <div class={`tf-arow tf-rowhost ${indent}`} {@attach win.row(item.key)}>
      <button type="button" class="tf-nm tf-aopen" title="Open {entry.name}" onclick={() => showPreview(entry)}>{entry.name}</button>
      <span class="tf-sz tf-rowval">{entry.size === null ? "" : sizeText(entry.size)}</span>
      <span class="tf-rowend"><span class="tf-rowacts">
        <button type="button" class="tf-btn tf-sm" onclick={() => download({ kind: "file", path: entry.path }, entry.name)}>Download</button>
      </span></span>
    </div>
  {/if}
{/snippet}

{#if root === undefined || root === "reading"}
  <p class="tf-dim">Reading the artifacts…</p>
{:else if !Array.isArray(root)}
  <p class="tf-dim">The artifacts could not be read: {root.failed}</p>
{:else}
  <div class="tf-atree" {@attach win.list}>
    {#if any}{#each win.slice(lines) as item (item.key)}{@render line(item)}{/each}{:else}<div class="tf-empty"><p>This run has published no artifacts yet.</p></div>{/if}
  </div>
  {#if any}
    <div class="tf-aall">
      <button type="button" class="tf-btn tf-sm" onclick={() => download({ kind: "all" }, "the artifacts")}>Download all as zip</button>
      <span class="tf-dim">Downloads go through knobas with the {sourceName} sign-in it already has, into ~/Downloads.</span>
    </div>
  {/if}
{/if}

{#if preview !== null}
  {@const entry = preview.entry}
  <TfFileViewer
    name={entry.name}
    line={[entry.path.includes("/") ? entry.path.slice(0, entry.path.lastIndexOf("/") + 1) : null, entry.size === null ? null : sizeText(entry.size)]
      .filter((part) => part !== null)
      .join(", ") || null}
    kind={attachmentKind({ name: entry.name, mime: null })}
    content={viewerContent(preview)}
    download={() => void download({ kind: "file", path: entry.path }, entry.name)}
    onclose={() => (preview = null)}
  />
{/if}
