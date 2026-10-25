<!--
  The page foot (#2080, R-10.15), after the body and before the comments:
  *Work it belongs to*, each ticket with its status now, then *Linked from*,
  the backlinks by space, each space a fold that starts closed. Either is
  left out when it has nothing in it. Attachments go here once built.

  Nothing remembers an open fold: Docs draws the foot afresh for every page
  it opens, so each opens closed.
-->
<script lang="ts">
  import type { DocPage, DocSpace } from "../../ipc/entity";
  import { jobHash } from "../route";
  import { backlinkGroups, linkedFromHeading, pageHash } from "./docs";

  let { page, spaces }: { page: DocPage; spaces: readonly DocSpace[] } = $props();

  const groups = $derived(backlinkGroups(page.backlinks, spaces, page.source_id));
</script>

{#if page.work.length > 0}
  <section class="tf-rd-foot">
    <h3>Work it belongs to</h3>
    <ul>
      {#each page.work as work (work.entity_id)}
        <li>
          <a href={jobHash(work.entity_id)}><span class="tf-mono">{work.key}</span> {work.title}</a
          >{#if work.status}<span class="tf-s">{work.status}</span>{/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
{#if page.backlinks.length > 0}
  <section class="tf-rd-foot tf-rd-linked">
    <h3>{linkedFromHeading(page.backlinks.length)}</h3>
    {#each groups as group (group.key)}
      <details>
        <summary
          >{#if group.key === null}Other pages{:else}<b>{group.key}</b>{#if group.name}{` ${group.name}`}{/if}{/if}<span
            class="tf-count">{group.links.length}</span
          ></summary
        >
        <ul>
          {#each group.links as link (link.entity_id)}
            <li><a href={pageHash(link.entity_id)}>{link.title}</a></li>
          {/each}
        </ul>
      </details>
    {/each}
  </section>
{/if}
