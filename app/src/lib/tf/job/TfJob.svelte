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

  **The Connections button (#820)** sits at the right of the tab bar (its
  trailing small actions, #2294, with *Refresh*), with the
  ticket's link count and its proposals, and opens the drawer (also `L`).
  **Link sets (#867)**: `job_link_sets` is read once per job opened, for the
  *Also on this job* chips above the tab's body (not on Conversation, R-4.4)
  and the drawer's set and parent groups; *Hide here* and *Show again* re-read
  it with the reach and the sections, and never edit the set (R-7.5).
  *Apply a link set…* (the bar and the drawer) and *Save as a link set…* (the
  drawer) are #869's dialogs: both save through `save_link_set`, then re-read
  the same way.
  the five sections (#817): Code, Builds, Assets, Monitors and Pages, each read
  through the job-reach read (`job_reach`, and `list_entities` scoped by it),
  so a row from the parent carries its label. Above them, a blocked ticket's
  red banner (#818, R-4.8), and on a To Do ticket of the reader's the *Start
  work* stepper (R-4.9) in place of Code and Builds, as prototype-6 draws it.
  The Map tab (#874) hides the card and draws Dependencies from the ticket
  (`deps/TfDeps.svelte` with `ticket` set): only what the ticket reaches.

  **What the credential cannot write (#2052).** Once per job opened,
  `refused_writes` is asked about the ticket's *Comment* and its move; a
  refused one is drawn disabled with the reason (the status select, Start
  work's move, the comment box's *Comment*) and queues nothing.

  **Refresh (#2134, #2107's pieces).** The ticket's *Refresh* or *Sync now*
  ends the tab bar, after Connections, so the Map tab, which hides the card,
  keeps it; its failure line and the withdrawn and turned-off banners sit
  under the tab bar on every tab. A refresh reads the ticket, its thread, its
  week and its reach again through the reader (`reread`), over what the page
  drew, and the echoes settle on those reads as on any other.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import type {
    EntityRow,
    LinkEntry,
    LinkSetDetail,
    LinkSetRow,
  } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { askInto, refusalOf, type RefusedWrite, type WriteAsk } from "../../shell/refused-writes";
  import { inTextField } from "../../shell/keys";
  import { toastQueued } from "../../shell/queued-toast";
  import type { Timer } from "../../shell/timer.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { Layers } from "../layers.svelte";
  import {
    alsoIn,
    connectedCount,
    hiddenOf,
    linkedOn,
    groupsOf,
    type GroupRow,
  } from "./connections";
  import { summary } from "./conversation";
  import {
    assetsRollup,
    blockerEntries,
    buildsRollup,
    byAttention,
    isMine,
    itemHash,
    keyOf,
    monitorsRollup,
    offersStartWork,
    originLabel,
    pagesRollup,
    runKindsOf,
    staleBuilds,
    type FrameKinds,
    type SectionRow,
  } from "./job";
  import { bridgePorts, bridgeSetPorts, type JobPorts, type JobSetPorts } from "./ports";
  import { applyToast, savedToast, withHandRule, withoutHandRule } from "../sets/apply";
  import TfApplySet from "./TfApplySet.svelte";
  import TfSaveSet from "./TfSaveSet.svelte";
  import { createJobReader } from "./job-reader.svelte";
  import { kindRegistry } from "../../shell/kind-registry.svelte";
  import { currentJob, linkRequest, type LinkTarget } from "./current.svelte";
  import { chosenJob } from "./chosen.svelte";
  import { askAgain, echoWrite, echoOn, echoesOn, sameWords, settleEchoes, watchEchoes } from "../queued.svelte";
  import { entityAddress } from "../address";
  import { jobHash } from "../route";
  import TfCodeSection from "./TfCodeSection.svelte";
  import TfDeps from "../deps/TfDeps.svelte";
  import { clones } from "../openin/clones.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import TfConnections from "./TfConnections.svelte";
  import TfConversation from "./TfConversation.svelte";
  import TfJobCard from "./TfJobCard.svelte";
  import TfLinkTo from "./TfLinkTo.svelte";
  import TfSection from "./TfSection.svelte";
  import TfStartWork from "./TfStartWork.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { ago } from "../../shell/time";
  import { pageVisible } from "../kept-pages.svelte";
  import { onShownAgain } from "../while-shown.svelte";

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
    sets = bridgeSetPorts,
    registry = kindRegistry,
    follower = writeFollower(),
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
    /** *Apply a link set…* and *Save as a link set…* (#869). */
    sets?: JobSetPorts;
    /** The adapters' claims, for the CI role's run kinds a run is routed by (#1230, #1256). */
    registry?: FrameKinds;
    /** Follows each queued move and comment, so one held or refused drops its echo (#1424). */
    follower?: WriteFollower;
  } = $props();

  let drawer = $state(false);
  let linking = $state(false);
  /** The other end the launcher's *Link to <key>* chose (#1500), preset in the dialog. */
  let preset = $state<LinkTarget | null>(null);
  /** *Apply a link set…* and *Save as a link set…* open (#869). */
  let applyingSet = $state(false);
  let savingSet = $state(false);

  // ---- the job's reads are the reader's (#1320) ----
  // svelte-ignore state_referenced_locally
  // Built once per mount over the ports this page was given: the reader owns the token.
  const reader = createJobReader(ports, { today: () => today(), registry: { load: () => registry.load() } });
  const detail = $derived(reader.detail);
  const offered = $derived(reader.transitions);
  const week = $derived(reader.week);
  const code = $derived(reader.code);
  const checkout = $derived(reader.checkout);
  const checkoutOf = $derived(reader.checkoutOf);
  const failure = $derived(reader.failure);
  const thread = $derived(reader.thread);
  const threadFailure = $derived(reader.threadFailure);
  const lastVisit = $derived(reader.lastVisit);
  const proposals = $derived(reader.proposals);
  const reached = $derived(reader.reached);
  const inherited = $derived(reader.inherited);
  const builds = $derived(reader.builds);
  const pages = $derived(reader.pages);
  const monitors = $derived(reader.monitors);
  const assets = $derived(reader.assets);
  const sources = $derived(reader.sources);
  const health = $derived(reader.health);
  const sync = $derived(reader.sync);
  const repos = $derived(reader.repos);
  /** The ticket's *Refresh* or *Sync now* (#2134). */
  const refresh = createDetailRefresh(untrack(() => ports.refresh));

  /** The ticket's writes its source's credential cannot make (#2052), asked once per job opened. */
  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so a re-read of the same ticket asks nothing again. */
  const asks = $derived.by(() => {
    const entity = detail?.row.entity_id;
    return JSON.stringify(entity === undefined ? [] : ["comment", "transition"].map((op) => ({ entity, op })));
  });
  /**
   * Bumped by a refresh and by the page being shown again, so the ticket's
   * writes are asked about again: an ask the app could not read is answered
   * disabled and kept nowhere (#2306), and only a new ask clears it.
   */
  let writesAsked = $state(0);
  $effect(() => {
    void writesAsked;
    return askInto(JSON.parse(asks) as WriteAsk[], ports.refusedWrites, (answer) => (refused = answer));
  });
  const commentRefused = $derived(refusalOf(refused, detail?.row.entity_id ?? null, "comment"));
  const moveRefused = $derived(refusalOf(refused, detail?.row.entity_id ?? null, "transition"));

  /** Whether keep-alive shows this job or holds it hidden (#2247). */
  const pageShown = pageVisible();
  onShownAgain(pageShown, () => (writesAsked += 1));
  /** A refresh's re-read (#2134): the ticket through the reader, and its writes asked about again. */
  function rereadJob() {
    writesAsked += 1;
    return reader.reread();
  }

  // The job on screen is what the timer chip and ⌘T start on (R-1.4); a job
  // keep-alive holds hidden (#2247) is not on screen.
  $effect(() => (pageShown() ? currentJob.set(detail?.row.entity_id ?? null) : undefined));
  // A job address resolved on screen makes its ticket the chosen job (spec
  // #2227): a bookmark, history, a link or the Tickets preview's button. By
  // its id, so a re-read of the same ticket (a timer start, a refresh) does
  // not take the choice back from a ticket chosen since.
  const resolvedId = $derived(detail?.row.entity_id ?? null);
  $effect(() => {
    if (pageShown() && resolvedId !== null) chosenJob.choose(resolvedId);
  });

  // *Link to <key>* from the launcher (#1500): this job's own dialog, over its
  // Connections drawer, with the result preset, once the job has drawn.
  $effect(() => {
    if (!detail || !pageShown()) return;
    const asked = linkRequest.take(keyOf(detail.row.entity_id));
    if (asked === null) return;
    preset = asked;
    drawer = true;
    linking = true;
  });

  $effect(() => {
    const wanted = segment;
    drawer = false;
    linking = false;
    // Only the address re-opens a job: what the reader reads before its first
    // await is not a reason to read again.
    untrack(() => {
      refresh.reset();
      reader.open(wanted);
    });
  });

  /** *Hide here* (R-7.5): one member off this ticket only, with *Undo*. The set is not touched. */
  async function hideHere(row: GroupRow) {
    if (!detail) return;
    const id = detail.row.entity_id;
    const key = keyOf(id);
    try {
      await ports.hideLinkSetMember(id, row.id, true);
      reader.rereadReach(id);
      push({
        text: `Hidden on ${key}. The link set stays as it is.`,
        action: { label: "Undo", run: () => void showAgain(row.id, id) },
      });
    } catch (cause) {
      push({ text: `Could not hide it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /**
   * *Show again*: the member back on the ticket it was hidden on. *Undo*
   * passes that ticket, since its toast outlives a move to another job.
   */
  async function showAgain(memberId: string, id = detail?.row.entity_id) {
    if (!id) return;
    try {
      await ports.hideLinkSetMember(id, memberId, false);
      reader.rereadReach(id);
    } catch (cause) {
      push({ text: `Could not show it again: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /**
   * *Apply* (R-7.8): the set saved with a by-hand rule for this ticket, then
   * the job re-read. *Undo* saves what that save answered without the rule,
   * on the ticket it was applied to, since the toast outlives a move.
   */
  async function applySet(set: LinkSetRow) {
    if (!detail) return;
    const id = detail.row.entity_id;
    applyingSet = false;
    try {
      const stored = await sets.saveLinkSet(withHandRule(set, id));
      reader.rereadReach(id);
      push({
        text: applyToast(stored, keyOf(id)),
        action: {
          label: "Undo",
          run: () =>
            void sets
              .saveLinkSet(withoutHandRule(stored, id))
              .then(() => reader.rereadReach(id))
              .catch((cause: unknown) => push({ text: `Could not undo it: ${ipcErrorMessage(cause)}`, tone: "err" })),
        },
      });
    } catch (cause) {
      push({ text: `Could not apply ${set.name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** *Save link set* (R-7.9): the dialog and the drawer close, the job re-reads, the toast opens the set. */
  function savedSet(set: LinkSetDetail) {
    savingSet = false;
    drawer = false;
    if (detail) reader.rereadReach(detail.row.entity_id);
    push({
      text: savedToast(set),
      action: { label: "Open it", run: () => (location.hash = `#/sets/${encodeURIComponent(set.id)}`) },
    });
  }

  async function unlinkEntry(entry: LinkEntry) {
    const { link } = entry;
    try {
      await ports.unlink(link.id);
      reader.rereadDetail();
      push({
        text: "Unlinked in knobas only.",
        action: {
          label: "Undo",
          run: () =>
            void ports
              .createLink(link.from_id, link.to_id, link.relation, link.note ?? undefined)
              .then(() => reader.rereadDetail())
              .catch((cause: unknown) => push({ text: `Could not link it again: ${ipcErrorMessage(cause)}`, tone: "err" })),
        },
      });
    } catch (cause) {
      push({ text: `Could not unlink: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  // ---- what this job queued, drawn at once (#1424) ----
  /** The surface the job's echoes are tagged with, for `watchEchoes` (#2103). */
  const JOB = "job";
  /** The status a queued move picked, drawn in the select until the ticket's status moves. */
  const statusEcho = $derived(detail ? echoOn(detail.row.entity_id, "status") : null);
  /** The comments queued here, drawn at the end of the thread until the thread holds them. */
  const queuedComments = $derived(detail ? echoesOn(detail.row.entity_id, "comment") : []);

  // The refresh after one of the job's writes, or a run of its source's end,
  // reads the ticket and its thread again (#2103), so the echoes below settle
  // without the job being opened again.
  $effect(() =>
    untrack(() =>
      watchEchoes(
        JOB,
        () => {
          reader.rereadDetail();
          reader.rereadThread();
        },
        { listen: ports.listen },
      ),
    ),
  );

  // Each read of the ticket drops a move once its status moved from where the move left it.
  $effect(() => {
    if (!detail) return;
    const id = detail.row.entity_id;
    const status = detail.status;
    untrack(() => settleEchoes(id, (echo) => echo.kind === "status" && echo.base !== status));
  });

  // Each read of the thread drops a comment it holds.
  $effect(() => {
    if (!detail || thread === null) return;
    const id = detail.row.entity_id;
    const said = thread.comments;
    untrack(() => settleEchoes(id, (echo) => echo.kind === "comment" && said.some((c) => sameWords(c.body, echo.text))));
  });

  async function comment(body: string): Promise<boolean> {
    if (!detail || commentRefused !== null) return false;
    const id = detail.row.entity_id;
    const key = keyOf(id);
    const source = detail.source;
    try {
      const write = await ports.submitWrite({ Comment: { entity: id, body } });
      echoWrite(id, "comment", body, {
        write,
        many: true,
        surface: JOB,
        source: source.id,
        following: {
          name: `Comment on ${key}`,
          resend: () => {
            askAgain(id, "comment", body);
            location.hash = `${jobHash(id)}/conversation`;
          },
        },
        follower,
      });
      toastQueued(`comment on ${key}`, source);
      return true;
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  // A clone that finished: the line reads its checkout again, and *Clone*
  // becomes *Open in…*.
  let seenClones = clones.finished;
  $effect(() => {
    if (clones.finished === seenClones) return;
    seenClones = clones.finished;
    untrack(() => reader.rereadCheckout());
  });

  /**
   * A move queued and drawn in the select, then the ticket and its transitions
   * read again: nothing else it reaches changed.
   */
  async function move(status: string) {
    if (!detail || moveRefused !== null) return;
    const id = detail.row.entity_id;
    const key = keyOf(id);
    const source = detail.source;
    const from = detail.status;
    try {
      const write = await ports.submitWrite({ Transition: { entity: id, status } });
      echoWrite(id, "status", status, {
        write,
        base: from,
        surface: JOB,
        source: source.id,
        following: {
          name: `Move of ${key} to ${status}`,
          resend: () => {
            askAgain(id, "status", status);
            location.hash = jobHash(id);
          },
        },
        follower,
      });
      reader.rereadDetail(id);
      toastQueued(`${key} to ${status}`, source);
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
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
      if (event.key.toLowerCase() !== "l" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      if (!detail || drawer || layers.open.length > 0 || inTextField(document.activeElement)) return;
      event.preventDefault();
      drawer = true;
    };
    window.addEventListener("keydown", onkey);
    return () => {
      window.removeEventListener("keydown", onkey);
      reader.close();
    };
  });

  /** A mirror row as a section draws it. */
  function line(row: EntityRow): SectionRow {
    return {
      id: row.entity_id,
      href: itemHash(row, runKindsOf(registry)),
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
      <a class="tf-btn" href="#/tickets">All tickets</a>
    </div>
  </div>
{:else if detail}
  {@const key = keyOf(detail.row.entity_id)}
  {@const address = jobHash(detail.row.entity_id)}
  <div class="tf-ws" class:tf-mapmode={current.id === "map"}>
    <TfJobCard
      {detail}
      others={reader.others}
      {offered}
      {week}
      {conversation}
      conversationFailure={threadFailure}
      {lastVisit}
      now={today()}
      {timer}
      queued={statusEcho?.text ?? null}
      queuedState={statusEcho?.state ?? "queued"}
      refused={moveRefused}
      onmove={move}
      onstop={() => ontimer?.()}
      onstart={() => onstarttimer(detail!.row.entity_id, detail!.row.title)}
    />
    <section class="tf-work">
      <div class="tf-wtabs" role="tablist">
        {#each TABS as each (each.id)}
          <a href="{address}{each.path}" role="tab" aria-selected={current.id === each.id}
            >{each.name}{#if each.id === "conversation" && conversation !== null}{#if conversation.comments > 0}{" "}<span
                    class="tf-count">{conversation.comments}</span
                  >{/if}{#if conversation.fresh > 0}{" "}<span class="tf-count tf-info">{conversation.fresh} new</span>{/if}{/if}</a
          >
        {/each}
        <div class="tf-trail">
          <button type="button" class="tf-btn tf-sm tf-conn" data-testid="job-connections" title="Connections (L)" onclick={() => (drawer = true)}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"
              ><path d="M6.5 9.5l3-3M7 4.5l1-1a2.8 2.8 0 0 1 4 4l-1 1M9 11.5l-1 1a2.8 2.8 0 0 1-4-4l1-1" /></svg
            ><b>{connectedCount(connected, groups)}</b> connected{#if proposals !== null && proposals.length > 0},
              <span class="tf-p">{proposals.length} proposed</span>{/if}
            <kbd>L</kbd>
          </button>
          <TfDetailRefresh {detail} {refresh} reread={rereadJob} page small />
        </div>
      </div>
      <TfDetailBanners {detail} failed={refresh.failed} />
      <div class="tf-wbody">
        {#if current.id === "work" && inherited !== null}
          <!-- R-4.4: hidden while the Conversation tab is open; the Map draws Dependencies alone, as prototype-6 does. -->
          <div class="tf-setsbar">
            <span class="tf-dim">Also on this job</span>
            {#each groups as group (group.key)}
              <a class="tf-setchip" href={group.href}><b>{group.label}</b> <span>{group.via}</span></a>{" "}
            {:else}
              <span class="tf-dim">no link sets yet</span>
            {/each}
            <button type="button" class="tf-linkish" onclick={() => (applyingSet = true)}>Apply a link set…</button>
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
              <a class="tf-btn tf-sm" href={entityAddress("ticket", blocker.entityId)}>Open {blocker.key}</a>
            </div>
          {/each}
          {#if starting}
            <TfStartWork {detail} {repos} suggested={suggestedRepo} {timer} api={ports.startWork} {onstarttimer} {moveRefused} />
          {/if}
          <div class="tf-grid">
            {#if !starting}
              <TfCodeSection rows={code} {reached} {checkout} {checkoutOf} {layers} {apps} />
              <TfSection id="builds" title="Builds" openLabel="Open Builds view" openHref="#/builds" footHref="#/builds" rows={lines(builds)} rollup={builds ? buildsRollup(builds) : null}>
                {#snippet before()}
                  {#each stale as warning (warning.sourceId)}
                    <div class="tf-stale">
                      {warning.text}
                      <a href="#/settings/source/{encodeURIComponent(warning.sourceId)}">{warning.fix}</a>
                    </div>
                  {/each}
                {/snippet}
              </TfSection>
            {/if}
            <TfSection id="assets" title="Assets" openLabel="Open in Dependencies" openHref="{address}/map" footHref="#/assets" rows={assetLines} rollup={assets ? assetsRollup(assets) : null} />
            <TfSection id="monitors" title="Monitors" openLabel="Open Monitors" openHref="#/monitors" footHref="#/monitors" onopen={() => (sessionFilter("monitors").scope = "job")} rows={lines(monitors)} rollup={monitors ? monitorsRollup(monitors) : null} />
            <TfSection id="pages" title="Pages" openLabel="Open Docs view" openHref="#/docs" footHref="#/docs" onopen={() => (sessionFilter("docs").scope = "job")} rows={lines(pages)} rollup={pages ? pagesRollup(pages) : null} />
          </div>
        {:else if current.id === "conversation"}
          <TfConversation
            jobKey={key}
            entityId={detail.row.entity_id}
            queued={queuedComments}
            {thread}
            {lastVisit}
            failure={threadFailure}
            now={today()}
            refused={commentRefused}
            oncomment={comment}
          />
        {:else}
          <!-- The Map tab (#874, R-11.10): Dependencies, kept to what the ticket reaches. -->
          <TfDeps ready={true} ticket={key} ticketId={detail.row.entity_id} host="job" />
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
      onlinkto={() => {
        preset = null;
        linking = true;
      }}
      onhide={(row) => void hideHere(row)}
      onshow={(memberId) => void showAgain(memberId)}
      onapplyset={() => (applyingSet = true)}
      onsaveset={() => (savingSet = true)}
    />
  {/if}
  {#if applyingSet}
    <TfApplySet
      jobKey={key}
      applied={new Set((inherited?.sets ?? []).map((set) => set.id))}
      read={sets.linkSets}
      {layers}
      onapply={(set) => void applySet(set)}
      onclose={() => (applyingSet = false)}
    />
  {/if}
  {#if savingSet}
    <TfSaveSet
      ticket={detail.row}
      links={connected.map((entry) => entry.other)}
      ports={{ saveLinkSet: sets.saveLinkSet, previewLinkSetRule: sets.previewLinkSetRule, getEntity: ports.getEntity }}
      {layers}
      onsaved={savedSet}
      onclose={() => (savingSet = false)}
    />
  {/if}
  {#if linking}
    <TfLinkTo
      jobKey={key}
      entityId={detail.row.entity_id}
      {preset}
      {ports}
      {layers}
      onclose={() => (linking = false)}
      onlinked={() => {
        linking = false;
        push({ text: "Linked in knobas" });
        reader.rereadDetail();
      }}
    />
  {/if}
{/if}
