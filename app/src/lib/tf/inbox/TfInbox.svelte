<!--
  The Inbox (#834, R-12.1 to R-12.3): `#/inbox`.

  The filter bar (text, *This job / All*, the kind chips), then a row per item
  newest first: the time, the item as one sentence (an err dot for a failure),
  its actions, and a second line with its kind and the job it belongs to as a
  link. The snoozed shelf is one line under the list, with return dates.
  A long inbox draws only the rows near the viewport (`shell/windowed.svelte.ts`, #2209).

  Over the inbox store the rooms UI already uses (`inbox/inbox.svelte.ts`:
  `inbox_items`, `inbox_count`, `snooze_inbox_item`, `complete_inbox_item`,
  `ack_alert`), `submit_write` for *Re-run*, and `job_reach` for which job a
  row belongs to. `refused_writes` (#2040) says which *Re-run* the source's
  credential cannot make: that one is drawn disabled with the reason.
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { onDestroy, onMount } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    getEntity,
    jobReach,
    listContexts,
    submitWrite,
    type ContextRow,
    type EntityDetail,
    type InboxEntry,
    type Reached,
  } from "../../ipc/entity";
  import { listSources, type QueuedWrite, type WriteOpPayload } from "../../ipc/sources";
  import { ACTION_FORMS } from "../../inbox/actions";
  import { inbox as sharedInbox, pickedMorning, snoozePresets, tomorrowValue, type Inbox } from "../../inbox/inbox.svelte";
  import { followWrite } from "../../shell/follow-write";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { SourceFacts } from "../job/job";
  import { jobHash } from "../route";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import { expiryOf, failed, KIND_CHIPS, kindOf, opener, rowActions, sentence, snoozedLine, untilLabel, visible } from "./inbox";

  /** The reads and the write this view makes beside the inbox store. */
  export interface InboxPorts {
    listContexts: () => Promise<ContextRow[]>;
    listSources: () => Promise<SourceFacts[]>;
    getEntity: (entityId: string) => Promise<EntityDetail>;
    jobReach: (entityId: string) => Promise<Reached[]>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** What the sources' credentials cannot write (#2040); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort;
  }

  let {
    ready,
    job,
    inbox = sharedInbox,
    ports = {
      listContexts: () => listContexts(),
      listSources: () => listSources(),
      getEntity: (entityId) => getEntity(entityId),
      jobReach: (entityId) => jobReach(entityId),
      submitWrite: (payload) => submitWrite(payload),
    },
    now: fixedNow,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The job the reader was last in, as a ticket key: what *This job* keeps. */
    job: string | null;
    /** The live store. A prop so a test needs no Tauri bridge. */
    inbox?: Inbox;
    ports?: InboxPorts;
    /** Injectable clock, so the time column and the presets are testable. */
    now?: Date;
  } = $props();

  /** Moved every thirty seconds, as the Day and Week tabs do, so a row says *yesterday* and *Tomorrow* means tomorrow once midnight passes. */
  // svelte-ignore state_referenced_locally
  let clock = $state(new Date());
  onMount(() => {
    if (fixedNow) return;
    const tick = setInterval(() => (clock = new Date()), 30_000);
    return () => clearInterval(tick);
  });
  const now = $derived(fixedNow ?? clock);

  const filter = sessionFilter("inbox");
  // Prototype-6 opens the Inbox on *All*.
  if (filter.scope === null) filter.scope = "all";

  const config = $derived<FilterConfig>({
    id: "inbox",
    placeholder: "Filter by text or ticket key",
    noun: "items",
    one: "item",
    scopes: [
      ...(job !== null ? [{ id: "job", label: "This job", tag: job, query: "" }] : []),
      { id: "all", label: "All", query: "" },
    ],
    chips: [{ key: "kind", label: "Kind", options: KIND_CHIPS.map((value) => ({ value, label: value })) }],
  });

  let jobs = $state<OpenJob[]>([]);
  const index = $derived(jobIndex(jobs));
  const scope = $derived<"job" | "all">(job !== null && filter.scope === "job" ? "job" : "all");

  const rows = $derived(
    visible(inbox.stream, { scope, job, kinds: filter.chips["kind"] ?? [], text: filter.text }, index),
  );
  const shelf = $derived(snoozedLine(inbox.snoozed));
  /** Thousands of rows draw only those near the viewport (#2209); `.tf-ib` is 63 px, 84 wrapped, 6 apart. */
  const win = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.item.key, estimate: 63, gap: 6 });

  $effect(() => {
    if (!ready) return;
    const current = job;
    // An answer that lands after the view is gone, or after the job changed, draws into nothing.
    let live = true;
    void readOpenJobs(ports, current).then((read) => {
      if (live) jobs = read;
    });
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (ready) void inbox.refresh();
  });

  /** The row writes the sources' credentials cannot make, asked after the rows are drawn (#2040). */
  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so a refresh that changes no row's writes asks nothing again. */
  const asked = $derived(
    JSON.stringify(
      inbox.stream.flatMap((entry) =>
        entry.item.entity_id === null
          ? []
          : rowActions(entry)
              .filter((action) => entry.actions.includes(action))
              .map((op) => ({ entity: entry.item.entity_id as string, op })),
      ),
    ),
  );
  $effect(() => askInto(JSON.parse(asked) as WriteAsk[], ports.refusedWrites, (answer) => (refused = answer)));
  const refusal = (entry: InboxEntry, action: string) => refusalOf(refused, entry.item.entity_id, action);

  $effect(() => {
    if (job === null && filter.scope === "job") filter.scope = "all";
  });

  /** The row whose snooze choices are open, if any. */
  let snoozing = $state<string | null>(null);

  /** The writes this view follows, stopped when it goes. */
  const following: (() => void)[] = [];
  onDestroy(() => {
    for (const stop of following.splice(0)) stop();
  });
  /** Re-runs whose `submit_write` has not answered: a double click is one press. */
  const queueing = new Set<string>();

  async function rerun(entry: InboxEntry) {
    const entity = entry.item.entity_id;
    const form = ACTION_FORMS["rerun_build"];
    if (entity === null || !form || form.kind !== "immediate" || queueing.has(entry.item.key)) return;
    queueing.add(entry.item.key);
    try {
      const write = await ports.submitWrite(form.build(entity));
      // Named for the toast's sign-in sentence; a list that fails still lets the toast say what was queued.
      const sources = await ports.listSources().catch(() => [] as SourceFacts[]);
      const source = sources.find((row) => row.id === entry.item.source_id);
      toastQueued(`${form.label.toLowerCase()} ${entry.item.title}`, {
        id: entry.item.source_id,
        display_name: source?.display_name ?? "",
      });
      following.push(followWrite(write, { name: form.label }));
      void inbox.refresh();
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      queueing.delete(entry.item.key);
    }
  }

  function ack(entry: InboxEntry) {
    push({ text: "Acked. The alert stays open until the monitor recovers." });
    void inbox.ack(entry.item.key);
  }

  function snooze(entry: InboxEntry, at: Date) {
    snoozing = null;
    push({ text: `Snoozed until ${untilLabel(at, now)}` });
    void inbox.snooze(entry.item.key, at);
  }

  /** A date the picker produced, at 09:00 local, as the presets are. */
  function fromPicker(entry: InboxEntry) {
    const at = pickedMorning(pickedValue, now);
    if (at !== null) snooze(entry, at);
  }

  /** What the open row's date field holds; only a press or Enter commits it (#1437). */
  let pickedValue = $state("");

  /** Each button's word: a source op's is its form's (`ACTION_FORMS`), knobas' own are here. */
  const LABEL: Record<string, string> = { ack: "Ack", snooze: "Snooze", done: "Done" };
  const labelOf = (action: string) => ACTION_FORMS[action]?.label ?? LABEL[action] ?? action;

  function press(entry: InboxEntry, action: string) {
    if (refusal(entry, action) !== null) return;
    if (action === "rerun_build") void rerun(entry);
    else if (action === "ack") ack(entry);
    else if (action === "snooze") {
      pickedValue = "";
      snoozing = snoozing === entry.item.key ? null : entry.item.key;
    }
    else if (action === "done") void inbox.complete(entry.item.key);
  }
