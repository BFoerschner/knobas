<!--
  The pull request page (#841, R-8.15, R-8.16, R-8.20), prototype-6's
  `renderPR`: `#/pr/<entity id>` and its tabs `/conversation`, `/diff` and
  `/commits`, which share the header and the one read.

  - **The header**: `PR #<n> <title>`, its state, head into base and author
    (*you* for the reader's own); the toolbar's *Review…*, or *Submit review
    (N pending)* in warn while line comments wait (#842, `TfPrReview`),
    *Update branch*, *Edit*, *Close* (or *Reopen*) and *Merge…* (#843,
    `TfPrMerge`). A write is drawn only where the source's view offers it
    (ADR-0020 rule 4).
  - **Overview**: the head commit's checks (a check whose link the mirror
    holds opens its run) and the description; beside it *Ready to merge?*
    (the source's *Approvals*, the checks and *Conflicts*, #994) and, when
    the repository is cloned, the local checkout.
  - **Conversation**: each comment and review in time order, and a comment
    box that queues `Comment`; *Comment* is drawn disabled with the source's
    reason where the token may not comment (#2030), and a press queues nothing.
  - **Files changed**: `TfPrFiles` (#842), which reads the deferred *Files*
    section when the tab opens.
  - **Commits**: the pull request's commits; one the mirror holds opens its
    diff on the repository page, and one with a proposed link says *proposed
    link, judge it*, which opens the proposal.

  Everything is the source's view of the pull request (`entity_view`), read
  once per pull request (its *Files* once more, when that tab opens); the
  tabs only choose what to draw of it. A muted line the source writes into a
  tab (comments the token may not read, checks it could not read) is drawn
  as a line where the source put it, on every tab (#2065).
-->
<script lang="ts" module>
  import type {
    CheckoutView,
    EntityDetail,
    EntityFilter,
    EntityPage,
    SourceView,
    SuggestionPage,
    UrlMatch,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { SourceFacts } from "../job/job";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface PrPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    entityCheckout: (entityId: string) => Promise<CheckoutView>;
    roomSuggestions: (sources: string[], ctx: string | null, limit: number) => Promise<SuggestionPage>;
    /** A check's link, to the run the mirror holds; never a pasted text. */
    resolveUrl: (url: string) => Promise<UrlMatch | null>;
    listSources: () => Promise<SourceFacts[]>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** *Viewed* on the pull request's files (#842). */
    prViewedFiles: (prId: string) => Promise<string[]>;
    setPrFileViewed: (prId: string, path: string, viewed: boolean) => Promise<string[]>;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    entityCheckout,
    entityView,
    getEntity,
    listEntities,
    prViewedFiles,
    resolveUrl,
    roomSuggestions,
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
  import { commitHash, readAllRows, repoHash, repoOfRow } from "../repo/repo";
  import { jobHash } from "../route";
  import { initials } from "../tickets/tickets";
  import {
    PR_TABS,
    commitParts,
    conversationOf,
    conversationParts,
    headerOf,
    overviewParts,
    prHash,
    readPrRoute,
    readyLines,
    tabCounts,
    type PrTab,
  } from "./pr";
  import { askAgain, echoWrite, echoOn, echoesOn, sameWords, settleEchoes, takeAgain, type Echo } from "../queued.svelte";
  import TfEntityAction from "../TfEntityAction.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfPrFiles from "./TfPrFiles.svelte";
  import TfPrMerge from "./TfPrMerge.svelte";
  import TfPrReview from "./TfPrReview.svelte";

  let {
    ready,
    rest,
    job,
    jobAt = null,
    ports = {
      getEntity: (entityId) => getEntity(entityId),
      entityView: (entityId, section) => entityView(entityId, section),
      listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
      entityCheckout: (entityId) => entityCheckout(entityId),
      roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit),
      resolveUrl: (url) => resolveUrl(url),
      listSources: () => listSources(),
      submitWrite: (payload) => submitWrite(payload),
      prViewedFiles: (id) => prViewedFiles(id),
      setPrFileViewed: (id, path, viewed) => setPrFileViewed(id, path, viewed),
    },
    pending = sharedPending,
    follower = writeFollower(),
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `pr` in the address. */
    rest: string[];
    /** The job the reader was last in, as a ticket key. */
    job: string | null;
    /** The address the job was opened at (#1525), which its chip links; its key when omitted. */
    jobAt?: string | null;
    ports?: PrPorts;
    /** The reader's pending line comments (#842): this viewer's, kept across restarts. */
    pending?: PendingCommentStore;
    /** Follows each queued write, so one held or refused drops its mark (#1424). */
    follower?: WriteFollower;
  } = $props();

  /** How many repositories one read asks for: a source with more is read on with `offset` (#1825). */
  const LIMIT = 1000;
  /** The proposals one read answers: `room_suggestions` takes no offset and answers at most 500 (`ROW_CEILING`, #1643), as many as Review reads. */
  const PROPOSALS = 500;

  const route = $derived(readPrRoute(rest));
  const prId = $derived(route.prId);
  const tab = $derived<PrTab>(route.tab);

  let detail = $state<EntityDetail | null>(null);
  let failed = $state<string | null>(null);
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
  /** The proposal waiting on each commit, by the commit's entity id. */
  let proposals = $state<Record<string, string>>({});

  $effect(() => {
    if (!ready || prId === null) return;
    const id = prId;
    let live = true;
    detail = null;
    failed = null;
    view = undefined;
    viewFailed = null;
    repo = null;
    checkout = null;
    mine = false;
    me = null;
    runs = {};
    proposals = {};
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
        if (live) failed = ipcErrorMessage(error);
      },
    );
    ports.entityView(id, null).then(
      (answer) => {
        if (!live) return;
        view = answer;
        for (const row of headerOf(answer).checks) {
          const url = row.url;
          if (url === null) continue;
          ports.resolveUrl(url).then(
            (match) => {
              if (live && match !== null) runs[url] = match;
            },
            () => {},
          );
        }
      },
      (error) => {
        if (!live) return;
        view = null;
        viewFailed = ipcErrorMessage(error);
      },
    );
    ports.roomSuggestions([], null, PROPOSALS).then(
      (page) => {
        if (!live) return;
        const found: Record<string, string> = {};
        for (const entry of page.rows) {
          for (const end of [entry.from, entry.to]) if (end.kind === "commit") found[end.entity_id] ??= entry.link.id;
        }
        proposals = found;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

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
  const number = $derived(prId === null ? null : prNumber(prId));
  const title = $derived(detail?.row.title ?? "");
  const conversation = $derived(conversationParts(view ?? null));
  const overview = $derived(overviewParts(view ?? null));
  const commitList = $derived(commitParts(view ?? null));
  const actions = $derived(view?.actions ?? []);
  const name = $derived(number === null ? "PR" : `PR ${number}`);

  /** The write each toolbar button stands for, where the source offers it. */
  const offered = (test: (action: ViewAction) => boolean) => actions.find(test) ?? null;
  const merge = $derived(offered((a) => a.write_op === "merge_pull_request"));
  const comment = $derived(offered((a) => a.write_op === "comment"));

  /** A toolbar write this page queues through its dialog: the button's words and the verb its toast and chip use. */
  interface Write {
    action: ViewAction;
    label: string;
    /** `update branch of`, `edit`, `close`, `reopen`: what the toast says was queued, before the PR's name. */
    phrase: string;
    /** `update`, `edit`, `close`, `reopen`: the chip is `<verb> queued`. */
    verb: string;
    /** What keys its button: the verb, or a source action's own id. */
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
    if (update) list.push({ action: update, label: "Update branch", phrase: "update branch of", verb: "update", key: "update" });
    if (edit) list.push({ action: edit, label: "Edit", phrase: "edit", verb: "edit", key: "edit" });
    if (setState) {
      const verb = setState.values["state"] === "closed" ? "close" : "reopen";
      list.push({ action: setState, label: verb === "close" ? "Close" : "Reopen", phrase: verb, verb, key: verb });
    }
    for (const own of actions.filter((a) => a.write_op === "source_action")) {
      const label = actionName(own.label);
      const verb = label.toLowerCase();
      list.push({ action: own, label, phrase: `${verb} on`, verb, key: `source:${own.values["id"] ?? label}` });
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
  /** The ticket knobas links this pull request to, by its key, for *Merge…*'s In Review line. */
  const ticket = $derived.by(() => {
    const linked = detail?.links.find((entry) => entry.other.kind === "ticket");
    return linked ? keyOf(linked.other.entity_id) : null;
  });
  let reviewing = $state(false);
  let merging = $state(false);

  /** The comments this session queued that the conversation did not hold when it was read. */
  const queuedComments = $derived(prId === null ? [] : echoesOn(prId, "comment").map((mark) => mark.text));
  /** The chips of what this session queued here, beside the state: `close queued`. */
  const chips = $derived(prId === null ? [] : echoesOn(prId).filter((mark) => mark.kind !== "comment").map((mark) => mark.text));

  /**
   * The writes whose landing the view's *State* reads (#1542): each goes once
   * the state moved from what it was when the write was queued. An edit, an
   * update, a review and a source's own action have no state to read, so their
   * chip lasts the session unless the queue holds or refuses the write.
   */
  const STATE_READ = ["merge", "close", "reopen"];

  /** Whether the read in `answer` shows `mark` landed. */
  function landedIn(answer: SourceView, mark: Echo): boolean {
    if (mark.kind === "comment") return conversationOf(answer).some((message) => sameWords(message.body, mark.text));
    const state = headerOf(answer).state?.text ?? null;
    return STATE_READ.includes(mark.kind) && state !== null && mark.base !== null && state !== mark.base;
  }

  // Each read of the view drops what it shows landed.
  $effect(() => {
    const answer = view;
    const id = prId;
    if (id === null || answer == null) return;
    untrack(() => settleEchoes(id, (mark) => landedIn(answer, mark)));
  });

  // *Edit and resend* on a refused comment: the box again, with its words.
  $effect(() => {
    if (prId === null) return;
    const again = takeAgain(prId, "comment");
    if (again !== null) draft = again;
  });

  /** Mark a write queued as `verb`, followed under `name`. */
  function markChip(id: string, verb: string, name: string, write: QueuedWrite) {
    echoWrite(id, verb, chipOf(verb), { write, base: header.state?.text ?? null, following: { name }, follower });
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

  /** The chip a write queued as `verb` draws, which is also what holds a button back. */
  const chipOf = (verb: string) => `${verb} queued`;

  /** The chip this session queued that holds the write keyed `key` back, or null while it is live. */
  function heldBy(key: string): string | null {
    if (prId === null) return null;
    const id = prId;
    for (const verb of HOLDS[key] ?? []) {
      const mark = echoOn(id, verb);
      if (mark !== null) return mark.text;
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

  let draft = $state("");
  let sending = $state(false);

  async function send() {
    const body = draft.trim();
    if (prId === null || body === "" || sending || (comment?.disabled ?? null) !== null) return;
    sending = true;
    const source = detail?.source ?? null;
    try {
      const id = prId;
      const write = await ports.submitWrite({ Comment: { entity: id, body } });
      echoWrite(id, "comment", body, {
        write,
        many: true,
        following: {
          name: "Comment",
          resend: () => {
            askAgain(id, "comment", body);
            location.hash = prHash(id, "conversation");
          },
        },
        follower,
      });
      toastQueued(`comment on ${name}`, source);
      draft = "";
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      sending = false;
    }
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

<div class="tf-full">
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#if job !== null}
      <a class="tf-jobchip tf-card" href={jobHash(jobAt ?? job)}>{job}</a><span class="tf-sep">›</span>
    {/if}
    <a class="tf-crumb" href="#/code">Code</a><span class="tf-sep">›</span>
    {#if repo !== null}
      <a class="tf-crumb" href={repoHash(repo.entity_id)}>{repo.title}</a><span class="tf-sep">›</span>
    {:else if detail?.row.path}
      <span class="tf-crumb">{detail.row.path}</span><span class="tf-sep">›</span>
    {/if}
    <strong>{name}</strong>
    <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
  </nav>

  {#if prId === null || failed !== null}
    <div class="tf-cbody">
      <p class="tf-cfail">{failed ?? "No pull request is named in the address."}</p>
    </div>
  {:else}
    <header class="tf-fhead">
      <div>
        <h1>{name} {title}</h1>
        <div class="tf-sub">
          {#if header.state !== null}<span class="tf-chip tf-{header.state.tone ?? 'muted'}">{header.state.text}</span>{/if}
          {#each chips as chip (chip)}{" "}<span class="tf-chip tf-warn">{chip}</span>{/each}
          {#if header.head !== null}
            {" "}<span class="tf-mono">{header.head}</span>{#if header.base !== null}{" "}into <span class="tf-mono">{header.base}</span>{/if}{#if header.author !== null},{/if}
          {/if}
          {#if header.author !== null}{" "}by {mine ? "you" : header.author}{/if}
        </div>
      </div>
      <div class="tf-acts">
        {#if waiting > 0 && takesReview}
          <button type="button" class="tf-btn tf-warnbtn" onclick={() => (reviewing = true)}>Submit review ({waiting} pending)</button>
        {:else if reviewable}
          <button type="button" class="tf-btn" onclick={() => (reviewing = true)}>Review…</button>
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
        {#if merge !== null}
          {@const hold = heldBy("merge")}
          <!-- Blocked by the source, it still opens its dialog, which names each reason (#843); held by a queued write, it opens nothing. -->
          <button
            type="button"
            class="tf-btn"
            class:tf-primary={hold === null}
            aria-disabled={hold !== null || merge.disabled !== null ? "true" : undefined}
            title={hold ?? merge.disabled ?? undefined}
            onclick={() => {
              if (hold === null) merging = true;
            }}>Merge…</button
          >
        {/if}
      </div>
    </header>
    <nav class="tf-tabs" aria-label="Pull request">
      {#each PR_TABS as entry (entry.id)}
        {@const count = counts[entry.id]}
        <a href={prHash(prId, entry.id)} aria-current={tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null && count > 0}{" "}<span class="tf-count">{count}</span>{/if}</a
        >
      {/each}
    </nav>
    <div class="tf-cbody" class:tf-pr={tab !== "diff"}>
      {#if tab === "diff"}
        <TfPrFiles
          {prId}
          entityView={ports.entityView}
          viewedFiles={ports.prViewedFiles}
          setViewed={ports.setPrFileViewed}
          {pending}
        />
      {:else}
        <div>
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
                          {#if run !== null}<a href={run}>{row.title}</a>{:else}{row.title}{/if}</td
                        >
                        <td>{[row.chip, row.meta].filter((bit) => bit !== null && bit !== "").join(", ")}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {:else}
                <h2 class="tf-cgroup-h">Description</h2>
                <div class="tf-panel tf-prdesc"><p>{part.description}</p></div>
              {/if}
            {/each}
          {:else if tab === "conversation"}
            <div class="tf-convo">
              {#each conversation as part, n (n)}
                {#if "line" in part}
                  {@render mutedLine(part.line)}
                {:else}
                {@const message = part.message}
                {@const own = me !== null && message.who === me}
                <article class="tf-msg">
                  <span class="tf-av" class:tf-me={own} aria-hidden="true">{initials(message.who)}</span>
                  <div class="tf-msg-body">
                    <div class="tf-who">
                      <b>{own ? "You" : message.who}</b>{message.at}
                      {#if message.verdict !== null}<span class="tf-chip tf-{message.verdict.tone ?? 'muted'}">{message.verdict.text}</span>{/if}
                    </div>
                    {#if message.body !== ""}<p>{message.body}</p>{/if}
                  </div>
                </article>
                {/if}
              {/each}
              {#each queuedComments as body, n (n)}
                <article class="tf-msg tf-queued">
                  <span class="tf-av tf-me" aria-hidden="true">{initials(me)}</span>
                  <div class="tf-msg-body">
                    <div class="tf-who"><b>You</b><span class="tf-chip tf-info">queued</span></div>
                    <p>{body}</p>
                  </div>
                </article>
              {/each}
              {#if conversation.length === 0 && queuedComments.length === 0}
                <div class="tf-empty">No comments yet.</div>
              {/if}
              {#if comment !== null}
                <div class="tf-compose">
                  <textarea bind:value={draft} placeholder="Comment on {name}" aria-label="Comment on {name}"></textarea>
                  <div class="tf-acts">
                    <button
                      type="button"
                      class="tf-btn"
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
                      {@const proposal = row.entity_id === null ? undefined : proposals[row.entity_id]}
                      {@const opens = row.entity_id !== null && repo !== null}
                      <tr class:tf-row={opens} onclick={(event) => openCommit(event, row)}>
                        <td class="tf-mono"
                          >{#if opens}<a href={commitHash(repo!.entity_id, row.entity_id!)}>{row.chip ?? ""}</a>{:else}{row.chip ?? ""}{/if}</td
                        >
                        <td class:tf-dim={row.chip === null}
                          >{row.title}{#if proposal !== undefined}{" "}<a class="tf-chip tf-info" href="#/review/{encodeURIComponent(proposal)}"
                              >proposed link, judge it</a
                            >{/if}</td
                        >
                        <td class="tf-dim tf-nowrap">{row.meta ?? ""}</td>
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
        <aside class="tf-side" aria-label="Ready to merge">
          <div class="tf-panel" hidden={readyRows.length === 0}>
            <header><h3>Ready to merge?</h3></header>
            <div class="tf-prready">
              {#each readyRows as line, n (n)}
                <span><span class="tf-dot tf-{line.tone}"></span> {line.text}</span>
              {/each}
            </div>
          </div>
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
    onqueued={(write) => markChip(prId, "review", "Review", write)}
    onclose={() => (reviewing = false)}
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
    onqueued={(write) => markChip(prId, "merge", "Merge", write)}
    onclose={() => (merging = false)}
  />
{/if}

{#if shown !== null && prId !== null}
  {@const write = shown}
  <TfEntityAction
    action={write.action}
    entityId={prId}
    title="{write.label} {name}"
    what="{write.phrase} {name}"
    source={detail?.source ?? null}
    submitWrite={ports.submitWrite}
    onqueued={(queued) => markChip(prId, write.verb, write.label, queued)}
    onclose={() => (shown = null)}
  />
{/if}
