<!--
  One page for a ticket and the job (#816, R-4.1, spec #2396): `#/ticket/<id>`,
  `#/job/<key or id>` and home's bare `#/job` (`page.ts` says which draws what).

  **Two moods, one page.** The card is on the yellow when the ticket is the
  chosen job (`chosen.svelte.ts`) and on a sheet while it is only read; the
  card's class is the only difference (`TfJobCard.svelte`). Reading never
  chooses: only a `#/job/<…>` address (`choose`), which then lands on the
  ticket's own address, and the card's foot (`footAction`) choose. *Start
  work* chooses and shows the stepper, *Resume work* chooses and starts the
  timer, *Work on this* chooses; pinned, the foot is the timer. The page's
  breadcrumb is *Tickets › KEY*; `Esc` takes the ticket page's rung (the
  opener, else Tickets) and does nothing at home.

  **The two columns** (`.tf-ws`): the card on the left, and the reading
  column (#2399) on the right: the title as the page's heading, whole; the
  *Start work* stepper while it runs and a blocked ticket's red banner
  (#818, R-4.8); the description through the Markdown component in a resting
  panel; and the whole conversation with its compose box. No tabs: an old
  `/map` or `/conversation` address draws the same page. Dependencies keeps
  its *This job* opening.

  Opening it reads the ticket, then **promotes its context** (`promote_context`,
  idempotent, so a ticket with a context keeps it and one without gets one)
  and reads what the ticket reaches (`job_reach`, and `list_entities` scoped
  by it) for the card's sections. The promotion happens once per ticket
  opened: re-reads after a write do not ask again.

  **The conversation (#819)** is read once per page opened: `visit_entity`
  records this visit and answers the last one, `entity_thread` the thread.
  A comment and a link are writes on any ticket, the chosen job or not.

  **The Connections drawer (#820)** opens with `L` and from the card's *Link
  something to it* (#2401). **Link sets (#867)**:
  `job_link_sets` is read once per page opened, for the drawer's set and
  parent groups; *Hide here* and *Show again* re-read it with the reach and
  the sections, and never edit the set (R-7.5). *Apply a link set…* and *Save
  as a link set…* (the drawer) are #869's dialogs: both save through
  `save_link_set`, then re-read the same way.

  **What the credential cannot write (#2052).** Once per page opened,
  `refused_writes` is asked about the ticket's *Comment* and its move; a
  refused one is drawn disabled with the reason (Start work's move, the
  comment box's *Comment*) and queues nothing.

  **Refresh (#2134, #2107's pieces).** The page draws no *Refresh* button
  (#2401): ⌘R runs the ticket's *Refresh*, or *Sync now* where its adapter
  reads no one item, through `TfDetailRefresh` drawn with no button. Its
  failure line and the withdrawn and turned-off banners sit at the top of
  the reading column. A refresh reads the ticket,
  its thread and its reach again through the reader (`reread`), over what the
  page drew, and the echoes settle on those reads as on any other.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import type { LinkEntry, LinkSetDetail, LinkSetRow } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { askInto, refusalOf, type RefusedWrite, type WriteAsk } from "../../shell/refused-writes";
  import { inTextField } from "../../shell/keys";
  import { toastQueued } from "../../shell/queued-toast";
  import type { Timer } from "../../shell/timer.svelte";
  import { push } from "../../shell/toasts.svelte";
  import type { Layers } from "../layers.svelte";
  import { alsoIn, hiddenOf, linkedOn, groupsOf, type GroupRow } from "./connections";
  import { blockerEntries, footAction, segmentKey, isMine, keyOf, offersStartWork, type FrameKinds } from "./job";
  import { bridgePorts, bridgeSetPorts, type JobPorts, type JobSetPorts } from "./ports";
  import { applyToast, savedToast, withHandRule, withoutHandRule } from "../sets/apply";
  import TfApplySet from "./TfApplySet.svelte";
  import TfSaveSet from "./TfSaveSet.svelte";
  import { createJobReader } from "./job-reader.svelte";
  import { kindRegistry } from "../../shell/kind-registry.svelte";
  import { currentJob, linkRequest, type LinkTarget } from "./current.svelte";
  import { chosenJob } from "./chosen.svelte";
  import { askAgain, echoWrite, echoesOn, sameWords, settleEchoes, watchEchoes } from "../queued.svelte";
  import { entityAddress } from "../address";
  import { replaceAddress } from "../opener";
  import { sectionHash, ticketHash } from "../route";
  import TfCrumbs from "../TfCrumbs.svelte";
  import type { EscTarget } from "../escape";
  import TfConnections from "./TfConnections.svelte";
  import TfConversation from "./TfConversation.svelte";
  import TfJobCard from "./TfJobCard.svelte";
  import { cardSections } from "./card-sections";
  import { cardWidth } from "./card-width.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { proseMarkdown } from "../../render/prose";
  import TfLinkTo from "./TfLinkTo.svelte";
  import TfStartWork from "./TfStartWork.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { onShownAgain } from "../while-shown.svelte";

  let {
    segment,
    choose = false,
    back = null,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports = bridgePorts,
    today = () => new Date(),
    sets = bridgeSetPorts,
    registry = kindRegistry,
    follower = writeFollower(),
  }: {
    /** What the address names: a key (`PAY-231`) or an entity id. */
    segment: string;
    /** A `#/job/<…>` address: choose the ticket as the job once read, and land on its own address. */
    choose?: boolean;
    /** Where `Esc` goes, which the breadcrumb's hint names (#2228); `null` where it does nothing. */
    back?: EscTarget | null;
    timer: Pick<Timer, "current" | "elapsed">;
    layers: Layers;
    /** The timer verb: with a timer running, stop it into the worklog draft. */
    ontimer: (() => void) | undefined;
    /** Start the timer on this ticket, stopping another into its draft first. */
    onstarttimer: (entityId: string, title: string) => void;
    ports?: JobPorts;
    today?: () => Date;
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
  const reader = createJobReader(ports, { registry: { load: () => registry.load() } });
  const detail = $derived(reader.detail);
  const failure = $derived(reader.failure);
  const thread = $derived(reader.thread);
  const threadFailure = $derived(reader.threadFailure);
  const lastVisit = $derived(reader.lastVisit);
  const proposals = $derived(reader.proposals);
  const reached = $derived(reader.reached);
  const inherited = $derived(reader.inherited);
  const sources = $derived(reader.sources);
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
  // A job address resolved on screen makes its ticket the chosen job and
  // lands on the ticket's own address (spec #2396): a bookmark, history, a
  // link, the timer chip or the Tickets preview's button. A ticket address
  // and home choose nothing. By its id, so a re-read of the same ticket (a
  // timer start, a refresh) does not take the choice back from a ticket
  // chosen since.
  const resolvedId = $derived(detail?.row.entity_id ?? null);
  $effect(() => {
    if (!choose || !pageShown() || resolvedId === null) return;
    const id = resolvedId;
    untrack(() => {
      chosenJob.choose(id);
      replaceAddress(ticketHash(id));
    });
  });
  /** Whether the ticket is the chosen job: the card's yellow, else its sheet. */
  const pinned = $derived(resolvedId !== null && chosenJob.id === resolvedId);

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
    stepping = false;
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
            location.hash = ticketHash(id);
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

  const mine = $derived(detail ? isMine(detail, sources) : false);
  /**
   * The stepper shows on the chosen job (*Start work* is what chooses a
   * ticket read), and once shown it stays on this page while the ticket
   * still offers it, so a timer that chooses another ticket meanwhile does
   * not take the steps away mid-run.
   */
  let stepping = $state(false);
  const offersSteps = $derived(detail ? offersStartWork(detail.status, mine) : false);
  $effect(() => {
    if (pinned && offersSteps) stepping = true;
  });
  const starting = $derived(stepping && offersSteps);
  const timerOnIt = $derived(
    detail !== null && detail !== undefined && timer.current?.target.kind === "entity" && timer.current.target.entity_id === detail.row.entity_id,
  );
  const action = $derived(detail ? footAction({ pinned, status: detail.status, mine, timerOnIt }) : null);

  /** The foot's button: choose the ticket, and what its label says beside. */
  function act() {
    if (!detail || action === null) return;
    const id = detail.row.entity_id;
    const title = detail.row.title;
    switch (action.does) {
      case "stop":
        ontimer?.();
        return;
      case "start-timer":
        onstarttimer(id, title);
        return;
      case "resume":
        chosenJob.choose(id);
        onstarttimer(id, title);
        return;
      case "start-work":
      case "choose":
        chosenJob.choose(id);
    }
  }
  const blocked = $derived(detail ? blockerEntries(detail) : []);
  /** The card's sections table (#2401), from what the reader has read so far. */
  const table = $derived(
    detail
      ? cardSections({
          entityId: detail.row.entity_id,
          subtasks: reader.subtasks,
          code: reader.code,
          unpushed: reader.unpushed,
          builds: reader.builds,
          pages: reader.pages,
          monitors: reader.monitors,
          assets: reader.assets,
        })
      : { rows: [], settled: false },
  );
  /** What *Start work* suggests: a repository the ticket already reaches. */
  const suggestedRepo = $derived(
    [...reached.values()].find((row) => row.kind === "repo")?.entity_id ?? null,
  );
</script>

{#if detail === undefined && failure === null}
  <div class="tf-full tf-ticketpage">
    <TfCrumbs parts={[{ name: "Tickets", href: sectionHash("tickets") }, { name: segmentKey(segment) }]} {back} />
    <div class="tf-empty"><p>Opening {segmentKey(segment)}…</p></div>
  </div>
{:else if detail === null || (detail === undefined && failure !== null)}
  <div class="tf-full tf-ticketpage">
    <TfCrumbs parts={[{ name: "Tickets", href: sectionHash("tickets") }, { name: segmentKey(segment) }]} {back} />
    <div class="tf-empty">
      {#if failure !== null}
        <p>Could not open {segmentKey(segment)}: {failure}</p>
      {:else}
        <p>No ticket {segmentKey(segment)} in the mirror. It may not have synced yet.</p>
      {/if}
      <a class="tf-btn" href="#/tickets">All tickets</a>
    </div>
  </div>
{:else if detail}
  {@const key = keyOf(detail.row.entity_id)}
  <div class="tf-full tf-ticketpage">
  <TfCrumbs parts={[{ name: "Tickets", href: sectionHash("tickets") }, { name: key }]} {back} />
  <div class="tf-ws" style:--tk-side="{cardWidth.px}px">
    <TfJobCard
      {detail}
      others={reader.others}
      parentTitle={reader.parentTitle}
      {mine}
      {pinned}
      {action}
      onaction={act}
      {table}
      onlink={() => (drawer = true)}
    />
    <section class="tf-work">
      <div class="tf-wbody">
        <div class="tf-tkread">
          <header class="tf-tkpagehead">
            <h1>{detail.row.title}</h1>
            <TfDetailRefresh {detail} {refresh} reread={rereadJob} page button={false} />
          </header>
          <TfDetailBanners {detail} failed={refresh.failed} />
          {#if failure !== null}
            <p class="tf-dim">Could not read what this ticket reaches: {failure}</p>
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
          <section aria-label="Description">
            <div class="tf-panel tf-tkdesc">
              {#if detail.description !== null && detail.description.trim() !== ""}
                {@const markdown = proseMarkdown(detail.description, detail.markup)}
                {#if markdown !== null}<TfMarkdown text={markdown} />{:else}<p class="tf-asks">{detail.description}</p>{/if}
              {:else}
                <p class="tf-dim">The ticket has no description.</p>
              {/if}
            </div>
          </section>
          <section aria-labelledby="tf-tktalk">
            <header><h2 id="tf-tktalk">Conversation</h2></header>
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
          </section>
        </div>
      </div>
    </section>
  </div>
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
