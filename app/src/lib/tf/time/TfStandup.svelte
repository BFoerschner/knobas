<!--
  Time > Standup (#829, R-13.4): the digest beside the protocol, ported from
  the rooms UI's `StandupView` and `ProtocolPanel` over the same reads and
  writes (`standup_digest`, `standup_protocol`, the notes commands,
  `publish_standup_protocol`, `standup_publish_target`,
  `create_action_item_ticket`, `action_item_ticket`).

  **The digest** is four lists, every line a link into the new shell, and
  *Copy as text*. The fourth, *Infrastructure* (#932), is the monitor alerts
  since the last working day, each drawn as its duration and the clock it
  opened at.

  **The protocol** is the day's note, drawn as prototype-6's form: who was
  there, a note per person, the action items. Every edit rewrites one section
  of the note's body (`standup-view.ts`) and saves after a pause, as the rooms
  panel did; the published page is made from that body.

  A publish and a ticket answer once their write is queued (#779) and are
  followed on the queue until they land, as in `ProtocolPanel`. A ticket that
  landed before this tab opened is read back from the protocol's links (#1433):
  the backend links the note to the ticket it found by the item's words, so an
  item whose words a linked ticket carries shows that ticket, not *Create
  ticket*. One filed before a reload whose ticket is not mirrored yet is read
  back from the protocol's `owed_tickets` and drawn as one filed now (#1951).
  Each filed ticket is an echo on the protocol's note (`tf/queued.svelte.ts`,
  #2104), *queued* and then *sent*, gone once the backend names its ticket:
  asked when the write is sent, on the refresh event and on every sync's
  end, so the ticket the post-write read missed is named by the sync that
  brings it.

  *Publish to Confluence* asks `refused_writes` about the stored target's
  page (#2053): a page whose space the source's credential may not add pages
  to draws it disabled with the reason. The first publish's picker asks
  about every page it offers.

  A date has one page. The form stays editable once published, and the read
  says whether the note has moved on from what the queue last carried to the
  page (`Protocol.changed_since`, F-327), so it survives leaving the tab. Then
  *Publish again* sends the whole note onto that page
  (`publish_standup_protocol_again`). One line under the heading says where it
  stands. Its usual words share one box, so the first keystroke after a publish
  moves nothing under the caret; a page edited in the source since
  (`page_moved`), and a write the queue held or refused, say so there too.

  The press stays one button in one place from *Publish* to *Queued* to *Sent*,
  so a second press lands on it and focus has somewhere to stay.

  `date` is the day the address names (`#/time/standup/<date>`, #1504), today
  without one.
-->
<script lang="ts">
  import TfKeyed from "../TfKeyed.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { onDestroy, tick, untrack } from "svelte";
  import { listen as tauriListen } from "@tauri-apps/api/event";

  import { ipcErrorMessage } from "../../ipc";
  import {
    actionItemTicket as realActionItemTicket,
    createActionItemTicket as realCreateTicket,
    entityBriefs as realEntityBriefs,
    getNote as realGetNote,
    listProjects as realListProjects,
    publishStandupProtocol as realPublish,
    publishStandupProtocolAgain as realPublishAgain,
    saveNote as realSaveNote,
    standupDigest as realStandupDigest,
    standupProtocol as realStandupProtocol,
    standupPublishTarget as realPublishTarget,
    type LinkEntry,
    type Project,
    type Protocol,
    type PublishTarget,
    type StandupDigest,
  } from "../../ipc/entity";
  import { search as realSearch } from "../../ipc/search";
  import { listAdapters as realListAdapters, listSources as realListSources, type QueuedWrite } from "../../ipc/sources";
  import { toastStopped, unconfirmedTeller, willNotGo, writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { toastQueued } from "../../shell/queued-toast";
  import { heldByPointer, holdUnderPointer } from "../row-hold";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import PublishTargetPicker from "../../standup/PublishTargetPicker.svelte";
  import { publishableSources } from "../../standup/protocol";
  import { digestWindows, LOOKBACK_DAYS } from "../../standup/standup";
  import { dayKey } from "../../time/day";
  import type { PipelinePorts } from "../builds/pipeline-ports";
  import { projectSource } from "../create-ticket";
  import { echoWrite, echoesOn, settleEchoes, watchEchoes, type Echo } from "../queued.svelte";
  import { keyOf } from "../job/job";
  import { entityAddress } from "../address";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfCreateTicket from "../TfCreateTicket.svelte";
  import {
    digestText,
    alertSince,
    itemTickets,
    lineHash,
    lineKey,
    ownerOf,
    protocolDay,
    readProtocol,
    withActionItem,
    withAttendees,
    withNote,
    yesterdayHeading,
  } from "./standup-view";

  interface StandupPorts {
    standupDigest: typeof realStandupDigest;
    standupProtocol: typeof realStandupProtocol;
    getNote: typeof realGetNote;
    saveNote: typeof realSaveNote;
    publishStandupProtocol: typeof realPublish;
    publishStandupProtocolAgain: typeof realPublishAgain;
    standupPublishTarget: typeof realPublishTarget;
    createActionItemTicket: typeof realCreateTicket;
    actionItemTicket: typeof realActionItemTicket;
    /** What the linked tickets and the publish target are: their titles and states, in one read for many. */
    entityBriefs: typeof realEntityBriefs;
    listProjects: typeof realListProjects;
    listSources: typeof realListSources;
    listAdapters: typeof realListAdapters;
    search: typeof realSearch;
    /** Where the queue's movements are heard (#779). */
    follower: WriteFollower;
    /** Where a sync's end is heard, to ask again for a ticket not yet named (#1951). */
    listen: PipelinePorts["listen"];
    /** *Copy as text*'s clipboard. */
    copy: (text: string) => Promise<void>;
    /** What the sources' credentials cannot write: the target's page (#2053), the projects' tickets (#2052); the IPC read when omitted. */
    refusedWrites?: RefusedWritesPort;
  }

  let {
    databaseReady = true,
    ports,
    now = () => new Date(),
    date = null,
    saveAfterMs = 700,
    layers = sharedLayers,
  }: {
    /** Whether the database answers yet: nothing is read before it does. */
    databaseReady?: boolean;
    ports?: Partial<StandupPorts>;
    now?: () => Date;
    /** The protocol's day from the address, `YYYY-MM-DD`; today when absent. */
    date?: string | null;
    /** How long a pause counts as "stopped typing", the rooms panel's. */
    saveAfterMs?: number;
    layers?: Layers;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, as the Day's ports are.
  const io: StandupPorts = {
    standupDigest: realStandupDigest,
    standupProtocol: realStandupProtocol,
    getNote: realGetNote,
    saveNote: realSaveNote,
    publishStandupProtocol: realPublish,
    publishStandupProtocolAgain: realPublishAgain,
    standupPublishTarget: realPublishTarget,
    createActionItemTicket: realCreateTicket,
    actionItemTicket: realActionItemTicket,
    entityBriefs: realEntityBriefs,
    listProjects: realListProjects,
    listSources: realListSources,
    listAdapters: realListAdapters,
    search: realSearch,
    follower: writeFollower(),
    listen: (event, handler) => tauriListen(event, handler),
    copy: (text) => navigator.clipboard.writeText(text),
    ...ports,
  };

  // svelte-ignore state_referenced_locally
  // Read once: the Time view keys this tab by its day, so another date is another mount.
  const day = date ?? dayKey(now());

  // -- the digest -------------------------------------------------------------

  let digest = $state<StandupDigest | null>(null);
  let digestFailure = $state<string | null>(null);

  async function readDigest() {
    const { today, earlier } = digestWindows(day);
    try {
      digest = await io.standupDigest(today, earlier);
      digestFailure = null;
    } catch (cause) {
      digestFailure = ipcErrorMessage(cause);
    }
  }

  async function copyDigest() {
    if (digest === null) return;
    try {
      await io.copy(digestText(digest));
      push({ text: "Copied as text" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  // -- the protocol -----------------------------------------------------------

  let protocol = $state<Protocol | null>(null);
  let bodyMd = $state("");
  let failure = $state<string | null>(null);
  let loaded = $state(false);
  let publishing = $state(false);
  /** Everyone pressed this session, so someone pressed off stays a chip to press on. */
  let known = $state<string[]>([]);
  /** What each person's box holds as typed; the body keeps it on one line. */
  let drafts = $state<Record<string, string>>({});
  let adding = $state<"person" | "item" | null>(null);
  let addText = $state("");
  let target = $state<{ stored: PublishTarget; reading: string } | null>(null);
  let timer: ReturnType<typeof setTimeout> | null = null;
  /** The protocol note's title, read once on open: its identity, which a save writes back unchanged. */
  let title = "";

  const form = $derived(readProtocol(bodyMd));
  const publication = $derived(protocol?.publication ?? null);
  const published = $derived(publication !== null && publication.page_entity_id !== null && publication.linked);
  /** Typed since the last read: the read compares what was saved, and the line can't wait for the save. */
  let edited = $state(false);
  const changedSince = $derived(published && (edited || (protocol?.changed_since ?? false)));
  const moved = $derived(protocol?.page_moved ?? false);
  const again = $derived(protocol?.again ?? null);

  async function openProtocol() {
    try {
      const opened = await io.standupProtocol(day);
      const note = await io.getNote(opened.note_id);
      protocol = opened;
      title = note.note.title;
      bodyMd = note.note.body_md;
      const read = readProtocol(bodyMd);
      known = [...read.attendees];
      drafts = { ...read.notes };
      failure = null;
      followPublication();
      followAgain();
      readOwed(opened);
      void readLinked(note.links, read.items);
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      loaded = true;
    }
  }

  /** Where a publish goes, as `Source › Page`; `null` before one is chosen. */
  async function readTarget() {
    try {
      const stored = await io.standupPublishTarget();
      if (stored === null) {
        target = null;
        return;
      }
      const [sources, page] = await Promise.all([
        io.listSources().catch(() => []),
        io.entityBriefs([stored.parent]).then((briefs) => briefs[0] ?? null, () => null),
      ]);
      const source = sources.find((candidate) => candidate.id === stored.source_id)?.display_name ?? stored.source_id;
      target = { stored, reading: `${source} › ${page?.title ?? keyOf(stored.parent)}` };
    } catch {
      target = null;
    }
  }

  let started = false;
  $effect(() => {
    if (!databaseReady || started) return;
    started = true;
    void readDigest();
    void openProtocol();
    void readTarget();
  });

  /** A new body: drawn at once, saved after a pause. */
  function edit(next: string) {
    bodyMd = next;
    edited = true;
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => void save(), saveAfterMs);
  }

  /**
   * Saves run one after another, so a slow write cannot land after a newer one
   * and put the older body back. Each takes the body when its turn comes, so
   * the last to run writes what is on screen.
   */
  let chain: Promise<unknown> = Promise.resolve();

  /** Write the body; answers whether it landed. The title is the protocol's identity and stays. */
  function save(): Promise<boolean> {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    const current = protocol;
    if (!current) return Promise.resolve(false);
    const turn = chain.then(async () => {
      try {
        await io.saveNote(current.note_id, title, bodyMd);
        failure = null;
        // The read says whether the note still differs: words typed and taken back read as the page again.
        if (protocol?.publication) void readStanding();
        return true;
      } catch (cause) {
        failure = ipcErrorMessage(cause);
        return false;
      }
    });
    chain = turn;
    return turn;
  }

  /** The notes read in the pills' order, so someone pressed off and on again keeps their place. */
  function toggle(name: string) {
    const present = form.attendees;
    edit(withAttendees(bodyMd, present.includes(name) ? present.filter((other) => other !== name) : known.filter((other) => other === name || present.includes(other))));
  }

  function addPerson(name: string) {
    const clean = name.trim();
    if (clean === "") return;
    if (!known.includes(clean)) known = [...known, clean];
    if (!form.attendees.includes(clean)) edit(withAttendees(bodyMd, [...form.attendees, clean]));
  }

  function note(name: string, text: string) {
    drafts[name] = text;
    edit(withNote(bodyMd, name, text));
  }

  function finishAdding() {
    if (adding === "person") addPerson(addText);
    else if (adding === "item") edit(withActionItem(bodyMd, addText));
    adding = null;
    addText = "";
  }

  /**
   * The add field takes the cursor as it mounts (#1511). Not `autofocus`:
   * Svelte honours that only while nothing but the body has focus, and WebKit
   * leaves focus in a notes box when *+ Add someone* is clicked, so the name
   * went into the notes.
   */
  function takeFocus(node: HTMLInputElement) {
    node.focus();
  }

  /** The add field is gone, so focus goes back to the button that opened it, and `Enter` adds the next. */
  function focusAdd(which: "person" | "item") {
    void tick().then(() => document.querySelector<HTMLElement>(`.tf-kept:not([hidden]) [data-su-add="${which}"], [data-su-add="${which}"]`)?.focus());
  }

  /**
   * Enter adds what was typed and the field stays, empty, for the next name or item: a standup has several. Enter on the
   * empty field closes it, as Esc does, without closing the view; held back, or the key would press the button focus lands on.
   */
  function addKey(event: KeyboardEvent) {
    const which = adding;
    if (which === null) return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (addText.trim() !== "") {
        if (which === "person") addPerson(addText);
        else edit(withActionItem(bodyMd, addText));
        addText = "";
        // What was added has just taken room above the field, which keeps focus: all of it in view for the next one, as a composer after a send (F-372).
        const field = event.currentTarget as HTMLElement;
        void tick().then(() => field.scrollIntoView?.({ block: "nearest" }));
        return;
      }
      adding = null;
      addText = "";
      focusAdd(which);
    }
    if (event.key === "Escape") {
      event.stopPropagation();
      addText = "";
      adding = null;
      focusAdd(which);
    }
  }

  // -- publishing ---------------------------------------------------------------

  /** The first publish asks where; every later one goes where the first said. */
  let dialog = $state<{ sources: { id: string; name: string }[]; chosen: PublishTarget | null } | null>(null);

  /** Whether the stored target's source refuses its credential a page under it (#2053), asked once it is read. */
  let refused = $state<RefusedWrite[]>([]);
  const storedParent = $derived(target?.stored.parent ?? null);
  const pageId = $derived(publication?.page_entity_id ?? null);
  $effect(() =>
    askInto(
      [...(storedParent === null ? [] : [{ entity: storedParent, op: "create_page" }]), ...(pageId === null ? [] : [{ entity: pageId, op: "update_page" }])],
      io.refusedWrites,
      (answer) => (refused = answer),
    ),
  );
  const publishRefusal = $derived(refusalOf(refused, storedParent, "create_page"));
  const againRefusal = $derived(refusalOf(refused, pageId, "update_page"));

  /** What the press says while its write is on its way, `null` when it is not. */
  const going = $derived.by(() => {
    if (publication !== null && !published) return publication.state === "held" ? "Held in the queue" : publication.state === "sent" ? "Sent" : "Queued";
    if (again?.state === "held") return "Held in the queue";
    if (again?.state === "pending" || (publishing && publication !== null)) return "Queued";
    return null;
  });
  const canAgain = $derived(published && changedSince && !moved && going === null);
  /** The press: a publish, *Publish again*, or either on its way, in one place. */
  let press = $state<HTMLButtonElement | null>(null);
  let pageLink = $state<HTMLAnchorElement | null>(null);

  /**
   * Take a new read of the protocol. Focus on the press stays there while it says *Queued*; with the write landed and
   * nothing to send, the press goes and the keeper gives focus to what held it, so a second `Enter` doesn't open the page (F-340).
   */
  function take(answer: Protocol) {
    protocol = answer;
    edited = timer !== null;
  }

  /** Bumped per read, so one answering late does not put an older protocol back. */
  let reads = 0;
  async function readStanding() {
    const mine = (reads += 1);
    let answer: Protocol;
    try {
      answer = await io.standupProtocol(day);
    } catch {
      return;
    }
    if (mine === reads) take(answer);
  }

  /**
   * A mouse's first publish: the press stays under the pointer as *Sent* once the page exists, until the pointer leaves it. The
   * link can then stand beside it, so a second press where the pointer rests can't open the page nobody aimed at (F-138's rule).
   */
  let pressHeld = $state(false);
  $effect(() => {
    if (!pressHeld) return;
    return holdUnderPointer(() => press, () => {
      const had = document.activeElement === press;
      press?.style.removeProperty("min-width");
      pressHeld = false;
      if (had) void tick().then(() => (press === null || !press.isConnected) && pageLink?.focus({ preventScroll: true }));
    });
  });

  function pressed(event: MouseEvent) {
    if (publishing) return;
    if (publication === null && heldByPointer(event)) {
      pressHeld = true;
      // *Queued* and *Sent* are shorter than *Publish to Confluence*: the box stays under the pointer as it was pressed.
      const box = event.currentTarget as HTMLElement;
      box.style.setProperty("min-width", `${box.getBoundingClientRect().width}px`);
    }
    if (publication === null) void publish();
    else if (canAgain && againRefusal === null) void publishAgain();
  }

  /** Where a publish's write goes, named in its toast. */
  /** What this date's publishing is about: *Publish again* takes the last toast's place (F-360). */
  const publishSubject = () => `standup-publish:${day}`;
  const publishSource = () => (target === null ? null : { id: target.stored.source_id, display_name: target.reading.split(" › ")[0] ?? "" });
  /** Where a sent publish shows. */
  const openThePage = {
    label: "Open the page",
    run: () => {
      if (pageId !== null) location.hash = `#/page/${encodeURIComponent(pageId)}`;
    },
  };

  /** *Publish again* (F-327): what the note says now, onto the page. A press beside a write still going queues nothing. */
  async function publishAgain() {
    if (publishing) return;
    publishing = true;
    try {
      if (!(await save())) return;
      reads += 1;
      take(await io.publishStandupProtocolAgain(day));
      failure = null;
      const source = publishSource();
      const write = protocol?.again ?? null;
      const page = pageId;
      const where = page === null ? undefined : openThePage;
      if (write === null) push({ text: "Sent: publish the protocol again.", subject: publishSubject(), ...(where === undefined ? {} : { action: where }) });
      // Already held or refused, the line under the heading says why.
      else if (write.state === "pending") {
        toastQueued("publish the protocol again", source, { write: { id: write.write_id, state: write.state }, follower: io.follower, where, subject: publishSubject() });
        followAgain();
      }
    } catch (cause) {
      // Most often the page moved since the last read, which the read now says.
      failure = ipcErrorMessage(cause);
      void readStanding();
    } finally {
      publishing = false;
    }
  }

  async function publish() {
    if (publishRefusal !== null) return;
    let stored: PublishTarget | null;
    try {
      stored = await io.standupPublishTarget();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
      return;
    }
    if (stored) {
      await send(undefined);
      return;
    }
    await openDialog();
  }

  async function openDialog() {
    try {
      const [sources, descriptors] = await Promise.all([io.listSources(), io.listAdapters()]);
      dialog = { sources: publishableSources(sources, descriptors), chosen: null };
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  async function send(chosen: PublishTarget | undefined) {
    publishing = true;
    try {
      // Save first and stop if it did not land: the page is made from the body
      // the backend holds, and a page cannot be taken back.
      if (!(await save())) return;
      reads += 1;
      take(await io.publishStandupProtocol(day, chosen));
      dialog = null;
      failure = null;
      const queued = protocol?.publication ?? null;
      const what = target ? `publish the protocol under ${target.reading}` : "publish the protocol to Confluence";
      toastQueued(what, publishSource(), queued === null ? { subject: publishSubject() } : { write: { id: queued.write_id, state: queued.state }, follower: io.follower, where: openThePage, subject: publishSubject() });
      if (chosen) void readTarget();
      followPublication();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      publishing = false;
    }
  }

  const following = new Map<number, () => void>();
  onDestroy(() => {
    if (timer !== null) {
      clearTimeout(timer);
      void save();
    }
    for (const stop of following.values()) stop();
    following.clear();
  });

  function unfollow(writeId: number) {
    following.get(writeId)?.();
    following.delete(writeId);
  }

  function followPublication() {
    const current = protocol?.publication;
    if (!current || current.linked || following.has(current.write_id)) return;
    const id = current.write_id;
    let told = false;
    // Sent and never answered (#1966): the page may be there, so no publish again.
    const unconfirmed = unconfirmedTeller({ name: "Publish" });
    following.set(
      id,
      io.follower.follow(id, (row) => {
        if (!unconfirmed(row) && willNotGo(row) && !told) {
          told = true;
          toastStopped(row, { name: "Publish", resend: () => void openDialog() });
        }
        void reread(id);
      }),
    );
  }

  /** A *Publish again* on its way: refused or held, it says so; settled, the read says what the page holds. */
  function followAgain() {
    const current = protocol?.again;
    if (!current || current.state !== "pending" || following.has(current.write_id)) return;
    const id = current.write_id;
    let told = false;
    const unconfirmed = unconfirmedTeller({ name: "Publish again" });
    following.set(
      id,
      io.follower.follow(id, (row) => {
        if (!unconfirmed(row) && willNotGo(row) && !told) {
          told = true;
          toastStopped(row, { name: "Publish again" });
        }
        void readStanding();
        if (row === undefined || willNotGo(row)) unfollow(id);
      }),
    );
  }

  async function reread(id: number) {
    const mine = (reads += 1);
    let answer: Protocol;
    try {
      answer = await io.standupProtocol(day);
    } catch {
      return;
    }
    if (mine === reads) take(answer);
    const latest = answer.publication;
    if (latest === null || latest.write_id !== id || latest.linked || latest.state === "held") unfollow(id);
  }

  // -- action items -------------------------------------------------------------

  /** The surface the action items' echoes are tagged with, for `watchEchoes` (#2104). */
  const STANDUP = "standup";
  /** What a filed ticket's echo sits on: the protocol's note. */
  const noteEchoes = (note: string) => `standup:${note}`;
  /** The tickets this session filed and named, by the action item's words. */
  let named = $state<{ writeId: number; text: string; ticket: string; status: string | null }[]>([]);
  /** The protocol's ticket echoes, oldest first: each a create on its way (#2104). */
  const filed = $derived(protocol === null ? [] : echoesOn(noteEchoes(protocol.note_id), "ticket"));

  /**
   * A create on its way, drawn *queued* and then *sent* (`tf/queued.svelte.ts`):
   * held or refused, it goes with a toast whose *Edit and resend* opens the
   * dialog again; sent, the backend is asked which ticket it made.
   */
  function echoTicket(note: string, write: { id: number; state?: QueuedWrite["state"] }, text: string, project: string | null, type: string) {
    echoWrite(noteEchoes(note), "ticket", text, {
      write,
      many: true,
      following: { name: "Ticket", resend: () => void openTicketDialog(text, project, type) },
      follower: io.follower,
      surface: STANDUP,
      source: project === null ? null : projectSource(project),
      onsent: () => void address(write.id),
    });
  }

  /**
   * The creates filed before this tab opened that the protocol does not link
   * yet (#1951), drawn as this session's own are. Only those on their way,
   * pending, sent, or held as unconfirmed, whose ticket may exist (#1966): a
   * refused one or one held on drift waits on the reader in the queue, and
   * the item offers *Create ticket* as it does once a live follow is told so.
   */
  function readOwed(opened: Protocol) {
    for (const owed of opened.owed_tickets) {
      if (owed.state !== "pending" && owed.state !== "sent" && !owed.may_have_landed) continue;
      if (echoesOn(noteEchoes(opened.note_id), "ticket").some((echo) => echo.write === owed.write_id)) continue;
      echoTicket(opened.note_id, { id: owed.write_id, state: owed.state }, owed.title, null, "Task");
    }
  }

  // The refresh after a create, or the end of a sync, which may be any
  // source's, asks again which ticket each create on its way made (#2104).
  $effect(() =>
    untrack(() =>
      watchEchoes(
        STANDUP,
        () => {
          for (const echo of filed) if (echo.write !== null) void address(echo.write);
        },
        { listen: io.listen, everyRun: true },
      ),
    ),
  );
  let ticketFor = $state<{ text: string; project: string | null; type: string } | null>(null);
  let projects = $state<Project[]>([]);

  async function openTicketDialog(text: string, project: string | null = null, type = "Task") {
    ticketFor = { text, project, type };
    try {
      projects = await io.listProjects();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  async function fileTicket(asked: { text: string; project: string; type: string }) {
    const current = protocol;
    if (!current) return;
    try {
      const made = await io.createActionItemTicket(
        current.note_id,
        asked.project,
        asked.type,
        asked.text,
        `From the standup protocol of ${day}.`,
      );
      ticketFor = null;
      echoTicket(current.note_id, { id: made.write_id, state: made.state }, asked.text, asked.project, asked.type);
      push({ text: `Ticket queued: ${asked.text}` });
      if (made.ticket_entity_id !== null) void landed(current.note_id, made.write_id, made.ticket_entity_id);
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  /** The echo's check (#2104): the backend names the ticket the create made, and the echo goes; refused or discarded, it goes too. */
  async function address(writeId: number) {
    const current = protocol;
    if (!current || !filed.some((echo) => echo.write === writeId)) return;
    let answer;
    try {
      answer = await io.actionItemTicket(current.note_id, writeId);
    } catch {
      return;
    }
    if (answer.state === "refused" || answer.state === "discarded") {
      settleEchoes(noteEchoes(current.note_id), (echo) => echo.write === writeId);
      return;
    }
    if (answer.ticket_entity_id === null) return;
    await landed(current.note_id, writeId, answer.ticket_entity_id);
  }

  /** The ticket is in the mirror: its key, and its status as the source says it. */
  async function landed(note: string, writeId: number, ticket: string) {
    const echo = echoesOn(noteEchoes(note), "ticket").find((one) => one.write === writeId);
    if (echo === undefined || named.some((made) => made.writeId === writeId)) return;
    named = [...named, { writeId, text: echo.text, ticket, status: null }];
    settleEchoes(noteEchoes(note), (one) => one.write === writeId);
    let status: string | null = null;
    try {
      const brief = (await io.entityBriefs([ticket]))[0];
      // Not mirrored yet: the key is enough to open it; the status comes with the next read.
      if (brief === undefined) return;
      status = brief.state?.text ?? null;
    } catch {
      // The key is enough to open it; the status comes with the next read.
      return;
    }
    named = named.map((made) => (made.writeId === writeId ? { ...made, status } : made));
  }

  /** The tickets the protocol already links for an item (#1433), each with its status once read. */
  let linked = $state<{ text: string; ticket: string; status: string | null }[]>([]);

  async function readLinked(links: LinkEntry[], items: readonly { text: string }[]) {
    const found = itemTickets(links, items);
    linked = found.map((match) => ({ ...match, status: null }));
    if (found.length === 0) return;
    // Every linked ticket's status in one read (the query audit, 2026-11-12: one `get_entity` a ticket).
    let briefs: Awaited<ReturnType<typeof io.entityBriefs>>;
    try {
      briefs = await io.entityBriefs([...new Set(found.map(({ ticket }) => ticket))]);
    } catch {
      return;
    }
    const status = new Map(briefs.map((brief) => [brief.entity_id, brief.state?.text ?? null]));
    linked = linked.map((known) => (status.has(known.ticket) ? { ...known, status: status.get(known.ticket)! } : known));
  }

  /** This session's newest create for the item's words, on its way or named, else the ticket the protocol links for them. */
  const filedFor = (text: string): { ticket: string | null; status: string | null; state: Echo["state"] } | null => {
    const going = [...filed].reverse().find((echo) => echo.text.trim() === text.trim());
    if (going !== undefined) return { ticket: null, status: null, state: going.state };
    const made = [...named].reverse().find((one) => one.text.trim() === text.trim()) ?? linked.find((known) => known.text === text);
    return made === undefined ? null : { ticket: made.ticket, status: made.status, state: "sent" };
  };
</script>

<div class="tf-su">
  <section class="tf-panel tf-headed tf-su-dig" aria-label="Your digest">
    <header>
      <h3>Your digest</h3>
      <button class="tf-btn tf-sm tf-ghost" disabled={digest === null} onclick={() => void copyDigest()}>
        Copy as text
      </button>
    </header>
    <div class="tf-su-body">
      {#if digestFailure}
        <p class="tf-tm-fail" role="alert">{digestFailure}</p>
      {/if}
      {#snippet list(heading: string, lines: StandupDigest["today"], silence: string, yesterday: boolean, alert = false)}
        <h4>{heading}</h4>
        {#if lines.length === 0}
          <p class="tf-su-none">{digest === null ? "Reading…" : silence}</p>
        {:else}
          <ul>
            {#each lines as line, index (index)}
              {@const href = lineHash(line)}
              {@const key = lineKey(line)}
              {#snippet words()}
                {#if key}<span class="tf-mono">{key}</span>{" "}{/if}{line.title}{#if alert}{" "}<TfKeyed text={line.reason} /><span
                    class="tf-dim">{`, ${alertSince(line)}`}</span
                  >{:else if yesterday}: <TfKeyed text={line.reason} />{:else}{" "}<span
                    class="tf-dim">(<TfKeyed text={line.reason} />)</span
                  >{/if}
              {/snippet}
              <li>
                {#if href}<a {href}>{@render words()}</a>{:else}<span>{@render words()}</span>{/if}
              </li>
            {/each}
          </ul>
        {/if}
      {/snippet}
      {@render list(
        yesterdayHeading(digest?.yesterday_day ?? null),
        digest?.yesterday ?? [],
        `Nothing of yours in the last ${LOOKBACK_DAYS} days.`,
        true,
      )}
      {@render list("Today", digest?.today ?? [], "Nothing touched yet today.", false)}
      {@render list("Blockers", digest?.blockers ?? [], "Nothing of yours is blocked.", false)}
      {@render list("Infrastructure", digest?.infrastructure ?? [], "No monitor alerts.", false, true)}
    </div>
  </section>

  <section class="tf-panel tf-headed tf-su-proto" aria-label="Protocol">
    <header>
      <h3>Protocol</h3>
      <span class="tf-dim tf-su-day">{protocolDay(day)}</span>
      <span class="tf-trail" class:tf-su-held={pressHeld}>
        <!-- The press stands before the link, so the link stays put when the press goes and a second press can't land on it. -->
        {#if publication === null || going !== null || canAgain || pressHeld}
          <button
            class="tf-btn tf-sm tf-primary"
            bind:this={press}
            disabled={publication === null && (!loaded || protocol === null)}
            aria-disabled={going !== null || pressHeld || (publication === null && publishing) ? "true" : undefined}
            {...going === null ? refusedAttrs(publication === null ? publishRefusal : againRefusal) : {}}
            onclick={pressed}
          >
            <!-- The first publish asks where, so it has the ellipsis; with a target stored it acts at once. -->
            {going ?? (pressHeld && published ? "Sent" : publication === null ? `Publish to Confluence${target === null ? "…" : ""}` : "Publish again")}
          </button>
        {/if}
        {#if published && publication?.page_entity_id}
          <a class="tf-btn tf-sm tf-ghost" bind:this={pageLink} href="#/page/{encodeURIComponent(publication.page_entity_id)}">Open the published page</a>
        {/if}
      </span>
    </header>
    <div class="tf-su-body">
      {#if failure}
        <p class="tf-tm-fail" role="alert">{failure}</p>
      {/if}
      {#if publication !== null}
        {@const stopped = published && moved ? "moved" : going === "Held in the queue" ? "held" : again?.state === "refused" ? "refused" : null}
        {#if stopped !== null}
          <div class="tf-stale tf-su-pub" role="status">
            {#if stopped === "moved"}
              Edited in Confluence since it was published, so knobas won't replace it. Open the page and copy your change across.
            {:else if stopped === "held"}
              <span>Held in the queue{(again?.detail ?? publication.detail) ? `: ${again?.detail ?? publication.detail}` : "."}</span>
              <button type="button" class="tf-btn tf-sm" onclick={openWriteQueue}>Show queue</button>
            {:else}
              <span>Confluence refused it{again?.detail ? `: ${again.detail}` : "."} The page keeps the earlier version.</span>
              <button type="button" class="tf-btn tf-sm" onclick={openWriteQueue}>Show queue</button>
            {/if}
          </div>
        {:else}
          <!-- One box for all three, as wide as the longest takes: the words change under the caret and nothing moves. -->
          <div class="tf-su-pub tf-su-publine" class:tf-su-pubchanged={published && changedSince} role="status">
            <span class:tf-off={published}>Queued. The page shows this once it lands.</span>
            <span class:tf-off={!published || changedSince}>Published. Change something and you can publish it again.</span>
            <span class:tf-off={!published || !changedSince}>Changed since it was published. The page still shows the earlier version.</span>
          </div>
        {/if}
      {/if}

      <div class="tf-su-h">
        <div class="tf-pane-h">Who was there</div>
        {#if adding !== "person"}
          <button class="tf-btn tf-sm tf-ghost" data-su-add="person" disabled={!loaded || protocol === null} onclick={() => (adding = "person")}>
            Add someone
          </button>
        {/if}
      </div>
      <div class="tf-su-people">
        <div class="tf-fchips" role="group" aria-label="Who was there">
          {#each known as name (name)}
            <button type="button" aria-pressed={form.attendees.includes(name)} onclick={() => toggle(name)}>
              {name}
            </button>
          {/each}
        </div>
        {#if adding === "person"}
          <input
            class="tf-su-add"
            aria-label="Who else was there"
            placeholder="Name"
            use:takeFocus
            bind:value={addText}
            onkeydown={addKey}
            onblur={finishAdding}
          />
        {/if}
      </div>

      {#each form.attendees as name (name)}
        <label class="tf-su-pn">
          <span>{name}</span>
          <textarea rows="2" placeholder="Notes" aria-label="Notes for {name}" value={drafts[name] ?? form.notes[name] ?? ""} oninput={(event) => note(name, event.currentTarget.value)}></textarea>
        </label>
      {/each}

      <div class="tf-su-h tf-su-gap">
        <div class="tf-pane-h">Action items</div>
        {#if adding !== "item"}
          <button class="tf-btn tf-sm tf-ghost" data-su-add="item" disabled={!loaded || protocol === null} onclick={() => (adding = "item")}>
            Add an action item
          </button>
        {/if}
      </div>
      {#each form.items as item, index (index)}
        {@const made = filedFor(item.text)}
        {@const owner = ownerOf(item.text)}
        <div class="tf-su-ai">
          <span class:tf-su-done={item.done}>{owner.what}{#if owner.who}{" "}<span class="tf-dim">{owner.who}</span>{/if}</span>
          {#if made === null}
            <button class="tf-btn tf-sm" onclick={() => void openTicketDialog(item.text)}>Create ticket…</button>
          {:else if made.ticket === null}
            <TfStateChip text={made.state} tone="info" />
          {:else}
            <a class="tf-mono" href={entityAddress("ticket", made.ticket)}>{keyOf(made.ticket)}</a>
            {#if made.status}<TfStateChip text={made.status} />{/if}
          {/if}
        </div>
      {/each}
      {#if adding === "item"}
        <input
          class="tf-su-add tf-su-wide"
          aria-label="New action item"
          placeholder="New action item"
          use:takeFocus
          bind:value={addText}
          onkeydown={addKey}
          onblur={finishAdding}
        />
      {/if}

      <p class="tf-su-foot">
        {#if target && publication !== null}
          New protocols publish under <b>{target.reading}</b>. Change it in <a href="#/settings/standup">Settings › Standup</a>.
        {:else if target}
          Publishes under <b>{target.reading}</b>. Change it in <a href="#/settings/standup">Settings › Standup</a>.
        {:else}
          The first publish asks where. Change it in <a href="#/settings/standup">Settings › Standup</a>.
        {/if}
      </p>
    </div>
  </section>
</div>

{#if dialog}
  <TfDialog why={dialog!.chosen === null && !publishing ? "Choose a page first." : null} title="Where do standup protocols go?" {layers} onclose={() => (dialog = null)}>
    {#snippet body()}
      <!-- In the fields' frame, as Settings › Standup and *Move notes* draw it: bare, the parent page's box read as a line of
           text with its placeholder for a value (the live run, 2026-11-09). -->
      <div class="tf-stg-pick">
        <PublishTargetPicker
          sources={dialog!.sources}
          search={io.search}
          refusedWrites={io.refusedWrites}
          onchange={(chosen) => (dialog!.chosen = chosen)}
        />
      </div>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button
        class="tf-btn tf-primary"
        disabled={dialog!.chosen === null || publishing}
        onclick={() => void send(dialog!.chosen!)}
      >
        Publish
      </button>
    {/snippet}
  </TfDialog>
{/if}

{#if ticketFor}
  <TfCreateTicket
    draft={{ project: ticketFor.project, type: ticketFor.type, title: ticketFor.text, body: "" }}
    {projects}
    {layers}
    refusedWrites={io.refusedWrites}
    onclose={() => (ticketFor = null)}
    onfile={(draft) => void fileTicket({ text: ticketFor!.text, project: draft.project, type: draft.type })}
  />
{/if}
