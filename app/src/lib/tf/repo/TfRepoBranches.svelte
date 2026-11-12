<!--
  The repository page's *Branches* tab (#839, R-8.8), prototype-6's
  `renderRepo(...,'branches')`: the filter (text, *Mine*, *No PR*, *Merged*),
  then the default branch marked *default* and every other branch with ahead
  and behind it (or *merged*), its open pull request, who committed its tip
  (*By*) and when; *New pull request* and *Delete…* where the source offers them.

  *By* and *Mine* are the tip's declared author (`code_facts`, #986), never
  the listing's line.

  The list is the repository's own view (`entity_view`, section `branches`);
  each branch's standing is that branch's view (`view/branch.rs`: its compare
  against the default branch and the count of what it lacks), read side by
  side, a few at a time. Whether a branch offers a pull request or a delete,
  and why a delete is held back, are its view's actions, never a reading of
  its facts (ADR-0020 rule 4).

  A pull request this session sent from a branch stands in its *Pull request*
  cell, *Queued* then *Sent*, a link to the *Pull requests* tab (F-264), and
  its *New pull request* goes, until the branch's view holds it (F-257).

  *Delete…* opens prototype-6's `openDeleteBranch` and queues `DeleteBranch`
  against the branch. A branch made or deleted this session is drawn at once
  with its echo (`tf/queued.svelte.ts`, #2105), *queued* and then *sent*,
  until the listing holds it or no longer does: read again on the refresh
  event and a run's end, not only as the tab opens.

  A listing knobas held is drawn at once under the held line (ADR-0042, *As
  knobas read it … Checking …*) and read again behind it. A branch the source
  lists and the mirror has not synced yet has its standing read at the id
  the source named (`unsynced`), and offers no *Delete…* until a sync holds
  it. Read again, the listing lands without moving the row the reader is on
  (`landInPlace`).
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import TfKey from "../TfKey.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { CodeFacts, ShownView, ViewRow } from "../../ipc/entity";
  import TfHeldNote from "../TfHeldNote.svelte";
  import { landInPlace } from "../land-in-place";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { itemHash } from "../job/job";
  import { jumpAcross } from "../opener";
  import TfDialog from "../settings/TfDialog.svelte";
  import { untrack } from "svelte";
  import { echoWrite, echoesOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onViewFresh } from "./view-fresh.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import {
    DELETED,
    MADE,
    MADE_PR,
    REPO_BRANCHES,
    REPO_PRS,
    branchStanding,
    compareHash,
    repoHash,
    sourceName,
    viewRows,
    visibleBranches,
    type BranchRow,
    type BranchStanding,
  } from "./repo";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";

  let {
    ports,
    source = null,
    repoId,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "submitWrite" | "codeFacts" | "listen">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    repoId: string;
    /** The page's refreshes (#2135): each reads the listing again over what the tab drew. */
    refreshed?: number;
  } = $props();

  /** How many branch views are read at once: a source's burst stays small. */
  const AT_ONCE = 4;

  let listed = $state<ViewRow[] | null>(null);
  /** A listing read under way: what is drawn stays, dimmed, until it answers (#2246). */
  let reading = $state(false);
  let failed = $state<string | null>(null);
  let standings = $state<Record<string, BranchStanding>>({});
  /** The id the source named for each listed branch the mirror has not synced yet, by name: its standing is read there. */
  let unsynced = $state<Map<string, string>>(new Map());
  /** When the listing drawn was read, where knobas held it (ADR-0042); and why the read behind it failed. */
  let heldAt = $state<string | null>(null);
  let heldFailed = $state<string | null>(null);
  /** The table, where a listing read again keeps the reader's place. */
  let table = $state<HTMLElement | undefined>(undefined);
  /** Each listing read, so only the newest answer lands. */
  let readNo = 0;
  let facts = $state<Record<string, CodeFacts>>({});

  $effect(() => {
    const id = repoId;
    let live = true;
    listed = null;
    failed = null;
    standings = {};
    facts = {};
    readBranches(id, () => live);
    return () => {
      live = false;
    };
  });

  // The refresh after a branch made or deleted here, or a run of its
  // source's end, reads the listing again (#2105).
  $effect(() => {
    const id = repoId;
    return untrack(() => watchEchoes(REPO_BRANCHES, () => readBranches(id, () => repoId === id, true), { listen: ports.listen }));
  });

  // A pull request sent from here lands on the repository's *Pull requests*: its refresh brings the branch's standing with it.
  $effect(() => {
    const id = repoId;
    return untrack(() => watchEchoes(REPO_PRS, () => readBranches(id, () => repoId === id, true), { listen: ports.listen }));
  });

  // The listing knobas held, read again behind it (ADR-0042); or the read behind it failed, which the held line says.
  onViewFresh(
    () => repoId,
    "branches",
    () => {
      const id = repoId;
      void readBranches(id, () => repoId === id, true);
    },
    untrack(() => ports.listen),
    (why) => (heldFailed = why),
  );

  // A refresh of the page reads the listing again, over what the tab drew (#2135).
  onRefreshed(
    () => refreshed,
    () => {
      const id = repoId;
      return readBranches(id, () => repoId === id, true);
    },
  );

  /**
   * The listing, then each branch's standing and its tip's author; each echo
   * the listing shows landed goes. `again` keeps what the tab drew when it
   * fails.
   */
  function readBranches(id: string, alive: () => boolean, again = false): Promise<unknown> {
    // Read again behind what is drawn, the listing stays live: dimmed it would be inert, and drop the reader's focus.
    if (!again) reading = true;
    const mine = ++readNo;
    return ports.entityView(id, "branches").then(
      (view) => {
        if (!alive() || mine !== readNo) return;
        reading = false;
        const shownView = view as ShownView | null;
        heldAt = shownView?.held_at ?? null;
        heldFailed = null;
        const section = view?.sections[0];
        unsynced = new Map((shownView?.unsynced ?? []).filter((row) => section === undefined || row.section === section.id).map((row) => [row.title, row.entity_id]));
        const read = viewRows(section);
        if (again) void landInPlace(table, () => (listed = read));
        else listed = read;
        const names = new Set(read.map((row) => row.title));
        settleEchoes(id, (echo) => (echo.kind === MADE && names.has(echo.text)) || (echo.kind === DELETED && !names.has(echo.text)));
        const every = read.flatMap((row) => (row.entity_id === null ? [] : [row.entity_id]));
        if (every.length > 0)
          ports.codeFacts(every).then(
            (answer) => {
              if (alive()) facts = Object.fromEntries(answer.map((entry) => [entry.entity_id, entry]));
            },
            () => {},
          );
        const ids = read.flatMap((row) => {
          const at = row.entity_id ?? unsynced.get(row.title) ?? null;
          return at === null || row.chip === "default" ? [] : [at];
        });
        let next = 0;
        const worker = async () => {
          while (alive() && next < ids.length) {
            const branchId = ids[next++]!;
            try {
              const standing = branchStanding(await ports.entityView(branchId, null));
              if (alive()) standings[branchId] = standing;
            } catch {
              // A branch whose view fails draws without its standing.
            }
          }
        };
        return Promise.all(Array.from({ length: AT_ONCE }, worker));
      },
      (error) => {
        if (!alive() || mine !== readNo) return;
        reading = false;
        if (!again) failed = ipcErrorMessage(error);
      },
    );
  }

  /** A branch's row, or the note a cut listing ends on (no id, no line). */
  const isNote = (row: ViewRow) => row.entity_id === null && row.meta === null && row.chip !== "default";

  /** Where each pull request this session sent stands, by the branch it proposes, until the branch's view holds it (F-257). */
  const prSent = $derived(new Map(echoesOn(repoId, MADE_PR).flatMap((echo) => (echo.base === null ? [] : [[echo.base, echo.state] as const]))));

  /** Branches this session queued that the listing does not hold yet, by name, with their echo's word. */
  const made = $derived(new Map(echoesOn(repoId, MADE).map((echo) => [echo.text, echo.state])));

  const rows = $derived.by((): BranchRow[] => {
    const branches = (listed ?? []).filter((row) => !isNote(row));
    return [
      ...branches.map(
        (row): BranchRow => ({
          name: row.title,
          entityId: row.entity_id,
          isDefault: row.chip === "default",
          meta: row.meta,
          // Read where it is drawn, not here: each standing that lands then
          // redraws its own row, not every row (#2200's class: a thousand
          // branches rebuilt the list a thousand times).
          get standing() {
            const at = row.entity_id ?? unsynced.get(row.title) ?? null;
            return at !== null ? (standings[at] ?? null) : null;
          },
          // The name the record gives the tip's author, else the account (F-107); `mine` compares the account.
          author: row.entity_id !== null ? (facts[row.entity_id]?.author_name ?? facts[row.entity_id]?.author ?? null) : null,
          mine: row.entity_id !== null && facts[row.entity_id]?.mine === true,
        }),
      ),
      ...[...made.keys()].filter((name) => !branches.some((row) => row.title === name)).map(
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
      const write = await ports.submitWrite({ DeleteBranch: { entity: row.entityId } });
      echoWrite(repoId, DELETED, row.name, { write, many: true, following: { name: `Delete of ${row.name}` }, surface: REPO_BRANCHES, source: source?.id ?? null });
      toastQueued(`delete branch ${row.name}`, source, { write });
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
<TfHeldNote {heldAt} source={sourceName(source)} failed={heldFailed} />
<TfWaiting phase={waitPhase({ drawn: listed !== null, reading, failed: failed !== null })} error={failed} columns={6}>
{#if shown.length === 0}
  <TfNoMatch noun="branches" {filter} />
{:else}
  <table class="tf-tbl tf-rbranches" bind:this={table}>
    <thead>
      <tr><th>Branch</th><th>Against {defaultName}</th><th>Pull request</th><th>By</th><th>Updated</th><th></th></tr>
    </thead>
    <tbody>
      {#each shown as row (row.name)}
        {@const standing = row.standing}
        {@const gone = echoesOn(repoId, DELETED).find((echo) => echo.text === row.name)?.state ?? null}
        <tr data-place>
          <td title={row.name}
            ><span class="tf-mono"><Marked text={row.name} typed={filter.text} /></span>{#if row.isDefault}{" "}<TfStateChip text="default" />{/if}{#if made.has(row.name)}{" "}<TfStateChip text={made.get(row.name)} tone="info" />{/if}{#if gone !== null}{" "}<TfStateChip text="delete {gone}" tone="info" />{/if}</td
          >
          <td
            >{#if standing?.merged}<TfStateChip text="merged" tone="ok" />{:else}{against(standing)}{/if}</td
          >
          <td
            >{#if standing?.pr}{#if standing.pr.entityId !== null}<a href={itemHash({ entity_id: standing.pr.entityId, kind: "pr" })}
                  >{standing.pr.number ?? ""}</a
                >{:else}{standing.pr.number ?? ""}{/if}{#if standing.pr.state}{" "}<TfStateChip text={standing.pr.state} tone={standing.pr.tone ?? "info"} />{/if}{:else if prSent.has(row.name)}<a class="tf-sentlink" href={repoHash(repoId, "prs")} title="Show in Pull requests" onclick={(event) => jumpAcross(event, "Branches")}><TfStateChip text={prSent.get(row.name)} tone="info" /></a>{:else if standing !== null}<span class="tf-dim">none</span>{/if}</td
          >
          <td>{row.mine ? "you" : (row.author ?? "")}</td>
          <td class="tf-dim tf-nowrap"><span class="tf-rowval">{row.meta ?? ""}</span></td>
          <td class="tf-nowrap tf-racts"
            ><span class="tf-rowend">{#if standing !== null && !gone}{#if standing.canOpenPr && !prSent.has(row.name)}<a
                  class="tf-btn tf-sm"
                  href={compareHash(repoId, row.name)}
                  onclick={(event) => jumpAcross(event, "Branches")}>New pull request</a
                >{" "}{/if}{#if standing.deletable && row.entityId !== null}<span class="tf-rowacts"><button
                  type="button"
                  class="tf-btn tf-sm tf-danger"
                  aria-disabled={standing.deleteBlocked !== null ? "true" : undefined}
                  title={standing.deleteBlocked ?? undefined}
                  onclick={() => {
                    if (standing.deleteBlocked === null) deleting = row;
                  }}>Delete…</button
                ></span>{/if}{/if}</span></td
          >
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
</TfWaiting>
{#each notes as note, n (n)}
  <p class="tf-dim tf-small tf-rbranches-note">{note.title}</p>
{/each}

{#if deleting !== null && deleting.standing !== null}
  {@const standing = deleting.standing}
  {@const row = deleting}
  <TfDialog onclose={() => (deleting = null)} className="tf-rdel">
    {#snippet title()}Delete <TfKey value={row.name} />?{/snippet}
    {#snippet body()}
      {#if standing.merged}
        <p>Every commit on this branch is in {defaultName}. Nothing is lost.</p>
      {:else}
        <div class="tf-banner tf-warn tf-dbanner">
          {plural(standing.ahead ?? 0)} not in {defaultName} and will be lost.
        </div>
      {/if}
      {#if standing.pr !== null}
        <p class="tf-dim">PR {standing.pr.number ?? ""} is open from this branch. {source ? sourceName(source) : "The code host"} closes it when the branch goes.</p>
      {/if}
      <p class="tf-dim tf-note">This deletes the branch on {sourceName(source)}. Your local checkout keeps its copy.</p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (deleting = null)}>Cancel</button>
      <button
        type="button"
        class="tf-btn tf-primary tf-danger"
        disabled={busy}
        onclick={confirmDelete}>Delete branch</button>
    {/snippet}
  </TfDialog>
{/if}
