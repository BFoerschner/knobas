<!--
  The compare page (#840, R-8.11 to R-8.14), prototype-6's `renderCompare`:
  `#/repo/<repo entity id>/compare/<branch>` under the *Pull requests* tab.

  - **The form**: *Into* (the default branch first) and *From* (changing it
    changes the address, so the page reads the other branch); *Title*
    prefilled `<key>: <summary>` and *Description* with the ticket's
    description ending `Closes <key>.` where the branch name holds a key the
    mirror has (`proposal` in `repo.ts`), and then *Link the PR to <key> in
    knobas* (#984), ticked: `submit_write` links the pull request to that
    ticket once it has gone and a sync has brought it in. *Create pull
    request* queues `CreatePullRequest` in the repository and lands on its
    pull requests.
  - **A pull request already open** from the branch is the source's overview's
    row: the page says so and offers *Open PR #N* instead.
  - **What it changes**: the diff from *Into* to *From* (#2271), the
    repository's `compare:<into>...<from>` section, drawn by the one diff
    component (`TfDiffView`): picking another *Into* reads it against that
    base. *Open as diff* opens the same diff at its own address. Beside it,
    from the branch's own view, what it carries over the default branch (its
    *Commits*).

  A branch the mirror does not hold yet (one this session's edit queued) has
  no view to read: the form still opens, since the pull request names the
  branch by name, and the change says when it will show.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { DiffFile, DiffRange } from "../../ipc/entity";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection, diffHash } from "../diff/address";
  import { rangeContext } from "../diff/context";
  import { itemHash, resolveTicket } from "../job/job";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import {
    branchStanding,
    compareHash,
    keyIn,
    proposal,
    repoHash,
    sourceName,
    type BranchOption,
    type BranchStanding,
    type ProposedFrom,
  } from "./repo";

  let {
    ports,
    source = null,
    repoId,
    branches,
    head,
    defaultBranch,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "listSources" | "submitWrite" | "fileBytes" | "piped">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    repoId: string;
    branches: readonly BranchOption[];
    /** The branch proposed, from the address. */
    head: string | null;
    defaultBranch: string | null;
    /** The page's refreshes (#2135): each reads the branch's standing and diff again over what the page drew. */
    refreshed?: number;
  } = $props();

  const listed = $derived(branches.find((option) => option.name === head) ?? null);
  /** The branch's entity: a refresh's new listing of the same branch reads nothing again on its own. */
  const listedId = $derived(listed?.entityId ?? null);
  const into = $derived(defaultBranch ?? branches[0]?.name ?? "");

  let standing = $state<BranchStanding | null>(null);
  /** What the branch changes: another branch's stays drawn, dimmed, while this one's is read (#2246). */
  let files = $state<DiffFile[] | null>(null);
  /** The section `files` was read for: until another base's diff answers, the page still draws the last one. */
  let filesFor = $state<string | null>(null);
  let reading = $state(false);
  /** The diff hit the parser's ceiling. */
  let cut = $state(false);
  /** What the compare's diff is between, where the source settled it (#2310): context reads at its shas. */
  let range = $state<DiffRange | null>(null);
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes, ports.piped));
  let failed = $state<string | null>(null);
  /** *Into*, the default branch proposed once: a new *From* keeps what the reader picked. */
  let base = $state(untrack(() => into));
  let title = $state("");
  let body = $state("");
  /** The ticket the branch's key names, and that key: what *Link the PR to <key>* links to (#984). */
  let ticket = $state<{ key: string; id: string } | null>(null);
  let link = $state(true);
  let busy = $state(false);

  /** The diff read: from *Into* to the branch, once the mirror holds the branch. */
  const section = $derived(listedId === null || head === null || base === "" ? null : compareSection(base, head));

  // The branch's view, read for each branch the address names.
  $effect(() => {
    const id = listedId;
    let live = true;
    standing = null;
    failed = null;
    if (id === null) return;
    readStanding(id, () => live);
    return () => {
      live = false;
    };
  });

  // Its diff, read again for each base and branch.
  $effect(() => {
    const asked = section;
    let live = true;
    failed = null;
    if (asked === null) return;
    readDiff(asked, () => live);
    return () => {
      live = false;
    };
  });

  // A refresh of the page reads the branch again, over what the page drew.
  onRefreshed(
    () => refreshed,
    () => {
      const id = listedId;
      const asked = section;
      if (id === null || asked === null) return Promise.resolve();
      return Promise.all([readStanding(id, () => listedId === id, true), readDiff(asked, () => section === asked, true)]);
    },
  );

  /** The branch's standing; `again` keeps what the page drew when a read fails. */
  function readStanding(id: string, alive: () => boolean, again = false): Promise<unknown> {
    return ports.entityView(id, null).then(
      (answer) => {
        if (alive()) standing = branchStanding(answer);
      },
      (error: unknown) => {
        if (alive() && !again) failed = ipcErrorMessage(error);
      },
    );
  }

  /** The diff `asked` names; `again` keeps what the page drew when a read fails. */
  function readDiff(asked: string, alive: () => boolean, again = false): Promise<unknown> {
    const fail = (error: unknown) => {
      if (alive() && !again) failed = ipcErrorMessage(error);
    };
    reading = true;
    return ports.entityView(repoId, asked).then((view) => {
      if (!alive()) return;
      reading = false;
      const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
      files = block?.type === "diff" ? block.files : [];
      cut = block?.type === "diff" && block.truncated;
      range = block?.type === "diff" ? (block.range ?? null) : null;
      filesFor = asked;
    }, (error) => {
      if (!alive()) return;
      reading = false;
      if (again) return;
      fail(error);
      files = null;
    });
  }

  // The form, proposed for each branch: from its ticket where it names one.
  // Only a new branch proposes anew; nothing else may wipe what was typed,
  // and *Into* is not the branch's to propose.
  $effect(() => {
    const name = head ?? "";
    let live = true;
    ({ title, body } = proposal(name, null));
    ticket = null;
    link = true;
    const key = keyIn(name);
    if (key === null) return;
    void (async () => {
      try {
        const found = await resolveTicket(key, await ports.listSources(), ports.getEntity);
        if (!live || found === null) return;
        const from: ProposedFrom = { key, summary: found.row.title, description: found.body_text };
        ({ title, body } = proposal(name, from));
        ticket = { key, id: found.row.entity_id };
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
  /** The files changed: this base's diff's, or before it is read the branch's own count, which is against the default branch. */
  const changed = $derived(
    files !== null && filesFor === section ? files.length : base === defaultBranch ? (standing?.files ?? null) : null,
  );
  /** Why the source draws the pull request disabled (#2030): its words, on the button. */
  const refused = $derived(standing?.prBlocked ?? null);
  const ready = $derived(head !== null && title.trim() !== "" && base !== "" && base !== head && !busy && refused === null);

  async function create() {
    if (!ready || head === null) return;
    busy = true;
    const words = title.trim();
    try {
      const linkTo = ticket !== null && link ? ticket.id : undefined;
      await ports.submitWrite({ CreatePullRequest: { entity: repoId, title: words, body, head, base } }, linkTo);
      toastQueued(`open PR ${words}`, source);
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
        {#if ticket !== null}
          <span></span><label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link the PR to {ticket.key} in knobas</label>
        {/if}
        <span></span><span class="tf-dim tf-small">Reviewers are asked on {sourceName(source)} after it opens.</span>
      </div>
      <div class="tf-cgo">
        {#if standing?.pr}
          <span class="tf-chip tf-info">PR {standing.pr.number ?? ""} is already open from this branch</span>
          {#if standing.pr.entityId !== null}
            <a class="tf-btn" href={itemHash({ entity_id: standing.pr.entityId, kind: "pr" })}>Open PR {standing.pr.number ?? ""}</a>
          {/if}
        {:else}
          <button
            type="submit"
            class="tf-btn tf-primary"
            disabled={!ready && refused === null}
            aria-disabled={refused !== null ? "true" : undefined}
            title={refused ?? undefined}>Create pull request</button>
        {/if}
        {#if changed !== null}
          <span class="tf-dim tf-small tf-cfiles">{changed} {changed === 1 ? "file" : "files"} changed against {base}</span>
        {/if}
      </div>
    </form>
    <h2 class="tf-cgroup-h tf-cchanges">
      What it changes
      {#if section !== null && head !== null}
        <span class="tf-acts"
          ><a class="tf-btn tf-sm tf-ghost" href={diffHash(repoId, base, head)}>Open as diff</a><TfOpenObject
            entityId={repoId}
            object={{ kind: "range", base, head }}
          /></span
        >
      {/if}
    </h2>
    {#if listed === null || listed.entityId === null}
      <div class="tf-empty">This branch is not on {sourceName(source)} yet, or knobas has not synced it. What it changes shows here once it has.</div>
    {:else}
      <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
        {#if files !== null && files.length === 0}
          <div class="tf-empty">This branch has no commits that {base} lacks.</div>
        {:else if files !== null}
          {#if range === null}
            <p class="tf-cnote" data-testid="no-range">Context does not expand here: the source did not settle which two commits this diff is between, as for a range from the repository's first commit or a branch that merged its base back in.</p>
          {/if}
          {#key `${filesFor}\n${range?.base}\n${range?.head}`}
            <TfDiffView {files} {cut} {context} openAt={head === null ? null : { entityId: repoId, at: head }} />
          {/key}
        {/if}
      </TfWaiting>
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
