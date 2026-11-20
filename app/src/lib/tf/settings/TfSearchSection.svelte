<!--
  Settings › Search (the everything-search decisions, 2026-11-20, D13): what *Everything* looks through beyond names and
  keys, a box each, and how much of the mirror each covers. A box filters what the next key matches and changes nothing
  kept, so it draws at once and writes behind, as Settings › Docs' boxes do; a write that fails puts the box back and
  says so. *Titles* matches names and keys whatever is set here.
-->
<script lang="ts">
  import {
    ipcErrorMessage,
    searchSettings as realRead,
    setSearchEverything as realWrite,
    type EverythingMatches,
    type SearchSettings,
  } from "../../ipc";
  import { thousands } from "../counted";

  let {
    read = realRead,
    write = realWrite,
  }: {
    /** The page's read; the IPC's when omitted. */
    read?: () => Promise<SearchSettings>;
    /** The boxes' write, answering the page as it now stands; the IPC's when omitted. */
    write?: (matches: EverythingMatches) => Promise<SearchSettings>;
  } = $props();

  let settings = $state.raw<SearchSettings | null>(null);
  let failed = $state<string | null>(null);
  let refused = $state<string | null>(null);
  $effect(() => {
    void read().then(
      (answer) => {
        settings = answer;
        failed = null;
      },
      (cause: unknown) => (failed = ipcErrorMessage(cause)),
    );
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
