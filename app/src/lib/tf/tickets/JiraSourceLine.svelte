<!--
  Where a Jira view comes from (#879, R-3.22, R-3.23), under the board head,
  in prototype-6's words (`vwSrcLine`): board or filter, its type, the active
  sprint and its end, the JQL (and a Kanban board's sub-filter), when the
  last sync read it, and *Open in Jira*. Then, per project the source does not
  sync that holds tickets on it, a warn line with *Sync <project> too*, which
  opens the source's page with the project added, unsaved.
-->
<script lang="ts">
  import type { JiraSource } from "../../ipc/search";
  import { ago } from "../../shell/time";
  import { sourceHash } from "../settings/settings";
  import { jiraKindWords, sprintEnds } from "./views";

  let {
    jira,
    name,
    sourceId,
    openExternal,
  }: {
    jira: JiraSource;
    /** The view's name, which is the board's or the filter's. */
    name: string;
    /** The Jira source the view comes from. */
    sourceId: string;
    openExternal: (url: string) => Promise<void>;
  } = $props();

  /** *Sync WEB too*: the source's page with the project added to what it syncs, unsaved. */
  function syncToo(project: string) {
    location.hash = sourceHash(sourceId, { field: "projects", value: project });
  }

  function open(event: MouseEvent) {
    event.preventDefault();
    if (jira.web_url) void openExternal(jira.web_url).catch(() => {});
  }
</script>

<p class="tf-vw-srcline">
  Jira decides which tickets, from the {jiraKindWords(jira)} <b>{name}</b>{#if jira.sprint}, active sprint {jira.sprint
      .name}{#if jira.sprint.end_date}{" "}(ends {sprintEnds(jira.sprint.end_date)}){/if}{/if}:
  <code>{jira.jql}</code>{#if jira.sub_query}{" "}and <code>{jira.sub_query}</code>{/if}. Read again at every sync, last
  {ago(jira.last_read)}.
  {#if jira.web_url}<a href={jira.web_url} onclick={open}>Open in Jira</a>{/if}
</p>
{#each jira.unsynced as { project, count } (project)}
  <p class="tf-vw-srcline tf-vw-warn">
    {count}
    {count === 1 ? "ticket on it is" : "tickets on it are"} in {project}, which the Jira source does not sync, so
    {count === 1 ? "it is" : "they are"} missing here.
    <button type="button" class="tf-btn tf-sm" onclick={() => syncToo(project)}>Sync {project} too</button>
  </p>
{/each}
