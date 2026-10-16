<!--
  The compare page (#840, R-8.11 to R-8.14), prototype-6's `renderCompare`:
  `#/repo/<repo entity id>/compare/<branch>` under the *Pull requests* tab.

  - **The form**: *Into* (the default branch first) and *From* (changing it
    changes the address, so the page reads the other branch); *Title*
    prefilled `<key>: <summary>` and *Description* with the ticket's
    description ending `Closes <key>.` where the branch name holds a key the
    mirror has (`proposal` in `repo.ts`). *Create pull request* queues
    `CreatePullRequest` in the repository and lands on its pull requests.
  - **A pull request already open** from the branch is the source's overview's
    row: the page says so and offers *Open PR #N* instead.
  - **What it changes**: the branch's own view, its deferred *Diff* (Gitea's
    `compare_diff` against the default branch) drawn by `TfDiff`, and beside
    it what it carries (its *Commits*).

  A branch the mirror does not hold yet (one this session's edit queued) has
  no view to read: the form still opens, since the pull request names the
  branch by name, and the change says when it will show.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { DiffFile } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import TfDiff from "../code/TfDiff.svelte";
  import { resolveTicket } from "../job/job";
  import type { RepoPorts } from "./TfRepo.svelte";
  import {
    branchStanding,
    compareHash,
    keyIn,
    proposal,
    repoHash,
    type BranchOption,
    type BranchStanding,
    type ProposedFrom,
  } from "./repo";

  let {
    ports,
    repoId,
    branches,
    head,
    defaultBranch,
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "listSources" | "submitWrite">;
    repoId: string;
    branches: readonly BranchOption[];
    /** The branch proposed, from the address. */
    head: string | null;
    defaultBranch: string | null;
  } = $props();

  const listed = $derived(branches.find((option) => option.name === head) ?? null);
  const into = $derived(defaultBranch ?? branches[0]?.name ?? "");

  let standing = $state<BranchStanding | null>(null);
  let files = $state<DiffFile[] | null>(null);
  let failed = $state<string | null>(null);
  let base = $state("");
  let title = $state("");
  let body = $state("");
  let busy = $state(false);

  // The branch's view and its diff, read for each branch the address names.
  $effect(() => {
    const id = listed?.entityId ?? null;
    let live = true;
    standing = null;
    files = null;
    failed = null;
    if (id === null) return;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports.entityView(id, null).then((view) => {
      if (live) standing = branchStanding(view);
    }, fail);
    ports.entityView(id, "diff").then((view) => {
      if (!live) return;
      const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
      files = block?.type === "diff" ? block.files : [];
    }, fail);
    return () => {
      live = false;
    };
  });

  // The form, proposed for each branch: from its ticket where it names one.
  $effect(() => {
    const name = head ?? "";
    let live = true;
    base = into;
    ({ title, body } = proposal(name, null));
    const key = keyIn(name);
    if (key === null) return;
    void (async () => {
      try {
        const ticket = await resolveTicket(key, await ports.listSources(), ports.getEntity);
        if (!live || ticket === null) return;
        const from: ProposedFrom = { key, summary: ticket.row.title, description: ticket.body_text ?? "" };
        ({ title, body } = proposal(name, from));
      } catch {
        // No ticket to propose from: the branch's own name stands.
      }
    })();
    return () => {
      live = false;
    };
  });

  const heads = $derived.by(() => {
    const names = branches.filter((option) => !option.isDefault).map((option) => option.name);
    return head !== null && !names.includes(head) ? [head, ...names] : names;
  });
  const changed = $derived(files?.length ?? standing?.files ?? null);
  const ready = $derived(head !== null && title.trim() !== "" && base !== "" && base !== head && !busy);

  async function create() {
    if (!ready || head === null) return;
    busy = true;
    const words = title.trim();
    try {
      await ports.submitWrite({ CreatePullRequest: { entity: repoId, title: words, body, head, base } });
      push({
        text: `Queued: open PR ${words}. Sends with the next sync.`,
        action: { label: "Show queue", run: openWriteQueue },
      });
      location.hash = repoHash(repoId, "prs");
    } catch (cause) {
      push({ text: `Could not queue the pull request: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<div class="tf-compare">
  <div>
    <h2 class="tf-cgroup-h">New pull request</h2>
    <form
      class="tf-panel tf-cform"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <div class="tf-formgrid">
        <label for="tf-cmp-into">Into</label>
        <select id="tf-cmp-into" bind:value={base}>
          {#each branches as option (option.name)}<option value={option.name}>{option.name}</option>{/each}
        </select>
        <label for="tf-cmp-from">From</label>
        <select
          id="tf-cmp-from"
          value={head ?? ""}
          onchange={(event) => (location.hash = compareHash(repoId, (event.currentTarget as HTMLSelectElement).value))}
        >
          {#each heads as name (name)}<option value={name}>{name}</option>{/each}
        </select>
        <label for="tf-cmp-title">Title</label>
        <input id="tf-cmp-title" type="text" autocomplete="off" bind:value={title} />
        <label for="tf-cmp-body" class="tf-ctop">Description</label>
        <textarea id="tf-cmp-body" rows="5" bind:value={body}></textarea>
        <span></span><span class="tf-dim tf-small">Reviewers are asked on Gitea after it opens.</span>
      </div>
      <div class="tf-cgo">
        {#if standing?.pr}
          <span class="tf-chip tf-info">PR {standing.pr.number ?? ""} is already open from this branch</span>
          {#if standing.pr.entityId !== null}
            <a class="tf-btn" href="#/pr/{encodeURIComponent(standing.pr.entityId)}">Open PR {standing.pr.number ?? ""}</a>
          {/if}
        {:else}
          <button type="submit" class="tf-btn tf-primary" disabled={!ready}>Create pull request</button>
        {/if}
        {#if changed !== null}
          <span class="tf-dim tf-small tf-cfiles">{changed} {changed === 1 ? "file" : "files"} changed against {defaultBranch ?? into}</span>
        {/if}
      </div>
    </form>
    <h2 class="tf-cgroup-h tf-cchanges">What it changes</h2>
    {#if failed !== null}
      <p class="tf-cfail">{failed}</p>
    {:else if listed === null || listed.entityId === null}
      <div class="tf-empty">This branch is not on Gitea yet, or knobas has not synced it. What it changes shows here once it has.</div>
    {:else if files !== null && files.length === 0}
      <div class="tf-empty">This branch has no commits that {defaultBranch ?? into} lacks.</div>
    {:else if files !== null}
      <TfDiff {files} />
    {/if}
  </div>
  <aside class="tf-side">
    <div class="tf-panel">
      <header><h3>Commits</h3></header>
      <div class="tf-rows">
        {#each standing?.commits ?? [] as commit, n (n)}
          <div class="tf-r"><span class="tf-mono tf-dim">{commit.chip ?? ""}</span><span class="tf-t">{commit.title}</span></div>
        {/each}
      </div>
    </div>
  </aside>
</div>
