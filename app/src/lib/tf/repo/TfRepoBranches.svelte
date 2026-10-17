<!--
  The repository page's *Branches* tab (#839, R-8.8), prototype-6's
  `renderRepo(...,'branches')`: the filter (text, *Mine*, *No PR*, *Merged*),
  then the default branch marked *default* and every other branch with ahead
  and behind it (or *merged*), its open pull request, who committed its tip
  (*By*) and when; *Open PR* and *Delete…* where the source offers them.

  *By* and *Mine* are the tip's declared author (`code_facts`, #986), never
  the listing's line.

  The list is the repository's own view (`entity_view`, section `branches`);
  each branch's standing is that branch's view (`view/branch.rs`: its compare
  against the default branch and the count of what it lacks), read side by
  side, a few at a time. Whether a branch offers a pull request or a delete,
  and why a delete is held back, are its view's actions, never a reading of
  its facts (ADR-0020 rule 4).

  *Delete…* opens prototype-6's `openDeleteBranch` and queues `DeleteBranch`
  against the branch. A branch made or deleted this session is drawn queued
  at once (`queued.svelte.ts`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CodeFacts, ViewRow } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { itemHash } from "../job/job";
  import TfDialog from "../settings/TfDialog.svelte";
  import { isDeleteQueued, madeNotListed, markDeleted, toastQueued } from "./queued.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { branchStanding, compareHash, viewRows, visibleBranches, type BranchRow, type BranchStanding } from "./repo";

  let {
    ports,
    repoId,
  }: {
    ports: Pick<RepoPorts, "entityView" | "submitWrite" | "codeFacts">;
    repoId: string;
  } = $props();

  /** How many branch views are read at once: a source's burst stays small. */
  const AT_ONCE = 4;

  let listed = $state<ViewRow[] | null>(null);
  let failed = $state<string | null>(null);
  let standings = $state<Record<string, BranchStanding>>({});
  let facts = $state<Record<string, CodeFacts>>({});

  $effect(() => {
    const id = repoId;
    let live = true;
    listed = null;
    failed = null;
    standings = {};
    facts = {};
    ports.entityView(id, "branches").then(
      (view) => {
        if (!live) return;
        listed = viewRows(view?.sections[0]);
        const every = listed.flatMap((row) => (row.entity_id === null ? [] : [row.entity_id]));
        if (every.length > 0)
          ports.codeFacts(every).then(
            (answer) => {
              if (live) facts = Object.fromEntries(answer.map((entry) => [entry.entity_id, entry]));
            },
            () => {},
          );
        const ids = listed.filter((row) => row.entity_id !== null && row.chip !== "default").map((row) => row.entity_id!);
        let next = 0;
        const worker = async () => {
          while (live && next < ids.length) {
            const branchId = ids[next++]!;
            try {
              const standing = branchStanding(await ports.entityView(branchId, null));
              if (live) standings[branchId] = standing;
            } catch {
              // A branch whose view fails draws without its standing.
            }
          }
        };
        for (let n = 0; n < AT_ONCE; n += 1) void worker();
      },
      (error) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  /** A branch's row, or the note a cut listing ends on (no id, no line). */
  const isNote = (row: ViewRow) => row.entity_id === null && row.meta === null && row.chip !== "default";

  /** Branches this session queued that the listing does not hold yet. */
  const made = $derived(new Set(madeNotListed(repoId, (listed ?? []).map((row) => row.title))));

  const rows = $derived.by((): BranchRow[] => {
    const branches = (listed ?? []).filter((row) => !isNote(row));
    return [
      ...branches.map(
        (row): BranchRow => ({
          name: row.title,
          entityId: row.entity_id,
          isDefault: row.chip === "default",
          meta: row.meta,
          standing: row.entity_id !== null ? (standings[row.entity_id] ?? null) : null,
          author: row.entity_id !== null ? (facts[row.entity_id]?.author ?? null) : null,
          mine: row.entity_id !== null && facts[row.entity_id]?.mine === true,
        }),
      ),
      ...[...made].map(
        (name): BranchRow => ({ name, entityId: null, isDefault: false, meta: null, standing: null, author: null, mine: false }),
      ),
    ];
  });
  const notes = $derived((listed ?? []).filter(isNote));
  const defaultName = $derived(rows.find((row) => row.isDefault)?.name ?? "the default branch");

  const filter = $derived(sessionFilter(`repo-branches:${repoId}`));
  const config = $derived<FilterConfig>({
    id: `repo-branches:${repoId}`,
    placeholder: "Filter branches",
    noun: "branches",
    one: "branch",
    chips: [{ key: "show", label: "Show", options: ["Mine", "No PR", "Merged"].map((value) => ({ value, label: value })) }],
  });
  const shown = $derived(visibleBranches(rows, filter.chips["show"] ?? [], filter.text));

  function against(standing: BranchStanding | null): string {
    if (standing === null || standing.ahead === null) return "";
    if (standing.merged) return "";
    return `${standing.ahead} ahead, ${standing.behind ?? 0} behind`;
  }

  /** The branch the delete dialog asks about. */
  let deleting = $state<BranchRow | null>(null);
  let busy = $state(false);

  async function remove(row: BranchRow) {
    if (row.entityId === null) return;
    busy = true;
    try {
      await ports.submitWrite({ DeleteBranch: { entity: row.entityId } });
      markDeleted(repoId, row.name);
      toastQueued(`delete branch ${row.name}`);
      deleting = null;
    } catch (cause) {
      push({ text: `Could not queue the delete: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  function confirmDelete() {
    if (deleting !== null) void remove(deleting);
  }

  const plural = (n: number) => (n === 1 ? "1 commit on this branch is" : `${n} commits on this branch are`);
</script>

<div class="tf-rbar">
  <FilterBar {config} {filter} shown={listed === null ? null : shown.length} total={listed === null ? null : rows.length} />
</div>
{#if failed !== null}
  <p class="tf-cfail">{failed}</p>
{:else if listed !== null && shown.length === 0}
  <div class="tf-empty">No branches match.</div>
{:else if shown.length > 0}
  <table class="tf-tbl tf-rbranches">
    <thead>
      <tr><th>Branch</th><th>Against {defaultName}</th><th>Pull request</th><th>By</th><th>Updated</th><th></th></tr>
    </thead>
    <tbody>
      {#each shown as row (row.name)}
        {@const standing = row.standing}
        {@const gone = isDeleteQueued(repoId, row.name)}
        <tr>
          <td class="tf-mono"
            ><Marked text={row.name} typed={filter.text} />{#if row.isDefault}{" "}<span class="tf-chip">default</span>{/if}{#if made.has(row.name)}{" "}<span
                class="tf-chip tf-info">queued</span
              >{/if}{#if gone}{" "}<span class="tf-chip tf-warn">delete queued</span>{/if}</td
          >
          <td
            >{#if standing?.merged}<span class="tf-chip tf-ok">merged</span>{:else}{against(standing)}{/if}</td
          >
          <td
            >{#if standing?.pr}{#if standing.pr.entityId !== null}<a href={itemHash({ entity_id: standing.pr.entityId, kind: "pr" })}
                  >{standing.pr.number ?? ""}</a
                >{:else}{standing.pr.number ?? ""}{/if}{#if standing.pr.state}{" "}<span class="tf-chip tf-{standing.pr.tone ?? 'info'}"
                  >{standing.pr.state}</span
                >{/if}{:else if standing !== null}<span class="tf-dim">none</span>{/if}</td
          >
          <td>{row.author ?? ""}</td>
          <td class="tf-dim tf-nowrap">{row.meta ?? ""}</td>
          <td class="tf-nowrap tf-racts"
            >{#if standing !== null && !gone}{#if standing.canOpenPr}<a class="tf-btn tf-sm" href={compareHash(repoId, row.name)}
                  >Open PR</a
                >{" "}{/if}{#if standing.deletable}<button
                  type="button"
                  class="tf-btn tf-sm tf-ghost"
                  aria-disabled={standing.deleteBlocked !== null ? "true" : undefined}
                  title={standing.deleteBlocked ?? undefined}
                  onclick={() => {
                    if (standing.deleteBlocked === null) deleting = row;
                  }}>Delete…</button
                >{/if}{/if}</td
          >
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
{#each notes as note, n (n)}
  <p class="tf-dim tf-small tf-rbranches-note">{note.title}</p>
{/each}

{#if deleting !== null && deleting.standing !== null}
  {@const standing = deleting.standing}
  {@const row = deleting}
  <TfDialog title="Delete {row.name}?" onclose={() => (deleting = null)} className="tf-rdel">
    {#snippet body()}
      {#if standing.merged}
        <p>Every commit on this branch is in {defaultName}. Nothing is lost.</p>
      {:else}
        <div class="tf-banner tf-warn tf-dbanner">
          {plural(standing.ahead ?? 0)} not in {defaultName} and will be lost.
        </div>
      {/if}
      {#if standing.pr !== null}
        <p class="tf-dim">PR {standing.pr.number ?? ""} is open from this branch. Gitea closes it when the branch goes.</p>
      {/if}
      <p class="tf-dim tf-note">This deletes the branch on Gitea. Your local checkout keeps its copy.</p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (deleting = null)}>Cancel</button>
      <button
        type="button"
        class="tf-btn {standing.merged ? 'tf-primary' : 'tf-danger'}"
        disabled={busy}
        onclick={confirmDelete}>Delete branch</button>
    {/snippet}
  </TfDialog>
{/if}
