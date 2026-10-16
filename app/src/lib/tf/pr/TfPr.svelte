<!--
  The pull request page (#841, R-8.15, R-8.16, R-8.20), prototype-6's
  `renderPR`: `#/pr/<entity id>` and its tabs `/conversation`, `/diff` and
  `/commits`, which share the header and the one read.

  - **The header**: `PR #<n> <title>`, its state, head into base and author
    (*you* for the reader's own); the toolbar's *Review…* (#842), *Update
    branch*, *Edit*, *Close* (or *Reopen*) and *Merge…* (#843). A write is
    drawn only where the source's view offers it (ADR-0020 rule 4); *Review…*
    and *Merge…* are drawn and say they are not built yet.
  - **Overview**: the head commit's checks (a check whose link the mirror
    holds opens its run) and the description; beside it *Ready to merge?*
    (the source's *Mergeable*, each standing review, the checks) and, when
    the repository is cloned, the local checkout.
  - **Conversation**: each comment and review in time order, and a comment
    box that queues `Comment`.
  - **Commits**: the pull request's commits; one the mirror holds opens its
    diff on the repository page, and one with a proposed link says *proposed
    link, judge it*, which opens the proposal.

  Everything is the source's view of the pull request (`entity_view`), read
  once per pull request; the tabs only choose what to draw of it.
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
  }

  /**
   * What this session queued on each pull request, by its entity id: the
   * chips beside its state (`close queued`) and the comments not yet in its
   * conversation. Module `$state`, never stored; a comment's mark goes once
   * the conversation holds its words.
   */
  const queuedChips: Record<string, string[]> = $state({});
  const queuedComments: Record<string, string[]> = $state({});

  /** Forget every mark: a new session, which is what each test is. */
  export function forgetPrQueued(): void {
    for (const bag of [queuedChips, queuedComments]) for (const key of Object.keys(bag)) delete bag[key];
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    entityCheckout,
    entityView,
    getEntity,
    listEntities,
    resolveUrl,
    roomSuggestions,
    submitWrite,
    type EntityRow,
    type ViewAction,
    type ViewRow,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { prNumber } from "../code/code";
  import { isMine, itemHash, username } from "../job/job";
  import { toastQueued } from "../repo/queued.svelte";
  import { commitHash, repoHash } from "../repo/repo";
  import { jobHash } from "../route";
  import { initials } from "../tickets/tickets";
  import {
    PR_TABS,
    commitRows,
    conversationOf,
    headerOf,
    prHash,
    readPrRoute,
    readyLines,
    tabCounts,
    type PrTab,
  } from "./pr";
  import TfPrAction from "./TfPrAction.svelte";

  let {
    ready,
    rest,
    job,
    ports = {
      getEntity: (entityId) => getEntity(entityId),
      entityView: (entityId, section) => entityView(entityId, section),
      listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
      entityCheckout: (entityId) => entityCheckout(entityId),
      roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit),
      resolveUrl: (url) => resolveUrl(url),
      listSources: () => listSources(),
      submitWrite: (payload) => submitWrite(payload),
    },
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `pr` in the address. */
    rest: string[];
    /** The job the reader was last in, as a ticket key. */
    job: string | null;
    ports?: PrPorts;
  } = $props();

  /** How many repositories and proposals one read asks for. */
  const LIMIT = 1000;

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
    ports.roomSuggestions([], null, LIMIT).then(
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
    ports.listEntities(filter, LIMIT, 0).then(
      (page) => {
        if (!live()) return;
        repo = page.rows.find((candidate) => candidate.title === row.path) ?? null;
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
  const ready_ = $derived(readyLines(view ?? null));
  const number = $derived(prId === null ? null : prNumber(prId));
  const title = $derived(detail?.row.title ?? "");
  const messages = $derived(conversationOf(view ?? null));
  const commits = $derived(commitRows(view ?? null));
  const actions = $derived(view?.actions ?? []);
  const name = $derived(number === null ? "PR" : `PR ${number}`);

  /** The write each toolbar button stands for, where the source offers it. */
  const find = (test: (action: ViewAction) => boolean) => actions.find(test) ?? null;
  const update = $derived(find((a) => a.write_op === "update_pull_request_branch"));
  const edit = $derived(find((a) => a.write_op === "edit_pull_request" && a.values["state"] === undefined));
  const setState = $derived(find((a) => a.write_op === "edit_pull_request" && a.values["state"] !== undefined));
  const merge = $derived(find((a) => a.write_op === "merge_pull_request"));
  const comment = $derived(find((a) => a.write_op === "comment"));
  /** *Review…* stands for the verdicts, offered only on somebody else's open pull request. */
  const reviewable = $derived(
    actions.some((a) => a.write_op === "submit_review" && (a.values["verdict"] === "approve" || a.values["verdict"] === "request_changes")),
  );

  /** The comments this session queued that the conversation does not hold yet. */
  const pendingComments = $derived(
    prId === null ? [] : (queuedComments[prId] ?? []).filter((body) => !messages.some((message) => message.body.trim() === body.trim())),
  );
  const chips = $derived(prId === null ? [] : (queuedChips[prId] ?? []));

  /** Where the reader's checkout stands against this pull request's head. */
  const local = $derived.by(() => {
    const tree = checkout?.tree;
    if (checkout?.path == null || !tree || tree.status !== "read") return null;
    const changed = tree.changed + tree.untracked;
    const where = tree.branch !== null && tree.branch === header.head ? "On this branch" : `On ${tree.branch ?? "a detached head"}`;
    const files = changed === 0 ? "clean" : `${changed} changed ${changed === 1 ? "file" : "files"}`;
    return { path: checkout.path, line: `${where}, ${files}` };
  });

  /** The dialog open: which action, under which heading and toast. */
  let dialog = $state<{ action: ViewAction; title: string; what: string; chip: string | null } | null>(null);

  function open(action: ViewAction, heading: string, what: string, chip: string | null) {
    if (action.disabled !== null) return;
    dialog = { action, title: heading, what, chip };
  }

  function mark(chip: string | null) {
    if (prId === null || chip === null) return;
    queuedChips[prId] = [...(queuedChips[prId] ?? []).filter((c) => c !== chip), chip];
  }

  let draft = $state("");
  let sending = $state(false);

  async function send() {
    const body = draft.trim();
    if (prId === null || body === "" || sending) return;
    sending = true;
    try {
      await ports.submitWrite({ Comment: { entity: prId, body } });
      queuedComments[prId] = [...(queuedComments[prId] ?? []), body];
      toastQueued(`comment on ${name}`);
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

  const NOT_BUILT = "Not built in the new shell yet";
</script>

<div class="tf-full">
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#if job !== null}
      <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a><span class="tf-sep">›</span>
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
        {#if reviewable}
          <button type="button" class="tf-btn" aria-disabled="true" title={NOT_BUILT}>Review…</button>
        {/if}
        {#if update !== null}
          <button
            type="button"
            class="tf-btn"
            aria-disabled={update.disabled !== null ? "true" : undefined}
            title={update.disabled ?? undefined}
            onclick={() => open(update, `Update ${name}`, `update branch of ${name}`, "update queued")}>Update branch</button
          >
        {/if}
        {#if edit !== null}
          <button
            type="button"
            class="tf-btn"
            aria-disabled={edit.disabled !== null ? "true" : undefined}
            title={edit.disabled ?? undefined}
            onclick={() => open(edit, `Edit ${name}`, `edit ${name}`, "edit queued")}>Edit</button
          >
        {/if}
        {#if setState !== null}
          {@const verb = setState.values["state"] === "closed" ? "Close" : "Reopen"}
          <button
            type="button"
            class="tf-btn"
            aria-disabled={setState.disabled !== null ? "true" : undefined}
            title={setState.disabled ?? undefined}
            onclick={() => open(setState, `${verb} ${name}`, `${verb.toLowerCase()} ${name}`, `${verb.toLowerCase()} queued`)}
            >{verb}</button
          >
        {/if}
        {#if merge !== null}
          <button type="button" class="tf-btn tf-primary" aria-disabled="true" title={merge.disabled ?? NOT_BUILT}>Merge…</button>
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
        <div class="tf-empty"><p>Files changed is not built in the new shell yet.</p></div>
      {:else}
        <div>
          {#if viewFailed !== null}
            <p class="tf-cfail">Could not read the pull request: {viewFailed}</p>
          {:else if view === null}
            <div class="tf-empty">This source draws nothing for a pull request.</div>
          {:else if view === undefined}
            <!-- The view has not answered yet. -->
          {:else if tab === "overview"}
            {#if header.checks.length > 0}
              <h2 class="tf-cgroup-h">Checks</h2>
              <table class="tf-tbl tf-prchecks">
                <tbody>
                  {#each header.checks as row, n (n)}
                    {@const run = runOf(row)}
                    <tr class:tf-row={run !== null} onclick={() => run !== null && (location.hash = run)}>
                      <td
                        ><span class="tf-dot tf-{row.tone ?? 'muted'}"></span>
                        {#if run !== null}<a href={run}>{row.title}</a>{:else}{row.title}{/if}</td
                      >
                      <td>{[row.chip, row.meta].filter((part) => part !== null && part !== "").join(", ")}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
            {#if header.description !== null}
              <h2 class="tf-cgroup-h">Description</h2>
              <div class="tf-panel tf-prdesc"><p>{header.description}</p></div>
            {/if}
          {:else if tab === "conversation"}
            <div class="tf-convo">
              {#each messages as message, n (n)}
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
              {/each}
              {#each pendingComments as body, n (n)}
                <article class="tf-msg">
                  <span class="tf-av tf-me" aria-hidden="true">{initials(me)}</span>
                  <div class="tf-msg-body">
                    <div class="tf-who"><b>You</b><span class="tf-chip tf-info">queued</span></div>
                    <p>{body}</p>
                  </div>
                </article>
              {/each}
              {#if messages.length === 0 && pendingComments.length === 0}
                <div class="tf-empty">No comments yet.</div>
              {/if}
              {#if comment !== null}
                <div class="tf-compose">
                  <textarea bind:value={draft} placeholder="Comment on {name}" aria-label="Comment on {name}"></textarea>
                  <div class="tf-acts">
                    <button type="button" class="tf-btn" disabled={draft.trim() === "" || sending} onclick={() => void send()}>Comment</button>
                  </div>
                </div>
              {/if}
            </div>
          {:else if tab === "commits"}
            {#if commits.length === 0}
              <div class="tf-empty">No commits.</div>
            {:else}
              <table class="tf-tbl tf-rcommits">
                <tbody>
                  {#each commits as row, n (n)}
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
          {/if}
        </div>
        <aside class="tf-side" aria-label="Ready to merge">
          <div class="tf-panel">
            <header><h3>Ready to merge?</h3></header>
            <div class="tf-prready">
              {#each ready_ as line, n (n)}
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
                  <button type="button" class="tf-btn tf-sm" aria-disabled="true" title={NOT_BUILT}>Open in…</button>
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

{#if dialog !== null && prId !== null}
  {@const open_ = dialog}
  <TfPrAction
    action={open_.action}
    {prId}
    title={open_.title}
    what={open_.what}
    submitWrite={ports.submitWrite}
    onqueued={() => mark(open_.chip)}
    onclose={() => (dialog = null)}
  />
{/if}
