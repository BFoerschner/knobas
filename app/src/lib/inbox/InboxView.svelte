<!--
  The inbox — one actionable stream (issue #45, spec §8).

  One queue, not a scored one: review requests, mentions, failed builds, new
  assignments, credential expiry and — since #446 — alerts on assets a context
  holds, newest first, with the action each item is asking for inline.

  **The snoozed shelf is a section, not a second view.** Story 15 is that
  snoozing is deferral rather than deletion, and a deferred item that vanished
  from the surface entirely would be indistinguishable from a deleted one. It
  is dimmed and it says when it comes back.

  **There is no bulk action and nothing here acts on a timer.** Every row is
  answered on its own, by a person, and every answer — the two knobas records
  and the writes it queues — lands in the activity stream.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../ipc";
  import { contextMembers, submitWrite, type InboxEntry } from "../ipc/entity";
  import { contexts as storedContexts } from "../shell/contexts.svelte";
  import Monogram from "../shell/Monogram.svelte";
  import { sourceMonogram } from "../shell/monogram";
  import { openExternal } from "../shell/open-external";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { ago } from "../shell/time";
  import { push } from "../shell/toasts.svelte";
  import { drawable } from "./actions";
  import { inbox as sharedInbox, snoozePresets, type Inbox } from "./inbox.svelte";

  let {
    router,
    inbox = sharedInbox,
    now: fixedNow,
  }: {
    router: Router;
    /** The live store. A prop so a test needs no Tauri bridge. */
    inbox?: Inbox;
    /** Injectable clock, so "2 h ago" and the presets are testable. */
    now?: Date;
  } = $props();

  const now = $derived(fixedNow ?? new Date());

  /** The row whose comment box is open, if any. One at a time. */
  let commenting = $state<string | null>(null);
  let draft = $state("");

  /** The row whose snooze menu is open, if any. */
  let snoozing = $state<string | null>(null);

  /**
   * Open the entity behind an item.
   *
   * Through `hashFor` rather than a template string: entity keys carry `#` and
   * `/` (`acme/payouts#144`), and unencoded the first truncates the fragment at
   * the browser level and the second reads as another path segment.
   *
   * The kind-agnostic `#/entity/<id>` alias, because an item's `kind` is the
   * mirror's word and the router's kind segment is the view's — resolving the
   * one from the other is `get_entity`'s job and it already does it.
   */
  function open(entry: InboxEntry) {
    const entityId = entry.item.entity_id;
    if (entityId === null) return;
    // An alert's way in is the **Tree at the affected asset** (spec #427 story
    // 61), not a room detail: its `entity_id` is the asset the monitor watches
    // and `#/asset/<id>` re-opens the Tree at its path. Every other category's
    // subject is a mirrored item, and a room detail is where those are read.
    router.go(
      entry.item.category === "alert"
        ? hashFor({ view: "assets", tab: "tree", assetId: entityId })
        : hashFor({ view: "room", ctx: router.ctx, detail: { kind: null, entityId } }),
    );
  }


  async function openInBrowser(url: string) {
    try {
      await openExternal(url);
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  /**
   * Send one action.
   *
   * `submitWrite` and nothing else: the inbox has no write path of its own,
   * and what comes back is the write *as queued*. So the toast says queued,
   * never sent — the queue decides send, pend or hold, and a surface that
   * reported "approved" from this return value would be reporting a hope.
   */
  async function act(entry: InboxEntry, op: string, body: string) {
    const entity = entry.item.entity_id;
    const form = drawable(entry.actions).find((candidate) => candidate.op === op)?.form;
    if (!entity || !form) return;
    try {
      await submitWrite(
        form.kind === "text" ? form.build(entity, body) : form.build(entity),
      );
      push({ text: `${form.label} queued for ${entry.item.title}` });
      commenting = null;
      draft = "";
      await inbox.refresh();
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  /**
   * The deadline a snooze preset can hang *after it expires* on.
   *
   * Only a credential expiry has one, and the expiry date itself is not on
   * the wire — the backend dates the item at `secret_expires_at` minus its
   * window (`knobas_core::inbox::WINDOW_DAYS`), so the expiry is re-derived
   * here as `occurred_at` plus that same window. `WINDOW_DAYS` below must
   * equal the backend's; a Rust test pins the two together
   * (`the_interfaces_window_matches_the_backends`).
   */
  const WINDOW_DAYS = 14;

  function presetsFor(entry: InboxEntry) {
    const deadline =
      entry.item.category === "credential_expiry"
        ? new Date(new Date(entry.item.occurred_at).getTime() + WINDOW_DAYS * 86_400_000)
        : null;
    return snoozePresets(now, deadline);
  }

  async function snooze(key: string, at: Date) {
    snoozing = null;
    await inbox.snooze(key, at);
  }

  /** A date the picker produced, at 09:00 local — the presets' own convention. */
  function fromPicker(key: string, value: string) {
    const [y, m, d] = value.split("-").map(Number);
    if (!y || !m || !d) return;
    void snooze(key, new Date(y, m - 1, d, 9, 0, 0, 0));
  }

  /**
   * The per-context filter (#47, spec §7's "inbox filter: 3 here").
   *
   * A *view* over the one stream, never a second read: membership comes from
   * `context_members` — the fixed one-hop rule, resolved server-side — and
   * the rows are the same rows the unfiltered inbox draws, so the two can
   * never disagree about what needs you. `""` is the whole inbox.
   *
   * The address can express it — `#/inbox/ctx/<id>` opens pre-filtered, which
   * is what the room's "N here" chip promises — and the mount reads it off
   * the route. After that the dropdown is a lens: changing it does not
   * rewrite the address, because a filter being *tried* is not a place being
   * *visited*, and `#/inbox` stays the one inbox address in every menu.
   */
  // `untrack` says the mount-time read is deliberate: the address seeds the
  // filter once, and later route changes remount the view anyway (the shell
  // draws it only while `view === "inbox"`).
  let ctxFilter = $state(
    untrack(() => (router.route.view === "inbox" ? (router.route.ctx ?? "") : "")),
  );
  let members = $state<Set<string> | null>(null);
  let membersToken = 0;
  $effect(() => {
    const ctx = ctxFilter;
    members = null;
    if (ctx === "") return;
    const mine = ++membersToken;
    void contextMembers(ctx)
      .then((ids) => {
        if (mine !== membersToken) return;
        members = new Set(ids);
      })
      .catch((rejection) => {
        if (mine !== membersToken) return;
        // Shown, not swallowed: a filter that silently failed open would show
        // the whole inbox labelled as one context's.
        push({ text: ipcErrorMessage(rejection), tone: "err" });
        ctxFilter = "";
      });
  });

  /** One rule for both shelves. While members load, the shelves hold back. */
  function inContext(entries: InboxEntry[]): InboxEntry[] {
    if (ctxFilter === "") return entries;
    const scope = members;
    if (scope === null) return [];
    return entries.filter(
      (entry) => entry.item.entity_id !== null && scope.has(entry.item.entity_id),
    );
  }

  const stream = $derived(inContext(inbox.stream));
  const snoozed = $derived(inContext(inbox.snoozed));
</script>

<section class="view">
  <div class="room-bar">
    <h1>
      Inbox
      <span class="k">{ctxFilter === "" ? inbox.count : `${stream.length} here`}</span>
    </h1>
    {#if storedContexts.all.length > 0}
      <!--
        Only offered once a context exists: a filter over an empty list is a
        control that can do nothing.
      -->
      <select class="sel-inline" aria-label="Filter by context" bind:value={ctxFilter}>
        <option value="">All contexts</option>
        {#each storedContexts.all as row (row.id)}
          <option value={row.id}>{row.title}</option>
        {/each}
      </select>
    {/if}
  </div>

  <div class="view-b">
    {#if inbox.error}
      <p class="empty fail">{inbox.error}</p>
    {/if}

    {#if stream.length === 0}
      <p class="empty">
        {#if ctxFilter === ""}
          Nothing needs you. Review requests, mentions, failed builds on your work, new
          assignments, expiring credentials and alerts on what your contexts hold arrive
          here.
        {:else}
          Nothing here needs you — nothing in the inbox is about this context's members.
        {/if}
      </p>
    {/if}

    {#each stream as entry (entry.item.key)}
      <div class="inbox-item">
        <Monogram text={sourceMonogram(entry.item.source_id)} label={entry.item.source_id} />
        <span class="when">{ago(entry.item.occurred_at, now)}</span>
        <span class="txt">
          {entry.item.title}
          <i>— {entry.item.reason}</i>
          <span class="ctxk">{entry.item.key}</span>
        </span>
        <span class="acts">
          {#if entry.item.entity_id}
            <button class="btn sm" onclick={() => open(entry)}>Open</button>
          {/if}
          {#if entry.item.category === "alert"}
            <!--
              Seen, not fixed (story 62): this clears the row and leaves the
              alert open in the Assets view and the top strip, where it stays
              until the monitor recovers.
            -->
            <button
              class="btn sm pri"
              disabled={inbox.busy}
              onclick={() => void inbox.ack(entry.item.key)}
            >
              Ack
            </button>
          {/if}
          {#if entry.item.web_url}
            <button class="btn sm ghost" onclick={() => void openInBrowser(entry.item.web_url!)}>
              In browser
            </button>
          {/if}
          {#each drawable(entry.actions) as action (action.op)}
            {#if action.form.kind === "immediate"}
              <button
                class="btn sm pri"
                disabled={inbox.busy}
                onclick={() => void act(entry, action.op, "")}
              >
                {action.form.label}
              </button>
            {:else}
              <button
                class="btn sm"
                onclick={() => {
                  commenting = commenting === entry.item.key ? null : entry.item.key;
                  draft = "";
                }}
              >
                {action.form.label}
              </button>
            {/if}
          {/each}
          <button
            class="btn sm ghost"
            onclick={() => (snoozing = snoozing === entry.item.key ? null : entry.item.key)}
          >
            Snooze
          </button>
          <button
            class="btn sm ghost"
            disabled={inbox.busy}
            onclick={() => void inbox.complete(entry.item.key)}
          >
            Done
          </button>
        </span>

        {#if commenting === entry.item.key}
          {@const form = drawable(entry.actions).find((a) => a.op === "comment")?.form}
          {#if form && form.kind === "text"}
            <span class="reply">
              <!-- svelte-ignore a11y_autofocus -->
              <input
                class="inp"
                bind:value={draft}
                placeholder={form.placeholder}
                aria-label="Reply to {entry.item.title}"
                autofocus
              />
              <button
                class="btn sm pri"
                disabled={draft.trim() === ""}
                onclick={() => void act(entry, "comment", draft)}
              >
                Send
              </button>
              <button class="btn sm ghost" onclick={() => (commenting = null)}>Cancel</button>
            </span>
          {/if}
        {/if}

        {#if snoozing === entry.item.key}
          <span class="reply">
            {#each presetsFor(entry) as preset (preset.label)}
              <button
                class="btn sm"
                disabled={inbox.busy}
                onclick={() => void snooze(entry.item.key, preset.at)}
              >
                {preset.label}
              </button>
            {/each}
            <input
              class="inp"
              type="date"
              aria-label="Snooze {entry.item.title} until"
              onchange={(event) => fromPicker(entry.item.key, event.currentTarget.value)}
            />
            <button class="btn sm ghost" onclick={() => (snoozing = null)}>Cancel</button>
          </span>
        {/if}
      </div>
    {/each}

    {#if snoozed.length > 0}
      <h2 class="lab snoozed-head">Snoozed</h2>
      {#each snoozed as entry (entry.item.key)}
        <div class="inbox-item dim">
          <Monogram text={sourceMonogram(entry.item.source_id)} label={entry.item.source_id} />
          <span class="when">returns {ago(entry.item.snoozed_until, now)}</span>
          <span class="txt">
            {entry.item.title}
            <i>— {entry.item.reason}</i>
          </span>
          <span class="acts">
            <button
              class="btn sm ghost"
              disabled={inbox.busy}
              onclick={() => void snooze(entry.item.key, now)}
            >
              Bring back
            </button>
            <button
              class="btn sm ghost"
              disabled={inbox.busy}
              onclick={() => void inbox.complete(entry.item.key)}
            >
              Done
            </button>
          </span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .snoozed-head {
    padding: 14px 14px 6px;
    border-top: 1px solid var(--hair);
  }
</style>
