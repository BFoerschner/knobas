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
  is queued the button gives way to *Ticket queued* (the shared queued
  module, kind `ticket`), and a sync of any source ending reads the links
  again until one holds a ticket of the queued title. A run with a ticket
  linked draws the ticket, and no *Create ticket*.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { LinkEntry, Project } from "../../ipc/entity";
  import type { SourceSummary } from "../../ipc/sources";
  import { toastQueued } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { createTicketPayload, projectSource, type TicketDraft } from "../create-ticket";
  import { itemHash, keyOf } from "../job/job";
  import { askAgain, markQueued, queuedMark, sameWords, settleQueued, takeAgain } from "../queued.svelte";
  import TfCreateTicket from "../TfCreateTicket.svelte";
  import { listenSynced, type RunPorts } from "./pipeline-ports";
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
    ports: Pick<RunPorts, "projects" | "links" | "submitWrite" | "listen" | "follower">;
    /** Bumped when the page reads the run again (#1450): the links are read again. */
    rereads?: number;
  } = $props();

  /** The kind of the queued mark on the run. */
  const TICKET = "ticket";

  let links = $state<LinkEntry[]>([]);
  let asking = $state<TicketDraft | null>(null);
  let projects = $state<Project[]>([]);

  const tickets = $derived(linkedTickets(links));
  const queued = $derived(queuedMark(runId, TICKET));

  function readLinks(id: string) {
    ports.links(id).then(
      (answer) => {
        if (id !== runId) return;
        links = answer;
        const held = linkedTickets(answer);
        settleQueued(id, (mark) => mark.kind === TICKET && held.some((ticket) => sameWords(ticket.title, mark.text)));
      },
      () => {},
    );
  }

  $effect(() => {
    const wanted = runId;
    void rereads;
    untrack(() => readLinks(wanted));
  });

  // While a ticket filed here is queued, the sync that brings it in may be any source's.
  $effect(() => {
    if (queued === null) return;
    const wanted = runId;
    return listenSynced(ports, null, () => {}, () => readLinks(wanted));
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
    const back = href;
    const title = filed.title.trim();
    const source = projectSource(filed.project);
    try {
      const write = await ports.submitWrite(createTicketPayload(filed), id);
      markQueued(id, TICKET, title, {
        write,
        following: {
          name: "Ticket",
          resend: () => {
            askAgain(id, TICKET, filed);
            location.hash = back;
          },
        },
        follower: ports.follower(),
      });
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
    <span class="tf-chip tf-info" title={queued.text}>Ticket queued</span>
  {:else}
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void open(draft)}>Create ticket</button>
  {/if}
{/each}

{#if asking !== null}
  <TfCreateTicket draft={asking} {projects} words onclose={() => (asking = null)} onfile={(filed) => void file(filed)} />
{/if}
