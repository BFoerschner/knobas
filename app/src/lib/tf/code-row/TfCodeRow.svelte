<!--
  A branch's or a commit's address (#1440): `#/branch/<entity id>` and
  `#/commit/<entity id>`, which `itemHash` gives every branch and commit row
  (#816). Neither has a page of its own: the repository page draws both, so
  this one reads the entity's declared parent (`owner/name`, ADR-0018), finds
  that repository in the entity's own source, and puts its address in place
  of this one: a branch its Branches tab (R-8.1), a commit its page under
  Commits (R-8.7). *Back* does not return to a page that only redirects. The
  id is never taken apart (ADR-0020): a source's key grammar is its own.

  A branch or commit with no declared parent, or one naming a repository the
  mirror does not hold, stays here and says so with its title. An id the
  mirror does not hold reads the shell's *Nothing lives at …*.
-->
<script module lang="ts">
  import { getEntity, listEntities, type EntityDetail, type EntityFilter, type EntityPage } from "../../ipc/entity";
  import { replaceAddress } from "../opener";

  /** The page's two reads, and where it sends the reader. */
  export interface CodeRowPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    /** Put `hash` in place of this address. */
    replace: (hash: string) => void;
  }

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: CodeRowPorts = {
    getEntity: (entityId) => getEntity(entityId),
    listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
    replace: replaceAddress,
  };

  /** A source's repositories, as the pull request page reads them for its header. */
  const LIMIT = 1000;
</script>

<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import { readAllRows, repoOfRow, underRepoHash, type UnderRepoKind } from "../repo/repo";

  let {
    head,
    id,
    ready,
    ports = BRIDGE,
  }: {
    head: UnderRepoKind;
    id: string;
    ready: boolean;
    ports?: CodeRowPorts;
  } = $props();

  /** Nothing yet (or on its way), no such entity, one under no repository the mirror holds, or a failed read. */
  let read = $state<{ missing: true } | { unplaced: string } | { failed: string } | null>(null);

  $effect(() => {
    if (!ready) return;
    const kind = head;
    const entityId = id;
    let live = true;
    read = null;
    void (async () => {
      try {
        const { row } = await ports.getEntity(entityId);
        if (!live) return;
        const repos =
          row.path === null
            ? []
            : await readAllRows(
                (offset) =>
                  ports.listEntities(
                    {
                      sources: [row.source_id],
                      kinds: ["repo"],
                      updated_within_days: null,
                      order: "updated_desc",
                      context: null,
                      project: null,
                      include_deleted: false,
                      reach: null,
                    },
                    LIMIT,
                    offset,
                  ),
                (rows) => repoOfRow(row, rows) !== null,
              );
        if (!live) return;
        const repo = repoOfRow(row, repos);
        if (repo === null) read = { unplaced: row.title };
        else ports.replace(underRepoHash(kind, row.entity_id, repo.entity_id));
      } catch (cause) {
        if (live) read = isIpcError(cause) && cause.code === "not_found" ? { missing: true } : { failed: ipcErrorMessage(cause) };
      }
    })();
    return () => {
      live = false;
    };
  });
</script>

{#if read === null}
  <!-- Not read yet, or on its way to the repository. -->
{:else if "missing" in read}
  <div class="tf-empty">
    <p>Nothing lives at <span class="tf-mono">{location.hash}</span> in the new shell.</p>
    <a class="tf-btn" href="#/tickets">Go to Tickets</a>
  </div>
{:else if "failed" in read}
  <div class="tf-empty"><p class="tf-cfail">Could not read {id}: {read.failed}</p></div>
{:else}
  <div class="tf-empty">
    <p>{read.unplaced} is not under a repository in the mirror.</p>
    <a class="tf-btn" href="#/code">Go to Code</a>
  </div>
{/if}
