<!--
  *Create ticket* in a failed run's problem block (#1451, R-9.11), after
  *Open the test file*: the shell's one create-ticket dialog
  (`TfCreateTicket`), prefilled from the failing test, the assertion and the
  run's address (`ticketDraftOf`), with a project picker over
  `list_projects`. Filing queues a `CreateTicket` through `submit_write`
  with the run as its `link_to`, so the ticket is linked to the run once a
  sync of its source brings it in (#984's arrival link).

  The run's tickets are its confirmed links (`get_entity`'s `links`), read
  with the page and again on each of its re-reads; while a ticket filed here
  is on its way the button gives way to its echo (`tf/queued.svelte.ts`,
  kind `ticket`, #2104): *Ticket queued*, then *Ticket sent* once the create
  landed. The refresh event naming the create (the ticket's arrival links it,
  #2095), or a sync of any source ending, reads the links again until one
  holds a ticket of the queued title. A run with a ticket
  linked draws the ticket, and no *Create ticket*.

  The echo dies with a reload; the create's row does not (#1953). So
  each read of the links also asks `owed_tickets`, the tickets filed from the
  run that it does not link yet, and one still on its way (`pending`, `sent`,
  or held as unconfirmed: sent and never answered, so the ticket may exist,
  #1966) is marked queued as if filed this session, followed while the queue
  still holds it. A refused one, or one held on drift, offers *Create ticket*,
  as it does once a live follow hears so: it waits on the reader in the
  queue. One held as unconfirmed stays queued until the reader sends it
  again or discards it.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { LinkEntry, Project } from "../../ipc/entity";
  import type { QueuedWrite, SourceSummary } from "../../ipc/sources";
  import { mayHaveLanded } from "../../shell/follow-write";
  import { toastQueued } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { createTicketPayload, draftOfPayload, projectSource, type TicketDraft } from "../create-ticket";
  import { itemHash, keyOf } from "../job/job";
  import { askAgain, echoWrite, echoOn, echoesOn, sameWords, settleEchoes, takeAgain, watchEchoes } from "../queued.svelte";
  import TfCreateTicket from "../TfCreateTicket.svelte";
  import type { RunPorts } from "./pipeline-ports";
  import { linkedTickets } from "./run";

  let {
    runId,
    href,
    draft,
    sources,
    ports,
    rereads = 0,
  }: {
    runId: string;
    /** The run's address, where *Edit and resend* goes back to. */
    href: string;
    /** What the dialog opens with. */
    draft: TicketDraft;
    /** The configured sources, to name the ticket's in the toast. */
    sources: readonly SourceSummary[];
    ports: Pick<RunPorts, "projects" | "links" | "owedTickets" | "submitWrite" | "listen" | "follower" | "refusedWrites">;
    /** Bumped when the page reads the run again (#1450): the links are read again. */
    rereads?: number;
  } = $props();

  /** The kind of the echo on the run. */
  const TICKET = "ticket";
  /** The surface its echo is tagged with, for `watchEchoes` (#2104). */
  const RUN_TICKET = "run-ticket";

  let links = $state<LinkEntry[]>([]);
  let asking = $state<TicketDraft | null>(null);
  let projects = $state<Project[]>([]);

  const tickets = $derived(linkedTickets(links));
  const queued = $derived(echoOn(runId, TICKET));

  /**
   * The owed tickets first, then the links: the owed read links a ticket the
   * mirror holds before it answers, so the links read after it draws that
   * ticket rather than offering *Create ticket* beside nothing queued.
   */
  function readLinks(id: string) {
    ports
      .owedTickets(id)
      .then(
        (owed) => {
          if (id === runId) markOwed(id, owed);
        },
        () => {},
      )
      .then(() => ports.links(id))
      .then(
        (answer) => {
          if (id !== runId) return;
          links = answer;
          const held = linkedTickets(answer);
          settleEchoes(id, (mark) => mark.kind === TICKET && held.some((ticket) => sameWords(ticket.title, mark.text)));
        },
        () => {},
      );
  }

  /** The newest ticket filed from the run before this page opened and still on its way, marked queued (#1953). */
  function markOwed(id: string, owed: readonly QueuedWrite[]) {
    const going = owed.filter((write) => write.state === "pending" || write.state === "sent" || mayHaveLanded(write));
    const marked = echoesOn(id, TICKET);
    if (going.some((write) => marked.some((mark) => mark.write === write.id))) return;
    const newest = going.at(-1);
    if (newest === undefined) return;
    const filed = draftOfPayload(newest.payload);
    if (filed === null) return;
    // A sent one is past where the queue could hold or refuse it: nothing to follow.
    const follow = newest.state !== "sent" ? { following: following(id, filed), follower: ports.follower() } : {};
    echoWrite(id, TICKET, filed.title, { write: newest, ...follow, surface: RUN_TICKET, source: projectSource(filed.project) });
  }

  /** What a held or refused create says, and *Edit and resend* back to the run with what was filed (not offered for an unconfirmed send, #1966). */
  function following(id: string, filed: TicketDraft) {
    const back = href;
    return {
      name: "Ticket",
      resend: () => {
        askAgain(id, TICKET, filed);
        location.hash = back;
      },
    };
  }

  $effect(() => {
    const wanted = runId;
    void rereads;
    untrack(() => readLinks(wanted));
  });

  // While a ticket filed here is on its way, the refresh naming it, or the
  // sync that brings it in, which may be any source's, reads the links again.
  $effect(() => {
    if (queued === null) return;
    const wanted = runId;
    return untrack(() => watchEchoes(RUN_TICKET, () => readLinks(wanted), { listen: ports.listen, everyRun: true }));
  });

  // *Edit and resend*: the dialog again, with what was filed.
  $effect(() => {
    const again = takeAgain<TicketDraft>(runId, TICKET);
    if (again !== null) untrack(() => void open(again));
  });

  async function open(from: TicketDraft) {
    asking = from;
    try {
      projects = await ports.projects();
    } catch (cause) {
      push({ text: `Could not read the projects: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function file(filed: TicketDraft & { project: string }) {
    asking = null;
    const id = runId;
    const title = filed.title.trim();
    const source = projectSource(filed.project);
    try {
      const write = await ports.submitWrite(createTicketPayload(filed), id);
      // An owed read that answered before this did has marked and followed it already: one follow, one toast.
      if (!echoesOn(id, TICKET).some((mark) => mark.write === write.id)) {
        echoWrite(id, TICKET, title, { write, following: following(id, filed), follower: ports.follower(), surface: RUN_TICKET, source });
      }
      const summary = sources.find((s) => s.id === source);
      toastQueued(`ticket ${title}`, { id: source, display_name: summary?.display_name ?? source });
    } catch (cause) {
      push({ text: `Could not queue the ticket: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

{#each tickets as ticket (ticket.entity_id)}
  <a class="tf-btn tf-sm tf-ghost" href={itemHash(ticket)} title={ticket.title}
    >Ticket <span class="tf-mono">{keyOf(ticket.entity_id)}</span></a
  >
{:else}
  {#if queued !== null}
    <span class="tf-chip tf-info" title={queued.text}>Ticket {queued.state}</span>
  {:else}
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void open(draft)}>Create ticket</button>
  {/if}
{/each}

{#if asking !== null}
  <TfCreateTicket
    draft={asking}
    {projects}
    words
    refusedWrites={ports.refusedWrites}
    onclose={() => (asking = null)}
    onfile={(filed) => void file(filed)}
  />
{/if}
