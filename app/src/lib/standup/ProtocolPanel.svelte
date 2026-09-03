<!--
  The standup protocol, under the digest (issue #289, spec #272 stories 64-69).

  `CONTEXT.md`'s **standup protocol**: *a note — one per date — holding
  attendees, per-person notes and action items. Publish creates a Confluence
  page from it under a configured parent and links note and page; the note
  stays the editable original.*

  ## It is a note, and it is edited like one

  There is no *Save*. The row exists before the first keystroke — the backend
  get-or-creates it when this panel opens — and every pause writes, which is
  `NoteView`'s decision and its reasoning in full: the write is idempotent, and
  the answer carries the refs it reconciled. What this panel does **not** share
  with `NoteView` is the slide-over frame: a protocol is read here, under the
  digest, next to the three lists it is the minutes of.

  ## Publishing asks once, and then never again

  The first *Publish* opens the target dialog; the answer becomes the setting,
  and every later publish uses it silently. With **two** Confluence sources the
  dialog has no preselection — `presumedSource` is what decides, and story 68
  is the reason: knobas picking one would put the team's standup in the wrong
  instance.

  Once published, the button is gone. A date has one page, and the reasoning is
  `knobas_app::protocol`'s ruling: delivery is at-least-once and a re-sent
  create has no version check, so knobas asks once and Confluence's own
  per-space title uniqueness is what catches the redelivery knobas cannot see.
  The panel says which page it is and links to it.

  ## Untrusted text

  The body is the reader's own markdown and is rendered as **text** — a
  `<textarea>` and, for the action items, plain text nodes. Nothing here is
  `{@html}`, which `shell/house-rules.test.ts` enforces anyway.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    createActionItemTicket as realCreateTicket,
    getNote as realGetNote,
    listProjects as realListProjects,
    publishStandupProtocol as realPublish,
    saveNote as realSaveNote,
    standupProtocol as realStandupProtocol,
    standupPublishTarget as realPublishTarget,
    type Project,
    type Protocol,
    type PublishTarget,
  } from "../ipc/entity";
  import { search as realSearch, type SearchHit } from "../ipc/search";
  import { listAdapters as realListAdapters, listSources as realListSources } from "../ipc/sources";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { push } from "../shell/toasts.svelte";
  import { actionItems, presumedSource, publishableSources } from "./protocol";

  interface ProtocolPorts {
    standupProtocol: typeof realStandupProtocol;
    getNote: typeof realGetNote;
    saveNote: typeof realSaveNote;
    publishStandupProtocol: typeof realPublish;
    standupPublishTarget: typeof realPublishTarget;
    createActionItemTicket: typeof realCreateTicket;
    listProjects: typeof realListProjects;
    listSources: typeof realListSources;
    listAdapters: typeof realListAdapters;
    search: typeof realSearch;
  }

  let {
    day,
    router,
    ports,
    saveAfterMs = 700,
  }: {
    /** The date this protocol is for, `YYYY-MM-DD`. */
    day: string;
    router: Router;
    ports?: Partial<ProtocolPorts>;
    /** How long a pause counts as "stopped typing" — `NoteView`'s prop. */
    saveAfterMs?: number;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io: ProtocolPorts = {
    standupProtocol: realStandupProtocol,
    getNote: realGetNote,
    saveNote: realSaveNote,
    publishStandupProtocol: realPublish,
    standupPublishTarget: realPublishTarget,
    createActionItemTicket: realCreateTicket,
    listProjects: realListProjects,
    listSources: realListSources,
    listAdapters: realListAdapters,
    search: realSearch,
    ...ports,
  };

  let protocol = $state<Protocol | null>(null);
  let body = $state("");
  let failure = $state<string | null>(null);
  let loaded = $state(false);
  let publishing = $state(false);

  /** The target dialog, open with the sources it found. */
  let dialog = $state<{
    sources: { id: string; name: string }[];
    sourceId: string | null;
    query: string;
    hits: SearchHit[];
    parent: string | null;
  } | null>(null);

  /** The create-ticket dialog, open on one action item. */
  let ticketFor = $state<{ text: string; project: string | null; type: string } | null>(null);
  let projects = $state<Project[]>([]);

  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    void open(day);
  });

  async function open(on: string) {
    try {
      const opened = await io.standupProtocol(on);
      const note = await io.getNote(opened.note_id);
      protocol = opened;
      body = note.note.body_md;
      failure = null;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      loaded = true;
    }
  }

  /**
   * Autosave. The text stays in the textarea whatever happens, so a failed
   * save costs a round trip and never a sentence — `NoteView`'s promise.
   */
  function typed(text: string) {
    body = text;
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => void save(), saveAfterMs);
  }

  async function save() {
    const open = protocol;
    if (!open) return;
    try {
      // The title is the protocol's identity — it is what the get-or-create
      // matches on — so it is never rewritten from here. Only the body is the
      // reader's to change.
      const note = await io.getNote(open.note_id);
      await io.saveNote(open.note_id, note.note.title, body);
      failure = null;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  /**
   * *Publish to Confluence*. Asks for the target the first time and never
   * again — the stored one is read first, and only its absence opens a dialog.
   */
  async function publish() {
    const stored = await io.standupPublishTarget().catch(() => null);
    if (stored) {
      await send(undefined);
      return;
    }
    const [sources, descriptors] = await Promise.all([
      io.listSources(),
      io.listAdapters(),
    ]).catch(() => [[], []] as const);
    const offered = publishableSources(sources, descriptors);
    dialog = {
      sources: offered,
      sourceId: presumedSource(offered),
      query: "",
      hits: [],
      parent: null,
    };
  }

  async function send(target: PublishTarget | undefined) {
    publishing = true;
    try {
      // Save first: publishing what is on screen rather than what the last
      // pause happened to have written is the only reading that is not a
      // surprise.
      await save();
      protocol = await io.publishStandupProtocol(day, target);
      dialog = null;
      failure = null;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      publishing = false;
    }
  }

  /** Pages of the chosen source, for the parent picker. */
  async function findPages(text: string) {
    const open = dialog;
    if (!open || open.sourceId === null) return;
    open.query = text;
    if (text.trim() === "") {
      open.hits = [];
      return;
    }
    try {
      // The kinds filter narrows the corpus at the backend; the source is
      // narrowed here, because a page's source is on the hit and the launcher's
      // own filter vocabulary is about what the *reader* typed.
      const found = await io.search({
        raw: text,
        limit: 20,
        filters: {
          sources: [],
          kinds: ["page"],
          updated_within_days: null,
          mine: false,
          authors: [],
        },
      });
      open.hits = found.groups
        .flatMap((group) => group.hits)
        .filter((hit) => hit.kind === "page" && hit.source_id === open.sourceId);
    } catch {
      open.hits = [];
    }
  }

  async function openTicketDialog(text: string) {
    ticketFor = { text, project: null, type: "Task" };
    try {
      projects = await io.listProjects();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  async function fileTicket() {
    const asked = ticketFor;
    const open = protocol;
    if (!asked || !open || asked.project === null) return;
    try {
      const filed = await io.createActionItemTicket(
        open.note_id,
        asked.project,
        asked.type,
        asked.text,
        `From the standup protocol of ${day}.`,
      );
      ticketFor = null;
      push({
        text:
          filed.ticket_entity_id === null
            ? "The ticket is on the write queue; it will be linked when it arrives."
            : `${filed.ticket_entity_id} filed and linked to the protocol.`,
      });
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  const items = $derived(actionItems(body));
  const publication = $derived(protocol?.publication ?? null);

  function addressOf(entityId: string): string {
    return hashFor({
      view: "room",
      ctx: router.ctx,
      detail: { kind: null, entityId },
    });
  }
</script>

<section class="pr">
  <div class="pr-h">
    <h2>Protocol</h2>
    {#if publication === null}
      <button
        class="btn"
        disabled={!loaded || publishing || protocol === null}
        onclick={() => void publish()}
      >
        Publish to Confluence
      </button>
    {:else if publication.page_entity_id !== null}
      <!--
        Published, and the page has an address. One page per date: the button
        is gone rather than disabled, because there is nothing left to press.
      -->
      <button class="link" onclick={() => router.go(addressOf(publication.page_entity_id!))}>
        Published as {protocol?.page_title}
      </button>
    {:else}
      <span class="wait">Publishing {protocol?.page_title} — the write is on the queue.</span>
    {/if}
  </div>

  {#if failure}
    <p class="fail">{failure}</p>
  {/if}

  {#if publication !== null && publication.page_entity_id !== null && !publication.linked}
    <!--
      The page exists and the mirror has not caught up. Said out loud rather
      than hidden: the link appears on its own at the next read, and a panel
      that showed nothing would look like a link that failed.
    -->
    <p class="sub">The page is made; its link appears once the next sync has read it.</p>
  {/if}
  {#if publication !== null && publication.detail !== null}
    <p class="sub">{publication.detail}</p>
  {/if}

  <textarea
    class="pr-b"
    value={body}
    disabled={!loaded}
    aria-label="Standup protocol"
    oninput={(event) => typed(event.currentTarget.value)}
  ></textarea>

  {#if items.length > 0}
    <div class="pr-a">
      <h3>Action items</h3>
      <ul>
        {#each items as item, index (index)}
          <li>
            <span class:done={item.done}>{item.text}</span>
            <button class="btn sm" onclick={() => void openTicketDialog(item.text)}>
              Create ticket
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

{#if dialog}
  <div class="dlg">
    <h3>Where do standup protocols go?</h3>
    {#if dialog.sources.length === 0}
      <p class="sub">
        No Confluence source is configured, so there is nowhere to publish yet.
      </p>
    {:else}
      <label>
        Confluence
        <select
          aria-label="Confluence to publish into"
          value={dialog.sourceId ?? ""}
          onchange={(event) => {
            dialog!.sourceId = event.currentTarget.value || null;
            dialog!.parent = null;
            dialog!.hits = [];
          }}
        >
          <!--
            An empty first option, and it is selected when there is more than
            one source: story 68 asks to be *asked*, and a preselected instance
            is an answer nobody gave.
          -->
          <option value="">Choose…</option>
          {#each dialog.sources as source (source.id)}
            <option value={source.id}>{source.name}</option>
          {/each}
        </select>
      </label>

      <label>
        Parent page
        <input
          type="search"
          aria-label="Parent page"
          placeholder="Standup protocols"
          disabled={dialog.sourceId === null}
          value={dialog.query}
          oninput={(event) => void findPages(event.currentTarget.value)}
        />
      </label>
      <ul class="hits">
        {#each dialog.hits as hit (hit.entity_id)}
          <li>
            <button
              class="link"
              class:picked={dialog.parent === hit.entity_id}
              onclick={() => (dialog!.parent = hit.entity_id)}
            >
              {hit.title}
              {#if hit.path}<span class="sub">{hit.path}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="dlg-f">
      <button class="btn" onclick={() => (dialog = null)}>Cancel</button>
      <button
        class="btn"
        disabled={dialog.sourceId === null || dialog.parent === null || publishing}
        onclick={() =>
          void send({ source_id: dialog!.sourceId!, parent: dialog!.parent! })}
      >
        Publish
      </button>
    </div>
  </div>
{/if}

{#if ticketFor}
  <div class="dlg">
    <h3>File a ticket</h3>
    <p class="sub">{ticketFor.text}</p>
    <label>
      Project
      <select
        aria-label="Project the ticket goes in"
        value={ticketFor.project ?? ""}
        onchange={(event) => (ticketFor!.project = event.currentTarget.value || null)}
      >
        <option value="">Choose…</option>
        {#each projects as project (`${project.source_id}:${project.key}`)}
          <option value={`${project.source_id}:${project.key}`}>
            {project.name ?? project.key}
          </option>
        {/each}
      </select>
    </label>
    <label>
      Type
      <input
        type="text"
        aria-label="Ticket type"
        value={ticketFor.type}
        oninput={(event) => (ticketFor!.type = event.currentTarget.value)}
      />
    </label>
    <div class="dlg-f">
      <button class="btn" onclick={() => (ticketFor = null)}>Cancel</button>
      <button
        class="btn"
        disabled={ticketFor.project === null || ticketFor.type.trim() === ""}
        onclick={() => void fileTicket()}
      >
        Create
      </button>
    </div>
  </div>
{/if}

<style>
  .pr {
    margin-top: 18px;
    border-top: 1px solid var(--hair);
    padding-top: 12px;
  }

  .pr-h {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 6px;
  }

  .pr h2 {
    margin: 0;
    font-family: var(--disp);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .pr-b {
    width: 100%;
    min-height: 220px;
    padding: 8px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
    color: var(--text);
    font: inherit;
    resize: vertical;
  }

  .pr-a {
    margin-top: 12px;
  }

  .pr-a h3 {
    margin: 0 0 4px;
    font-size: 12px;
    color: var(--muted);
  }

  .pr-a ul {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pr-a li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    padding: 4px 8px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
  }

  .done {
    color: var(--muted);
    text-decoration: line-through;
  }

  .dlg {
    display: grid;
    gap: 8px;
    margin-top: 12px;
    padding: 12px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
  }

  .dlg h3 {
    margin: 0;
    font-size: 13px;
  }

  .dlg label {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }

  .dlg-f {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .hits {
    display: grid;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  button.link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    text-align: left;
    color: var(--link);
    cursor: pointer;
  }

  button.link.picked {
    text-decoration: underline;
  }

  .sub {
    color: var(--muted);
    font-size: 12px;
  }

  .wait {
    color: var(--muted);
    font-size: 12px;
  }

  .fail {
    color: var(--fail);
    font-size: 12px;
  }
</style>
