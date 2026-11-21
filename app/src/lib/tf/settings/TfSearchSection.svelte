<!--
  Settings › Search (the everything-search decisions, 2026-11-20, D13): what *Everything* looks through beyond names and
  keys, a box each, and how much of the mirror each covers. A box filters what the next key matches and changes nothing
  kept, so it draws at once and writes behind, as Settings › Docs' boxes do; a write that fails puts the box back and
  says so. *Titles* matches names and keys whatever is set here.

  *File contents kept on this computer* is the one control that changes what is kept: the bound on the file text, the
  text kept against it, how many repositories are read, and *Read every repository again*. Read again as the files
  pass reports each repository (`code:files`).
-->
<script lang="ts">
  import { listen as tauriListen } from "@tauri-apps/api/event";

  import {
    EVENTS,
    ipcErrorMessage,
    rereadFiles as realReread,
    searchSettings as realRead,
    setSearchEverything as realWrite,
    setSearchFilesKept as realKept,
    type EverythingMatches,
    type SearchSettings,
  } from "../../ipc";
  import { formatBytes } from "../../sources/diagnostics";
  import { thousands } from "../counted";

  let {
    read = realRead,
    write = realWrite,
    kept = realKept,
    reread = realReread,
    listen = (handler: () => void) => tauriListen(EVENTS.codeFiles, handler),
  }: {
    /** The page's read; the IPC's when omitted. */
    read?: () => Promise<SearchSettings>;
    /** The boxes' write, answering the page as it now stands; the IPC's when omitted. */
    write?: (matches: EverythingMatches) => Promise<SearchSettings>;
    /** *Keep at most*'s write. */
    kept?: (mb: number) => Promise<SearchSettings>;
    /** *Read every repository again*. */
    reread?: () => Promise<SearchSettings>;
    /** The files pass's progress, which reads the page again. */
    listen?: (handler: () => void) => Promise<() => void>;
  } = $props();

  let settings = $state.raw<SearchSettings | null>(null);
  let failed = $state<string | null>(null);
  let refused = $state<string | null>(null);
  let rereading = $state(false);
  $effect(() => {
    void read().then(
      (answer) => {
        settings = answer;
        failed = null;
      },
      (cause: unknown) => (failed = ipcErrorMessage(cause)),
    );
  });
  // The pass's progress, at most once a second: the status follows the fill.
  $effect(() => {
    let off: (() => void) | null = null;
    let dead = false;
    let waiting: ReturnType<typeof setTimeout> | null = null;
    void listen(() => {
      waiting ??= setTimeout(() => {
        waiting = null;
        void read().then((answer) => (settings = answer), () => {});
      }, 1000);
    }).then(
      (stop) => (dead ? stop() : (off = stop)),
      () => {},
    );
    return () => {
      dead = true;
      off?.();
      if (waiting !== null) clearTimeout(waiting);
    };
  });

  /** A box a line: what it matches, a line of help where its words need one, and what it covers. */
  interface Box {
    key: keyof EverythingMatches;
    label: string;
    help: string | null;
    held: (read: SearchSettings) => string;
  }
  const count = (n: number, one: string, many: string) => `${thousands(n)} ${n === 1 ? one : many}`;
  const BOXES: readonly Box[] = [
    { key: "ticket_text", label: "Ticket descriptions and comments", help: null, held: (s) => count(s.tickets, "ticket", "tickets") },
    { key: "pr_text", label: "Pull request descriptions and comments", help: null, held: (s) => count(s.pull_requests, "pull request", "pull requests") },
    { key: "page_text", label: "Docs pages and their comments", help: null, held: (s) => count(s.pages, "page", "pages") },
    { key: "build_logs", label: "Build logs", help: "The end of each pipeline's last finished run.", held: (s) => count(s.pipelines, "pipeline", "pipelines") },
    { key: "file_names", label: "File names", help: "Every file on each repository's default branch. A match shows the repository.", held: (s) => count(s.files, "file", "files") },
    {
      key: "file_text",
      label: "File contents",
      help: "Text files on each repository's default branch. A match shows the repository and the line.",
      held: (s) => count(s.text_files, "file", "files"),
    },
  ];

  /** *Keep at most*'s choices, in MB. */
  const KEEPS: readonly [number, string][] = [
    [0, "None"],
    [500, "500 MB"],
    [1024, "1 GB"],
    [2048, "2 GB"],
    [5120, "5 GB"],
    [10240, "10 GB"],
  ];

  /** One box set or cleared: drawn now, written behind, put back if the write fails. */
  function flip(key: keyof EverythingMatches, on: boolean): void {
    const before = settings;
    if (before === null) return;
    const asked = { ...before.everything, [key]: on };
    settings = { ...before, everything: asked };
    refused = null;
    void write(asked).then(
      (answer) => {
        // A later press's answer is the one to keep.
        if (settings?.everything === asked) settings = answer;
      },
      (cause: unknown) => {
        if (settings?.everything === asked) settings = before;
        refused = `Could not save the box: ${ipcErrorMessage(cause)}`;
      },
    );
  }

  function keep(event: Event): void {
    const mb = Number((event.currentTarget as HTMLSelectElement).value);
    const before = settings;
    if (before === null) return;
    settings = { ...before, kept_mb: mb };
    refused = null;
    void kept(mb).then(
      (answer) => (settings = answer),
      (cause: unknown) => {
        settings = before;
        refused = `Could not save how much is kept: ${ipcErrorMessage(cause)}`;
      },
    );
  }

  function again(): void {
    rereading = true;
    refused = null;
    void reread().then(
      (answer) => {
        settings = answer;
        rereading = false;
      },
      (cause: unknown) => {
        rereading = false;
        refused = `Could not ask for every repository again: ${ipcErrorMessage(cause)}`;
      },
    );
  }

  /** What the meter fills: the text kept of the bound, in a hundredth. */
  const filled = $derived(settings === null || settings.kept_mb === 0 ? 0 : Math.min(100, Math.round((settings.text_bytes / (settings.kept_mb * 1024 * 1024)) * 100)));
  /** The status line and the line under it (D13). */
  const status = $derived.by(() => {
    if (settings === null) return null;
    const s = settings;
    if (s.kept_mb === 0) return { line: "No file text is kept.", under: "Files are still found by their names." };
    const owed = Math.max(0, s.repositories - s.repositories_read);
    const line = `${formatBytes(s.text_bytes)} from ${s.repositories_read} of ${count(s.repositories, "repository", "repositories")}.`;
    const read = "Read after each sync, from your clone where you have one, else from the forge.";
    if (s.text_bytes > s.kept_mb * 1024 * 1024) return { line, under: `More than the bound: the largest are read again with names only after the next sync. ${read}` };
    return { line, under: owed === 0 ? read : `${owed} ${owed === 1 ? "is" : "are"} waiting for their first read. ${read}` };
  });
