<!--
  The ticket page (spec #2227): `#/ticket/<entity id>`, one ticket to read
  before taking it on. It reads the ticket's detail, the sources (whose
  usernames say whether it is mine) and *Around this ticket* through the same
  section tally the Tickets preview reads (`reachTally`), and draws them with
  the preview's own component on the ticket-yellow card.

  It never promotes the ticket's context and never chooses the job: the one
  primary button goes to the job's address, and the job page does both.

  Its breadcrumb is its own, `Tickets › <key>`, as only its read knows the
  key a source gave. A ticket the mirror cannot read draws the job page's
  treatment of an unknown key.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { EntityDetail } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { keyOf } from "../../time/format";
  import type { EscTarget } from "../escape";
  import { isMine, type SourceFacts } from "../job/job";
  import type { JobPorts } from "../job/ports";
  import { reachTally } from "../job/sections";
  import { sectionHash } from "../route";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TicketPreview from "../tickets/TicketPreview.svelte";
  import { aroundLines, ticketFacts, type AroundLine } from "../tickets/tickets";

  let {
    id,
    ready,
    back = null,
    timedTicket = null,
    ports,
  }: {
    /** The ticket's entity id, as the address names it. */
    id: string;
    ready: boolean;
    /** Where `Esc` goes, which the breadcrumb's hint names (#2228). */
    back?: EscTarget | null;
    /** The ticket key the timer runs on, or `null`. */
    timedTicket?: string | null;
    ports: Pick<JobPorts, "getEntity" | "listSources" | "listEntities" | "jobReach">;
  } = $props();

  /** `undefined` while it is read, `null` when the mirror holds no such ticket. */
  let detail = $state<EntityDetail | null | undefined>(undefined);
  let failure = $state<string | null>(null);
  let sources = $state<SourceFacts[]>([]);
  let around = $state<AroundLine[] | null>(null);

  // Once per ticket, once the database answers. A read the next id overtakes lands nothing.
  $effect(() => {
    if (!ready) return;
    const asked = id;
    let dead = false;
    untrack(() => {
      detail = undefined;
      failure = null;
      around = null;
      ports
        .getEntity(asked)
        .then((found) => {
          if (!dead) detail = found.row.kind === "ticket" ? found : null;
        })
        .catch((error: unknown) => {
          if (dead) return;
          if ((error as { code?: string } | null)?.code === "not_found") detail = null;
          else failure = ipcErrorMessage(error);
        });
      ports
        .listSources()
        .then((rows) => {
          if (!dead) sources = rows;
        })
        .catch(() => {});
      void reachTally(ports, asked).then((tally) => {
        if (!dead) around = aroundLines(tally);
      });
    });
    return () => {
      dead = true;
    };
  });

  const key = $derived(detail ? keyOf(detail.row.entity_id) : keyOf(id));
  const crumbs = $derived([{ name: "Tickets", href: sectionHash("tickets") }, { name: key }]);
</script>

<div class="tf-full tf-ticketpage">
  <TfCrumbs parts={crumbs} {back} />
  {#if failure !== null}
    <div class="tf-empty">
      <p>Could not open {key}: {failure}</p>
      <a class="tf-btn" href={sectionHash("tickets")}>All tickets</a>
    </div>
  {:else if detail === null}
    <div class="tf-empty">
      <p>No ticket {key} in the mirror. It may not have synced yet.</p>
      <a class="tf-btn" href={sectionHash("tickets")}>All tickets</a>
    </div>
  {:else if detail === undefined}
    <div class="tf-empty"><p>Opening {key}…</p></div>
  {:else}
    <TicketPreview
      page
      card={ticketFacts(detail, isMine(detail, sources))}
      description={detail.body_text}
      {around}
      timerOnIt={timedTicket === key}
    />
  {/if}
</div>
