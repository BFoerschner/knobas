<!--
  The Inbox (#834, R-12.1 to R-12.3): `#/inbox`.

  The filter bar (*This job / All*, the kind chips; its search is the launcher's `#i`, #2425), then a row per item
  newest first: the time, the item as one sentence (an err dot for a failure),
  its actions, and a second line with its kind and the job it belongs to as a
  link. The snoozed shelf is a fold under the list, its rows in the list's shape,
  each saying when it comes back, with *Wake*.

  Answers are drawn at once (#775), so a press says so in a toast that collapses
  repeats (`tally`) and takes them back with *Undo*: a snooze with a date now
  brings an item back from the shelf or from *Done*, because the backend's snooze
  clears `done_at`. An ack has no way back, and its toast says what it left open.
  An answer that fails says so in a toast too, where the reader is looking.
  A long inbox draws only the rows near the viewport (`shell/windowed.svelte.ts`, #2209).

  Over the inbox store the rooms UI already uses (`inbox/inbox.svelte.ts`:
  `inbox_items`, `inbox_count`, `snooze_inbox_item`, `complete_inbox_item`,
  `ack_alert`), `submit_write` for *Re-run*, and `job_reach` for which job a
  row belongs to. `refused_writes` (#2040) says which *Re-run* the source's
  credential cannot make: that one is drawn disabled with the reason.
-->
<script lang="ts">
  import { pageVisible } from "../kept-pages.svelte";
  import { everyWhileShown } from "../while-shown.svelte";
  import { clockLabel } from "../../time/format";
  import { onDestroy } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { health } from "../../shell/health.svelte";
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
  import { inbox as sharedInbox, pickedMorning, snoozePresets, tomorrowValue, type Inbox, type InboxRefusal } from "../../inbox/inbox.svelte";
  import { followWrite } from "../../shell/follow-write";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { push, tally } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { isFiltering, type FilterConfig } from "../filter/filter";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import type { SourceFacts } from "../job/job";
  import { ticketHash } from "../route";
  import { reenterWords, secretNoun } from "../settings/settings";
  import TfKey from "../TfKey.svelte";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import { backLabel, expiryOf, failed, KIND_CHIPS, kindOf, opener, rowActions, sentence, untilLabel, visible } from "./inbox";

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
    /** The chosen job (spec #2227), as a ticket key: what *This job* keeps. */
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
  // Paused while keep-alive holds the page hidden, read at once on its return (#2249).
  const shown = pageVisible();
  everyWhileShown(() => !fixedNow && shown(), 30_000, () => (clock = new Date()));
  const now = $derived(fixedNow ?? clock);

  const filter = sessionFilter("inbox");
  // Prototype-6 opens the Inbox on *All*.
  if (filter.scope === null) filter.scope = "all";

  const config = $derived<FilterConfig>({
    id: "inbox",
    placeholder: "Filter inbox",
    // The Inbox's search is the launcher's, `#i` (#2425): `/` opens it.
    search: false,
    noun: "items",
    one: "item",
    // With no job there is nothing to choose between, so a lone *All* is not drawn.
    scopes: job === null ? [] : [{ id: "job", label: "This job", count: jobCount(), title: `What is about ${job}`, query: "" }, { id: "all", label: "All", query: "" }],
    chips: [{ key: "kind", label: "Kind", options: KIND_CHIPS.map((value) => ({ value, label: value })) }],
  });

  /** How many items *This job* holds, before the kind chips, for its count badge. */
  function jobCount(): number {
    return visible(inbox.stream, { scope: "job", job, kinds: [] }, index).length;
  }
  let jobs = $state<OpenJob[]>([]);
  const index = $derived(jobIndex(jobs));
  const scope = $derived<"job" | "all">(job !== null && filter.scope === "job" ? "job" : "all");

  const rows = $derived(
    visible(inbox.stream, { scope, job, kinds: filter.chips["kind"] ?? [] }, index),
  );
  const shelf = $derived(inbox.snoozed);
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

  /** The sources, once read: what a credential row calls its fix. A failed read leaves the row saying where it goes. */
  let sources = $state<SourceFacts[] | null>(null);
  $effect(() => {
    if (!ready) return;
    let live = true;
    ports.listSources().then(
      (read) => {
        if (live) sources = read;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });
  /** A credential row's fix, in the status bar's words (`status.ts`): the same problem, the same press. */
  function credentialFix(sourceId: string): string | null {
    const source = sources?.find((row) => row.id === sourceId);
    if (source === undefined) return null;
    return health.get(sourceId)?.state === "missing_secret" ? `Enter ${secretNoun(source.auth_kind)}` : reenterWords(source.auth_kind);
  }

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

  /** The reason a toast has said, so the line above the list does not say it again. */
  let toasted = $state<string | null>(null);

  /** An answer that failed, in a toast at the press: the row may be far below the list's top. */
  function saysFailure(what: string, verb: string, again: () => void) {
    return ({ reason, gone }: InboxRefusal) => {
      toasted = reason;
      if (gone) push({ text: `${what} has already left the inbox. It was resolved at its source.` });
      else push({ text: `Could not ${verb} ${what}: ${reason}`, tone: "err", action: { label: "Try again", run: again } });
    };
  }

  /** Back in the stream: a snooze until now, which clears a snooze and a *Done* alike. */
  function bringBack(entry: InboxEntry) {
    const what = sentence(entry.item);
    void inbox.snooze(entry.item.key, new Date(), saysFailure(what, "bring back", () => bringBack(entry)));
  }

  const ackToast = tally<null>((held) => ({
    text: held.length === 1 ? "Acked. The alert stays open until the monitor recovers." : `${held.length} alerts acked. They stay open until their monitors recover.`,
  }));

  function ack(entry: InboxEntry) {
    ackToast(null);
    void inbox.ack(entry.item.key, saysFailure(sentence(entry.item), "ack", () => ack(entry)));
  }

  const snoozeToast = tally<{ entry: InboxEntry; until: string }>((held) => {
    const until = new Set(held.map((one) => one.until));
    const when = until.size === 1 ? ` until ${held[0]!.until}` : "";
    return {
      text: held.length === 1 ? `Snoozed${when}` : `${held.length} items snoozed${when}`,
      action: { label: "Undo", run: () => held.forEach((one) => bringBack(one.entry)) },
    };
  });

  function snooze(entry: InboxEntry, at: Date) {
    snoozing = null;
    snoozeToast({ entry, until: untilLabel(at, now) });
    void inbox.snooze(entry.item.key, at, saysFailure(sentence(entry.item), "snooze", () => snooze(entry, at)));
  }

  const doneToast = tally<InboxEntry>((held) => ({
    text: held.length === 1 ? `Done: ${sentence(held[0]!.item)}` : `${held.length} items done`,
    action: { label: "Undo", run: () => held.forEach(bringBack) },
  }));

  function done(entry: InboxEntry) {
    doneToast(entry);
    void inbox.complete(entry.item.key, saysFailure(sentence(entry.item), "finish", () => done(entry)));
  }

  /** *Wake*: back in the stream now. */
  function wake(entry: InboxEntry) {
    void inbox.snooze(entry.item.key, new Date(), saysFailure(sentence(entry.item), "wake", () => wake(entry)));
  }

  /** A date the picker produced, at 09:00 local, as the presets are. */
  function fromPicker(entry: InboxEntry) {
    const at = pickedMorning(pickedValue, now);
    if (at !== null) snooze(entry, at);
  }

  /** What the open row's date field holds; only a press or Enter commits it (#1437). */
  let pickedValue = $state("");

  /** Each button's word: a source op's is its form's (`ACTION_FORMS`), knobas' own are here. */
  const LABEL: Record<string, string> = { ack: "Ack", snooze: "Snooze…", done: "Done" };
  const labelOf = (action: string) => ACTION_FORMS[action]?.label ?? LABEL[action] ?? action;

  function press(entry: InboxEntry, action: string) {
    if (refusal(entry, action) !== null) return;
    if (action === "rerun_build") void rerun(entry);
    else if (action === "ack") ack(entry);
    else if (action === "snooze") {
      pickedValue = "";
      snoozing = snoozing === entry.item.key ? null : entry.item.key;
    }
    else if (action === "done") done(entry);
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
  {#if inbox.error !== null && inbox.error !== toasted}
    <p class="tf-ifail">Could not read the inbox: {inbox.error}</p>
  {/if}
  {#snippet context(item: InboxEntry["item"], back: string | null)}
    {@const belongs = index.jobOf(item)}
    {@const belongsId = index.jobIdOf(item)}
    <div class="tf-ctx">
      {kindOf(item.category)}{#if belongs !== null}, job <TfKey value={belongs} href={ticketHash(belongsId ?? belongs)} />{:else}, no job{/if}{#if back !== null}, back {back}{/if}
    </div>
  {/snippet}
  <div class="tf-inbox" {@attach win.list}>
    {#each win.slice(rows) as entry (entry.item.key)}
      {@const item = entry.item}
      {@const open = opener(item, credentialFix)}
      <div class="tf-ib" {@attach win.row(item.key)}>
        <time datetime={item.occurred_at}>{clockLabel(item.occurred_at, now, true)}</time>
        <div class="tf-what">
          {#if failed(item)}<span class="tf-dot tf-err"></span>{" "}{/if}{sentence(item)}
        </div>
        <div class="tf-acts tf-rowend">
          {#if open !== null}
            <a class="tf-btn tf-sm" href={open.href}>{open.label}</a>
          {/if}
          {#snippet rowAction(action: ReturnType<typeof rowActions>[number], ghost: boolean)}
            <button
              type="button"
              class="tf-btn tf-sm"
              class:tf-ghost={ghost}
              {...refusedAttrs(refusal(entry, action))}
              aria-expanded={action === "snooze" ? snoozing === item.key : undefined}
              onclick={() => press(entry, action)}>{labelOf(action)}</button
            >
          {/snippet}
          {#each rowActions(entry).filter((action) => action !== "done" && action !== "snooze") as action (action)}
            {@render rowAction(action, false)}
          {/each}
          <span class="tf-rowacts" class:tf-on={snoozing === item.key}>
            {#each rowActions(entry).filter((action) => action === "done" || action === "snooze") as action (action)}
              {" "}{@render rowAction(action, true)}
            {/each}
          </span>
        </div>
        {@render context(item, null)}
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
        {#if isFiltering(filter)}<TfNoMatch noun="items" {filter} />
        {:else}<div class="tf-empty">Nothing here. New items appear as sources sync.</div>{/if}
      {/if}
    {/each}
  </div>
  {#if shelf.length > 0}
    <details class="tf-collapse tf-ishelf">
      <summary>Snoozed <span class="tf-count">{shelf.length}</span></summary>
      <div class="tf-ishelf-rows">
        {#each shelf as entry (entry.item.key)}
          <div class="tf-ib">
            <time datetime={entry.item.occurred_at}>{clockLabel(entry.item.occurred_at, now, true)}</time>
            <div class="tf-what">
              {#if failed(entry.item)}<span class="tf-dot tf-err"></span>{" "}{/if}{sentence(entry.item)}
            </div>
            <div class="tf-acts tf-rowend">
              <button type="button" class="tf-btn tf-sm" onclick={() => wake(entry)}>Wake</button>
            </div>
            {@render context(entry.item, backLabel(entry.item))}
          </div>
        {/each}
      </div>
    </details>
  {/if}
</div>
