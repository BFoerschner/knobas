<!--
  The job (#816, R-4.1): `#/job/<key>`, a ticket's working view.

  Opening it reads the ticket, then **promotes its context** (`promote_context`,
  idempotent, so a ticket with a context keeps it and one without gets one) and
  reads the Code section's rows inside that context. The promotion happens
  once per ticket opened: re-reads after a status move do not ask again.

  The layout is the prototype's: the yellow card in a fixed 320px column, the
  work on the right with the tabs *Work*, *Map* and *Conversation*. Work holds

  **The conversation (#819)** is read once per job opened: `visit_entity`
  records this visit and answers the last one, `entity_thread` the thread. The
  card's summary and the Conversation tab both draw that one read.

  **The Connections button (#820)** sits at the right of the tab bar, with the
  ticket's link count and its proposals, and opens the drawer (also `L`).
  **Link sets (#867)**: `job_link_sets` is read once per job opened, for the
  *Also on this job* chips above the tab's body (not on Conversation, R-4.4)
  and the drawer's set and parent groups; *Hide here* and *Show again* re-read
  it with the reach and the sections, and never edit the set (R-7.5).
  the five sections (#817): Code, Builds, Assets, Monitors and Pages, each read
  through the job-reach read (`job_reach`, and `list_entities` scoped by it),
  so a row from the parent carries its label. Above them, a blocked ticket's
  red banner (#818, R-4.8), and on a To Do ticket of the reader's the *Start
  work* stepper (R-4.9) in place of Code and Builds, as prototype-6 draws it.
  The Map tab is TF5's, an empty pane that names itself until then.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import type {
    CheckoutView,
    EntityDetail,
    EntityFilter,
    EntityOrder,
    EntityRow,
    JobLinkSets,
    LinkEntry,
    Reached,
    SuggestionEntry,
    Thread,
  } from "../../ipc/entity";
  import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
  import { ipcErrorMessage } from "../../ipc";
  import { inTextField } from "../../shell/keys";
  import type { Timer } from "../../shell/timer.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { dayKey } from "../../time/day";
  import { weekWindows } from "../../time/week";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { Layers } from "../layers.svelte";
  import {
    PROPOSAL_LIMIT,
    alsoIn,
    connectedCount,
    hiddenOf,
    linkedOn,
    groupsOf,
    proposalsFor,
    type GroupRow,
  } from "./connections";
  import { summary } from "./conversation";
  import {
    CODE_KINDS,
    assetTone,
    assetsRollup,
    blockerEntries,
    buildsRollup,
    byAttention,
    isEntityId,
    isMine,
    itemHash,
    keyOf,
    monitorsRollup,
    offersStartWork,
    originLabel,
    pagesRollup,
    resolveTicket,
    staleBuilds,
    weekReading,
    type AssetLine,
    type SectionRow,
    type SourceFacts,
    type WeekReading,
  } from "./job";
  import { bridgePorts, type JobPorts } from "./ports";
  import { currentJob } from "./current.svelte";
  import TfCodeSection from "./TfCodeSection.svelte";
  import { clones } from "../openin/clones.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import TfConnections from "./TfConnections.svelte";
  import TfConversation from "./TfConversation.svelte";
  import TfJobCard from "./TfJobCard.svelte";
  import TfLinkTo from "./TfLinkTo.svelte";
  import TfSection from "./TfSection.svelte";
  import TfStartWork from "./TfStartWork.svelte";
  import { ago } from "../../shell/time";

  let {
    segment,
    tab,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports = bridgePorts,
    today = () => new Date(),
    apps = sharedApps,
  }: {
    /** What the address names: a key (`PAY-231`) or an entity id. */
    segment: string;
    /** The job's tab from the address, `null` for Work. */
    tab: string | null;
    timer: Pick<Timer, "current" | "elapsed">;
    layers: Layers;
    /** The timer verb: with a timer running, stop it into the worklog draft. */
    ontimer: (() => void) | undefined;
    /** Start the timer on this ticket, stopping another into its draft first. */
    onstarttimer: (entityId: string, title: string) => void;
    ports?: JobPorts;
    today?: () => Date;
    /** The *Open in…* applications (#844). */
    apps?: Apps;
  } = $props();

  /** `undefined` while reading, `null` when nothing answers to the address. */
  let detail = $state<EntityDetail | null | undefined>(undefined);
  let offered = $state<string[] | null>(null);
  let week = $state<WeekReading | null>(null);
  let code = $state<EntityRow[] | null>(null);
  let checkout = $state<CheckoutView | null>(null);
  let checkoutOf = $state<string | null>(null);
  let failure = $state<string | null>(null);
  let thread = $state<Thread | null>(null);
  let threadFailure = $state<string | null>(null);
  /** When the reader last opened this job before now: `null` for never. */
  let lastVisit = $state<string | null>(null);
  let proposals = $state<SuggestionEntry[] | null>(null);
  let drawer = $state(false);
  let linking = $state(false);
  /** The job-reach read, by entity id: the rows' labels. */
  let reached = $state<Map<string, Reached>>(new Map());
  /** The link sets that apply and the parent (#867), `null` until read. */
  let inherited = $state<JobLinkSets | null>(null);
  let builds = $state<EntityRow[] | null>(null);
  let pages = $state<EntityRow[] | null>(null);
  let monitors = $state<EntityRow[] | null>(null);
  let assets = $state<AssetLine[] | null>(null);
  let sources = $state<SourceFacts[]>([]);
  let health = $state<CredentialHealth[]>([]);
  let sync = $state<SourceSyncStatus[]>([]);
  /** Every repository, for *Start work*'s dropdown; `null` until asked. */
  let repos = $state<{ entity_id: string; title: string }[] | null>(null);

  // The job on screen is what the timer chip and ⌘T start on (R-1.4).
  $effect(() => currentJob.set(detail?.row.entity_id ?? null));

  /** The read's generation, so an answer for a ticket left behind is dropped. */
  let token = 0;

  $effect(() => {
    const wanted = segment;
    const mine = ++token;
    detail = undefined;
    offered = null;
    week = null;
    code = null;
    checkout = null;
    checkoutOf = null;
    failure = null;
    thread = null;
    threadFailure = null;
    lastVisit = null;
    proposals = null;
    drawer = false;
    linking = false;
    reached = new Map();
    inherited = null;
    builds = null;
    pages = null;
    monitors = null;
    assets = null;
    repos = null;
    // Only the address re-opens a job: what `open` reads before its first
    // await (the ports) is not a reason to read again.
    untrack(() => void open(wanted, mine));
  });

  async function open(wanted: string, mine: number) {
    try {
      const listed = await ports.listSources().catch(() => [] as SourceFacts[]);
      if (mine !== token) return;
      sources = listed;
      const found = await resolveTicket(wanted, isEntityId(wanted) ? [] : listed, ports.getEntity);
      if (mine !== token) return;
      detail = found;
      if (found === null) return;
      const id = found.row.entity_id;
      void readWorkflow(id, mine);
      void readWeek(id, mine);
      void readConversation(id, mine);
      void readStale(mine);
      void readInherited(id, mine);
      // The reach starts from the ticket's context, so the context first.
      const context = await ports.promoteContext(id);
      if (mine !== token) return;
      void readProposals(context.id, id, mine);
      void readReach(id, mine);
      readSections(id, mine);
      if (offersStartWork(found.status, isMine(found, listed))) void readRepos(mine);
      await readCode(id, mine);
    } catch (cause) {
      if (mine === token) failure = ipcErrorMessage(cause);
    }
  }

  /** How many rows a section reads: a job reaches tens, not thousands. */
  const SECTION_LIMIT = 200;
  /** How many reached assets are read one by one for the Assets section. */
  const ASSET_LIMIT = 50;

  /** A read of `kinds`, scoped to what `id` reaches, or unscoped with `null`. */
  function filterOf(id: string | null, kinds: string[], order: EntityOrder): EntityFilter {
    return {
      sources: [],
      kinds,
      updated_within_days: null,
      order,
      context: null,
      project: null,
      include_deleted: false,
      reach: id,
    };
  }

  /** One section's rows, through the reach; `[]` when the read fails. */
  async function readRows(id: string, kinds: string[], order: EntityOrder, mine: number): Promise<EntityRow[] | null> {
    try {
      const page = await ports.listEntities(filterOf(id, kinds, order), SECTION_LIMIT, 0);
      return mine === token ? page.rows : null;
    } catch {
      // A section that cannot be read draws as having nothing; the Work tab's
      // line says when the reads themselves fail.
      return mine === token ? [] : null;
    }
  }

  /** The reach itself, for the labels, and the reached assets for their section. */
  async function readReach(id: string, mine: number) {
    let rows: Reached[] = [];
    try {
      rows = await ports.jobReach(id);
    } catch {
      // No labels, and an Assets section with nothing in it.
    }
    if (mine !== token) return;
    reached = new Map(rows.map((row) => [row.entity_id, row]));
    const ids = rows.filter((row) => row.kind === "asset").slice(0, ASSET_LIMIT);
    const read = await Promise.allSettled(ids.map((row) => ports.getAsset(row.entity_id)));
    if (mine !== token) return;
    assets = read.flatMap((answer) =>
      answer.status === "fulfilled"
        ? [
            {
              id: answer.value.asset.id,
              name: answer.value.asset.name,
              type: answer.value.asset.type_label,
              environment: answer.value.effective_environment?.value ?? null,
              on: answer.value.held_by.at(-1)?.name ?? null,
              tone: assetTone(answer.value.asset.health),
            },
          ]
        : [],
    );
  }

  /** The link sets that apply and the parent; nothing of either when the read fails. */
  async function readInherited(id: string, mine: number) {
    try {
      const read = await ports.jobLinkSets(id);
      if (mine === token) inherited = read;
    } catch {
      if (mine === token) inherited = { sets: [], parent: null };
    }
  }

  /**
   * After *Hide here* or *Show again*: the groups, the labels and the
   * sections' rows again, since a hidden member leaves the job's reach. The
   * checkout line stays as it was.
   */
  function rereadReach(id: string) {
    const mine = token;
    void readInherited(id, mine);
    void readReach(id, mine);
    readSections(id, mine);
    void readRows(id, [...CODE_KINDS], "updated_desc", mine).then((rows) => {
      if (mine === token && rows) code = rows;
    });
  }

  /** Builds, Pages and Monitors, each through the reach. */
  function readSections(id: string, mine: number) {
    void readRows(id, ["build"], "latest_per_parent", mine).then((rows) => {
      if (mine === token && rows) builds = rows;
    });
    void readRows(id, ["page"], "updated_desc", mine).then((rows) => {
      if (mine === token && rows) pages = rows;
    });
    void readRows(id, ["monitor"], "updated_desc", mine).then((rows) => {
      if (mine === token && rows) monitors = rows;
    });
  }

  /** *Hide here* (R-7.5): one member off this ticket only, with *Undo*. The set is not touched. */
  async function hideHere(row: GroupRow) {
    if (!detail) return;
    const id = detail.row.entity_id;
    const key = keyOf(id);
    try {
      await ports.hideLinkSetMember(id, row.id, true);
      rereadReach(id);
      push({
        text: `Hidden on ${key}. The link set stays as it is.`,
        action: { label: "Undo", run: () => void showAgain(row.id) },
      });
    } catch (cause) {
      push({ text: `Could not hide it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** *Show again*: the member back on this ticket. */
  async function showAgain(memberId: string) {
    if (!detail) return;
    const id = detail.row.entity_id;
    try {
      await ports.hideLinkSetMember(id, memberId, false);
      rereadReach(id);
    } catch (cause) {
      push({ text: `Could not show it again: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** What the Builds section's warning reads: every source's credential and last run. */
  async function readStale(mine: number) {
    const [credentials, runs] = await Promise.all([
      ports.credentialHealth().catch(() => [] as CredentialHealth[]),
      ports.syncStatus().catch(() => [] as SourceSyncStatus[]),
    ]);
    if (mine !== token) return;
    health = credentials;
    sync = runs;
  }

  async function readRepos(mine: number) {
    try {
      const page = await ports.listEntities(filterOf(null, ["repo"], "title_asc"), SECTION_LIMIT, 0);
      if (mine === token) repos = page.rows.map((row) => ({ entity_id: row.entity_id, title: row.title }));
    } catch {
      if (mine === token) repos = [];
    }
  }

  async function readWorkflow(id: string, mine: number) {
    try {
      const statuses = await ports.reachableTransitions(id);
      if (mine === token) offered = statuses;
    } catch {
      // A workflow that cannot be read offers nothing: the select shows where
      // the ticket stands, and the move waits for a read that works.
      if (mine === token) offered = [];
    }
  }

  async function readWeek(id: string, mine: number) {
    try {
      const now = today();
      const answer = await ports.weekTimesheet(weekWindows(dayKey(now)));
      if (mine === token) week = weekReading(answer, id, dayKey(now));
    } catch {
      // *Time this week* is an extra; the line stays at its ellipsis.
    }
  }

  async function readConversation(id: string, mine: number) {
    // The visit is written first and answers the one before it; a visit that
    // cannot be written reads as no earlier visit, so nothing is called new.
    const [visit, read] = await Promise.allSettled([ports.visitEntity(id), ports.entityThread(id)]);
    if (mine !== token) return;
    lastVisit = visit.status === "fulfilled" ? visit.value : null;
    if (read.status === "fulfilled") thread = read.value;
    else threadFailure = ipcErrorMessage(read.reason);
  }

  async function readProposals(context: string, id: string, mine: number) {
    try {
      const page = await ports.roomSuggestions([], context, PROPOSAL_LIMIT);
      if (mine === token) proposals = proposalsFor(page.rows, id);
    } catch {
      // A tray that cannot be read proposes nothing; the button says connected only.
      if (mine === token) proposals = [];
    }
  }

  /** Re-read the ticket after a link changed, keeping what else the job read. */
  async function relink() {
    if (!detail) return;
    const mine = token;
    try {
      const again = await ports.getEntity(detail.row.entity_id);
      if (mine === token) detail = again;
    } catch {
      // The drawer keeps the rows it has; the next open reads them again.
    }
  }

  async function unlinkEntry(entry: LinkEntry) {
    const { link } = entry;
    try {
      await ports.unlink(link.id);
      void relink();
      push({
        text: "Unlinked in knobas only.",
        action: {
          label: "Undo",
          run: () =>
            void ports
              .createLink(link.from_id, link.to_id, link.relation, link.note ?? undefined)
              .then(relink)
              .catch((cause: unknown) => push({ text: `Could not link it again: ${ipcErrorMessage(cause)}`, tone: "err" })),
        },
      });
    } catch (cause) {
      push({ text: `Could not unlink: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function comment(body: string): Promise<boolean> {
    if (!detail) return false;
    const key = keyOf(detail.row.entity_id);
    try {
      await ports.submitWrite({ Comment: { entity: detail.row.entity_id, body } });
      push({
        text: `Queued: comment on ${key}. Sends with the next sync.`,
        action: { label: "Show queue", run: openWriteQueue },
      });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  /** How many rows the section reads: the membership is one hop, so this holds it. */
  const CODE_LIMIT = 200;
  /** How many branches and repositories are asked for a clone before giving up. */
  const CHECKOUT_TRIES = 3;

  async function readCode(id: string, mine: number) {
    const page = await ports.listEntities(filterOf(id, [...CODE_KINDS], "updated_desc"), CODE_LIMIT, 0);
    if (mine !== token) return;
    code = page.rows;
    const sorted = byAttention(page.rows);
    const candidates = [
      ...sorted.filter((row) => row.kind === "branch"),
      ...sorted.filter((row) => row.kind === "repo"),
    ].slice(0, CHECKOUT_TRIES);
    // The first clone found; else the first repository, which then offers
    // *Clone* on the line (R-8.25).
    let uncloned: { view: CheckoutView; of: string } | null = null;
    for (const row of candidates) {
      try {
        const view = await ports.entityCheckout(row.entity_id);
        if (mine !== token) return;
        if (view.path) {
          checkout = view;
          checkoutOf = row.entity_id;
          break;
        }
        if (uncloned === null && row.kind === "repo") uncloned = { view, of: row.entity_id };
      } catch {
        // A checkout that cannot be read is a checkout not shown.
      }
    }
    if (checkout === null && uncloned !== null) {
      checkout = uncloned.view;
      checkoutOf = uncloned.of;
    }
  }

  // A clone that finished: the line reads its checkout again, and *Clone*
  // becomes *Open in…*.
  let seenClones = clones.finished;
  $effect(() => {
    if (clones.finished === seenClones) return;
    seenClones = clones.finished;
    const of = checkoutOf;
    const mine = token;
    if (of === null || checkout?.path) return;
    void ports.entityCheckout(of).then(
      (view) => {
        if (mine === token && of === checkoutOf) checkout = view;
      },
      () => {},
    );
  });

  async function move(status: string) {
    if (!detail) return;
    const key = keyOf(detail.row.entity_id);
    try {
      await ports.submitWrite({ Transition: { entity: detail.row.entity_id, status } });
      push({
        text: `Queued: ${key} to ${status}. Sends with the next sync.`,
        action: { label: "Show queue", run: openWriteQueue },
      });
    } catch (cause) {
      push({ text: `Could not queue the move: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** The job's tabs, in prototype-6's order and words; an unknown tab is Work. */
  const TABS = [
    { id: "work", name: "Work", path: "" },
    { id: "map", name: "Map", path: "/map" },
    { id: "conversation", name: "Conversation", path: "/conversation" },
  ] as const;
  const current = $derived(TABS.find((candidate) => candidate.id === tab) ?? TABS[0]);
  const conversation = $derived(thread === null ? null : summary(thread, lastVisit));
  const connected = $derived(detail ? linkedOn(detail) : []);
  const groups = $derived(groupsOf(inherited));

  onMount(() => {
    // `L` opens the drawer (R-4.3), outside a text field and with nothing open over the page.
    const onkey = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "l" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      if (!detail || drawer || layers.open.length > 0 || inTextField(document.activeElement)) return;
      event.preventDefault();
      drawer = true;
    };
    window.addEventListener("keydown", onkey);
    return () => {
      window.removeEventListener("keydown", onkey);
      token += 1;
    };
  });

  /** A mirror row as a section draws it. */
  function line(row: EntityRow): SectionRow {
    return {
      id: row.entity_id,
      href: itemHash(row),
      title: row.title,
      mono: false,
      repo: null,
      // Where it sits in its source, then when it last changed: "ENG ›
      // Payments, 2026-08-19", as prototype-6's page rows read.
      sub: [row.path, row.updated_at ? ago(row.updated_at) : null].filter(Boolean).join(", "),
      tone: row.state?.tone ?? null,
      chip: row.state,
      from: originLabel(reached.get(row.entity_id)),
    };
  }
  const lines = (rows: EntityRow[] | null) => (rows === null ? null : byAttention(rows).map(line));

  const TONE_RANK: Record<string, number> = { err: 0, warn: 1 };
  const assetLines = $derived<SectionRow[] | null>(
    assets === null
      ? null
      : [...assets]
          .sort((a, b) => (TONE_RANK[a.tone ?? ""] ?? 2) - (TONE_RANK[b.tone ?? ""] ?? 2) || a.name.localeCompare(b.name))
          .map((asset) => ({
            id: asset.id,
            href: itemHash({ entity_id: asset.id, kind: "asset" }),
            title: asset.name,
            mono: true,
            repo: null,
            sub: [asset.type, asset.environment, asset.on ? `on ${asset.on}` : null].filter(Boolean).join(", "),
            tone: asset.tone,
            chip: null,
            from: originLabel(reached.get(asset.id)),
          })),
  );

  const mine = $derived(detail ? isMine(detail, sources) : false);
  const starting = $derived(detail ? offersStartWork(detail.status, mine) : false);
  const blocked = $derived(detail ? blockerEntries(detail) : []);
  const stale = $derived(staleBuilds(sources, health, sync, builds ?? [], today()));
  /** What *Start work* suggests: a repository the ticket already reaches. */
  const suggestedRepo = $derived(
    [...reached.values()].find((row) => row.kind === "repo")?.entity_id ?? null,
  );
</script>

{#if detail === undefined && failure === null}
  <div class="tf-empty"><p>Opening {segment}…</p></div>
{:else if detail === null || (detail === undefined && failure !== null)}
  <div class="tf-full">
    <div class="tf-empty">
      {#if failure !== null}
        <p>Could not open {segment}: {failure}</p>
      {:else}
        <p>No ticket {segment} in the mirror. It may not have synced yet.</p>
      {/if}
      <a class="tf-btn" href="#/tickets">All tickets <kbd>Esc</kbd></a>
    </div>
  </div>
{:else if detail}
  {@const key = keyOf(detail.row.entity_id)}
  <div class="tf-ws">
    <TfJobCard
      {detail}
      {offered}
      {week}
      {conversation}
      conversationFailure={threadFailure}
      {lastVisit}
      now={today()}
      {timer}
      onmove={move}
      onstop={() => ontimer?.()}
      onstart={() => onstarttimer(detail!.row.entity_id, detail!.row.title)}
    />
    <section class="tf-work">
      <div class="tf-wtabs" role="tablist">
        {#each TABS as each (each.id)}
          <a href="#/job/{encodeURIComponent(key)}{each.path}" role="tab" aria-selected={current.id === each.id}
            >{each.name}{#if each.id === "conversation" && conversation !== null}{#if conversation.comments > 0}{" "}<span
                    class="tf-count">{conversation.comments}</span
                  >{/if}{#if conversation.fresh > 0}{" "}<span class="tf-count tf-info">{conversation.fresh} new</span>{/if}{/if}</a
          >
        {/each}
        <button type="button" class="tf-conn" data-testid="job-connections" title="Connections (L)" onclick={() => (drawer = true)}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"
            ><path d="M6.5 9.5l3-3M7 4.5l1-1a2.8 2.8 0 0 1 4 4l-1 1M9 11.5l-1 1a2.8 2.8 0 0 1-4-4l1-1" /></svg
          ><b>{connectedCount(connected, groups)}</b> connected{#if proposals !== null && proposals.length > 0},
            <span class="tf-p">{proposals.length} proposed</span>{/if}
          <kbd>L</kbd>
        </button>
      </div>
      <div class="tf-wbody">
        {#if current.id !== "conversation" && inherited !== null}
          <!-- R-4.4: hidden while the Conversation tab is open. -->
          <div class="tf-setsbar">
            <span class="tf-dim">Also on this job</span>
            {#each groups as group (group.key)}
              <a class="tf-setchip" href={group.href}><b>{group.label}</b> <span>{group.via}</span></a>{" "}
            {:else}
              <span class="tf-dim">no link sets yet</span>
            {/each}
          </div>
        {/if}
        {#if current.id === "work"}
          {#if failure !== null}
            <p class="tf-dim">Could not read what this job holds: {failure}</p>
          {/if}
          {#each blocked as blocker (blocker.entityId)}
            <div class="tf-blocked">
              <strong>Blocked by {blocker.key} {blocker.title}</strong>
              {#if blocker.note}<span>{blocker.note}</span>{/if}
              <a class="tf-btn tf-sm" href="#/job/{encodeURIComponent(blocker.key)}">Open {blocker.key}</a>
            </div>
          {/each}
          {#if starting}
            <TfStartWork {detail} {repos} suggested={suggestedRepo} {timer} api={ports.startWork} {onstarttimer} />
          {/if}
          <div class="tf-grid">
            {#if !starting}
              <TfCodeSection rows={code} {reached} {checkout} {checkoutOf} {layers} {apps} />
              <TfSection id="builds" title="Builds" openLabel="Open Builds view" openHref="#/builds" footHref="#/builds" rows={lines(builds)} rollup={builds ? buildsRollup(builds) : null}>
                {#snippet before()}
                  {#each stale as warning (warning.sourceId)}
                    <div class="tf-stale">
                      {warning.text}
                      <a href="#/settings/source/{encodeURIComponent(warning.sourceId)}">Re-enter password</a>
                    </div>
                  {/each}
                {/snippet}
              </TfSection>
            {/if}
            <TfSection id="assets" title="Assets" openLabel="Open in Dependencies" openHref="#/job/{encodeURIComponent(key)}/map" footHref="#/assets" rows={assetLines} rollup={assets ? assetsRollup(assets) : null} />
            <TfSection id="monitors" title="Monitors" openLabel="Open Monitors" openHref="#/monitors" footHref="#/monitors" rows={lines(monitors)} rollup={monitors ? monitorsRollup(monitors) : null} />
            <TfSection id="pages" title="Pages" openLabel="Open Docs view" openHref="#/docs" footHref="#/docs" onopen={() => (sessionFilter("docs").scope = "job")} rows={lines(pages)} rollup={pages ? pagesRollup(pages) : null} />
          </div>
        {:else if current.id === "conversation"}
          <TfConversation
            jobKey={key}
            {thread}
            {lastVisit}
            failure={threadFailure}
            now={today()}
            oncomment={comment}
          />
        {:else}
          <div class="tf-empty">
            <p>{current.name} is not built in the new shell yet.</p>
          </div>
        {/if}
      </div>
    </section>
  </div>
  {#if drawer}
    <TfConnections
      jobKey={key}
      links={connected}
      {groups}
      hidden={hiddenOf(inherited)}
      alsoIn={(entityId) => alsoIn(inherited, entityId)}
      {proposals}
      {layers}
      onclose={() => (drawer = false)}
      onunlink={(entry) => void unlinkEntry(entry)}
      onlinkto={() => (linking = true)}
      onhide={(row) => void hideHere(row)}
      onshow={(memberId) => void showAgain(memberId)}
    />
  {/if}
  {#if linking}
    <TfLinkTo
      jobKey={key}
      entityId={detail.row.entity_id}
      {ports}
      {layers}
      onclose={() => (linking = false)}
      onlinked={() => {
        linking = false;
        push({ text: "Linked in knobas" });
        void relink();
      }}
    />
  {/if}
{/if}
