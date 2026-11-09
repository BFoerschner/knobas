<!--
  The pull request page (#841, R-8.15, R-8.16, R-8.20), prototype-6's
  `renderPR`: `#/pr/<entity id>` and its tabs `/conversation`, `/diff` and
  `/commits`, which share the header and the one read.

  - **The header**: `PR #<n> <title>`, its state, labels, head into base and
    author (*you* for the reader's own); what the toolbar queued this session
    stands at the tab row's right end, where a chip can't wrap the line and move
    the tabs (F-206), and a thread's reply and resolve stay in their thread
    (F-261), with *Submit review… (N pending)* in warn while line comments wait
    (#842, `TfPrReview`, F-254). The toolbar's *Review…*,
    *Update branch*, *Edit*, *Close* (or *Reopen*) and *Merge…* (#843,
    `TfPrMerge`). A write is drawn only where the source's view offers it
    (ADR-0020 rule 4).
  - **Overview**: the head commit's checks (a check whose link the mirror
    holds opens its run) and the description; beside it *Ready to merge?*
    (the source's *Approvals*, its required checks and *Conflicts*, #994,
    with a verdict that holds back wherever *Merge…* is refused) atop the
    side rail (#2278): the reviewers and their state, the checks with the
    required ones marked and a failing one first, the builds of the head
    the mirror holds, the conflicts and, when the repository is cloned,
    the local checkout. The rail sits beside every tab but *Files changed*.
  - **Conversation**: each comment and review in time order, and a comment
    box that queues `Comment`; *Comment* is drawn disabled with the source's
    reason where the token may not comment (#2030), and a press queues nothing.
  - **Files changed**: `TfPrFiles` (#842), which reads the deferred *Files*
    section when the tab opens.
  - **Commits**: the pull request's commits; one the mirror holds opens its
    diff on the repository page.

  Everything is the source's view of the pull request (`entity_view`), read
  once per pull request (its *Files* once more, when that tab opens); the
  tabs only choose what to draw of it. A muted line the source writes into a
  tab (comments the token may not read, checks it could not read) is drawn
  as a line where the source put it, on every tab (#2065).
-->
<script lang="ts" module>
  import type {
    CheckoutView,
    CodeFacts,
    EntityDetail,
    EntityFilter,
    EntityPage,
    SourceView,
    UrlMatch,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { SourceFacts } from "../job/job";
  import type { DetailRefreshPorts } from "../refresh.svelte";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface PrPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    /** Which of the commits are the reader's own (F-221); the bridge's when left out. */
    codeFacts?: (entityIds: string[]) => Promise<CodeFacts[]>;
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    entityCheckout: (entityId: string) => Promise<CheckoutView>;
    /** A check's link, to the run the mirror holds; never a pasted text. */
    resolveUrl: (url: string) => Promise<UrlMatch | null>;
    listSources: () => Promise<SourceFacts[]>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** *Viewed* on the pull request's files (#842). */
    prViewedFiles: (prId: string, blobs: Record<string, string>) => Promise<string[]>;
    setPrFileViewed: (prId: string, path: string, viewed: boolean, blobs: Record<string, string>) => Promise<string[]>;
    /** Hears `item:refreshed` and `sync:state`, which read the live view again for the page's echoes (#2103); the bridge's when left out. */
    listen?: EchoListen;
    /** The header's *Refresh* (#2107); the bridge's when left out. */
    refresh?: DetailRefreshPorts;
  }
</script>

<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { tick, untrack } from "svelte";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    entityCheckout,
    codeFacts,
    entityView,
    getEntity,
    listEntities,
    prViewedFiles,
    resolveUrl,
    setPrFileViewed,
    submitWrite,
    type EntityRow,
    type ViewAction,
    type ViewRow,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { pendingComments as sharedPending, type PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import { writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { toastQueued } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { actionName } from "../../detail/view/actions";
  import { prNumber } from "../code/code";
  import { isMine, itemHash, keyOf, username } from "../job/job";
  import { commitLine } from "../repo/file-tree";
  import { commitHash, readAllRows, repoHash, repoOfRow } from "../repo/repo";
  import { drafts } from "../drafts.svelte";
  import { inlineEditor, sendKey } from "../inline-editor";
  import { pageVisible } from "../kept-pages.svelte";
  import { stateWord } from "../state-word";
  import type { EscTarget } from "../escape";
  import { initials } from "../tickets/tickets";
  import {
    PR_TABS,
    REQUESTED,
    REVIEW_SENT,
    SOURCE_ACTION,
    chipAct,
    chipWords,
    commitParts,
    conversationOf,
    conflictsOf,
    conversationParts,
    headerOf,
    knowsPrTab,
    overviewParts,
    prHash,
    railChecks,
    checkReading,
    readiness,
    readPrRoute,
    readyLines,
    reviewChoices,
    reviewersOf,
    tabCounts,
    type PrTab,
    type Verdict,
  } from "./pr";
  import { askAgain, echoWrite, echoOn, echoesOn, sameWords, sentWrites, settleEchoes, takeAgain, watchEchoes, type Echo, type EchoListen } from "../queued.svelte";
  import TfEntityAction from "../TfEntityAction.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfFindBar from "../TfFindBar.svelte";
  import TfKey from "../TfKey.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { diffOf, rangeOf } from "./files";
  import { BY_POINTER_MS } from "../press-guard";
  import TfPrFiles from "./TfPrFiles.svelte";
  import TfPrMerge from "./TfPrMerge.svelte";
  import TfPrReview from "./TfPrReview.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { parentPage } from "../page-name.svelte";

  let {
    ready,
    rest,
    back = null,
    ports = {
      getEntity: (entityId) => getEntity(entityId),
      entityView: (entityId, section) => entityView(entityId, section),
      listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
      entityCheckout: (entityId) => entityCheckout(entityId),
      resolveUrl: (url) => resolveUrl(url),
      listSources: () => listSources(),
      submitWrite: (payload) => submitWrite(payload),
      prViewedFiles: (id, blobs) => prViewedFiles(id, blobs),
      setPrFileViewed: (id, path, viewed, blobs) => setPrFileViewed(id, path, viewed, blobs),
    },
    pending = sharedPending,
    follower = writeFollower(),
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `pr` in the address. */
    rest: string[];
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
    ports?: PrPorts;
    /** The reader's pending line comments (#842): this viewer's, kept across restarts. */
    pending?: PendingCommentStore;
    /** Follows each queued write, so one held or refused drops its echo (#1424). */
    follower?: WriteFollower;
  } = $props();

  /** How many repositories one read asks for: a source with more is read on with `offset` (#1825). */
  const LIMIT = 1000;

  const route = $derived(readPrRoute(rest));
  const prId = $derived(route.prId);
  const tab = $derived<PrTab>(route.tab);
  /** The address names a tab the page has; one it does not have is said under the tabs, not drawn as Overview. */
  const tabKnown = $derived(knowsPrTab(rest));

  let detail = $state<EntityDetail | null>(null);
  let failed = $state<string | null>(null);
  /** The read failed because no such pull request is held, not because the read broke. */
  let missing = $state(false);
  /** Bumped by *Try again*. */
  let attempt = $state(0);
  /** `undefined` while the view has not answered; `null` for a source with none. */
  let view = $state<SourceView | null | undefined>(undefined);
  let viewFailed = $state<string | null>(null);
  let repo = $state<EntityRow | null>(null);
  let checkout = $state<CheckoutView | null>(null);
  let mine = $state(false);
  /** The account the reader has in the pull request's source, `null` where none is configured. */
  let me = $state<string | null>(null);
  /** Each check's run, by its link, once the mirror answered. */
  let runs = $state<Record<string, UrlMatch>>({});
  /** The header's *Refresh* or *Sync now* (#2107). */
  const refresh = createDetailRefresh(untrack(() => ports.refresh));

  $effect(() => {
    if (!ready || prId === null) return;
    void attempt;
    const id = prId;
    let live = true;
    detail = null;
    missing = false;
    failed = null;
    view = undefined;
    viewFailed = null;
    repo = null;
    checkout = null;
    mine = false;
    me = null;
    runs = {};
    untrack(() => refresh.reset());
    ports.getEntity(id).then(
      (found) => {
        if (!live) return;
        detail = found;
        ports.listSources().then(
          (sources) => {
            if (!live) return;
            mine = isMine(found, sources);
            me = username(sources.find((source) => source.id === found.row.source_id));
          },
          () => {},
        );
        readRepo(found.row, () => live);
      },
      (error) => {
        if (live) {
          failed = ipcErrorMessage(error);
          missing = isIpcError(error) && error.code === "not_found";
        }
      },
    );
    readView(id, () => live);
    return () => {
      live = false;
    };
  });

  /** The surface the page's echoes are tagged with, for `watchEchoes` (#2103). */
  const PR_SURFACE = "pr";
  /** The newest read of the view asked: an older answer is dropped. */
  let viewAsked = 0;

  /**
   * The live view (ADR-0020); each echo its answer shows landed goes (#1542,
   * #2103). `again` is a re-read for the echoes, which keeps what the page
   * drew when it fails.
   */
  /** The commits on the Commits tab that are the reader's own: those read *You* (F-221). */
  let myCommits = $state<ReadonlySet<string>>(new Set());

  function readMyCommits(answer: SourceView | null, live: () => boolean) {
    const ids = commitParts(answer).flatMap((part) => ("commits" in part ? part.commits.flatMap((row) => (row.entity_id === null ? [] : [row.entity_id])) : []));
    if (ids.length === 0) {
      myCommits = new Set();
      return;
    }
    (ports.codeFacts ?? codeFacts)(ids).then(
      (facts) => {
        if (live()) myCommits = new Set(facts.filter((entry) => entry.mine).map((entry) => entry.entity_id));
      },
      () => {},
    );
  }

  function readView(id: string, live: () => boolean, again = false) {
    const asked = ++viewAsked;
    // A live read asked once a write was sent is a read of the source after it.
    const sentBefore = untrack(() => sentWrites(id));
    ports.entityView(id, null).then(
      (answer) => {
        if (!live() || asked !== viewAsked) return;
        view = answer;
        readMyCommits(answer, () => live() && asked === viewAsked);
        untrack(() => settleEchoes(id, (echo) => landedIn(answer, echo, sentBefore)));
        for (const row of headerOf(answer).checks) {
          const url = row.url;
          if (url === null) continue;
          ports.resolveUrl(url).then(
            (match) => {
              if (live() && match !== null) runs[url] = match;
            },
            () => {},
          );
        }
      },
      (error) => {
        if (!live() || asked !== viewAsked || again) return;
        view = null;
        viewFailed = ipcErrorMessage(error);
      },
    );
  }

  // The refresh after one of the page's writes, or a run of its source's end,
  // reads the live view again (#2103), so its echoes settle without the page
  // being opened again.
  $effect(() => {
    if (!ready) return;
    return untrack(() =>
      watchEchoes(
        PR_SURFACE,
        () => {
          const id = prId;
          if (id !== null && view != null) readView(id, () => prId === id, true);
        },
        { listen: ports.listen },
      ),
    );
  });

  /**
   * A refresh's re-read (#2107): the pull request's row and its live view,
   * over what the page drew; a failed read keeps it.
   */
  async function reread() {
    const id = prId;
    if (id === null) return;
    readView(id, () => prId === id, true);
    try {
      const found = await ports.getEntity(id);
      if (prId === id) detail = found;
    } catch {
      // The page keeps the row it drew.
    }
  }

  /** The repository the pull request is under: its declared parent, `owner/name`, in its source. */
  function readRepo(row: EntityRow, live: () => boolean) {
    if (row.path === null) return;
    const filter: EntityFilter = {
      sources: [row.source_id],
      kinds: ["repo"],
      updated_within_days: null,
      order: "updated_desc",
      context: null,
      project: null,
      include_deleted: false,
      reach: null,
    };
    // The source's repositories are read on past one read's rows until the
    // pull request's own turns up (#1825): its repository may sit anywhere.
    readAllRows(
      (offset) => ports.listEntities(filter, LIMIT, offset),
      (rows) => repoOfRow(row, rows) !== null,
    ).then(
      (rows) => {
        if (!live()) return;
        repo = repoOfRow(row, rows);
        if (repo === null) return;
        ports.entityCheckout(repo.entity_id).then(
          (answer) => {
            if (live()) checkout = answer;
          },
          () => {},
        );
      },
      () => {},
    );
  }

  const header = $derived(headerOf(view ?? null));
  const counts = $derived(tabCounts(view ?? null));
  const readyRows = $derived(readyLines(view ?? null));
  const standing = $derived(readiness(view ?? null));
  /** A button's label as words in a sentence or a title: the ellipsis is the button's alone. */
  const plainLabel = (label: string) => label.replace(/…$/, "");
  /** How a finished pull request ended, in one line; `null` while it is open. */
  const outcome = $derived.by(() => {
    const word = header.state?.text.toLowerCase();
    if (word === "merged") return { text: header.base === null ? "Merged" : `Merged into ${header.base}`, tone: "ok" };
    if (word === "closed") return { text: "Closed without merging", tone: "muted" };
    return null;
  });
  const reviewers = $derived(reviewersOf(view ?? null));
  const rail = $derived(railChecks(view ?? null));
  const conflicts = $derived(conflictsOf(view ?? null));
  const builds = $derived(detail?.builds ?? []);
  /** The head's builds that no check names: one that is both is already on the page, twice over. */
  const buildsBeyondChecks = $derived(builds.filter((build) => !rail.some((check) => check.row.title === build.title)));
  const number = $derived(prId === null ? null : prNumber(prId));
  const title = $derived(detail?.row.title ?? "");
  const conversation = $derived(conversationParts(view ?? null));
  const overview = $derived(overviewParts(view ?? null));
  const commitList = $derived(commitParts(view ?? null));
  const actions = $derived(view?.actions ?? []);
  const name = $derived(number === null ? "PR" : `PR ${number}`);
  // Its last part is what *Back to <name>* calls this page once it is left (spec #2227).
  const crumbs = $derived<Crumb[]>([
    { name: "Code", href: "#/code" },
    ...(repo !== null ? [{ name: repo.title, href: repoHash(repo.entity_id), code: true }] : detail?.row.path ? [{ name: detail.row.path, code: true }] : []),
    { name: prId === null || failed !== null ? "Pull request" : name },
  ]);
  // A pull request sits under its repository, as its crumb has it.
  parentPage(() => (repo === null ? null : { hash: repoHash(repo.entity_id), name: repo.title }));

  /** The write each toolbar button stands for, where the source offers it. */
  const offered = (test: (action: ViewAction) => boolean) => actions.find(test) ?? null;
  const merge = $derived(offered((a) => a.write_op === "merge_pull_request"));
  const comment = $derived(offered((a) => a.write_op === "comment"));

  /** A toolbar write this page queues through its dialog: the button's words and the verb its toast uses. */
  interface Write {
    action: ViewAction;
    label: string;
    /** `update branch of`, `edit`, `close`, `reopen`: what the toast says was queued, before the PR's name. */
    phrase: string;
    /** The dialog primary's words, where they are not the label. */
    primary?: string;
    /** The dialog's title, where the label and the name don't read as one. */
    heading?: string;
    /** `update`, `edit`, `close`, `reopen`, or a source's label in lower case: how the toast says it with what was chosen. */
    verb: string;
    /** What keys its button and its echo: the verb, or a source action's own id. */
    key: string;
  }

  /**
   * *Update branch*, *Edit*, then *Close* or *Reopen*, each where the source
   * offers it, then the source's own actions (#1194, ADR-0029) in its order:
   * their words are the source's label, and the shell never reads their op.
   */
  const writes = $derived.by((): Write[] => {
    const update = offered((a) => a.write_op === "update_pull_request_branch");
    const edit = offered((a) => a.write_op === "edit_pull_request" && a.values["state"] === undefined);
    const setState = offered((a) => a.write_op === "edit_pull_request" && a.values["state"] !== undefined);
    const list: Write[] = [];
    if (update) list.push({ action: update, label: "Update branch…", phrase: "update branch of", verb: "update", key: "update", heading: `Update the branch of ${name}` });
    if (edit) list.push({ action: edit, label: "Edit…", phrase: "edit", verb: "edit", key: "edit", primary: "Save changes" });
    if (setState) {
      const verb = setState.values["state"] === "closed" ? "close" : "reopen";
      list.push({
        action: setState,
        label: verb === "close" ? "Close…" : "Reopen…",
        phrase: verb,
        verb,
        key: verb,
        primary: verb === "close" ? "Close pull request" : "Reopen pull request",
      });
    }
    for (const own of actions.filter((a) => a.write_op === "source_action")) {
      const label = actionName(own.label);
      const verb = label.toLowerCase();
      // A source's action that asks for something first says so, as every label does (design system, Labels).
      list.push({ action: own, label: own.fields.length > 0 && !label.endsWith("…") ? `${label}…` : label, phrase: `${verb} on`, verb, key: `${SOURCE_ACTION}${own.values["id"] ?? label}` });
    }
    return list;
  });
  /** *Review…* stands for the verdicts, offered only on somebody else's open pull request. */
  const reviewable = $derived(
    actions.some((a) => a.write_op === "submit_review" && (a.values["verdict"] === "approve" || a.values["verdict"] === "request_changes")),
  );
  /** The reader's line comments waiting for a review (#842). */
  const waiting = $derived(prId === null ? 0 : pending.of(prId).length);
  /** Whether the source takes a review now: any of its `submit_review` actions. */
  const takesReview = $derived(actions.some((a) => a.write_op === "submit_review"));
  /** Why no verdict can be sent (a token that may not write, #2030): *Review…* is disabled like its neighbours, as its dialog could not finish. */
  const reviewRefused = $derived.by(() => {
    const choices = reviewChoices(actions, waiting, header.base);
    return choices.length > 0 && choices.every((choice) => choice.action === null) ? (choices.find((choice) => choice.why !== null)?.why ?? null) : null;
  });
  /** The ticket knobas links this pull request to, by its key, for *Merge…*'s In Review line. */
  const linkedTicket = $derived(detail?.links?.find((entry) => entry.other.kind === "ticket")?.other ?? null);
  const ticket = $derived(linkedTicket === null ? null : keyOf(linkedTicket.entity_id));
  let reviewing = $state(false);
  let merging = $state(false);
  /** The page, which ⌘F's bar finds in on any tab but the diff, which has its own. */
  let pageEl = $state<HTMLElement | null>(null);

  /** *Request…* in the Reviewers panel, where the source offers it to this reader (ADR-0020 rule 4). */
  const request = $derived(offered((a) => a.write_op === "request_reviewers"));
  /** Who *Request…* may ask: the source's choices, less those asked this session. */
  const askable = $derived.by(() => {
    const field = request?.fields.find((f) => f.name === "reviewers");
    if (field?.kind.type !== "choice" || prId === null) return [];
    const asked = new Set(echoesOn(prId, REQUESTED).map((echo) => echo.text));
    return field.kind.options.filter((option) => !asked.has(option.value));
  });
  let picking = $state(false);
  let requestFace = $state<HTMLButtonElement | null>(null);
  /** Where focus stands when the last person who could be asked was asked, and *Request…* went with them. */
  let reviewersPanel = $state<HTMLDivElement | null>(null);
  const pageShown = pageVisible();

  function closePick() {
    picking = false;
  }
  /** The name *Request…* shows for `account`, the login a request is sent by (F-107); the account where the source gave none. */
  function askedName(account: string): string {
    const field = request?.fields.find((f) => f.name === "reviewers");
    return (field?.kind.type === "choice" ? field.kind.options.find((option) => option.value === account)?.label : undefined) ?? account;
  }
  /** The rail's reviewers: the read's, then those asked here that it does not name yet. */
  const railReviewers = $derived.by(() => {
    const read = reviewers.map((reviewer) => ({ ...reviewer, note: null as string | null }));
    if (prId === null) return read;
    for (const echo of echoesOn(prId, REQUESTED))
      if (!read.some((reviewer) => reviewer.account === echo.text))
        read.push({ who: askedName(echo.text), account: echo.text, state: "Review requested", tone: "muted", note: echo.state });
    // The reader's own verdict, on its way: until the read holds it, this row says where it went (F-251).
    const verdict = echoOn(prId, "review");
    if (verdict === null) return read;
    return [
      ...read.filter((reviewer) => reviewer.account !== me),
      { who: me ?? "You", account: me ?? "", state: REVIEW_SENT[verdict.text as Verdict].state, tone: "info", note: verdict.state },
    ];
  });

  /** Ask `who` for a review (#2303): queued, drawn requested at once, and read back by the refresh after it. */
  async function requestReview(who: string) {
    const id = prId;
    if (id === null || request === null || request.disabled !== null) return;
    closePick();
    try {
      const write = await ports.submitWrite({ RequestReviewers: { entity: id, reviewers: [who] } });
      echoWrite(id, REQUESTED, who, { write, many: true, following: { name: "Review request" }, follower, surface: PR_SURFACE, source: detail?.source?.id ?? null });
      toastQueued(`review request to ${askedName(who)} on ${name}`, detail?.source ?? null, { write });
    } catch (cause) {
      push({ text: `Could not ask ${askedName(who)} for a review: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** The comments this session queued that the conversation did not hold when it was read. */
  const queuedComments = $derived(prId === null ? [] : echoesOn(prId, "comment"));
  /**
   * The chips of what the toolbar queued this session, by an allow-list (F-261): `chipAct`'s kinds and a review request. `TfPrFiles` queues
   * a thread's reply and resolve on this pull request too, and they draw in their thread.
   */
  const chips = $derived(prId === null ? [] : echoesOn(prId).filter((echo) => echo.kind === REQUESTED || chipAct(echo) !== null));

  /**
   * The writes whose landing the view's *State* reads (#1542): each goes once
   * the state moved from what it was when the write was queued. An edit, an
   * update, a review and a source's own action have no state to read: the
   * view is the source's own, read live (ADR-0020), so any read of it asked
   * once the write was sent shows it (#2103).
   */
  const STATE_READ = ["merge", "close", "reopen"];

  /** Whether the read in `answer` shows `echo` landed; `sentBefore` holds the writes sent when it was asked. */
  function landedIn(answer: SourceView | null, echo: Echo, sentBefore: ReadonlySet<number>): boolean {
    if (echo.kind === "comment") return conversationOf(answer).some((message) => sameWords(message.body, echo.text));
    // A review request (#2303) lands once the read names the reviewer.
    if (echo.kind === REQUESTED) return reviewersOf(answer).some((reviewer) => reviewer.account === echo.text);
    if (!STATE_READ.includes(echo.kind)) return echo.write !== null && sentBefore.has(echo.write);
    const state = headerOf(answer).state?.text ?? null;
    return state !== null && echo.base !== null && state !== echo.base;
  }

  // *Edit and resend* on a refused comment: the box again, with its words.
  $effect(() => {
    if (prId === null) return;
    const again = takeAgain(prId, "comment");
    if (again !== null) drafts.put(`comment:${prId}`, again);
  });

  /**
   * *Open PR #N* can sit right over *Merge…*, so the second press of a double click would open a dialog on landing. A press with
   * a still pointer this soon after arriving is the one before it. A pointer that moved here was aimed, so it goes at once.
   * `Enter` is safe: arriving puts focus on the title. Only the toolbar, as a press elsewhere on the page opens no dialog.
   */
  const SETTLING_MS = 800;
  let arrivedAt = 0;
  $effect(() => {
    void prId;
    if (pageShown()) arrivedAt = performance.now();
  });
  function holdArrivingPress(acts: HTMLElement) {
    const hold = (event: MouseEvent) => {
      if (event.detail > 0 && performance.now() - arrivedAt < SETTLING_MS) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    const aimed = (event: PointerEvent) => {
      if (event.movementX !== 0 || event.movementY !== 0) arrivedAt = -SETTLING_MS;
    };
    acts.addEventListener("click", hold, { capture: true });
    acts.addEventListener("pointermove", aimed);
    return () => {
      acts.removeEventListener("click", hold, { capture: true });
      acts.removeEventListener("pointermove", aimed);
    };
  }

  /** Echo a toolbar write under its `kind` (`close`, or `source:<id>`), followed under `name`. */
  function echoChip(id: string, kind: string, name: string, write: QueuedWrite, chose = "") {
    const own = kind.startsWith(SOURCE_ACTION);
    // A source's action says its button's words, and what was chosen names it: *Add label backport*, and a second label is a second chip.
    const text = own && chose !== "" ? `${name} ${chose}` : own ? name : kind;
    echoWrite(id, kind, text, { write, base: header.state?.text ?? null, following: { name }, follower, surface: PR_SURFACE, source: detail?.source?.id ?? null, ...(own && chose !== "" ? { many: true } : {}) });
  }

  /** Echo a review queued with `verdict`: its chip and its row under Reviewers say which (F-251). */
  function echoReview(id: string, verdict: Verdict, write: QueuedWrite) {
    echoWrite(id, "review", verdict, { write, base: header.state?.text ?? null, following: { name: "Review" }, follower, surface: PR_SURFACE, source: detail?.source?.id ?? null });
  }

  /**
   * The chips that hold each toolbar write back (#1441), as the Branches tab
   * drops a queued delete's actions: a queued merge makes every write here
   * stale, and a queued close or reopen makes *Merge…* and another state
   * change stale. A source's own actions are never held.
   */
  const STATE_CHANGES = ["close", "reopen"];
  /** By a write's key, the verbs whose queued write holds it back. */
  const HOLDS: Record<string, readonly string[]> = {
    merge: ["merge", ...STATE_CHANGES],
    update: ["merge"],
    edit: ["merge"],
    close: ["merge", ...STATE_CHANGES],
    reopen: ["merge", ...STATE_CHANGES],
  };

  /** The chip an echo draws, which is also what holds a button back: what it names, then where its write is (#2103). */
  function chipOf(echo: Echo): string {
    return chipWords(echo);
  }

  /** The chip this session queued that holds the write keyed `key` back, or null while it is live. */
  function heldBy(key: string): string | null {
    if (prId === null) return null;
    const id = prId;
    for (const verb of HOLDS[key] ?? []) {
      const echo = echoOn(id, verb);
      if (echo !== null) return chipOf(echo);
    }
    return null;
  }

  /** Where the reader's checkout stands against this pull request's head. */
  const local = $derived.by(() => {
    const tree = checkout?.tree;
    if (checkout?.path == null || !tree || tree.status !== "read") return null;
    const changed = tree.changed + tree.untracked;
    const where = tree.branch !== null && tree.branch === header.head ? "On this branch" : `On ${tree.branch ?? "a detached head"}`;
    const files = changed === 0 ? "clean" : `${changed} changed ${changed === 1 ? "file" : "files"}`;
    return { path: checkout.path, line: `${where}, ${files}` };
  });

  /** The write whose dialog is open. */
  let shown = $state<Write | null>(null);

  function open(write: Write) {
    if (write.action.disabled === null && heldBy(write.key) === null) shown = write;
  }

  /** The composer's words are this pull request's own draft, through `Esc`, a move to another and a return (the key table). */
  const draft = $derived(prId === null ? "" : drafts.get(`comment:${prId}`));
  let sending = $state(false);
  let composer = $state<HTMLTextAreaElement | null>(null);
  /**
   * A second press with a still pointer lands on *Comment*, disabled by now, whose press gives focus to what holds it: it goes
   * back to the box. Only with the pointer on *Comment*, because `Esc` steps out to the same holder, and that step must land.
   */
  let sentAt = 0;
  function keepInComposer(event: FocusEvent) {
    const holder = event.currentTarget as HTMLElement;
    if (event.target === holder && Date.now() - sentAt < BY_POINTER_MS && holder.querySelector("button:hover") !== null) composer?.focus({ preventScroll: true });
  }

  async function send() {
    const body = draft.trim();
    if (prId === null || body === "" || sending || (comment?.disabled ?? null) !== null) return;
    sending = true;
    // *Comment* goes off with its draft, so focus stays in the box, whichever way this was sent (F-266).
    composer?.focus({ preventScroll: true });
    sentAt = Date.now();
    const source = detail?.source ?? null;
    try {
      const id = prId;
      const write = await ports.submitWrite({ Comment: { entity: id, body } });
      echoWrite(id, "comment", body, {
        write,
        many: true,
        surface: PR_SURFACE,
        source: source?.id ?? null,
        following: {
          name: "Comment",
          resend: () => {
            askAgain(id, "comment", body);
            location.hash = prHash(id, "conversation");
          },
        },
        follower,
      });
      toastQueued(`comment on ${name}`, source, { write });
      drafts.forget(`comment:${id}`);
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      sending = false;
    }
    // The comment has just taken room above the box, which focus came back to: all of it in view, *Comment* too (F-162).
    await tick();
    composer?.closest(".tf-compose")?.scrollIntoView?.({ block: "nearest" });
  }

  /** A check's row opens its run where the mirror holds it. */
  function runOf(row: ViewRow): string | null {
    const match = row.url === null ? undefined : runs[row.url];
    return match ? itemHash(match) : null;
  }

  function openCommit(event: MouseEvent, row: ViewRow) {
    if (row.entity_id === null || repo === null || (event.target as Element | null)?.closest("a")) return;
    location.hash = commitHash(repo.entity_id, row.entity_id);
  }
</script>

{#snippet mutedLine(words: string)}
  <!-- A muted line the source wrote into a tab, where it put it (#2065). -->
  <p class="tf-dim tf-prline">{words}</p>
{/snippet}

<div class="tf-full" bind:this={pageEl}>
  <TfCrumbs parts={crumbs} {back} />

  {#if prId === null || failed !== null}
    {#if failed !== null && !missing}
      <TfNothingHere message="This pull request could not be read: {failed}" onretry={() => attempt++} />
    {:else}
      <TfNothingHere message="knobas does not hold this pull request. It may not have synced yet." href="#/code/prs" action="All pull requests" />
    {/if}
  {:else}
    <header class="tf-fhead">
      <div>
        <h1>{name} {title}</h1>
        <div class="tf-sub">
          {#if header.state !== null}<TfStateChip text={header.state.text} tone={header.state.tone ?? 'muted'} />{/if}
          {#each detail?.labels ?? [] as label (label)}
            {" "}<span class="tf-lab">{label}</span>
          {/each}
          {#if header.head !== null}
            {" "}<span class="tf-mono">{header.head}</span>{#if header.base !== null}{" "}into <span class="tf-mono">{header.base}</span>{/if}{#if header.author !== null},{/if}
          {/if}
          {#if header.author !== null}{" "}by {mine ? "you" : header.author}{/if}
          {#if ticket !== null && linkedTicket !== null}{" "}<TfKey chip value={ticket} href={itemHash(linkedTicket)} />{/if}
        </div>
      </div>
      <div class="tf-acts" {@attach holdArrivingPress}>
        {#if merge !== null}
          {@const hold = heldBy("merge")}
          <!-- The primary comes first (#2238 header order). Blocked by the source it is a default button, enabled, because it opens a dialog that names each reason (#843); held by a queued write, it opens nothing and looks it. -->
          <button
            type="button"
            class="tf-btn"
            class:tf-primary={hold === null && merge.disabled === null}
            aria-disabled={hold !== null ? "true" : undefined}
            title={hold ?? merge.disabled ?? undefined}
            onclick={() => {
              if (hold === null) merging = true;
            }}>Merge…</button
          >
        {/if}
        {#if reviewable}
          <button
            type="button"
            class="tf-btn"
            aria-disabled={reviewRefused !== null ? "true" : undefined}
            title={reviewRefused ?? undefined}
            onclick={() => {
              if (reviewRefused === null) reviewing = true;
            }}>Review…</button
          >
        {/if}
        {#each writes as write (write.key)}
          {@const why = heldBy(write.key) ?? write.action.disabled}
          <button
            type="button"
            class="tf-btn"
            aria-disabled={why !== null ? "true" : undefined}
            title={why ?? undefined}
            onclick={() => open(write)}>{write.label}</button
          >
        {/each}
        {#if repo !== null && number !== null && header.base !== null && header.head !== null}
          <TfOpenObject
            entityId={repo.entity_id}
            size="md"
            object={{ kind: "pull_request", number: Number(number.slice(1)), base: header.base, head: header.head, branch: header.head, item: prId }}
            ends={() => (prId === null ? Promise.resolve(null) : ports.entityView(prId, "files").then(rangeOf))}
          />
        {/if}
        {#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page />{/if}
      </div>
    </header>
    {#if detail !== null}<TfDetailBanners {detail} {refresh} {reread} />{/if}
    <nav class="tf-tabs" aria-label="Pull request">
      {#each PR_TABS as entry (entry.id)}
        {@const count = counts[entry.id]}
        <a href={prHash(prId, entry.id)} aria-current={tabKnown && tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null && count > 0}{" "}<span class="tf-count">{count}</span>{/if}</a
        >
      {/each}
      {#if chips.length > 0 || (waiting > 0 && takesReview)}
        <!-- Under the buttons they answer, and on a row whose height nothing here changes: in the meta line a chip wrapped it, and in the toolbar
             *Submit review…* took the title's room, and either moved the tabs under a reader in the middle of a review (F-206, F-254). -->
        <span class="tf-prchips">
          <!-- Keyed on the write: the same words pressed twice are two writes, and a shared key would blank the page. Each keeps its whole words in its title, as a long one ends in an ellipsis rather than scroll the tabs (F-261). -->
          {#each chips as echo (echo.write ?? `${echo.kind}\n${echo.text}`)}
            <!-- A review request names the act and the login as written: a state word would turn it into *Sven sent*. -->
            {#if echo.kind === REQUESTED}<span class="tf-chip tf-info" title="Review request {echo.state}">Review asked of {echo.text}</span>{:else}<span class="tf-chip tf-info" title={chipOf(echo)}>{chipOf(echo)}</span>{/if}
          {/each}
          {#if waiting > 0 && takesReview}
            <button
              type="button"
              class="tf-btn tf-sm"
              class:tf-warnbtn={reviewRefused === null}
              aria-disabled={reviewRefused !== null ? "true" : undefined}
              title={reviewRefused ?? undefined}
              onclick={() => {
                if (reviewRefused === null) reviewing = true;
              }}>Submit review… ({waiting} pending)</button
            >
          {/if}
        </span>
      {/if}
    </nav>
    {#if !tabKnown}
      <TfNothingHere message="This pull request has no such tab." href={prHash(prId)} action="Overview" />
    {:else}
    <div class="tf-cbody" class:tf-pr={tab !== "diff"}>
      {#if tab === "diff"}
        <TfPrFiles
          {prId}
          entityView={ports.entityView}
          viewedFiles={ports.prViewedFiles}
          setViewed={ports.setPrFileViewed}
          {pending}
          submitWrite={ports.submitWrite}
          source={detail?.source?.id ?? null}
          listen={ports.listen}
          {follower}
          openAt={repo === null ? null : { entityId: repo.entity_id, at: header.head }}
          {me}
        />
      {:else}
        <div>
          <TfFindBar root={() => pageEl} />
          {#if viewFailed !== null}
            <p class="tf-cfail">Could not read the pull request: {viewFailed}</p>
          {:else if view === null}
            <div class="tf-empty">This source draws nothing for a pull request.</div>
          {:else if view === undefined}
            <!-- The view has not answered yet. -->
          {:else if tab === "overview"}
            {#each overview as part, p (p)}
              {#if "line" in part}
                {@render mutedLine(part.line)}
              {:else if "checks" in part}
                <h2 class="tf-cgroup-h">Checks</h2>
                <table class="tf-tbl tf-prchecks">
                  <tbody>
                    {#each part.checks as row, n (n)}
                      {@const run = runOf(row)}
                      <tr class:tf-row={run !== null} onclick={() => run !== null && (location.hash = run)}>
                        <td
                          ><span class="tf-dot tf-{row.tone ?? 'muted'}"></span>
                          {#if run !== null}<a href={run}>{row.title}</a>{:else}{row.title}{/if}{#if rail.some((check) => check.row === row && check.required)}{" "}<span class="tf-prreq">Required</span>{/if}</td
                        >
                        <td>{checkReading(row)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {:else}
                <h2 class="tf-cgroup-h">Description</h2>
                <div class="tf-panel tf-prdesc">
                  {#if part.markdown}<TfMarkdown text={part.description} />{:else}<p>{part.description}</p>{/if}
                </div>
              {/if}
            {/each}
          {:else if tab === "conversation"}
            <div class="tf-convo tf-convo-full">
              {#each conversation as part, n (n)}
                {#if "line" in part}
                  {@render mutedLine(part.line)}
                {:else}
                {@const message = part.message}
                {@const own = me !== null && message.account === me}
                <article class="tf-msg">
                  <span class="tf-av" class:tf-me={own} aria-hidden="true">{initials(message.who)}</span>
                  <div class="tf-msg-body">
                    <div class="tf-who">
                      <b>{own ? "You" : message.who}</b>{message.at}
                      {#if message.verdict !== null}<TfStateChip text={message.verdict.text} tone={message.verdict.tone ?? 'muted'} />{/if}
                    </div>
                    {#if message.body !== "" && message.markdown}<TfMarkdown text={message.body} />{:else if message.body !== ""}<p>{message.body}</p>{/if}
                  </div>
                </article>
                {/if}
              {/each}
              {#each queuedComments as echo, n (n)}
                <article class="tf-msg tf-queued">
                  <span class="tf-av tf-me" aria-hidden="true">{initials(me)}</span>
                  <div class="tf-msg-body">
                    <div class="tf-who"><b>You</b><TfStateChip text={echo.state} tone="info" /></div>
                    <p>{echo.text}</p>
                  </div>
                </article>
              {/each}
              {#if conversation.length === 0 && queuedComments.length === 0}
                <div class="tf-empty">No comments yet.</div>
              {/if}
              {#if comment !== null}
                <div class="tf-compose" tabindex="-1" onfocus={keepInComposer}>
                  <textarea
                    bind:this={composer}
                    value={draft}
                    oninput={(event) => prId !== null && drafts.put(`comment:${prId}`, event.currentTarget.value)}
                    placeholder="Comment on {name}"
                    aria-label="Comment on {name}"
                    {@attach sendKey(() => void send())}
                  ></textarea>
                  <div class="tf-acts">
                    <button
                      type="button"
                      class="tf-btn tf-sm tf-primary"
                      disabled={comment.disabled === null && (draft.trim() === "" || sending)}
                      {...refusedAttrs(comment.disabled)}
                      onclick={() => void send()}>Comment</button
                    >
                  </div>
                </div>
              {/if}
            </div>
          {:else if tab === "commits"}
            {#each commitList as part, p (p)}
              {#if "line" in part}
                {@render mutedLine(part.line)}
              {:else}
                <table class="tf-tbl tf-rcommits">
                  <tbody>
                    {#each part.commits as row, n (n)}
                      {@const opens = row.entity_id !== null && repo !== null}
                      <tr class:tf-row={opens} onclick={(event) => openCommit(event, row)}>
                        <td class="tf-mono"
                          >{#if opens}<a href={commitHash(repo!.entity_id, row.entity_id!)}>{row.chip ?? ""}</a>{:else}{row.chip ?? ""}{/if}</td
                        >
                        <td class:tf-dim={row.chip === null}>{row.title}</td>
                        <td class="tf-dim tf-nowrap">{commitLine(row, row.entity_id !== null && myCommits.has(row.entity_id))}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            {:else}
              <div class="tf-empty">No commits.</div>
            {/each}
          {/if}
        </div>
        <aside class="tf-side" aria-label="Pull request status">
          {#if standing.lines.length === 0}
            <!-- The question makes no sense on a finished pull request: the panel says how it ended, or is not drawn. -->
            {#if merge === null && outcome !== null}
              <div class="tf-panel">
                <header><h3>Outcome</h3></header>
                <div class="tf-prready"><div class="tf-prverdict tf-{outcome.tone}">{outcome.text}</div></div>
              </div>
            {/if}
          {:else}
          <div class="tf-panel">
            <header><h3>Ready to merge?</h3></header>
            <div class="tf-prready">
              {#if standing.verdict !== null}
                <div class="tf-prverdict tf-{standing.verdict.tone}">
                  {standing.verdict.text}{#if standing.verdict.why !== null}<span class="tf-prwhy">{standing.verdict.why}</span>{/if}
                </div>
              {/if}
              {#each standing.lines as line, n (n)}
                <div class="tf-prck tf-{line.tone}">
                  <span class="tf-prmk" aria-hidden="true">{line.tone === "ok" ? "✓" : line.tone === "err" ? "✕" : "●"}</span>
                  <span>{line.text}{#if line.why !== undefined}<span class="tf-prwhy">{line.why}</span>{/if}</span>
                </div>
              {/each}
            </div>
          </div>
          {/if}
          {#if view}
            <div class="tf-panel tf-prreviewers" tabindex="-1" bind:this={reviewersPanel}>
              <header
                ><h3>Reviewers</h3>
                {#if request !== null && askable.length > 0}
                  <div class="tf-acts">
                    <button
                      type="button"
                      class="tf-btn tf-sm tf-ghost"
                      bind:this={requestFace}
                      aria-expanded={picking}
                      aria-disabled={request.disabled !== null ? "true" : undefined}
                      title={request.disabled ?? undefined}
                      onclick={() => {
                        if (request.disabled === null) picking = !picking;
                      }}>Request…</button
                    >
                  </div>
                {/if}</header
              >
              {#if picking && request !== null}
                <!-- `Esc` closes the list before it leaves the page. A page keep-alive holds hidden answers to no press. -->
                <div class="tf-prpick" role="group" aria-labelledby="tf-prpick-h" {@attach pageShown() && inlineEditor(closePick, { back: () => requestFace ?? reviewersPanel })}>
                  <p class="tf-prpick-h" id="tf-prpick-h">Ask for a review</p>
                  {#each askable as option (option.value)}
                    <button type="button" onclick={() => void requestReview(option.value)}
                      ><span class="tf-av" aria-hidden="true">{initials(option.label)}</span> {option.label}</button
                    >
                  {/each}
                </div>
              {/if}
              {#if railReviewers.length === 0 && waiting === 0}
                <p class="tf-prnote">No reviews yet.</p>
              {:else}
                <div class="tf-prlist">
                  {#each railReviewers as reviewer (reviewer.account)}
                    {@const own = me !== null && reviewer.account === me}
                    <div class="tf-prit">
                      <span class="tf-av" class:tf-me={own} aria-hidden="true">{initials(reviewer.who)}</span>
                      <span class="tf-prgrow">{own ? "You" : reviewer.who}{#if reviewer.note !== null}<span class="tf-dim">{` · ${reviewer.note}`}</span>{/if}</span>
                      <TfStateChip text={reviewer.state} tone={reviewer.tone ?? 'muted'} />
                    </div>
                  {/each}
                  {#if waiting > 0}
                    <!-- The reader's own unsent review (#2276): its pending comments, kept across restarts. -->
                    <div class="tf-prit">
                      <span class="tf-av tf-me" aria-hidden="true">{initials(me ?? "you")}</span>
                      <span class="tf-prgrow">You <span class="tf-dim">· {waiting} pending {waiting === 1 ? "comment" : "comments"}</span></span>
                      <TfStateChip text="Draft" tone="warn" />
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
            <!-- Overview's own list has these in full: the rail keeps them in reach on the other tabs. -->
            {#if rail.length > 0 && tab !== "overview"}
              <div class="tf-panel tf-prrail-checks">
                <header><h3>Checks</h3></header>
                <div class="tf-prlist">
                  {#each rail as check, n (n)}
                    {@const run = runOf(check.row)}
                    <div class="tf-prit">
                      <span class="tf-dot tf-{check.row.tone ?? 'muted'}"></span>
                      <span class="tf-prgrow" title={check.row.title}>{#if run !== null}<a href={run}>{check.row.title}</a>{:else}{check.row.title}{/if}</span>
                      {#if check.required}<span class="tf-prreq">Required</span>{/if}
                      <span class="tf-dim">{stateWord(check.row.chip ?? "")}</span>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
            {#if buildsBeyondChecks.length > 0}
              <div class="tf-panel tf-prbuilds">
                <header><h3>Builds of the head</h3><span class="tf-dim">from the mirror</span></header>
                <div class="tf-prlist">
                  {#each buildsBeyondChecks as build (build.entity_id)}
                    <div class="tf-prit">
                      <span class="tf-dot tf-{build.state?.tone ?? 'muted'}"></span>
                      <span class="tf-prgrow" title={build.title}><a href={itemHash(build)}>{build.title}</a></span>
                      {#if build.state !== null}<span class="tf-dim">{stateWord(build.state.text)}</span>{/if}
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
            <!-- With none, Ready to merge says so: a panel to say it again adds nothing. -->
            {#if conflicts !== null && conflicts.conflicts}
              <div class="tf-panel tf-prconflicts">
                <header><h3>Conflicts</h3></header>
                <p class="tf-prnote" class:tf-prconflict={conflicts.conflicts}>
                  {conflicts.text}.{#if conflicts.conflicts}{" "}Resolve them in a checkout and push; they cannot be resolved here.{/if}
                </p>
              </div>
            {/if}
          {/if}
          {#if local !== null && repo !== null}
            <div class="tf-panel">
              <header><h3>Local checkout</h3></header>
              <div class="tf-prlocal">
                <span class="tf-mono">{local.path}</span>
                <span>{local.line}</span>
                <div class="tf-prlocal-acts">
                  <TfOpenIn entityId={repo.entity_id} title={repo.title} checkout={checkout ?? undefined} />
                  <a class="tf-btn tf-sm tf-ghost" href={repoHash(repo.entity_id, "local")}>Local changes</a>
                </div>
              </div>
            </div>
          {/if}
        </aside>
      {/if}
    </div>
    {/if}
  {/if}
</div>

{#if reviewing && prId !== null}
  <TfPrReview
    {actions}
    {prId}
    {name}
    base={header.base}
    {pending}
    source={detail?.source ?? null}
    submitWrite={ports.submitWrite}
    onqueued={(write, verdict) => echoReview(prId, verdict, write)}
    onclose={() => (reviewing = false)}
    headOf={() => ports.entityView(prId, "files").then((answer) => diffOf(answer)?.commentable?.head ?? null)}
  />
{/if}

{#if merging && merge !== null && prId !== null}
  <TfPrMerge
    action={merge}
    {prId}
    {name}
    head={header.head}
    base={header.base}
    source={detail?.source ?? null}
    ready={readyRows}
    {ticket}
    submitWrite={ports.submitWrite}
    onqueued={(write) => echoChip(prId, "merge", "Merge", write)}
    onclose={() => (merging = false)}
  />
{/if}

{#if shown !== null && prId !== null}
  {@const write = shown}
  <TfEntityAction
    action={write.action}
    entityId={prId}
    title={write.heading ?? (/^add\b/i.test(write.label) ? `${plainLabel(write.label)} to ${name}` : `${plainLabel(write.label)} ${name}`)}
    what={(chose) => (chose === "" || !write.key.startsWith(SOURCE_ACTION) ? `${write.phrase} ${name}` : `${write.verb} ${chose} on ${name}`)}
    primary={write.primary ?? null}
    source={detail?.source ?? null}
    submitWrite={ports.submitWrite}
    onqueued={(queued, chose) => echoChip(prId, write.key, plainLabel(write.label), queued, chose)}
    onclose={() => (shown = null)}
  />
{/if}