</script>

<header class="tf-stg-h"><h1>Search</h1><p>What Everything looks through. Titles looks at names and keys only, whatever is set here.</p></header>
<section class="tf-stg-sec" aria-label="Everything also matches">
  <h2>Everything also matches</h2>
  <p class="tf-stg-p">Names and keys are always matched. A box you clear is kept and synced as before. Everything stops looking there.</p>
  {#if failed !== null}
    <p class="tf-stg-p tf-stg-errt" role="status">Could not read what Everything matches: {failed}</p>
  {:else if settings === null}
    <p class="tf-stg-p" role="status">Reading what Everything matches…</p>
  {:else}
    {@const now = settings}
    <div class="tf-stg-boxes">
      {#each BOXES as box (box.key)}
        <label class="tf-stg-box"
          ><input type="checkbox" checked={now.everything[box.key]} onchange={(event) => flip(box.key, event.currentTarget.checked)} /><span
            >{box.label}{#if box.help}<small>{box.help}</small>{/if}</span
          ><span class="tf-stg-held">{box.held(now)}</span></label
        >
      {/each}
    </div>
    {#if refused !== null}<p class="tf-stg-p tf-stg-errt" role="status">{refused}</p>{/if}
  {/if}
</section>
{#if settings !== null && status !== null}
  {@const now = settings}
  <section class="tf-stg-sec" aria-label="File contents kept on this computer">
    <h2>File contents kept on this computer</h2>
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-search-kept">Keep at most<small>Of file text. On disk it takes about two fifths of that.</small></label>
      <div class="tf-stg-c">
        <select id="tf-search-kept" class="tf-stg-in" value={String(now.kept_mb)} onchange={keep}>
          {#if !KEEPS.some(([mb]) => mb === now.kept_mb)}<option value={String(now.kept_mb)}>{formatBytes(now.kept_mb * 1024 * 1024)}</option>{/if}
          {#each KEEPS as [mb, label] (mb)}<option value={String(mb)}>{label}</option>{/each}
        </select>
        <!-- An SVG and not a styled width: no inline style under the CSP (house rules). -->
        <svg class="tf-stg-meter" viewBox="0 0 100 6" preserveAspectRatio="none" aria-hidden="true"><rect width={filled} height="6" /></svg>
        <p class="tf-stg-status" role="status">{status.line}<small>{status.under}</small></p>
      </div>
    </div>
    <div class="tf-stg-row">
      <span class="tf-stg-l">Left out<small>Found by name only.</small></span>
      <div class="tf-stg-c">
        <p class="tf-stg-p">
          Files over 512 KB, binaries, lock files, minified and generated files, and the folders node_modules, vendor, target, dist and build.
          Whatever .gitattributes marks linguist-generated or linguist-vendored.
        </p>
        <button type="button" class="tf-btn" disabled={rereading} onclick={again}>Read every repository again</button>
      </div>
    </div>
  </section>
{/if}
