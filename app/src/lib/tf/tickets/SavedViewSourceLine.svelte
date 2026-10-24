<!--
  Where a saved view comes from (#879, R-3.22, R-3.23), under the board head,
  in prototype-6's words (`vwSrcLine`): board or filter, its type, the active
  sprint and its end, the JQL (and a Kanban board's sub-filter), when the
  last sync read it, and *Open in Jira*, naming the view's own ticket system
  (`system`, #2032). Then, per project the source does not
  sync that holds tickets on it, a warn line with *Sync <project> too*, which
  opens the source's page with the project added, unsaved.
-->
<script lang="ts">
  import type { SavedViewSource } from "../../ipc/search";
  import { ago } from "../../shell/time";
  import { sourceHash } from "../settings/settings";
  import { savedViewKindWords, sprintEnds } from "./views";

  let {
    savedViewSource,
    name,
    sourceId,
    openExternal,
  }: {
    savedViewSource: SavedViewSource;
    /** The view's name, which is the board's or the filter's. */
    name: string;
    /** The source the view comes from. */
    sourceId: string;
    openExternal: (url: string) => Promise<void>;
  } = $props();

  /** *Sync WEB too*: the source's page with the project added to what it syncs, unsaved. */
  function syncToo(project: string) {
    location.hash = sourceHash(sourceId, { field: "projects", value: project });
  }

  function open(event: MouseEvent) {
    event.preventDefault();
    if (savedViewSource.web_url) void openExternal(savedViewSource.web_url).catch(() => {});
  }
</script>

<p class="tf-vw-srcline">
  {savedViewSource.system} decides which tickets, from the {savedViewKindWords(savedViewSource)} <b>{name}</b>{#if savedViewSource.sprint}, active sprint {savedViewSource.sprint
      .name}{#if savedViewSource.sprint.end_date}{" "}(ends {sprintEnds(savedViewSource.sprint.end_date)}){/if}{/if}:
  <code>{savedViewSource.source_query}</code>{#if savedViewSource.sub_query}{" "}and <code>{savedViewSource.sub_query}</code>{/if}. Read again at every sync, last
  {ago(savedViewSource.last_read)}.
  {#if savedViewSource.web_url}<a href={savedViewSource.web_url} onclick={open}>Open in {savedViewSource.system}</a>{/if}
</p>
{#each savedViewSource.unsynced as { project, count } (project)}
  <p class="tf-vw-srcline tf-vw-warn">
    {count}
    {count === 1 ? "ticket on it is" : "tickets on it are"} in {project}, which the {savedViewSource.system} source does not sync, so
    {count === 1 ? "it is" : "they are"} missing here.
    <button type="button" class="tf-btn tf-sm" onclick={() => syncToo(project)}>Sync {project} too</button>
  </p>
{/each}
