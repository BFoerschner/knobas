<!--
  The inbox — one actionable stream (issue #45, spec §8).

  One queue, not a scored one: review requests, mentions, failed builds, new
  assignments and credential expiry, newest first, with the action each item is
  asking for inline.

  **The snoozed shelf is a section, not a second view.** Story 15 is that
  snoozing is deferral rather than deletion, and a deferred item that vanished
  from the surface entirely would be indistinguishable from a deleted one. It
  is dimmed and it says when it comes back.

  **There is no bulk action and nothing here acts on a timer.** Every row is
  answered on its own, by a person, and every answer — the two knobas records
  and the writes it queues — lands in the activity stream.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { submitWrite, type InboxEntry } from "../ipc/entity";
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
  function open(entityId: string) {
    router.go(hashFor({ view: "room", ctx: router.ctx, detail: { kind: null, entityId } }));
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
</script>

<section class="view">
  <div class="room-bar">
    <h1>
      Inbox
      <span class="k">{inbox.count}</span>
    </h1>
  </div>

  <div class="view-b">
    {#if inbox.error}
      <p class="empty fail">{inbox.error}</p>
    {/if}

    {#if inbox.stream.length === 0}
      <p class="empty">
        Nothing needs you. Review requests, mentions, failed builds on your work, new
        assignments and expiring credentials arrive here.
      </p>
    {/if}

    {#each inbox.stream as entry (entry.item.key)}
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
            <button class="btn sm" onclick={() => open(entry.item.entity_id!)}>Open</button>
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

    {#if inbox.snoozed.length > 0}
      <h2 class="lab snoozed-head">Snoozed</h2>
      {#each inbox.snoozed as entry (entry.item.key)}
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
