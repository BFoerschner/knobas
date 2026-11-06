<!--
  The page foot (#2080, R-10.15), after the body and before the comments:
  *Work it belongs to*, each ticket with its status now, then *Linked from*:
  up to `LINKED_FLAT` backlinks are listed outright, each saying its space
  where they come from more than one, so reaching a single link takes no click.
  Past that they go by space, each space a fold that starts closed. Either is
  left out when it has nothing in it. Attachments go here once built.

  Nothing remembers an open fold: Docs draws the foot afresh for every page
  it opens, so each opens closed.

  A ticket linked by hand here has *Unlink* on its row (F-433), with *Undo* in the
  toast; a link the source states or a set draws is the ticket's drawer's to handle.
-->
<script lang="ts">
  import type { DocPage, DocSpace, LinkEntry } from "../../ipc/entity";
  import { entityAddress } from "../address";
  import { stateWord } from "../state-word";
  import TfKey from "../TfKey.svelte";
  import { backlinkGroups, LINKED_FLAT, linkedFromHeading, pageHash } from "./docs";

  let {
    page,
    spaces,
    links = [],
    onunlink,
  }: {
    page: DocPage;
    spaces: readonly DocSpace[];
    /** The page's own links, which say which ticket here was linked by hand. */
    links?: readonly LinkEntry[];
    onunlink?: ((link: LinkEntry["link"]) => void) | undefined;
  } = $props();

  const byHand = (entityId: string) => links.find((entry) => entry.other.entity_id === entityId && entry.link.origin === "manual")?.link ?? null;

  const groups = $derived(backlinkGroups(page.backlinks, spaces, page.source_id));
</script>

{#if page.work.length > 0}
  <section class="tf-rd-foot">
    <h3>Work it belongs to</h3>
    <ul>
      {#each page.work as work (work.entity_id)}
        <li>
          <a href={entityAddress("ticket", work.entity_id)}><TfKey value={work.key} /> {work.title}</a
          >{#if work.status}<span class="tf-s">{stateWord(work.status)}</span>{/if}{#if onunlink && work.linked && byHand(work.entity_id)}{@const link = byHand(work.entity_id)!}<button
              type="button"
              class="tf-btn tf-sm tf-ghost tf-rd-unlink"
              aria-label="Unlink {work.key}"
              onclick={() => onunlink(link)}>Unlink</button
            >{/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
{#if page.backlinks.length > 0}
  <section class="tf-rd-foot tf-rd-linked">
    <h3>{linkedFromHeading(page.backlinks.length)}</h3>
    {#if page.backlinks.length <= LINKED_FLAT}
      <ul>
        {#each groups as group (group.key)}
          {#each group.links as link (link.entity_id)}
            <li>
              <a href={pageHash(link.entity_id)}>{link.title}</a
              >{#if groups.length > 1 && group.key !== null}<span class="tf-s">{group.name ?? group.key}</span>{/if}
            </li>
          {/each}
        {/each}
      </ul>
    {:else}
      {#each groups as group (group.key)}
      <details>
        <summary
          >{#if group.key === null}Other pages{:else}<b>{group.name ?? group.key}</b>{/if}<span
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
    {/if}
  </section>
{/if}
