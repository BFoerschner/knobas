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
  - **From the default branch** (2026-11-13): *From* offers every branch, the
    default one too, so a pull request can bring the default branch into
    another. A branch can't go into itself, so choosing the default as *From*
    while *Into* is the default moves *Into* to the branch the reader came
    from, else the newest other branch, and a line above the form says so; an
    *Into* the reader picked stays, and one the page moved goes back to the
    default branch once *From* leaves it. Its *Title* names both ends. The
    branch's own view measures a branch against the default branch, so none
    is read for the default branch: its *Commits* are the compare's own, and a
    pull request this session sent from it holds back only that *Into*.

  **Predicted at the checkout, checked behind** (ADR-0041, the smoothness
  pass of PR #2479): where the repository's checkout holds the tips the
  mirror last synced for *Into* and *From*, the diff is read there at once
  (`entity_range_diff` answers `predicted_at`) and drawn, and the source's
  compare is asked behind it (`entity_source_range_diff`). The same, and a
  line says it was checked; otherwise a branch moved since the sync, the
  source's diff replaces it in place and a line says so. *Create pull
  request* never waits for the check: a pull request sent before it
  answered and that the check then finds different is told in a toast.

  **The names first** (the stored-compare pass of PR #2479, 2026-11-14): with a clone, the changed files' names are read
  there beside the list with counts (`entity_range_names`, one `git diff --raw -M` that reads no file) and drawn under
  the same line the moment they land; the counts and the commits fill the rows in place when the list lands, and each
  file's lines are read meanwhile between the same two commits. Nothing moves: the names are the list less its counts.

  **A branch the list does not hold yet** (the third smoothness pass of PR
  #2479): one made on the forge since the list was read, or one this
  session's edit queued. Its compare is read straight from the source by
  name (`entity_range_diff`), which fetches its tip into the clone behind
  the answer, so the diff draws without waiting for the list or a sync. A
  branch the source lists and the mirror has not synced is read at the id
  the source named, standing and all. The page says a branch is not there
  only once the source answered the compare 404 and no read of the list is
  still under way (`branchesReading`); a branch this session is making says
  so instead, and its diff comes when a read lists it.
-->
<script lang="ts">
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import { tick, untrack } from "svelte";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import { proseMarkdown } from "../../render/prose";
  import type { DiffFile, DiffRange, RangeCommit, RangeDiff, RangeFiles } from "../../ipc/entity";
  import type { DiffFill } from "../diff/diff";
  import { fillEnds, keepList, namesList, rangeFill, seedPages } from "../diff/fill";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { escThroughUntouched } from "../escape";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection, diffHash, rangeDiffVia } from "../diff/address";
  import { endWords } from "../diff/rendered";
  import { rangeContext } from "../diff/context";
  import { itemHash, resolveTicket } from "../job/job";
  import { echoWrite, echoesOn, watchEchoes } from "../queued.svelte";
  import { placeRestoring } from "../return-place";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { onViewFresh } from "./view-fresh.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import TfHeldNote from "../TfHeldNote.svelte";
  import { heldHead } from "../held-head.svelte";
  import { compareReads, drawnCompare, keepDrawnCompare, takeCompareEarly } from "./compare-early";
  import { waitPhase } from "../waiting";
  import { Windowed } from "../../shell/windowed.svelte";
  import { shortEnd } from "../diff/sha";
  import { thousands } from "../counted";
  import {
    branchStanding,
    compareHash,
    keyIn,
    MADE_PR,
    mirrorStanding,
    proposal,
    REPO_PRS,
    repoHash,
    sourceName,
    type BranchOption,
    type BranchStanding,
    type ProposedFrom,
  } from "./repo";
  import type { BranchPull } from "../../ipc/entity";

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
    branchesReading = false,
    shown = true,
    guessed = null,
    pulls = "failed",
    onNewBranch = () => {},
  }: {
    ports: Pick<RepoPorts, "entityView" | "behindView" | "getEntity" | "listSources" | "submitWrite" | "fileBytes" | "piped" | "rangeDiff" | "sourceRangeDiff" | "rangeFiles" | "rangeFile" | "rangeFilePages" | "sourceRangeFiles" | "cancelRead" | "rangeNames" | "listen">;
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
    /** A read of the branch list is under way, which may yet list a branch the source's compare did not find. */
    branchesReading?: boolean;
    /** Whether this tab is the one shown: arriving on it, *Title* takes focus if nothing else has it. */
    shown?: boolean;
    /** The branch the header's *New pull request* chose for the reader, and why, which the page says above the form (F-063). */
    guessed?: { branch: string; why: string } | null;
    /**
     * The branches as the mirror holds them, each with the pull request open from it (`repo_branch_pulls`, read by the
     * repository's page): the form's shape until the branch's own standing answers. `null` while that read is out.
     */
    pulls?: readonly BranchPull[] | "failed" | null;
    /** Opens *New branch…*, which is what a repository of one branch can do here. */
    onNewBranch?: () => void;
  } = $props();

  /** One branch or none, and the address names no other: there is nothing to compare. */
  const lone = $derived(branches.length <= 1 && (head === null || branches.some((option) => option.name === head)));
  const listed = $derived(branches.find((option) => option.name === head) ?? null);
  /** The branch's entity: a refresh's new listing of the same branch reads nothing again on its own. */
  const listedId = $derived(listed?.entityId ?? null);
  /** *From* is the default branch: its pull request goes into another branch (2026-11-13). */
  const fromDefault = $derived(head !== null && head === defaultBranch);
  /** The newest branch but the default and *From*: the list's order after the default one, as the header's *New pull request* reads it. */
  const newestOther = $derived(branches.find((option) => !option.isDefault && option.name !== head)?.name ?? null);
  /** *Into* proposed: the default branch, or for a pull request from it, the newest other branch. */
  const into = $derived(fromDefault ? (newestOther ?? "") : (defaultBranch ?? branches[0]?.name ?? ""));
  /** Where the branch's standing is read: none for the default branch, which its view measures against nothing. */
  const standingId = $derived(fromDefault ? null : listedId);

  let standing = $state<BranchStanding | null>(null);
  /** The standing read for `standingId` has answered, or failed. */
  let standingRead = $state(false);
  /**
   * The pull request open from the branch: its standing's once that answers, until then the mirror's (`mirrorStanding`).
   * The form is *New pull request* with Title and Description where none is, *Compare* without them where one is, so the
   * diff under it stood 175 px lower for a branch with one until the standing said so (the ux20 Windows pass, 5120 px).
   * The standing is a forge round trip (696 ms on a branch's first Compare in the ux21 overs pass); the mirror has it
   * locally, read by the repository's page before Compare opens.
   */
  const openPr = $derived(standing !== null ? standing.pr : Array.isArray(pulls) && head !== null ? (mirrorStanding(pulls, head)?.pr ?? null) : null);
  /**
   * The page waits unseen in its room only while the mirror's read is out, as on an address opened straight onto
   * Compare, and at most `FORM_HOLD_MS`: then shows once, whole (ADR of 2026-11-17, *Nothing moves once drawn*). It
   * waited for the standing itself, 500 ms at most on a first open, and the standing then came later than that and moved
   * the diff anyway (Fable's ruling, the ux21 overs pass).
   */
  const FORM_HOLD_MS = 100;
  const drawnForm = heldHead(
    () => standingId,
    () => standingId === null || standingRead || pulls !== null,
    FORM_HOLD_MS,
  );
  /** What the branch changes: another branch's stays drawn, dimmed, while this one's is read (#2246). */
  // Raw: thousands of files, replaced whole on each read and never changed in place (the compare pass, 2026-11-13).
  let files = $state.raw<DiffFile[] | null>(null);
  /** The section `files` was read for: until another base's diff answers, the page still draws the last one. */
  let filesFor = $state<string | null>(null);
  let reading = $state(false);
  /** The diff hit the parser's ceiling. */
  let cut = $state(false);
  /** What the compare's diff is between, where the source settled it (#2310): context reads at its shas. */
  let range = $state.raw<DiffRange | null>(null);
  /** A rendered file's caption: each side's branch and where it is read (`main at 4f2a91c`). */
  const sideNamesOf = (at: DiffRange) => {
    const [into, from] = [`${base} at ${endWords(at.base)}`, `${head ?? "head"} at ${endWords(at.head)}`];
    return () => ({ old: into, new: from });
  };
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes, ports.piped));
  let failed = $state<string | null>(null);
  /** The source answered the compare 404: it holds no such branch, as of that read. */
  let missing = $state(false);
  /**
   * The drawn diff was read at the checkout between the tips the mirror last synced (ADR-0041): `checked` once the
   * source's compare answered the same. `null` for a diff the source answered.
   */
  let predicted = $state<{ section: string; checked: boolean } | null>(null);
  /**
   * The commits the source's compare listed, by the section it was read for: *Commits* from the default branch, whose own
   * view measures it against nothing. A diff predicted at the checkout lists none, so the check behind it brings them.
   */
  let compareCommits = $state<{ section: string; commits: RangeCommit[] } | null>(null);
  /**
   * Keeps what the source's compare `diff` listed for `asked`, where it listed any. `longer`: a check of the same diff,
   * whose list stands only where it is longer (Gitea's compare lists at most 250 commits, the clone's 500: the check
   * replaced a branch's 300 with 250, the big-compare pass, 2026-11-16).
   */
  function keepCommits(asked: string, diff: RangeDiff, longer = false): void {
    const listed = diff.range?.commits ?? [];
    if (longer && compareCommits?.section === asked && compareCommits.commits.length >= listed.length) return;
    if (listed.length > 0 || compareCommits?.section !== asked) compareCommits = listed.length > 0 ? { section: asked, commits: listed } : null;
  }
  /**
   * The drawn list is what knobas last read of the source's compare between these two names (`held_at`, the forge-waits
   * pass, 2026-11-13): drawn at once under the held line while the source's list is read behind it, and replaced in place,
   * quietly, where it moved. `done` once that read answered; `failed` where it did not.
   */
  let held = $state<{ section: string; at: string; done: boolean; failed: string | null } | null>(null);
  /**
   * The drawn files carry their counts. False while the clone's names are drawn (`entity_range_names`, the
   * stored-compare pass, 2026-11-14): the list with counts, read beside them, fills them in place.
   */
  let counted = $state(true);
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
  /** *Into* where the page moved it because *From* became the default branch, and why: said above the form until the reader picks another. */
  let intoMoved = $state<{ branch: string; why: "came" | "newest" } | null>(
    untrack(() => (fromDefault && addressed === null && newestOther !== null ? { branch: newestOther, why: "newest" } : null)),
  );
  // *From* the default branch with *Into* the default too (the page shown at `compare/<default>` while *Into* stood there): *Into*
  // is the newest other branch. *From* leaving the default branch takes an *Into* the page moved back to the default branch.
  $effect.pre(() => {
    const from = fromDefault;
    untrack(() => {
      if (from) {
        if (addressed === null && base === head && newestOther !== null) {
          base = newestOther;
          intoMoved = { branch: newestOther, why: "newest" };
        }
      } else if (intoMoved !== null) {
        if (addressed === null && base === intoMoved.branch && defaultBranch !== null) base = defaultBranch;
        intoMoved = null;
      }
    });
  });
  /** *Into* and *From* are one branch: there is nothing to compare, and no diff, commits or open pull request to speak of. */
  const same = $derived(base === head);
  let title = $state("");
  let body = $state("");
  /** The ticket the branch's key names, and that key: what *Link the PR to <key>* links to (#984). */
  let ticket = $state<{ key: string; id: string } | null>(null);
  let link = $state(true);
  let busy = $state(false);

  /** The diff read: from *Into* to the branch, whenever they are two branches, listed or not: the source's compare reads it by name. */
  const section = $derived(head === null || base === "" || base === head ? null : compareSection(base, head));
  /** The branch is one this session is making, which no read holds yet: its write's word. */
  const making = $derived(listed?.made ?? null);
  /** The source said there is no such branch, and nothing still reading could say otherwise. */
  const absent = $derived(missing && !branchesReading && making === null && filesFor !== section);
  /**
   * What *Open as diff* and *Open in…* name (Spec 5 of the review of spec #2234): the two commits the source settled
   * the drawn compare between, so the diff page can read it at a checkout that holds both (ADR-0036; a branch name is
   * never read locally); the branch names until the compare has answered, or where it settled none.
   */
  const ends = $derived(range !== null && filesFor === section && head !== null ? { base: range.base, head: range.head } : { base, head: head ?? "" });

  // The branch's view, read for each branch the address names.
  $effect(() => {
    const id = standingId;
    let live = true;
    standing = null;
    standingRead = false;
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
    missing = false;
    if (asked === null) return;
    const read = newRead();
    readDiff(asked, () => live, false, read);
    return () => {
      live = false;
      // Another pick: the read it superseded stops at the backend too, its check behind with it.
      stopRead(read);
      stopRead(`${read}-check`);
    };
  });

  // A branch the list did not hold when its diff was read is listed now (a read of the list found it, or the branch this
  // session made landed): its diff is read again where none is drawn for it, as the source may hold it now.
  // Only for the branch the page already shows: a pick changes the listed id too, and the effect above reads its diff
  // (the compare pass, 2026-11-13: every pick read the whole diff twice, at once).
  let listedWas = untrack(() => listedId);
  let sectionWas = untrack(() => section);
  $effect(() => {
    const id = listedId;
    const asked = section;
    const [was, wasFor] = [listedWas, sectionWas];
    listedWas = id;
    sectionWas = asked;
    if (id === was || asked !== wasFor) return;
    if (id !== null && asked !== null && untrack(() => files === null || filesFor !== asked)) void readDiff(asked, () => section === asked);
  });

  // A refresh of the page reads the branch again, over what the page drew.
  onRefreshed(
    () => refreshed,
    () => {
      const id = standingId;
      const asked = section;
      if (asked === null || (id === null && !fromDefault)) return Promise.resolve();
      return Promise.all([id === null ? null : readStanding(id, () => standingId === id, true), readDiff(asked, () => section === asked, true)]);
    },
  );

  /**
   * The pull request this session sent from this branch, until the source's view holds it (F-257). Not while its own press is
   * leaving: the form stays as the tab hides, so *Pull requests* can take focus from it for the row it landed on (F-242).
   */
  const sentFrom = $derived(
    head === null || busy
      ? undefined
      : // From the default branch, one sent into another branch holds nothing back here.
        echoesOn(repoId, MADE_PR).find((echo) => echo.base === head && (!fromDefault || echo.into === undefined || echo.into === base)),
  );
  $effect(() => {
    if (!shown) busy = false;
  });
  // Its landing reads the branch again, so the line it leaves is the source's own *Open PR*, not the form.
  $effect(() =>
    untrack(() =>
      watchEchoes(
        REPO_PRS,
        () => {
          const id = standingId;
          if (id !== null) void readStanding(id, () => standingId === id, true);
        },
        { listen: ports.listen },
      ),
    ),
  );

  /**
   * The branch's standing. It adds the commits list and an open pull request's line to a form that can be sent
   * without them, so a source that does not answer leaves the page as drawn (ADR-0042): the diff is read on its own.
   * Behind the diff (`behindView`, the ux16 overs pass, 2026-11-17): what knobas holds of it draws at once, and its
   * requests to the source wait for the compare's, which on Björn's PC took 450 to 600 ms beside them and 40 to 90 alone.
   */
  function readStanding(id: string, alive: () => boolean, again = false): Promise<unknown> {
    void again;
    return (ports.behindView ?? ports.entityView)(id, null).then(
      (answer) => {
        if (!alive()) return;
        standing = branchStanding(answer);
        standingRead = true;
      },
      () => {
        if (alive()) standingRead = true;
      },
    );
  }

  // The standing knobas held, read again behind it (ADR-0042).
  onViewFresh(
    () => standingId,
    null,
    () => {
      const id = standingId;
      if (id !== null) void readStanding(id, () => standingId === id, true);
    },
    untrack(() => ports.listen),
  );

  /**
   * Each read of the diff is named, so the pick that supersedes it calls it off at the backend (`cancelRead`, the compare
   * pass, 2026-11-13): a branch of 10,000 files left for another stops costing the clone and the forge.
   */
  const uid = $props.id();
  let reads = 0;
  const newRead = () => `compare-${uid}-${++reads}`;
  const stopRead = (read: string) => void ports.cancelRead?.(read).catch(() => {});
  /**
   * The two ends the drawn list's files are read between, a page of lines at a time as each comes near the view: the
   * commits it settled, else the names it was read by. `null` while the diff drawn carries its lines (no `rangeFiles`).
   */
  let fillAt = $state<{ base: string; head: string; local: boolean } | null>(null);
  /** The same two ends, read at the same place: the fill stays, and the lines read under it. */
  const sameEnds = (a: { base: string; head: string; local: boolean } | null, b: { base: string; head: string; local: boolean }) =>
    a !== null && a.base === b.base && a.head === b.head && a.local === b.local;
  const fill = $derived.by((): DiffFill | null => {
    const read = ports.rangeFile;
    const at = fillAt;
    // Many files' first pages in one read, and read ahead where the list was read at the clone (the big-compare pass).
    return read === undefined || at === null ? null : rangeFill(read, repoId, at, false, stopRead, ports.rangeFilePages, at.local);
  });

  /**
   * The diff `asked` names; `again` keeps what the page drew when a read fails. Where the ports read a range a file at a
   * time (`rangeFiles`), this is the list of its files, each file's lines read by the diff as it comes near the view
   * (`fill`): what draws in a frame or two at any number of files.
   */
  function readDiff(asked: string, alive: () => boolean, again = false, read = newRead()): Promise<unknown> {
    const fail = (error: unknown) => {
      if (!alive() || again) return;
      failed = ipcErrorMessage(error);
      missing = isIpcError(error) && error.code === "not_found";
    };
    // Read again behind what is drawn (a refresh, the page shown again), the diff stays live: dimmed it would be inert,
    // and drop the reader's focus. Another base or branch dims what is drawn while its own diff is read (#2246).
    if (!again) reading = true;
    const [from, to] = [base, head ?? ""];
    // Local-first, as every diff (ADR-0036, Spec 6 of the review of spec #2234): branch names are the source's, and
    // its compare names the two commits (`range`) that *Open as diff* takes to a checkout.
    const listing = ports.rangeFiles;
    // The list the repository page asked early (`compare-early.ts`), answered or on its way; one that failed is read anew.
    const early = listing === undefined || again ? null : takeCompareEarly(repoId, from, to);
    if (listing !== undefined) compareReads(repoId, from, to);
    // The clone's names, drawn while the list with counts is read beside them, where they land first (the stored-compare
    // pass): one `git diff --raw` that reads no file's lines, where the counts cost git a read of every file.
    let landed = false;
    // A pair drawn before this session (a swap back, the other end picked again): drawn now, in the pick's own frame, under
    // the clone's line, while the reads below go out as for any pick and fill it in place (the big-compare pass).
    // Only the clone's: a forge's list knobas holds is the backend's held read, drawn under its own line.
    const before = again || listing === undefined ? undefined : drawnCompare(repoId, from, to);
    // Untracked: this runs in the read's effect, which must not read again when what it draws changes.
    if (before !== undefined && before.list.read_at === "checkout") untrack(() => {
      const list = before.list;
      reading = false;
      failed = null;
      missing = false;
      files = list.files;
      counted = before.counted;
      const at = { ...fillEnds(list, from, to), local: true };
      if (!sameEnds(fillAt, at)) fillAt = at;
      cut = list.truncated;
      range = list.range;
      filesFor = asked;
      keepCommits(asked, list);
      moved = null;
      predicted = { section: asked, checked: false };
      held = null;
      checking = null;
    });
    if (!again && listing !== undefined) {
      // The names asked with the early list, where they were (before the page was built); else asked now.
      void (early?.names ?? ports.rangeNames?.(repoId, from, to))?.then(
        (names) => {
          // Not over a counted list this pick drew already (the pair drawn before, below).
          if (!alive() || landed || names === null || (filesFor === asked && counted)) return;
          // The names of the two commits the drawn list stands at are that list: nothing to lay down again (a pair drawn
          // before, or read ahead, drew them in the pick's frame; laying 4,200 files down again was a task of its own).
          if (filesFor === asked && sameRange(range, names.range)) return;
          const list = namesList(names);
          keepDrawnCompare(repoId, from, to, list, false);
          reading = false;
          failed = null;
          missing = false;
          files = list.files;
          counted = false;
          const at = { ...fillEnds(list, from, to), local: true };
          if (!sameEnds(fillAt, at)) fillAt = at;
          cut = false;
          range = list.range;
          filesFor = asked;
          keepCommits(asked, list);
          moved = null;
          // Read at the clone as of the last sync: the line the counted list keeps, so nothing under it moves.
          predicted = { section: asked, checked: false };
          held = null;
          checking = null;
        },
        () => {},
      );
    }
    const answer: Promise<RangeDiff> =
      early !== null
        ? early.list.catch(() => listing!(repoId, from, to, false, read))
        : listing !== undefined
          ? listing(repoId, from, to, false, read)
          : (ports.rangeDiff ?? rangeDiffVia(ports.entityView))(repoId, from, to, false);
    return answer.then((answered) => {
      landed = true;
      if (!alive()) return;
      // The first files' lines the list came with, kept for the fill's first asks (Fable's ruling, the ux18 pass).
      const diff = listing === undefined ? answered : seedPages(repoId, answered as RangeFiles);
      reading = false;
      failed = null;
      missing = false;
      // The counted list of the two commits the drawn counted list stands at is that list: the rows stay as they are.
      if (!(counted && filesFor === asked && sameRange(range, diff.range) && files?.length === diff.files.length)) files = diff.files;
      counted = true;
      if (listing !== undefined) {
        keepDrawnCompare(repoId, from, to, diff as RangeFiles, true);
        // *Open as diff* of this compare draws it from here (the big-compare pass).
        keepList(repoId, diff as RangeFiles);
      }
      // The same two ends keep the fill, so the lines read under the names stay.
      const at = listing === undefined ? null : { ...fillEnds(diff, from, to), local: diff.read_at === "checkout" };
      if (at === null || !sameEnds(fillAt, at)) fillAt = at;
      cut = diff.truncated;
      range = diff.range;
      filesFor = asked;
      keepCommits(asked, diff);
      moved = null;
      predicted = null;
      held = null;
      checking = null;
      const checkable = (ports.sourceRangeFiles ?? ports.sourceRangeDiff) !== undefined;
      // A list knobas held (`RangeFiles.held_at`); the whole diff the older ports answer never is one.
      const heldAt = (diff as { held_at?: string }).held_at;
      if (diff.predicted_at !== undefined && checkable) checkPredicted(asked, from, to, diff, alive, `${read}-check`);
      else if (heldAt !== undefined && checkable) checkPredicted(asked, from, to, diff, alive, `${read}-check`, heldAt);
    }, (error) => {
      landed = true;
      if (!alive()) return;
      reading = false;
      if (again) return;
      // The clone's names drawn stand, as a prediction the source did not confirm.
      if (!counted && filesFor === asked) return;
      fail(error);
      files = null;
    });
  }

  /** Whether two ranges are the same two commits (whole shas name content that never changes). */
  const sameRange = (a: DiffRange | null, b: DiffRange | null) => a !== null && b !== null && a.base === b.base && a.head === b.head;

  /** Whether two answers for one range are the same diff: the same two commits where both name them, else the same files and counts. */
  function sameDiff(a: RangeDiff, b: RangeDiff): boolean {
    if (a.range !== null && b.range !== null) return a.range.base === b.range.base && a.range.head === b.range.head;
    const shape = (d: RangeDiff) => JSON.stringify(d.files.map((f) => [f.path, f.additions, f.deletions]));
    return shape(a) === shape(b);
  }


  /**
   * The source's compare, asked behind a predicted diff: the same checks it; another replaces it in place, and says so.
   * Its list of files where the page reads a file at a time (`sourceRangeFiles`), never its every line.
   */
  function checkPredicted(asked: string, from: string, to: string, shown: RangeDiff, alive: () => boolean, read: string, heldAt?: string) {
    const listing = ports.sourceRangeFiles;
    const ask = listing !== undefined && fillAt !== null ? (a: string, b: string, c: string) => listing(a, b, c, read) : ports.sourceRangeDiff;
    if (ask === undefined) return;
    if (heldAt === undefined) predicted = { section: asked, checked: false };
    else held = { section: asked, at: heldAt, done: false, failed: null };
    const differs = ask(repoId, from, to).then(
      (answered) => {
        const fresh = listing !== undefined && fillAt !== null ? seedPages(repoId, answered as RangeFiles) : answered;
        const same = sameDiff(shown, fresh);
        if (alive() && filesFor === asked) {
          keepCommits(asked, fresh, same);
          if (heldAt !== undefined) held = { section: asked, at: heldAt, done: true, failed: null };
          if (same) {
            if (heldAt === undefined) predicted = { section: asked, checked: true };
          } else {
            files = fresh.files;
            if (fillAt !== null) {
              const at = { ...fillEnds(fresh, from, to), local: false };
              if (!sameEnds(fillAt, at)) fillAt = at;
              keepDrawnCompare(repoId, from, to, fresh as RangeFiles, true);
            }
            cut = fresh.truncated;
            range = fresh.range;
            predicted = null;
            // A list knobas held said from when it was: the source's replaces it quietly.
            if (heldAt === undefined) moved = `${to} or ${from} moved on ${sourceName(source)} since the last sync: this is the change as it stands there now.`;
          }
        }
        return !same;
      },
      // The source could not answer: the prediction stands, unchecked; a held list says why.
      (error) => {
        if (heldAt !== undefined && alive() && filesFor === asked) held = { section: asked, at: heldAt, done: false, failed: ipcErrorMessage(error) };
        return false;
      },
    );
    checking = { section: asked, differs };
  }

  /**
   * **The next picks, read ahead** (the big-compare pass, 2026-11-16): once the pair shown is drawn and the page is idle,
   * the names of the pairs a reader picks next are read at the clone, one after another (the six newest other branches as
   * *From*, three other bases as *Into*), and kept as a pair drawn is, so such a pick draws its list in its own frame; the
   * pick's reads go out as for any pick and fill it in place. A clone only: names of a pair the clone cannot answer are
   * none, and nothing is asked of the forge.
   */
  const AHEAD_FROM = 6;
  const AHEAD_INTO = 3;
  $effect(() => {
    const read = ports.rangeNames;
    const [into, from] = [base, head];
    const ready = shown && filesFor === section && files !== null && !reading && predicted !== null;
    if (read === undefined || ports.rangeFiles === undefined || from === null || into === "" || into === from || !ready) return;
    const pairs: [string, string][] = [];
    const other = (name: string) => name !== from && name !== into;
    for (const option of branches) {
      if (pairs.length >= AHEAD_FROM) break;
      // The default branch as *From* moves *Into*: not a pick read ahead.
      if (other(option.name) && !option.isDefault && option.made === undefined) pairs.push([into, option.name]);
    }
    let bases = 0;
    for (const option of branches) {
      if (bases >= AHEAD_INTO) break;
      if (other(option.name) && option.made === undefined) {
        pairs.push([option.name, from]);
        bases++;
      }
    }
    const todo = pairs.filter(([a, b]) => drawnCompare(repoId, a, b) === undefined);
    if (todo.length === 0) return;
    let live = true;
    let stop: (() => void) | null = null;
    const later = (then: () => void) => {
      if (typeof requestIdleCallback === "function") {
        // Within 300 ms whatever else the page is doing: the diff's own read-ahead of lines takes the idle turns too, and
        // a pick's names are worth more than a far file's lines.
        const id = requestIdleCallback(then, { timeout: 300 });
        stop = () => cancelIdleCallback(id);
      } else {
        const id = setTimeout(then, 100);
        stop = () => clearTimeout(id);
      }
    };
    const step = (at: number) => {
      stop = null;
      const pair = todo[at];
      if (!live || pair === undefined) return;
      const [a, b] = pair;
      void read(repoId, a, b)
        .then(
          (names) => {
            if (names !== null && drawnCompare(repoId, a, b) === undefined) keepDrawnCompare(repoId, a, b, namesList(names), false);
          },
          () => {},
        )
        .finally(() => {
          if (live) later(() => step(at + 1));
        });
    };
    later(() => step(0));
    return () => {
      live = false;
      stop?.();
    };
  });

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

  // A pull request may go from the default branch into another (2026-11-13), so *From* offers it too.
  const heads = $derived.by(() => {
    const names = branches.map((option) => option.name);
    return head !== null && !names.includes(head) ? [head, ...names] : names;
  });

  /**
   * *From* picked: the page reads that branch, and *Into* stays as it stands. The default branch picked while *Into* is the
   * default too moves *Into* to the branch the reader came from, else the newest other; an *Into* the page moved goes back
   * to the default branch once *From* leaves it. The address names *Into* wherever it is not the default branch.
   */
  function pickFrom(value: string) {
    let next = base;
    if (value === defaultBranch && base === value) {
      const came = head !== null && head !== value && branches.some((option) => option.name === head) ? head : null;
      const other = came ?? branches.find((option) => !option.isDefault && option.name !== value)?.name ?? null;
      if (other !== null) {
        next = other;
        intoMoved = { branch: other, why: came !== null ? "came" : "newest" };
      }
    } else if (value !== defaultBranch && intoMoved !== null) {
      if (intoMoved.branch === base && defaultBranch !== null) next = defaultBranch;
      intoMoved = null;
    }
    base = next;
    // From the default branch, *Into* is never the default proposal, so the address always names it.
    location.hash = compareHash(repoId, value, value !== defaultBranch && next === (defaultBranch ?? branches[0]?.name ?? "") ? null : next);
  }

  /** A pull request from the default branch is titled by both ends: the branch's own name says nothing of where it goes. */
  const mergeTitle = (from: string, to: string) => `Merge ${from} into ${to}`;
  let mergeProposed = "";
  // Over the name proposed, or the last such title only: typed words stay.
  $effect(() => {
    const [from, to] = [head, base];
    if (!fromDefault || from === null || to === "" || to === from) return;
    untrack(() => {
      const next = mergeTitle(from, to);
      if (title === mergeProposed || title === proposal(from, null).title) title = next;
      mergeProposed = next;
    });
  });
  /** The files changed: this base's diff's, or before it is read the branch's own count, which is against the default branch. */
  const changed = $derived(
    files !== null && filesFor === section ? files.length : base === defaultBranch ? (standing?.files ?? null) : null,
  );
  /** Why the source draws the pull request disabled (#2030): its words, on the button. */
  const refused = $derived(standing?.prBlocked ?? null);
  const ready = $derived(head !== null && title.trim() !== "" && base !== "" && base !== head && !busy && refused === null && sentFrom === undefined && !absent);
  /** What the primary waits for, in words beside it (design system, the enabled rule). */
  const needs = $derived(
    same
      ? "Into and From are the same branch, so there is nothing to compare."
      : absent
        ? `${sourceName(source)} has no branch ${head ?? ""} to open a pull request from.`
        : (refused ?? (title.trim() === "" ? "Give it a title." : null)),
  );
  /**
   * What the compare itself listed, wherever it listed any: the clone's `git log` of the range or the source's compare,
   * which come with the diff's list (the compare pass, 2026-11-13), so *Commits* never waits for the branch's own view, a
   * read of seconds on the forge for a branch knobas has not read. Else, from the default branch, nothing (its view
   * measures nothing); else the branch's own, against the default branch.
   */
  const commits = $derived(
    compareCommits !== null && compareCommits.section === section && filesFor === section
      ? compareCommits.commits.map((commit) => ({ entity_id: null, title: commit.title, meta: commit.meta, tone: null, chip: shortEnd(commit.sha), url: null }))
      : fromDefault
        ? []
        : (standing?.commits ?? []),
  );

  /** *Commits* a few hundred long draws the rows its box shows. By place: the list is the compare's, in its order. */
  const commitWin = new Windowed({ count: () => commits.length, key: (i) => i, estimate: 35, scrolls: true, from: 40, overscan: 350 });

  // One commit says what the change is better than its branch's name does (F-241). Only over the name proposed: typed words stay.
  $effect(() => {
    const only = commits.length === 1 ? (commits[0]?.title ?? "") : "";
    if (only === "" || ticket !== null || fromDefault) return;
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
  // On arriving at the form, and again if a ticket's words replace the proposal below; once the page shows, since a
  // field held unseen takes no focus.
  $effect(() => {
    if (shown && drawnForm.shown && titleField !== null) untrack(selectTitle);
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
      echoWrite(repoId, MADE_PR, words, { write, base: head, into: base, many: true, following: { name: `Pull request ${words}` }, surface: REPO_PRS, source: source?.id ?? null });
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
<div class="tf-compare" class:tf-heldbelow={!drawnForm.shown} {@attach sideWidth("compare")}>
  <TfSideEdge side="compare" />
  <div>
    <h2 class="tf-cgroup-h">{openPr || sentFrom !== undefined ? "Compare" : "New pull request"}</h2>
    {#if guessed !== null && guessed.branch === head && !openPr && sentFrom === undefined}
      <p class="tf-cstart">Starting from <span class="tf-mono">{guessed.branch}</span>, {guessed.why}.</p>
    {/if}
    {#if fromDefault && intoMoved !== null && intoMoved.branch === base && sentFrom === undefined}
      <p class="tf-cstart">
        {#if intoMoved.why === "came"}Into moved to <span class="tf-mono">{base}</span>, the branch you came from.{:else}Into is <span class="tf-mono">{base}</span>, the newest other branch.{/if}
      </p>
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
        <select id="tf-cmp-into" class="tf-cbranch" bind:value={base} onchange={() => (intoMoved = null)}>
          {#each branches as option (option.name)}<option value={option.name}>{option.name}</option>{/each}
        </select>
        <label for="tf-cmp-from">From</label>
        <select
          id="tf-cmp-from"
          class="tf-cbranch"
          value={head ?? ""}
          onchange={(event) => pickFrom((event.currentTarget as HTMLSelectElement).value)}
        >
          {#each heads as name (name)}<option value={name}>{name}</option>{/each}
        </select>
        {#if !openPr && sentFrom === undefined}
          <label for="tf-cmp-title">Title</label>
          <input id="tf-cmp-title" type="text" autocomplete="off" bind:this={titleField} bind:value={title} {@attach escThroughUntouched} />
          <label for="tf-cmp-body">Description</label>
          <textarea id="tf-cmp-body" rows="5" bind:value={body}></textarea>
          {#if ticket !== null}
            <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link the PR to <TfKey value={ticket.key} /> in knobas</label>
          {:else if head !== null && keyIn(head) !== null}
            <!-- Its room while the branch's key is looked up, so the diff under the form stays where it drew. -->
            <span class="tf-ck tf-cheld" aria-hidden="true"><input type="checkbox" tabindex="-1" disabled /> Link the PR to <TfKey value={keyIn(head) ?? ""} /> in knobas</span>
          {/if}
          <span class="tf-fwide tf-dim tf-small">Reviewers are asked on {sourceName(source)} after it opens.</span>
        {/if}
      </div>
      <div class="tf-cgo">
        {#if same && openPr}
          <p class="tf-dwhy" role="status">{needs}</p>
        {:else if sentFrom !== undefined && !openPr}
          <span class="tf-cexists">A pull request from {fromDefault ? `${head} into ${base}` : "this branch"} is on its way: {sentFrom.text}.</span>
          <a class="tf-btn tf-sm" href={repoHash(repoId, "prs")}>Show in Pull requests</a>
        {:else if openPr}
          <span class="tf-cexists">PR {openPr.number ?? ""} is already open from this branch.</span>
          {#if openPr.entityId !== null}
            <a class="tf-btn tf-sm" href={itemHash({ entity_id: openPr.entityId, kind: "pr" })}>Open PR {openPr.number ?? ""}</a>
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
    <div class="tf-panel tf-headed tf-ccommits">
      <header><h3>Commits</h3>{#if commits.length > 0}<span class="tf-ccount">{thousands(commits.length)}</span>{/if}</header>
      {#if commits.length > 0}
        <!-- One line a commit in a box of about ten that scrolls (the big-compare pass, 2026-11-16): hundreds of commits
             stood here two lines each and pushed the diff thousands of pixels down. -->
        <div class="tf-rows tf-crows" aria-label="Commits" {@attach commitWin.list}>
          {#if commitWin.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach commitWin.room("before")}></div>{/if}
          {#each commitWin.slice(commits) as commit, n (commitWin.start + n)}
            <div class="tf-r" title={commit.title} {@attach commitWin.row(commitWin.start + n)}><span class="tf-mono tf-dim">{commit.chip ?? ""}</span><span class="tf-t"><TfKeyed text={commit.title} /></span></div>
          {/each}
          {#if commitWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach commitWin.room("after")}></div>{/if}
        </div>
      {:else if fromDefault ? filesFor === section && files !== null && files.length === 0 : standing !== null}
        <p class="tf-prnote tf-cnone">Nothing that {fromDefault ? base : (defaultBranch ?? "the default branch")} lacks.</p>
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
    {#if making !== null && files === null}
      <div class="tf-empty" role="status">
        {head} is being made on {sourceName(source)} ({making}). What it changes shows here once it is there.
      </div>
    {:else if absent}
      <div class="tf-empty" role="status">{sourceName(source)} has no branch {head}: it was deleted, renamed, or never pushed.</div>
    {:else}
      {#if listed === null && files === null && (reading || missing)}
        <p class="tf-cnote" role="status">Reading {head} from {sourceName(source)}: knobas had not listed it yet.</p>
      {/if}
      <TfWaiting phase={waitPhase({ drawn: files !== null, reading: reading || (missing && branchesReading), failed: failed !== null && !missing })} error={failed} shape="lines">
        {#if files !== null && files.length === 0}
          <div class="tf-empty">This branch has no commits that {base} lacks.</div>
        {:else if files !== null}
          {#if moved !== null}
            <p class="tf-cnote" role="status">{moved}</p>
          {:else if held !== null && held.section === filesFor}
            <!-- The held line (ADR-0042): its words go when the source's list lands, its room stays, so the list holds still. -->
            <TfHeldNote heldAt={held.done ? null : held.at} source={sourceName(source)} failed={held.failed} />
          {:else if predicted !== null && predicted.section === filesFor}
            <p class="tf-cnote">
              {predicted.checked ? `Read from your clone: the same as on ${sourceName(source)}.` : `Read from your clone as of the last sync; checking ${sourceName(source)}…`}
            </p>
          {/if}
          {#if range === null}
            <p class="tf-cnote" data-testid="no-range" title="As for a range from the repository's first commit, or a branch that merged its base back in.">Context does not expand here: the source did not settle which two commits this diff is between.</p>
          {/if}
          {#key `${filesFor}\n${range?.base}\n${range?.head}`}
            <TfDiffView
              {files}
              {cut}
              {context}
              {fill}
              {counted}
              calm={!reading}
              sideNames={range === null ? null : sideNamesOf(range)}
              openAt={head === null ? null : { entityId: repoId, at: head }} />
          {/key}
        {/if}
      </TfWaiting>
    {/if}
  </section>
  {/if}
</div>
{/if}
