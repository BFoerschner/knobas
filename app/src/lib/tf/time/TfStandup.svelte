<!--
  Time > Standup (#829, R-13.4): the digest beside the protocol, ported from
  the rooms UI's `StandupView` and `ProtocolPanel` over the same reads and
  writes (`standup_digest`, `standup_protocol`, the notes commands,
  `publish_standup_protocol`, `standup_publish_target`,
  `create_action_item_ticket`, `action_item_ticket`).

  **The digest** is three lists, every line a link into the new shell, and
  *Copy as text*. Prototype-6's fourth list, *Infrastructure*, waits for a
  field `StandupDigest` does not carry (named in the PR, a follow-up).

  **The protocol** is the day's note, drawn as prototype-6's form: who was
  there, a note per person, the action items. Every edit rewrites one section
  of the note's body (`standup-view.ts`) and saves after a pause, as the rooms
  panel did; the published page is made from that body.

  A publish and a ticket answer once their write is queued (#779) and are
  followed on the queue until they land, as in `ProtocolPanel`.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    actionItemTicket as realActionItemTicket,
    createActionItemTicket as realCreateTicket,
    getEntity as realGetEntity,
    getNote as realGetNote,
    listProjects as realListProjects,
    publishStandupProtocol as realPublish,
    saveNote as realSaveNote,
    standupDigest as realStandupDigest,
    standupProtocol as realStandupProtocol,
    standupPublishTarget as realPublishTarget,
    type Project,
    type Protocol,
    type PublishTarget,
    type StandupDigest,
  } from "../../ipc/entity";
  import { search as realSearch } from "../../ipc/search";
  import { listAdapters as realListAdapters, listSources as realListSources } from "../../ipc/sources";
  import { toastStopped, willNotGo, writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { push } from "../../shell/toasts.svelte";
  import PublishTargetPicker from "../../standup/PublishTargetPicker.svelte";
  import { publishableSources } from "../../standup/protocol";
  import { digestWindows, LOOKBACK_DAYS } from "../../standup/standup";
  import { dayKey } from "../../time/day";
  import { keyOf } from "../job/job";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import {
    digestText,
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
    standupPublishTarget: typeof realPublishTarget;
    createActionItemTicket: typeof realCreateTicket;
    actionItemTicket: typeof realActionItemTicket;
    getEntity: typeof realGetEntity;
    listProjects: typeof realListProjects;
    listSources: typeof realListSources;
    listAdapters: typeof realListAdapters;
    search: typeof realSearch;
    /** Where the queue's movements are heard (#779). */
    follower: WriteFollower;
    /** *Copy as text*'s clipboard. */
    copy: (text: string) => Promise<void>;
  }

  let {
    databaseReady = true,
    ports,
    now = () => new Date(),
    saveAfterMs = 700,
    layers = sharedLayers,
  }: {
    /** Whether the database answers yet: nothing is read before it does. */
    databaseReady?: boolean;
    ports?: Partial<StandupPorts>;
    now?: () => Date;
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
    standupPublishTarget: realPublishTarget,
    createActionItemTicket: realCreateTicket,
    actionItemTicket: realActionItemTicket,
    getEntity: realGetEntity,
    listProjects: realListProjects,
    listSources: realListSources,
    listAdapters: realListAdapters,
    search: realSearch,
    follower: writeFollower(),
    copy: (text) => navigator.clipboard.writeText(text),
    ...ports,
  };

  // svelte-ignore state_referenced_locally
  const day = dayKey(now());

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

  const form = $derived(readProtocol(bodyMd));
  const publication = $derived(protocol?.publication ?? null);

  async function openProtocol() {
    try {
      const opened = await io.standupProtocol(day);
      const note = await io.getNote(opened.note_id);
      protocol = opened;
      bodyMd = note.note.body_md;
      const read = readProtocol(bodyMd);
      known = [...read.attendees];
      drafts = { ...read.notes };
      failure = null;
      followPublication();
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
        io.getEntity(stored.parent).catch(() => null),
      ]);
      const source = sources.find((candidate) => candidate.id === stored.source_id)?.display_name ?? stored.source_id;
      target = { stored, reading: `${source} › ${page?.row.title ?? keyOf(stored.parent)}` };
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
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => void save(), saveAfterMs);
  }

  /** Write the body; answers whether it landed. The title is the protocol's identity and stays. */
  async function save(): Promise<boolean> {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    const current = protocol;
    if (!current) return false;
    try {
      const note = await io.getNote(current.note_id);
      await io.saveNote(current.note_id, note.note.title, bodyMd);
      failure = null;
      return true;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
      return false;
    }
  }

  function toggle(name: string) {
    const present = form.attendees;
    edit(withAttendees(bodyMd, present.includes(name) ? present.filter((other) => other !== name) : [...present, name]));
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

  /** Enter adds what was typed; Esc drops it without closing the view. */
  function addKey(event: KeyboardEvent) {
    if (event.key === "Enter") finishAdding();
    if (event.key === "Escape") {
      event.stopPropagation();
      addText = "";
      adding = null;
    }
  }

  // -- publishing ---------------------------------------------------------------

  /** The first publish asks where; every later one goes where the first said. */
  let dialog = $state<{ sources: { id: string; name: string }[]; chosen: PublishTarget | null } | null>(null);

  async function publish() {
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
      protocol = await io.publishStandupProtocol(day, chosen);
      dialog = null;
      failure = null;
      push({ text: target ? `Publishing to Confluence under ${target.reading}` : "Publishing to Confluence" });
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
    following.set(
      id,
      io.follower.follow(id, (row) => {
        if (willNotGo(row) && !told) {
          told = true;
          toastStopped(row, { name: "Publish", resend: () => void openDialog() });
        }
        void reread(id);
      }),
    );
  }

  async function reread(id: number) {
    let answer: Protocol;
    try {
      answer = await io.standupProtocol(day);
    } catch {
      return;
    }
    protocol = answer;
    const latest = answer.publication;
    if (latest === null || latest.write_id !== id || latest.linked || latest.state === "held") unfollow(id);
  }

  // -- action items -------------------------------------------------------------

  /** The tickets filed this session, by the action item's words. */
  let filed = $state<
    { writeId: number; text: string; project: string; type: string; ticket: string | null; status: string | null }[]
  >([]);
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

  async function fileTicket() {
    const asked = ticketFor;
    const current = protocol;
    if (!asked || !current || asked.project === null) return;
    try {
      const made = await io.createActionItemTicket(
        current.note_id,
        asked.project,
        asked.type,
        asked.text,
        `From the standup protocol of ${day}.`,
      );
      ticketFor = null;
      filed = [
        ...filed,
        { writeId: made.write_id, text: asked.text, project: asked.project, type: asked.type, ticket: null, status: null },
      ];
      push({ text: `Ticket queued: ${asked.text}` });
      if (made.ticket_entity_id === null) followTicket(made.write_id);
      else void landed(made.write_id, made.ticket_entity_id);
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  function followTicket(writeId: number) {
    following.set(
      writeId,
      io.follower.follow(writeId, (row) => {
        const echo = filed.find((made) => made.writeId === writeId);
        if (!echo) return unfollow(writeId);
        if (willNotGo(row)) {
          unfollow(writeId);
          filed = filed.filter((made) => made.writeId !== writeId);
          toastStopped(row, { name: "Ticket", resend: () => void openTicketDialog(echo.text, echo.project, echo.type) });
          return;
        }
        if (row === undefined) void address(writeId);
      }),
    );
  }

  async function address(writeId: number) {
    const current = protocol;
    if (!current) return;
    let answer;
    try {
      answer = await io.actionItemTicket(current.note_id, writeId);
    } catch {
      return;
    }
    if (answer.state === "refused" || answer.state === "discarded") {
      unfollow(writeId);
      filed = filed.filter((made) => made.writeId !== writeId);
      return;
    }
    if (answer.ticket_entity_id === null) return;
    unfollow(writeId);
    await landed(writeId, answer.ticket_entity_id);
  }

  /** The ticket is in the mirror: its key, and its status as the source says it. */
  async function landed(writeId: number, ticket: string) {
    let status: string | null = null;
    try {
      status = (await io.getEntity(ticket)).row.state?.text ?? null;
    } catch {
      // The key is enough to open it; the status comes with the next read.
    }
    filed = filed.map((made) => (made.writeId === writeId ? { ...made, ticket, status } : made));
  }

  const filedFor = (text: string) => [...filed].reverse().find((made) => made.text === text) ?? null;
</script>

<div class="tf-su">
  <section class="tf-panel tf-su-dig" aria-label="Your digest">
    <header>
      <h3>Your digest</h3>
      <button class="tf-btn tf-sm tf-ghost tf-open" disabled={digest === null} onclick={() => void copyDigest()}>
        Copy as text
      </button>
    </header>
    <div class="tf-su-body">
      {#if digestFailure}
        <p class="tf-tm-fail" role="alert">{digestFailure}</p>
      {/if}
      {#snippet list(heading: string, lines: StandupDigest["today"], silence: string, yesterday: boolean)}
        <h4>{heading}</h4>
        {#if lines.length === 0}
          <p class="tf-su-none">{digest === null ? "Reading…" : silence}</p>
        {:else}
          <ul>
            {#each lines as line, index (index)}
              {@const href = lineHash(line)}
              {@const key = lineKey(line)}
              {#snippet words()}
                {#if key}<span class="tf-mono">{key}</span>{" "}{/if}{line.title}{#if yesterday}{`: ${line.reason}`}{:else}{" "}<span
                    class="tf-dim">({line.reason})</span
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
    </div>
  </section>

  <section class="tf-panel tf-su-proto" aria-label="Protocol">
    <header>
      <h3>Protocol</h3>
      <span class="tf-dim tf-su-day">{protocolDay(day)}</span>
      {#if publication === null}
        <button
          class="tf-btn tf-sm tf-primary tf-open"
          disabled={!loaded || publishing || protocol === null}
          onclick={() => void publish()}
        >
          Publish to Confluence
        </button>
      {:else if publication.page_entity_id !== null && publication.linked}
        <a class="tf-btn tf-sm tf-ghost tf-open" href="#/page/{encodeURIComponent(publication.page_entity_id)}">Published</a>
      {:else}
        <span class="tf-chip tf-open">{publication.state === "held" ? "held in the queue" : "publishing"}</span>
      {/if}
    </header>
    <div class="tf-su-body">
      {#if failure}
        <p class="tf-tm-fail" role="alert">{failure}</p>
      {/if}
      {#if publication?.detail}
        <p class="tf-su-none">{publication.detail}</p>
      {/if}

      <div class="tf-pane-h">Who was there</div>
      <div class="tf-su-people">
        {#each known as name (name)}
          <button class="tf-su-val" aria-pressed={form.attendees.includes(name)} onclick={() => toggle(name)}>
            {name}
          </button>
        {/each}
        {#if adding === "person"}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="tf-su-add"
            aria-label="Who else was there"
            placeholder="Name"
            autofocus
            bind:value={addText}
            onkeydown={addKey}
            onblur={finishAdding}
          />
        {:else}
          <button class="tf-btn tf-sm tf-ghost" disabled={!loaded || protocol === null} onclick={() => (adding = "person")}>
            + Add someone
          </button>
        {/if}
      </div>

      {#each form.attendees as name (name)}
        <label class="tf-su-pn">
          <span>{name}</span>
          <textarea rows="2" placeholder="Notes" aria-label="Notes for {name}" value={drafts[name] ?? form.notes[name] ?? ""} oninput={(event) => note(name, event.currentTarget.value)}></textarea>
        </label>
      {/each}

      <div class="tf-pane-h tf-su-gap">Action items</div>
      {#each form.items as item, index (index)}
        {@const made = filedFor(item.text)}
        {@const owner = ownerOf(item.text)}
        <div class="tf-su-ai">
          <span class:tf-su-done={item.done}>{owner.what}{#if owner.who}{" "}<span class="tf-dim">{owner.who}</span>{/if}</span>
          {#if made === null}
            <button class="tf-btn tf-sm" onclick={() => void openTicketDialog(item.text)}>Create ticket</button>
          {:else if made.ticket === null}
            <span class="tf-chip">queued</span>
          {:else}
            <a class="tf-mono" href="#/job/{encodeURIComponent(keyOf(made.ticket))}">{keyOf(made.ticket)}</a>
            {#if made.status}<span class="tf-chip">{made.status}</span>{/if}
          {/if}
        </div>
      {/each}
      {#if adding === "item"}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="tf-su-add tf-su-wide"
          aria-label="New action item"
          placeholder="New action item"
          autofocus
          bind:value={addText}
          onkeydown={addKey}
          onblur={finishAdding}
        />
      {:else}
        <button class="tf-btn tf-sm tf-ghost" disabled={!loaded || protocol === null} onclick={() => (adding = "item")}>
          + Add an action item
        </button>
      {/if}

      <p class="tf-su-foot">
        {#if target}
          Publishes under <b>{target.reading}</b>. Change it in <a href="#/settings/standup">Settings › Standup</a>.
        {:else}
          The first publish asks where. Change it in <a href="#/settings/standup">Settings › Standup</a>.
        {/if}
      </p>
    </div>
  </section>
</div>

{#if dialog}
  <TfDialog title="Where do standup protocols go?" {layers} onclose={() => (dialog = null)}>
    {#snippet body()}
      <PublishTargetPicker sources={dialog!.sources} search={io.search} onchange={(chosen) => (dialog!.chosen = chosen)} />
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
  <TfDialog title="Create a ticket" {layers} onclose={() => (ticketFor = null)}>
    {#snippet body()}
      <p class="tf-dlg-note">{ticketFor!.text}</p>
      <div class="tf-formgrid">
        <span>Project</span>
        <select
          aria-label="Project the ticket goes in"
          value={ticketFor!.project ?? ""}
          onchange={(event) => (ticketFor!.project = event.currentTarget.value || null)}
        >
          <option value="">Choose…</option>
          {#each projects as project (`${project.source_id}:${project.key}`)}
            <option value={`${project.source_id}:${project.key}`}>{project.name ?? project.key}</option>
          {/each}
        </select>
        <span>Type</span>
        <input
          type="text"
          aria-label="Ticket type"
          value={ticketFor!.type}
          oninput={(event) => (ticketFor!.type = event.currentTarget.value)}
        />
      </div>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (ticketFor = null)}>Cancel</button>
      <button
        class="tf-btn tf-primary"
        disabled={ticketFor!.project === null || ticketFor!.type.trim() === ""}
        onclick={() => void fileTicket()}
      >
        Create ticket
      </button>
    {/snippet}
  </TfDialog>
{/if}
