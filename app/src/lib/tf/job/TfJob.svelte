<!--
  The job (#816, R-4.1): `#/job/<key>`, a ticket's working view.

  Opening it reads the ticket, then **promotes its context** (`promote_context`,
  idempotent, so a ticket with a context keeps it and one without gets one) and
  reads the Code section's rows inside that context. The promotion happens
  once per ticket opened: re-reads after a status move do not ask again.

  The layout is the prototype's: the yellow card in a fixed 320px column, the
  work on the right with the tabs *Work*, *Map* and *Conversation*. Work holds
  the Code section; the other four sections are #817's, the Map tab TF5's and
  the Conversation tab #819's, each an empty pane that names itself until then.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import type { CheckoutCommand, CheckoutView, EntityDetail, EntityRow } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import type { Timer } from "../../shell/timer.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { dayKey } from "../../time/day";
  import { weekWindows } from "../../time/week";
  import type { Layers } from "../layers.svelte";
  import { CODE_KINDS, byAttention, isEntityId, keyOf, resolveTicket, weekReading, type WeekReading } from "./job";
  import { bridgePorts, type JobPorts } from "./ports";
  import TfCodeSection from "./TfCodeSection.svelte";
  import TfJobCard from "./TfJobCard.svelte";

  let {
    segment,
    tab,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports = bridgePorts,
    today = () => new Date(),
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
  } = $props();

  /** `undefined` while reading, `null` when nothing answers to the address. */
  let detail = $state<EntityDetail | null | undefined>(undefined);
  let offered = $state<string[] | null>(null);
  let week = $state<WeekReading | null>(null);
  let code = $state<EntityRow[] | null>(null);
  let checkout = $state<CheckoutView | null>(null);
  let checkoutOf = $state<string | null>(null);
  let commands = $state<CheckoutCommand[]>([]);
  let failure = $state<string | null>(null);

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
    // Only the address re-opens a job: what `open` reads before its first
    // await (the ports) is not a reason to read again.
    untrack(() => void open(wanted, mine));
  });

  async function open(wanted: string, mine: number) {
    try {
      const sources = isEntityId(wanted) ? [] : await ports.listSources();
      const found = await resolveTicket(wanted, sources, ports.getEntity);
      if (mine !== token) return;
      detail = found;
      if (found === null) return;
      const id = found.row.entity_id;
      void readWorkflow(id, mine);
      void readWeek(id, mine);
      const context = await ports.promoteContext(id);
      if (mine !== token) return;
      await readCode(context.id, mine);
    } catch (cause) {
      if (mine === token) failure = ipcErrorMessage(cause);
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

  /** How many rows the section reads: the membership is one hop, so this holds it. */
  const CODE_LIMIT = 200;
  /** How many branches and repositories are asked for a clone before giving up. */
  const CHECKOUT_TRIES = 3;

  async function readCode(context: string, mine: number) {
    const page = await ports.listEntities(
      {
        sources: [],
        kinds: [...CODE_KINDS],
        updated_within_days: null,
        order: "updated_desc",
        context,
        project: null,
        include_deleted: false,
      },
      CODE_LIMIT,
      0,
    );
    if (mine !== token) return;
    code = page.rows;
    const sorted = byAttention(page.rows);
    const candidates = [
      ...sorted.filter((row) => row.kind === "branch"),
      ...sorted.filter((row) => row.kind === "repo"),
    ].slice(0, CHECKOUT_TRIES);
    for (const row of candidates) {
      try {
        const view = await ports.entityCheckout(row.entity_id);
        if (mine !== token) return;
        if (view.path) {
          checkout = view;
          checkoutOf = row.entity_id;
          break;
        }
      } catch {
        // A checkout that cannot be read is a checkout not shown.
      }
    }
    if (checkout !== null && commands.length === 0) {
      commands = await ports.checkoutCommands().catch(() => []);
    }
  }

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

  onMount(() => () => {
    token += 1;
  });
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
      {timer}
      onmove={move}
      onstop={() => ontimer?.()}
      onstart={() => onstarttimer(detail!.row.entity_id, detail!.row.title)}
    />
    <section class="tf-work">
      <div class="tf-wtabs" role="tablist">
        {#each TABS as each (each.id)}
          <a href="#/job/{encodeURIComponent(key)}{each.path}" role="tab" aria-selected={current.id === each.id}>{each.name}</a>
        {/each}
      </div>
      <div class="tf-wbody">
        {#if current.id === "work"}
          {#if failure !== null}
            <p class="tf-dim">Could not read what this job holds: {failure}</p>
          {/if}
          <div class="tf-grid">
            <TfCodeSection rows={code} {checkout} {checkoutOf} {commands} onopen={ports.openCheckout} {layers} />
          </div>
        {:else}
          <div class="tf-empty">
            <p>{current.name} is not built in the new shell yet.</p>
          </div>
        {/if}
      </div>
    </section>
  </div>
{/if}
