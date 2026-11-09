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
  - **A pull request this session sent** from the branch (F-257): the same,
    with the line saying so and the way to its row, so a second press can't
    send it again. A write that won't go drops its echo, and the form is back
    with what was typed.
  - **One branch**: nothing to compare, so one empty state with *New branch…*.
  - **What it changes**: the diff from *Into* to *From* (#2271), the
    repository's `compare:<into>...<from>` section, drawn by the one diff
    component (`TfDiffView`), across the page under the form and *Commits*:
    picking another *Into* reads it against that base. *Open as diff* opens the
    same diff at its own address. Beside the form, from the branch's own view,
    what it carries over the default branch (its *Commits*).
  - **Into and From the same**: nothing to compare, so one line says so where the
    form ends, and there is no diff, no *Commits* and no open pull request's line.

  **Predicted at the checkout, checked behind** (ADR-0041, the smoothness
  pass of PR #2479): where the repository's checkout holds the tips the
  mirror last synced for *Into* and *From*, the diff is read there at once
  (`entity_range_diff` answers `predicted_at`) and drawn, and the source's
  compare is asked behind it (`entity_source_range_diff`). The same, and a
  line says it was checked; otherwise a branch moved since the sync, the
  source's diff replaces it in place and a line says so. *Create pull
  request* never waits for the check: a pull request sent before it
  answered and that the check then finds different is told in a toast.

  A branch the mirror does not hold yet (one this session's edit queued) has
  no view to read: the form still opens, since the pull request names the
  branch by name, and the change says when it will show.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { proseMarkdown } from "../../render/prose";
  import type { DiffFile, DiffRange, RangeDiff } from "../../ipc/entity";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { escThroughUntouched } from "../escape";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection, diffHash, rangeDiffVia } from "../diff/address";
  import { rangeContext } from "../diff/context";
  import { itemHash, resolveTicket } from "../job/job";
  import { echoWrite, echoesOn, watchEchoes } from "../queued.svelte";
  import { placeRestoring } from "../return-place";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { onViewFresh } from "./view-fresh.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import {
    branchStanding,
    compareHash,
    keyIn,
    MADE_PR,
    proposal,
    REPO_PRS,
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
    shown = true,
    guessed = null,
    onNewBranch = () => {},
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "listSources" | "submitWrite" | "fileBytes" | "piped" | "rangeDiff" | "sourceRangeDiff" | "listen">;
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
    /** Whether this tab is the one shown: arriving on it, *Title* takes focus if nothing else has it. */
    shown?: boolean;
    /** The branch the header's *New pull request* chose for the reader, and why, which the page says above the form (F-063). */
    guessed?: { branch: string; why: string } | null;
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
  /**
   * The drawn diff was read at the checkout between the tips the mirror last synced (ADR-0041): `checked` once the
   * source's compare answered the same. `null` for a diff the source answered.
   */
  let predicted = $state<{ section: string; checked: boolean } | null>(null);
  /** The source's compare differed from the predicted diff: a branch moved since the sync, and its diff now stands. */
  let moved = $state<string | null>(null);
  /** The check of the drawn predicted diff, by its section: whether the source's compare differed. */
  let checking: { section: string; differs: Promise<boolean> } | null = null;
  /** *Into*, the default branch proposed once: a new *From* keeps what the reader picked. */
  const addressed = $derived(named !== null && branches.some((option) => option.name === named) ? named : null);
  let base = $state(untrack(() => addressed ?? into));
  // An address that names another base (the launcher, while this page is open) moves *Into* to it.
  $effect.pre(() => {
    const asked = addressed;
    if (asked !== null) untrack(() => (base = asked));
  });
  /** *Into* and *From* are one branch: there is nothing to compare, and no diff, commits or open pull request to speak of. */
  const same = $derived(base === head);
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

  /**
   * The pull request this session sent from this branch, until the source's view holds it (F-257). Not while its own press is
   * leaving: the form stays as the tab hides, so *Pull requests* can take focus from it for the row it landed on (F-242).
   */
  const sentFrom = $derived(head === null || busy ? undefined : echoesOn(repoId, MADE_PR).find((echo) => echo.base === head));
  $effect(() => {
    if (!shown) busy = false;
  });
  // Its landing reads the branch again, so the line it leaves is the source's own *Open PR*, not the form.
  $effect(() =>
    untrack(() =>
      watchEchoes(
        REPO_PRS,
        () => {
          const id = listedId;
          if (id !== null) void readStanding(id, () => listedId === id, true);
        },
        { listen: ports.listen },
      ),
    ),
  );

  /**
   * The branch's standing. It adds the commits list and an open pull request's line to a form that can be sent
   * without them, so a source that does not answer leaves the page as drawn (ADR-0042): the diff is read on its own.
   */
  function readStanding(id: string, alive: () => boolean, again = false): Promise<unknown> {
    void again;
    return ports.entityView(id, null).then(
      (answer) => {
        if (alive()) standing = branchStanding(answer);
      },
      () => {},
    );
  }

  // The standing knobas held, read again behind it (ADR-0042).
  onViewFresh(
    () => listedId,
    null,
    () => {
      const id = listedId;
      if (id !== null) void readStanding(id, () => listedId === id, true);
    },
    untrack(() => ports.listen),
  );

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
      moved = null;
      predicted = null;
      checking = null;
      if (diff.predicted_at !== undefined && ports.sourceRangeDiff !== undefined) checkPredicted(asked, from, to, diff, alive);
    }, (error) => {
      if (!alive()) return;
      reading = false;
      if (again) return;
      fail(error);
      files = null;
    });
  }

  /** Whether two answers for one range are the same diff: the same two commits where both name them, else the same files and counts. */
  function sameDiff(a: RangeDiff, b: RangeDiff): boolean {
    if (a.range !== null && b.range !== null) return a.range.base === b.range.base && a.range.head === b.range.head;
    const shape = (d: RangeDiff) => JSON.stringify(d.files.map((f) => [f.path, f.additions, f.deletions]));
    return shape(a) === shape(b);
  }

  /** The source's compare, asked behind a predicted diff: the same checks it; another replaces it in place, and says so. */
  function checkPredicted(asked: string, from: string, to: string, shown: RangeDiff, alive: () => boolean) {
    const ask = ports.sourceRangeDiff;
    if (ask === undefined) return;
    predicted = { section: asked, checked: false };
    const differs = ask(repoId, from, to).then(
      (fresh) => {
        const same = sameDiff(shown, fresh);
        if (alive() && filesFor === asked) {
          if (same) predicted = { section: asked, checked: true };
          else {
            files = fresh.files;
            cut = fresh.truncated;
            range = fresh.range;
            predicted = null;
            moved = `${to} or ${from} moved on ${sourceName(source)} since the last sync: this is the change as it stands there now.`;
          }
        }
        return !same;
      },
      // The source could not answer: the prediction stands, unchecked.
      () => false,
    );
    checking = { section: asked, differs };
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
        selectTitle();
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
  const ready = $derived(head !== null && title.trim() !== "" && base !== "" && base !== head && !busy && refused === null && sentFrom === undefined);
  /** What the primary waits for, in words beside it (design system, the enabled rule). */
  const needs = $derived(same ? "Into and From are the same branch, so there is nothing to compare." : (refused ?? (title.trim() === "" ? "Give it a title." : null)));
  const commits = $derived(standing?.commits ?? []);

  // One commit says what the change is better than its branch's name does (F-241). Only over the name proposed: typed words stay.
  $effect(() => {
    const only = commits.length === 1 ? (commits[0]?.title ?? "") : "";
    if (only === "" || ticket !== null) return;
    untrack(() => {
      if (title === proposal(head ?? "", null).title) {
        title = only;
        selectTitle();
      }
    });
  });

  /** *Title*, ready to type over, unless the reader already has focus somewhere of their own. A press on *New pull request* leaves it on nothing. */
  let titleField = $state<HTMLInputElement | null>(null);
  function selectTitle(): void {
    void tick().then(() => {
      if (placeRestoring()) return;
      const at = document.activeElement;
      const free = at === null || at === document.body || at === titleField || at.closest("[inert], [hidden]") !== null;
      if (!free || titleField === null || titleField.getClientRects().length === 0 || !untrack(() => shown)) return;
      titleField.focus({ preventScroll: true });
      titleField.select();
      titleGivenAt = performance.now();
    });
  }
  /**
   * *New pull request* by key lands here with *Title* focused, so the second `Enter` of a double press would send the form. A
   * pull request is not undone by a toast, so an `Enter` this soon after the app gave it focus, or one held down, is not taken as sending it.
   */
  const ARRIVING_MS = 800;
  let titleGivenAt = -ARRIVING_MS;
  function holdArrivingEnter(form: HTMLFormElement) {
    const hold = (event: KeyboardEvent) => {
      if (event.key === "Enter" && event.target === titleField && (event.repeat || performance.now() - titleGivenAt < ARRIVING_MS)) event.preventDefault();
    };
    form.addEventListener("keydown", hold);
    return () => form.removeEventListener("keydown", hold);
  }
  // On arriving at the form, and again if a ticket's words replace the proposal below.
  $effect(() => {
    if (shown && titleField !== null) untrack(selectTitle);
  });

  async function create() {
    if (!ready || head === null) return;
    busy = true;
    const words = title.trim();
    try {
      const linkTo = ticket !== null && link ? ticket.id : undefined;
      const write = await ports.submitWrite({ CreatePullRequest: { entity: repoId, title: words, body, head, base } }, linkTo);
      const prs = repoHash(repoId, "prs");
      // The press lands on the tab that shows it, so the toast has nowhere to offer.
      toastQueued(`open PR ${words}`, source, { write });
      // The row it lands on, so the tab doesn't read as if nothing happened (F-242).
      echoWrite(repoId, MADE_PR, words, { write, base: head, many: true, following: { name: `Pull request ${words}` }, surface: REPO_PRS, source: source?.id ?? null });
      // Sent before the check of what the page predicted answered: if the source's compare then differs, the pull
      // request holds the branches as they stand there now, not as the page drew them, and the reader is told.
      const pending = checking !== null && predicted?.section === checking.section && !predicted.checked ? checking.differs : null;
      const from = head;
      void pending?.then((differs) => {
        if (differs)
          push({
            text: `“${words}” was opened from ${from} as ${sourceName(source)} holds it now, which moved since the last sync: its changes differ from the ones this page showed. Review its files.`,
            action: { label: "Pull requests", run: () => (location.hash = prs) },
          });
      });
      location.hash = prs;
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
    <h2 class="tf-cgroup-h">{standing?.pr || sentFrom !== undefined ? "Compare" : "New pull request"}</h2>
    {#if guessed !== null && guessed.branch === head && !standing?.pr && sentFrom === undefined}
      <p class="tf-cstart">Starting from <span class="tf-mono">{guessed.branch}</span>, {guessed.why}.</p>
    {/if}
    <form
      class="tf-panel tf-cform"
      {@attach holdArrivingEnter}
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
        {#if !standing?.pr && sentFrom === undefined}
          <label for="tf-cmp-title">Title</label>
          <input id="tf-cmp-title" type="text" autocomplete="off" bind:this={titleField} bind:value={title} {@attach escThroughUntouched} />
          <label for="tf-cmp-body">Description</label>
          <textarea id="tf-cmp-body" rows="5" bind:value={body}></textarea>
          {#if ticket !== null}
            <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link the PR to <TfKey value={ticket.key} /> in knobas</label>
          {/if}
          <span class="tf-fwide tf-dim tf-small">Reviewers are asked on {sourceName(source)} after it opens.</span>
        {/if}
      </div>
      <div class="tf-cgo">
        {#if same && standing?.pr}
          <p class="tf-dwhy" role="status">{needs}</p>
        {:else if sentFrom !== undefined && !standing?.pr}
          <span class="tf-cexists">A pull request from this branch is on its way: {sentFrom.text}.</span>
          <a class="tf-btn tf-sm" href={repoHash(repoId, "prs")}>Show in Pull requests</a>
        {:else if standing?.pr}
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
          {#if needs !== null}<p class="tf-dwhy" role="status">{needs}</p>{/if}
        {/if}
        {#if changed !== null && !same}
          <span class="tf-dim tf-small tf-cfiles">{changed} {changed === 1 ? "file" : "files"} changed against {base}</span>
        {/if}
      </div>
    </form>
  </div>
  {#if !same}
  <aside class="tf-side">
    <div class="tf-panel">
      <header><h3>Commits</h3></header>
      {#if commits.length > 0}
        <div class="tf-rows">
          {#each commits as commit, n (n)}
            <div class="tf-r"><span class="tf-mono tf-dim">{commit.chip ?? ""}</span><span class="tf-t tf-t2" title={commit.title}><TfKeyed text={commit.title} /></span></div>
          {/each}
        </div>
      {:else if standing !== null}
        <p class="tf-prnote tf-cnone">Nothing that {defaultBranch ?? "the default branch"} lacks.</p>
      {/if}
    </div>
  </aside>
  <section class="tf-cdiff">
    <h2 class="tf-cgroup-h">
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
    {#if listed === null || listed.entityId === null}
      <div class="tf-empty">This branch is not on {sourceName(source)} yet, or knobas has not synced it. What it changes shows here once it has.</div>
    {:else}
      <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
        {#if files !== null && files.length === 0}
          <div class="tf-empty">This branch has no commits that {base} lacks.</div>
        {:else if files !== null}
          {#if moved !== null}
            <p class="tf-cnote" role="status">{moved}</p>
          {:else if predicted !== null && predicted.section === filesFor}
            <p class="tf-cnote">
              {predicted.checked ? `Read from your clone: the same as on ${sourceName(source)}.` : `Read from your clone as of the last sync; checking ${sourceName(source)}…`}
            </p>
          {/if}
          {#if range === null}
            <p class="tf-cnote" data-testid="no-range" title="As for a range from the repository's first commit, or a branch that merged its base back in.">Context does not expand here: the source did not settle which two commits this diff is between.</p>
          {/if}
          {#key `${filesFor}\n${range?.base}\n${range?.head}`}
            <TfDiffView {files} {cut} {context} openAt={head === null ? null : { entityId: repoId, at: head }} />
          {/key}
        {/if}
      </TfWaiting>
    {/if}
  </section>
  {/if}
</div>
{/if}
