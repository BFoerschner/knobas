<!--
  The compare page (#840, R-8.11 to R-8.14), prototype-6's `renderCompare`:
  `#/repo/<repo entity id>/compare/<branch>` under the *Pull requests* tab,
  or `compare/<base>...<branch>` with *Into* named too (#2427, the
  launcher's *Create pull request…*).

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
    row: *Into* and *From* stay, so another branch can be picked, and one line
    with *Open PR #N* stands where the rest of the form would.
  - **One branch**: nothing to compare, so one empty state with *New branch…*.
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
  import { proseMarkdown } from "../../render/prose";
  import type { DiffFile, DiffRange } from "../../ipc/entity";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import TfKey from "../TfKey.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection, diffHash, rangeDiffVia } from "../diff/address";
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
    base: named = null,
    defaultBranch,
    refreshed = 0,
    branchRefused = null,
    onNewBranch = () => {},
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "listSources" | "submitWrite" | "fileBytes" | "piped" | "rangeDiff">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    repoId: string;
    branches: readonly BranchOption[];
    /** The branch proposed, from the address. */
    head: string | null;
    /** *Into*, where the address names it; one the repository does not list proposes the default branch. */
    base?: string | null;
    defaultBranch: string | null;
    /** The page's refreshes (#2135): each reads the branch's standing and diff again over what the page drew. */
    refreshed?: number;
    /** Why the source draws *New branch…* disabled, where it does. */
    branchRefused?: string | null;
    /** Opens *New branch…*, which is what a repository of one branch can do here. */
    onNewBranch?: () => void;
  } = $props();

  /** One branch or none, and the address names no other: there is nothing to compare. */
  const lone = $derived(branches.length <= 1 && (head === null || branches.some((option) => option.name === head)));
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
  const addressed = $derived(named !== null && branches.some((option) => option.name === named) ? named : null);
  let base = $state(untrack(() => addressed ?? into));
  // An address that names another base (the launcher, while this page is open) moves *Into* to it.
  $effect.pre(() => {
    const asked = addressed;
    if (asked !== null) untrack(() => (base = asked));
  });
  let title = $state("");
  let body = $state("");
  /** The ticket the branch's key names, and that key: what *Link the PR to <key>* links to (#984). */
  let ticket = $state<{ key: string; id: string } | null>(null);
  let link = $state(true);
  let busy = $state(false);

  /** The diff read: from *Into* to the branch, once the mirror holds the branch and they are two branches. */
  const section = $derived(listedId === null || head === null || base === "" || base === head ? null : compareSection(base, head));
  /**
   * What *Open as diff* and *Open in…* name (Spec 5 of the review of spec #2234): the two commits the source settled
   * the drawn compare between, so the diff page can read it at a checkout that holds both (ADR-0036; a branch name is
   * never read locally); the branch names until the compare has answered, or where it settled none.
   */
  const ends = $derived(range !== null && filesFor === section && head !== null ? { base: range.base, head: range.head } : { base, head: head ?? "" });

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
    const [from, to] = [base, head ?? ""];
    // Local-first, as every diff (ADR-0036, Spec 6 of the review of spec #2234): branch names are the source's, and
    // its compare names the two commits (`range`) that *Open as diff* takes to a checkout.
    return (ports.rangeDiff ?? rangeDiffVia(ports.entityView))(repoId, from, to, false).then((diff) => {
      if (!alive()) return;
      reading = false;
      files = diff.files;
      cut = diff.truncated;
      range = diff.range;
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
        // The ticket's description, not its search text, written as Markdown
        // for the pull request's body (#2308).
        const description = found.description === null ? "" : (proseMarkdown(found.description, found.markup) ?? found.description);
        const from: ProposedFrom = { key, summary: found.row.title, description };
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
  /** What the primary waits for, in words beside it (design system, the enabled rule). */
  const needs = $derived(refused ?? (base === head ? "Pick two different branches." : title.trim() === "" ? "Give it a title." : null));
  const commits = $derived(standing?.commits ?? []);

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

{#if lone}
  <div class="tf-empty">
    {#if branches.length === 0}
      <p>This repository has no branches yet, so there is nothing to compare.</p>
    {:else}
      <p>This repository has one branch, <TfKey value={branches[0]!.name} />, so there is nothing to compare.</p>
      <button
        type="button"
        class="tf-btn tf-sm"
        aria-disabled={branchRefused !== null ? "true" : undefined}
        title={branchRefused ?? undefined}
        onclick={() => {
          if (branchRefused === null) onNewBranch();
        }}>New branch…</button>
    {/if}
  </div>
{:else}
<div class="tf-compare">
  <div>
    <h2 class="tf-cgroup-h">{standing?.pr ? "Compare" : "New pull request"}</h2>
    <form
      class="tf-panel tf-cform"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <div class="tf-formgrid">
        <label for="tf-cmp-into">Into</label>
        <select id="tf-cmp-into" class="tf-cbranch" bind:value={base}>
          {#each branches as option (option.name)}<option value={option.name}>{option.name}</option>{/each}
        </select>
        <label for="tf-cmp-from">From</label>
        <select
          id="tf-cmp-from"
          class="tf-cbranch"
          value={head ?? ""}
          onchange={(event) => (location.hash = compareHash(repoId, (event.currentTarget as HTMLSelectElement).value, base === into ? null : base))}
        >
          {#each heads as name (name)}<option value={name}>{name}</option>{/each}
        </select>
        {#if !standing?.pr}
          <label for="tf-cmp-title">Title</label>
          <input id="tf-cmp-title" type="text" autocomplete="off" bind:value={title} />
          <label for="tf-cmp-body">Description</label>
          <textarea id="tf-cmp-body" rows="5" bind:value={body}></textarea>
          {#if ticket !== null}
            <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link the PR to <TfKey value={ticket.key} /> in knobas</label>
          {/if}
          <span class="tf-fwide tf-dim tf-small">Reviewers are asked on {sourceName(source)} after it opens.</span>
        {/if}
      </div>
      <div class="tf-cgo">
        {#if standing?.pr}
          <span class="tf-cexists">PR {standing.pr.number ?? ""} is already open from this branch.</span>
          {#if standing.pr.entityId !== null}
            <a class="tf-btn tf-sm" href={itemHash({ entity_id: standing.pr.entityId, kind: "pr" })}>Open PR {standing.pr.number ?? ""}</a>
          {/if}
        {:else}
          <button
            type="submit"
            class="tf-btn tf-sm tf-primary"
            disabled={!ready && refused === null}
            aria-disabled={refused !== null ? "true" : undefined}
            title={refused ?? undefined}>Create pull request</button>
          {#if needs !== null}<span class="tf-dim tf-small">{needs}</span>{/if}
        {/if}
        {#if changed !== null}
          <span class="tf-dim tf-small tf-cfiles">{changed} {changed === 1 ? "file" : "files"} changed against {base}</span>
        {/if}
      </div>
    </form>
    <h2 class="tf-cgroup-h tf-cchanges">
      What it changes
      {#if section !== null && head !== null}
        <span class="tf-trail"
          ><a class="tf-btn tf-sm tf-ghost" href={diffHash(repoId, ends.base, ends.head)}>Open as diff</a><TfOpenObject
            entityId={repoId}
            object={{ kind: "range", base: ends.base, head: ends.head }}
          /></span
        >
      {/if}
    </h2>
    {#if base === head}
      <div class="tf-empty">Into and From are the same branch, so there is nothing to compare.</div>
    {:else if listed === null || listed.entityId === null}
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
      {#if commits.length > 0}
        <div class="tf-rows">
          {#each commits as commit, n (n)}
            <div class="tf-r"><span class="tf-mono tf-dim">{commit.chip ?? ""}</span><span class="tf-t tf-t2" title={commit.title}>{commit.title}</span></div>
          {/each}
        </div>
      {:else if standing !== null}
        <p class="tf-prnote tf-cnone">Nothing that {defaultBranch ?? "the default branch"} lacks.</p>
      {/if}
    </div>
  </aside>
</div>
{/if}