</script>

<header class="tf-fhead">
  <div>
    <h1>Inbox</h1>
    <div class="tf-sub">What asks for you, newest first. Each row says which job it belongs to.</div>
  </div>
</header>
<FilterBar {config} {filter} shown={inbox.answered ? rows.length : null} total={inbox.answered ? inbox.stream.length : null} />
<div class="tf-ibody">
  {#if inbox.error !== null}
    <p class="tf-ifail">{inbox.error}</p>
  {/if}
  <div
    class="tf-inbox"
    {@attach win.list}
    style:padding-top={win.before > 0 ? `${win.before}px` : null}
    style:padding-bottom={win.after > 0 ? `${win.after}px` : null}
  >
    {#each win.slice(rows) as entry (entry.item.key)}
      {@const item = entry.item}
      {@const open = opener(item)}
      {@const belongs = index.jobOf(item)}
      {@const belongsId = index.jobIdOf(item)}
      <div class="tf-ib" {@attach win.row(item.key)}>
        <time datetime={item.occurred_at}>{clockLabel(item.occurred_at, now, true)}</time>
        <div class="tf-what">
          {#if failed(item)}<span class="tf-dot tf-err"></span>{" "}{/if}<Marked text={sentence(item)} typed={filter.text} />
        </div>
        <div class="tf-acts">
          {#if open !== null}
            <a class="tf-btn tf-sm" href={open.href}>{open.label}</a>
          {/if}
          {#each rowActions(entry) as action (action)}
            <button
              type="button"
              class="tf-btn tf-sm"
              class:tf-ghost={action === "done"}
              {...refusedAttrs(refusal(entry, action))}
              aria-expanded={action === "snooze" ? snoozing === item.key : undefined}
              onclick={() => press(entry, action)}>{labelOf(action)}</button
            >
          {/each}
        </div>
        <div class="tf-ctx">
          {kindOf(item.category)}{#if belongs !== null}, job <a href={jobHash(belongsId ?? belongs)} class="tf-mono"
              >{belongs}</a
            >{:else}, no job{/if}
        </div>
        {#if snoozing === item.key}
          <div class="tf-isnooze" role="group" aria-label="Snooze until">
            {#each snoozePresets(now, expiryOf(item)) as preset (preset.label)}
              <button type="button" class="tf-btn tf-sm" onclick={() => snooze(entry, preset.at)}>{preset.label}</button>
            {/each}
            <input
              type="date"
              aria-label="Snooze until a date"
              min={tomorrowValue(now)}
              bind:value={pickedValue}
              onkeydown={(event) => {
                if (event.key === "Enter") fromPicker(entry);
              }}
            />
            <button
              type="button"
              class="tf-btn tf-sm"
              disabled={pickedMorning(pickedValue, now) === null}
              onclick={() => fromPicker(entry)}>Snooze</button
            >
            {#if pickedValue !== "" && pickedMorning(pickedValue, now) === null}
              <span class="tf-dim">Pick a day after today</span>
            {/if}
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (snoozing = null)}>Cancel</button>
          </div>
        {/if}
      </div>
    {:else}
      {#if inbox.answered}
        <div class="tf-empty">Nothing here. New items appear as sources sync.</div>
      {/if}
    {/each}
  </div>
  {#if shelf !== null}
    <p class="tf-dim tf-ishelf">{shelf}</p>
  {/if}
</div>
