<!--
  The Inbox (#834, R-12.1 to R-12.3): `#/inbox`.

  The filter bar (*This job / All*, the kind chips; its search is the launcher's `#i`, #2425), then a row per item
  newest first: the time, the item as one sentence, its subject first (an err
  dot for a failure), its actions, and a second line with its kind, the job it
  belongs to as a link, and what the sentence leaves out (an alert's path, a
  build's status). The snoozed shelf is a fold under the list, its rows in the list's shape,
  each saying when it comes back, with *Wake*.

  **The sentence is the link (F-132):** to the row's place, and a click elsewhere on the
  row follows it unless it made a selection, so a row's words can be selected (F-142). A
  button is only for what a link can't say: *Ack*, *Re-run*, a credential's fix, and on
  hover *Snooze…* and *Done*. `j` and `k` walk the rows, as on Tickets (F-143).

  **A row leaving under a pointer holds its place (F-138).** After a mouse press takes a
  row away, it stays as an empty row of its height until the pointer leaves it, and the
  list closes up then, so a second press where the pointer rests lands on nothing. A key
  press closes at once and moves focus to the same action on the next row. A second `Enter`
  repeats what the first did or does nothing, and never opens a row: a next row with no such
  action, or a press by mouse, leaves focus on the row itself, for `Tab` and `j` to go on from.

  Answers are drawn at once (#775), so a press says so in a toast that collapses
  repeats (`tally`) and takes them back with *Undo*: a snooze with a date now
  brings an item back from the shelf or from *Done*, because the backend's snooze
  clears `done_at`. An ack is knobas' own too: its toast says what it left open,
  and *Undo* takes it back (`unack_alert`), so the alert asks again.
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
  import { PressHold } from "../press-hold.svelte";
  import { everyWhileShown } from "../while-shown.svelte";
  import { inlineEditor } from "../inline-editor";
  import { snoozeFloat, spentAt } from "./snooze-float";
  import { clockLabel } from "../../time/format";
  import { flushSync, onDestroy, onMount, tick } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
  import { health } from "../../shell/health.svelte";
  import {
    getEntity,
    jobReach,
    listContexts,
    pipelines,
    submitWrite,
    type ContextRow,
    type EntityDetail,
    type InboxEntry,
    type PipelineRow,
    type Reached,
  } from "../../ipc/entity";
  import { listSources, type QueuedWrite, type WriteOpPayload } from "../../ipc/sources";
  import { ACTION_FORMS } from "../../inbox/actions";
  import { inbox as sharedInbox, leftTheInbox, pickedMorning, snoozePresets, tomorrowValue, type Inbox, type InboxRefusal } from "../../inbox/inbox.svelte";
  import { followWrite } from "../../shell/follow-write";
  import { inTextField } from "../../shell/keys";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { push, tally } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { isFiltering, type FilterConfig } from "../filter/filter";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { pipelineHash } from "../builds/builds";
  import { heldByPointer, heldHeight, holdUnderPointer } from "../row-hold";
  import type { SourceFacts } from "../job/job";
  import { ticketHash } from "../route";
  import { reenterWords, secretNoun } from "../settings/settings";
  import TfKey from "../TfKey.svelte";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import { backLabel, expiryOf, failed, fixOf, KIND_CHIPS, kindOf, named, opener, rowActions, sentence, untilLabel, visible } from "./inbox";

  /** The reads and the write this view makes beside the inbox store. */
  export interface InboxPorts {
    listContexts: () => Promise<ContextRow[]>;
    listSources: () => Promise<SourceFacts[]>;
    getEntity: (entityId: string) => Promise<EntityDetail>;
    jobReach: (entityId: string) => Promise<Reached[]>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** Which pipeline a run belongs to, for *Show in runs*; the bridge's when left out. */
    pipelines?: () => Promise<Pick<PipelineRow, "entity_id" | "recent">[]>;
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

  /** The time column: the one ladder, with today's word left out, since a column here is only read as recent. */
  const when = (at: string) => clockLabel(at, now, true);

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

  /** The row a mouse press took away, still holding its place under the pointer (F-138). */
  let holding = $state.raw<{ entry: InboxEntry; list: "stream" | "shelf"; at: number; height: number } | null>(null);
  /** Its key while the list no longer holds it. A failed answer's re-read brings it back, and that row is drawn instead. */
  const heldKey = $derived.by(() => {
    if (holding === null) return null;
    const key = holding.entry.item.key;
    return (holding.list === "stream" ? rows : shelf).some((row) => row.item.key === key) ? null : key;
  });
  const withHeld = (list: readonly InboxEntry[], which: "stream" | "shelf"): readonly InboxEntry[] =>
    holding === null || holding.list !== which || heldKey === null ? list : [...list.slice(0, holding.at), holding.entry, ...list.slice(holding.at)];
  const drawn = $derived(withHeld(rows, "stream"));
  const shelfDrawn = $derived(withHeld(shelf, "shelf"));

  /** Thousands of rows draw only those near the viewport (#2209); `.tf-ib` is 63 px, 84 wrapped, 6 apart. */
  const win = new Windowed({ count: () => drawn.length, key: (i) => drawn[i]!.item.key, estimate: 63, gap: 6 });

  function scrollerOf(node: Element): Element | null {
    for (let at = node.parentElement; at !== null; at = at.parentElement) {
      if (/auto|scroll/.test(getComputedStyle(at).overflowY)) return at;
    }
    return null;
  }

  /**
   * Before a press takes `entry` away. A mouse's press holds the row, and focus rests on the next row itself. A key's has
   * `detail` 0: the list closes at once, and focus goes to the same action on the next row, or that row where it has none.
   */
  function leaves(entry: InboxEntry, list: "stream" | "shelf", action: string, event?: MouseEvent) {
    const among = list === "stream" ? rows : shelf;
    const row = heldByPointer(event) ? (event?.currentTarget as Element | null)?.closest(".tf-ib") : null;
    if (row !== null && row !== undefined) {
      const box = row.getBoundingClientRect();
      holding = { entry, list, at: among.findIndex((one) => one.item.key === entry.item.key), height: box.height };
      // A woken row lands in the list above the shelf, which would slide the held row out from under the pointer.
      void tick().then(() => {
        const held = document.querySelector(".tf-ib-held");
        const moved = held === null ? 0 : held.getBoundingClientRect().top - box.top;
        if (held !== null && moved !== 0) scrollerOf(held)?.scrollBy(0, moved);
      });
    }
    // By `detail`, not by `row`: a pick from the Snooze float is a pointer's press with no row around it.
    keepPlace(entry, among, heldByPointer(event) ? null : action);
  }

  // Held until the pointer leaves the row, the page is left, or a wheel or key takes the reader on.
  $effect(() => {
    if (heldKey === null) return;
    if (!shown()) {
      holding = null;
      return;
    }
    return holdUnderPointer(() => document.querySelector(".tf-ib-held"), () => (holding = null));
  });

  /**
   * A click on the row that made no selection follows its sentence's link (F-142). The link, the key, the buttons and the
   * snooze row answer for themselves, and a modified click is theirs too. A drag that ended on the sentence selected words, so it doesn't follow.
   */
  function followRow(event: MouseEvent) {
    const selection = window.getSelection();
    const row = event.currentTarget as HTMLElement;
    if (selection !== null && !selection.isCollapsed && selection.anchorNode !== null && row.contains(selection.anchorNode)) {
      event.preventDefault();
      return;
    }
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    if ((event.target as Element).closest("a, button, input, label, .tf-isnooze")) return;
    row.querySelector<HTMLElement>(".tf-what a")?.click();
  }

  /**
   * Chrome and WebKit won't start a selection on a link, so a plain press on the sentence takes its `href` away until the
   * button comes up. A click that selected nothing then follows it as ever, and ⌘-click is left alone (F-142).
   */
  function liftHref(event: MouseEvent) {
    const link = event.currentTarget as HTMLAnchorElement;
    const href = link.getAttribute("href");
    if (href === null || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    link.removeAttribute("href");
    const back = () => {
      link.setAttribute("href", href);
      window.removeEventListener("mouseup", back, true);
      window.removeEventListener("blur", back);
    };
    window.addEventListener("mouseup", back, true);
    window.addEventListener("blur", back);
  }

  /** The rows `j` and `k` walk: the list, then an open shelf's. */
  let shelfEl = $state<HTMLDetailsElement | null>(null);

  /** `j`/`k` and the arrows (F-143): to the next or previous row's link, as Tickets walks its cards. */
  function walk(by: 1 | -1) {
    const keys = [...rows, ...(shelfEl?.open ? shelf : [])].map((row) => row.item.key);
    if (keys.length === 0) return;
    const on = document.activeElement?.closest<HTMLElement>(".tf-ib[data-row]")?.dataset["row"];
    const at = on === undefined ? -1 : keys.indexOf(on);
    const to = at < 0 ? (by === 1 ? 0 : keys.length - 1) : Math.min(keys.length - 1, Math.max(0, at + by));
    if (to < rows.length) win.reveal(to);
    flushSync();
    const row = document.querySelector<HTMLElement>(`.tf-ib[data-row="${CSS.escape(keys[to]!)}"]`);
    row?.querySelector<HTMLElement>("a, button")?.focus();
    row?.scrollIntoView?.({ block: "nearest" });
  }

  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!shown()) return;
      if (event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const by = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[event.key] as 1 | -1 | undefined;
      if (by === undefined) return;
      const at = document.activeElement;
      // A date field's own arrows change its date, and an open dialog has the keys.
      if (at instanceof HTMLElement && (inTextField(at) || at.closest(".tf-dialog, .tf-isnooze, select") !== null)) return;
      event.preventDefault();
      walk(by);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });

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
  /** A Re-run answered holds about two seconds more (F-150), as Builds' does: a second press where the hand rests queues nothing. */
  const rerunHold = new PressHold();
  /** The held Re-runs whose write has left the queue, so the button says *Sent*. */
  const rerunSent = new SvelteSet<string>();
  /** What a held Re-run says, as Builds' does; `null` when it does not hold. */
  const rerunWord = (key: string) => (rerunHold.held(key) ? (rerunSent.has(key) ? "Sent" : "Queued") : null);

  /** *Show in runs*: the failed run's pipeline, where the new run is listed; a run no list holds opens by its own address. */
  async function showInRuns(entry: InboxEntry) {
    const run = entry.item.entity_id;
    const all = await (ports.pipelines ?? pipelines)().catch(() => [] as Pick<PipelineRow, "entity_id" | "recent">[]);
    const owner = all.find((pipeline) => pipeline.recent.some((row) => row.entity_id === run));
    const to = owner !== undefined ? pipelineHash(owner.entity_id) : opener(entry.item)?.href;
    if (to !== undefined) location.hash = to;
  }

  async function rerun(entry: InboxEntry) {
    const entity = entry.item.entity_id;
    const form = ACTION_FORMS["rerun_build"];
    if (entity === null || !form || form.kind !== "immediate" || queueing.has(entry.item.key) || rerunHold.held(entry.item.key)) return;
    queueing.add(entry.item.key);
    try {
      const write = await ports.submitWrite(form.build(entity));
      rerunSent.delete(entry.item.key);
      rerunHold.hold(entry.item.key);
      // Named for the toast's sign-in sentence; a list that fails still lets the toast say what was queued.
      const sources = await ports.listSources().catch(() => [] as SourceFacts[]);
      const source = sources.find((row) => row.id === entry.item.source_id);
      toastQueued(`${form.label.toLowerCase()} ${entry.item.title}`, {
        id: entry.item.source_id,
        display_name: source?.display_name ?? "",
      }, { write, where: { label: "Show in runs", run: () => void showInRuns(entry) } });
      const key = entry.item.key;
      following.push(followWrite(write, { name: form.label, onleave: () => rerunSent.add(key), onstop: () => rerunHold.release(key) }));
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
      if (gone) push({ text: leftTheInbox(what) });
      else push({ text: `Could not ${verb} ${what}: ${reason}`, tone: "err", action: { label: "Try again", run: again } });
    };
  }

  /** Back in the stream: a snooze until now, which clears a snooze and a *Done* alike. */
  function bringBack(entry: InboxEntry) {
    const what = named(entry.item);
    void inbox.snooze(entry.item.key, new Date(), saysFailure(what, "bring back", () => bringBack(entry)));
  }

  /** *Undo* on an ack: the alert asks again, and the re-read brings its row back. */
  function unack(entry: InboxEntry) {
    void inbox.unack(entry.item.key, saysFailure(named(entry.item), "take back the ack on", () => unack(entry)));
  }

  const ackToast = tally<InboxEntry>((held) => ({
    text: held.length === 1 ? "Acked. The alert stays open until the monitor recovers." : `${held.length} alerts acked. They stay open until their monitors recover.`,
    action: { label: "Undo", run: () => held.forEach(unack) },
  }));

  function ack(entry: InboxEntry, event?: MouseEvent) {
    leaves(entry, "stream", "ack", event);
    ackToast(entry);
    void inbox.ack(entry.item.key, saysFailure(named(entry.item), "ack", () => ack(entry)));
  }

  const snoozeToast = tally<{ entry: InboxEntry; until: string }>((held) => {
    const until = new Set(held.map((one) => one.until));
    const when = until.size === 1 ? ` until ${held[0]!.until}` : "";
    return {
      text: held.length === 1 ? `Snoozed${when}` : `${held.length} items snoozed${when}`,
      action: { label: "Undo", run: () => held.forEach((one) => bringBack(one.entry)) },
    };
  });

  /**
   * A row that leaves takes focus with it, so the reader's place moves on to the row that takes its place, or the one before
   * the last: to the same action there, or else to the row itself, because a second `Enter` must not open a row nobody aimed
   * at. `j` and `Tab` go on from a row as from anything in it. With no row left: the shelf's fold, then the woken row itself,
   * then the page's heading (F-148). The shell's focus keeper can't stand in: it knows neither the fold nor a row as a place.
   */
  function keepPlace(entry: InboxEntry, among: readonly InboxEntry[], action: string | null) {
    const at = among.findIndex((row) => row.item.key === entry.item.key);
    const next = (among[at + 1] ?? among[at - 1])?.item.key;
    void tick().then(() => {
      if (document.activeElement !== document.body && document.activeElement !== null) return;
      const rowAt = (key: string | undefined) => (key === undefined ? null : document.querySelector<HTMLElement>(`.tf-ib:not(.tf-ib-held)[data-row="${CSS.escape(key)}"]`));
      const row = rowAt(next);
      const place =
        (action === null ? null : row?.querySelector<HTMLElement>(`[data-act="${action}"]`)) ??
        row ??
        document.querySelector<HTMLElement>(".tf-ishelf > summary") ??
        rowAt(entry.item.key) ??
        document.querySelector<HTMLElement>(".tf-inbox-h");
      if (place?.matches(".tf-ib")) place.tabIndex = -1;
      place?.focus({ preventScroll: true });
      if (action !== null) place?.scrollIntoView?.({ block: "nearest" });
    });
  }

  /** The box a pointer's pick was made in. What stood under the float is about to be under a still pointer, and a second press must land on nothing. */
  let spent = $state.raw<DOMRect | null>(null);
  $effect(() => {
    if (spent === null) return;
    if (!shown()) {
      spent = null;
      return;
    }
    return holdUnderPointer(() => document.querySelector(".tf-isnooze-spent"), () => (spent = null));
  });

  function snooze(entry: InboxEntry, at: Date, event?: MouseEvent) {
    if (heldByPointer(event)) spent = (event?.currentTarget as Element).closest(".tf-isnooze")?.getBoundingClientRect() ?? null;
    snoozing = null;
    leaves(entry, "stream", "snooze", event);
    snoozeToast({ entry, until: untilLabel(at, now) });
    void inbox.snooze(entry.item.key, at, saysFailure(named(entry.item), "snooze", () => snooze(entry, at)));
  }

  const doneToast = tally<InboxEntry>((held) => ({
    text: held.length === 1 ? `Done: ${named(held[0]!.item)}` : `${held.length} items done`,
    action: { label: "Undo", run: () => held.forEach(bringBack) },
  }));

  function done(entry: InboxEntry, event?: MouseEvent) {
    leaves(entry, "stream", "done", event);
    doneToast(entry);
    void inbox.complete(entry.item.key, saysFailure(named(entry.item), "finish", () => done(entry)));
  }

  /** *Wake*: back in the stream now, which is often out of sight above the shelf, so a toast says so. *Undo* snoozes it again as it was. */
  const wakeToast = tally<InboxEntry>((held) => ({
    text: held.length === 1 ? `Back in the inbox: ${named(held[0]!.item)}` : `${held.length} items back in the inbox`,
    action: {
      label: "Undo",
      run: () =>
        held.forEach((one) => {
          const until = one.item.snoozed_until;
          if (until) void inbox.snooze(one.item.key, new Date(until), saysFailure(named(one.item), "snooze", () => {}));
        }),
    },
  }));

  function wake(entry: InboxEntry, event?: MouseEvent) {
    leaves(entry, "shelf", "wake", event);
    wakeToast(entry);
    void inbox.snooze(entry.item.key, new Date(), saysFailure(named(entry.item), "wake", () => wake(entry))).then(async () => {
      // A key's landing has to be seen, and the woken row has just taken room above the shelf and carried it down (F-148).
      if (heldByPointer(event)) return;
      await tick();
      if (document.activeElement !== document.body) document.activeElement?.scrollIntoView?.({ block: "nearest" });
    });
  }

  /** A date the picker produced, at 09:00 local, as the presets are. *Snooze* with no date goes to the field and says what it wants (F-148). */
  function fromPicker(entry: InboxEntry, event?: MouseEvent) {
    const at = pickedMorning(pickedValue, now);
    if (at !== null) return snooze(entry, at, event);
    askedDay = true;
    (event?.currentTarget as Element | undefined)?.closest(".tf-isnooze")?.querySelector("input")?.focus();
  }

  /** What the open row's date field holds; only a press or Enter commits it (#1437). */
  let pickedValue = $state("");
  /** *Snooze* was pressed with no day picked. */
  let askedDay = $state(false);

  /** Each button's word: a source op's is its form's (`ACTION_FORMS`), knobas' own are here. */
  const LABEL: Record<string, string> = { ack: "Ack", snooze: "Snooze…", done: "Done" };
  const labelOf = (action: string) => ACTION_FORMS[action]?.label ?? LABEL[action] ?? action;

  function press(entry: InboxEntry, action: string, event: MouseEvent) {
    if (refusal(entry, action) !== null) return;
    if (action === "rerun_build") void rerun(entry);
    else if (action === "ack") ack(entry, event);
    else if (action === "snooze") {
      pickedValue = "";
      askedDay = false;
      snoozing = snoozing === entry.item.key ? null : entry.item.key;
    }
    else if (action === "done") done(entry, event);
  }
</script>

<header class="tf-fhead">
  <div>
    <h1 class="tf-inbox-h" tabindex="-1">Inbox</h1>
    <div class="tf-sub">What asks for you, newest first. Each row says which job it belongs to.</div>
  </div>
</header>
<FilterBar {config} {filter} shown={inbox.answered ? rows.length : null} total={inbox.answered ? inbox.stream.length : null} />
<div class="tf-ibody">
  {#if inbox.error !== null && inbox.error !== toasted}
    <p class="tf-ifail">Could not read the inbox: {inbox.error}</p>
  {/if}
  {#snippet said(item: InboxEntry["item"])}
    {@const place = opener(item)}
    <div class="tf-what">
      {#if failed(item)}<span class="tf-dot tf-err"></span>{/if}{#if place !== null}<a href={place.href} onmousedown={liftHref}>{sentence(item)}</a>{:else}{sentence(item)}{/if}
    </div>
  {/snippet}
  {#snippet context(item: InboxEntry["item"], back: string | null)}
    {@const belongs = index.jobOf(item)}
    {@const belongsId = index.jobIdOf(item)}
    <div class="tf-ctx">
      {kindOf(item.category)}{#if belongs !== null}, job <TfKey value={belongs} href={ticketHash(belongsId ?? belongs)} />{:else}, no job{/if}{#if back !== null}, back {back}{/if}{#if item.detail}{" · "}<span class="tf-ctx-more">{item.detail}</span>{/if}
    </div>
  {/snippet}
  <div class="tf-inbox" {@attach win.list}>
    {#each win.slice(drawn) as entry (entry.item.key)}
      {@const item = entry.item}
      {@const fix = fixOf(item, credentialFix)}
      {#if heldKey === item.key}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="tf-ib tf-ib-held" data-row={item.key} aria-hidden="true" onmousedown={(event) => event.preventDefault()} {@attach win.row(item.key)} {@attach heldHeight(holding?.height ?? 0)}></div>
      {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="tf-ib" data-row={item.key} onclick={followRow} {@attach win.row(item.key)}>
        <time datetime={item.occurred_at}>{when(item.occurred_at)}</time>
        {@render said(item)}
        <!-- Before the actions: Tab goes down the row's words, then right to what it can do (F-143). -->
        {@render context(item, null)}
        <div class="tf-acts tf-rowend">
          {#if fix !== null}
            <a class="tf-btn tf-sm" href={fix.href}>{fix.label}</a>
          {/if}
          {#snippet rowAction(action: ReturnType<typeof rowActions>[number], ghost: boolean)}
            <button
              type="button"
              class="tf-btn tf-sm"
              class:tf-ghost={ghost}
              {...action === "rerun_build" && rerunWord(item.key) !== null ? { "aria-disabled": "true" as const } : refusedAttrs(refusal(entry, action))}
              data-snooze={action === "snooze" ? item.key : undefined}
              data-act={action}
              aria-expanded={action === "snooze" ? snoozing === item.key : undefined}
              onclick={(event) => press(entry, action, event)}
              >{#if action === "rerun_build"}<span class="tf-keepw" data-keep="Queued"><span>{rerunWord(item.key) ?? labelOf(action)}</span></span>{:else}{labelOf(action)}{/if}</button
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
        {#if snoozing === item.key}
          <div
            class="tf-isnooze"
            role="group"
            aria-label="Snooze until"
            {@attach snoozeFloat(() => document.querySelector<HTMLElement>(`[data-snooze="${CSS.escape(item.key)}"]`), () => (snoozing = null))}
            {@attach inlineEditor(() => (snoozing = null), {
              focus: "button",
              escCloses: true,
              back: () => document.querySelector<HTMLElement>(`[data-snooze="${CSS.escape(item.key)}"]`),
            })}
          >
            {#each snoozePresets(now, expiryOf(item)) as preset (preset.label)}
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={(event) => snooze(entry, preset.at, event)}>{preset.label}</button>
            {/each}
            <div class="tf-isnooze-day">
              <input
                type="date"
                aria-label="Snooze until a date"
                min={tomorrowValue(now)}
                bind:value={pickedValue}
                onkeydown={(event) => {
                  if (event.key === "Enter") fromPicker(entry);
                }}
              />
              <button type="button" class="tf-btn tf-sm" onclick={(event) => fromPicker(entry, event)}>Snooze</button>
            </div>
            {#if (askedDay || pickedValue !== "") && pickedMorning(pickedValue, now) === null}
              <span class="tf-dim">Pick a day after today</span>
            {/if}
          </div>
        {/if}
      </div>
      {/if}
    {:else}
      {#if inbox.answered}
        {#if isFiltering(filter)}<TfNoMatch noun="items" {filter} />
        {:else}<div class="tf-empty">Nothing here. New items appear as sources sync.</div>{/if}
      {/if}
    {/each}
  </div>
  {#if shelfDrawn.length > 0}
    <details class="tf-collapse tf-ishelf" bind:this={shelfEl}>
      <summary>Snoozed <span class="tf-count">{shelf.length}</span></summary>
      <div class="tf-ishelf-rows">
        {#each shelfDrawn as entry (entry.item.key)}
          {#if heldKey === entry.item.key}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="tf-ib tf-ib-held" data-row={entry.item.key} aria-hidden="true" onmousedown={(event) => event.preventDefault()} {@attach heldHeight(holding?.height ?? 0)}></div>
          {:else}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="tf-ib" data-row={entry.item.key} onclick={followRow}>
            <time datetime={entry.item.occurred_at}>{when(entry.item.occurred_at)}</time>
            {@render said(entry.item)}
            {@render context(entry.item, backLabel(entry.item))}
            <div class="tf-acts tf-rowend">
              <button type="button" class="tf-btn tf-sm" data-act="wake" onclick={(event) => wake(entry, event)}>Wake</button>
            </div>
          </div>
          {/if}
        {/each}
      </div>
    </details>
  {/if}
</div>

{#if spent !== null}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="tf-isnooze-spent" aria-hidden="true" onmousedown={(event) => event.preventDefault()} {@attach spentAt(spent)}></div>
{/if}
