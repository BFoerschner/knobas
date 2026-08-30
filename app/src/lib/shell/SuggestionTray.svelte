<!--
  The room's suggestion tray: the links knobas can see, waiting to be answered.

  **A query, not a store** (#41). It holds no state of its own beyond the answer
  to its own read: what is in it is `knobas.proposed_link` filtered by the room,
  and accepting or dismissing is a write to that same row followed by another
  read. Nothing here caches a decision.

  It owns its read the way a `Tile` does, and for the same reason — the tray is
  the thing that knows what it draws. What it also owns, and a tile does not, is
  running **detection**: #41's story 22 is "detection runs without me asking",
  and the honest reading of that is that a surface triggers it rather than the
  user. Two triggers, both here: opening a room, and a sync run finishing.

  A pass is idempotent and a pass over an unchanged mirror writes nothing, which
  is what makes running one on every room switch affordable. On a corpus large
  enough for that to stop being true, the pass belongs on the sync scheduler's
  own completion — a backend change, not a change to this file's contract.

  The room scope is the `sources` list for a derived room, and — since #47 —
  the context's one-hop membership (ADR-0008) for a stored room, resolved
  server-side from confirmed links only. The tray gained a scope, not a store.
-->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  import { EVENTS, ipcErrorMessage } from "../ipc";
  import {
    acceptSuggestion,
    detectSuggestions,
    dismissSuggestion,
    roomSuggestions,
    type SuggestionEntry,
    type SuggestionPage,
  } from "../ipc/entity";
  import type { SourceSyncStatus } from "../ipc/sources";
  import type { ActivityRow } from "../ipc/entity";
  import { linkChanges } from "../detail/links.svelte";
  import Monogram from "./Monogram.svelte";
  import { kindMonogram, kindSingular } from "./kinds";
  import { kindRegistry } from "./kind-registry.svelte";
  import { hashFor } from "./router.svelte";
  import { classBadge, waitingLabel } from "./suggestions";
  import { push } from "./toasts.svelte";

  let {
    sources,
    ctx = null,
    onopen,
  }: {
    /** A derived room's membership; `[]` is every source. */
    sources: string[];
    /**
     * A stored context's room (#47): proposals scope to its one-hop
     * membership plus the context's own entity, resolved server-side.
     */
    ctx?: string | null;
    /** Navigate to an address (spec §2). The shell's router is the only one. */
    onopen: (hash: string) => void;
  } = $props();

  /**
   * How many rows the tray holds.
   *
   * The strip scrolls (`.tray { overflow: auto }`) and the heading carries the
   * room's whole `total`, so this is a window and never the truth about how
   * many are waiting.
   */
  const PAGE = 50;

  let page = $state<SuggestionPage | null>(null);
  let error = $state<string | null>(null);
  /** Ids being answered right now, so a row cannot be pressed twice. */
  let busy = $state<string[]>([]);

  /**
   * The generation of the newest request, so a slow read cannot overwrite a
   * newer room's answer. Plain, not a rune — writing it is not a render.
   */
  let token = 0;

  async function refresh(detect: boolean, mine: number) {
    try {
      if (detect) await detectSuggestions();
      const answer = await roomSuggestions(sources, ctx, PAGE);
      if (mine !== token) return;
      page = answer;
      error = null;
    } catch (rejection) {
      if (mine !== token) return;
      // Shown, not swallowed: a tray that renders empty on a failed read is
      // indistinguishable from a room with nothing to suggest.
      error = ipcErrorMessage(rejection);
    }
  }

  $effect(() => {
    // Read for their dependencies: the effect re-runs when the room changes.
    const room = sources;
    void ctx;
    const mine = ++token;
    let dead = false;
    const off: Array<() => void> = [];

    /** Keep an `unlisten`, or run it if the effect already tore down. */
    function hold(pending: Promise<() => void>) {
      void pending
        .then((unlisten) => {
          if (dead) unlisten();
          else off.push(unlisten);
        })
        .catch(() => {});
    }

    page = null;
    error = null;
    void refresh(true, mine);

    // A run that just finished is new material. `running: false` is the
    // transition, and the payload is coarse by rule, so this is the only
    // signal there is that the mirror moved.
    hold(
      listen<SourceSyncStatus>(EVENTS.syncState, (event) => {
        if (dead || event.payload.running) return;
        void refresh(true, mine);
      }),
    );

    // A link mutation somewhere else in the app moves this list too, and until
    // #70 nothing told the tray so: hand-drawing the reverse of a live proposal
    // withdraws it, and the row stayed drawn here until something else happened
    // to refresh — *Accept* on it then resolving to nothing at all, visibly.
    //
    // Filtered to the verbs that change what this list contains, because
    // `activity:new` carries every write-back and comment in the app and a
    // blanket refresh here would re-read the tray on all of them.
    hold(
      listen<ActivityRow>(EVENTS.activityNew, (event) => {
        if (dead || !TRAY_VERBS.includes(event.payload.verb)) return;
        void refresh(true, mine);
      }),
    );

    void room;
    return () => {
      dead = true;
      for (const unlisten of off) unlisten();
    };
  });

  /**
   * The activity verbs that change the proposal set.
   *
   * `linked` and `unlinked` because detection's suppression reads the pair in
   * both directions with no filter, so drawing or withdrawing a link is what
   * makes a proposal appear or stop being proposable; `accepted` and `dismissed`
   * because a proposal answered anywhere leaves this list. Everything else
   * `activity:new` carries — a comment, a queued write-back, a sync — reaches
   * the tray through `sync:state` if it reaches it at all.
   */
  const TRAY_VERBS = ["linked", "unlinked", "accepted", "dismissed"];

  async function answer(entry: SuggestionEntry, accept: boolean) {
    const id = entry.link.id;
    // `busy` is what disables both buttons on this row, and that is the whole
    // of the double-press guard: an early return here as well would be a second
    // mechanism for one rule, and a dead one -- a disabled button does not
    // dispatch. Found by mutating the early return away and watching every test
    // stay green.
    busy = [...busy, id];
    try {
      await (accept ? acceptSuggestion(id) : dismissSuggestion(id));
      // Accepting adds a link, and a detail slide-over open behind the room
      // would otherwise keep showing the panel as it was.
      if (accept) linkChanges.count += 1;
      push({ text: accept ? `Linked ${entry.to.title || entry.to.entity_id}` : "Suggestion dismissed" });
      // Re-read rather than splice the row out: the tray *is* the query, and a
      // local removal would be a second copy of the truth.
      await refresh(false, token);
    } catch (rejection) {
      push({ text: ipcErrorMessage(rejection), tone: "err" });
    } finally {
      busy = busy.filter((held) => held !== id);
    }
  }

  function addressOf(end: SuggestionEntry["from"]): string {
    return hashFor({ view: "room", ctx: "all", detail: { kind: end.kind, entityId: end.entity_id } });
  }

  const labelOf = (kind: string) => kindSingular(kind, kindRegistry.info(kind));
  const monogramOf = (kind: string) => kindMonogram(kind, kindRegistry.info(kind));

  const rows = $derived(page?.rows ?? []);
</script>

<section class="tray" aria-label="Suggested links">
  <div class="tile-h">
    <span class="lab">Suggested links</span>
    <span class="cnt">{page ? waitingLabel(page.total) : "…"}</span>
    <span class="acts"></span>
  </div>

  {#if error}
    <!-- Text: an `IpcError.message` can carry whatever a source said. -->
    <div class="empty"><p class="fail">{error}</p></div>
  {:else if rows.length > 0}
    {#each rows as entry (entry.link.id)}
      {@const badge = classBadge(entry.link.rule_class)}
      <div class="row sug">
        <Monogram text={monogramOf(entry.from.kind)} label={labelOf(entry.from.kind)} />
        <!-- Both ends open, so an ambiguous suggestion can be checked before
             it is answered (#41 story 20). Titles are raw source text and are
             rendered as text (gotcha 7). -->
        <button
          class="t open"
          title="Open {entry.from.entity_id}"
          onclick={() => onopen(addressOf(entry.from))}
        >
          {entry.from.title || entry.from.entity_id}
          {#if entry.from.deleted_at}<span class="wd">withdrawn</span>{/if}
        </button>
        <button
          class="t open"
          title="Open {entry.to.entity_id}"
          onclick={() => onopen(addressOf(entry.to))}
        >
          <span class="arrow">{entry.link.relation}</span>
          {entry.to.title || entry.to.entity_id}
          {#if entry.to.deleted_at}<span class="wd">withdrawn</span>{/if}
        </button>
        <span class="why" title={entry.link.reason ?? ""}>
          {#if badge}
            <span class="cls" class:guess={badge.speculative}>{badge.label}</span>
          {/if}
          {entry.link.reason}
        </span>
        <span class="acts">
          <button
            class="btn sm pri"
            disabled={busy.includes(entry.link.id)}
            onclick={() => answer(entry, true)}
          >
            Accept
          </button>
          <button
            class="btn sm ghost"
            title="Dismissing is remembered — this suggestion will not come back."
            disabled={busy.includes(entry.link.id)}
            onclick={() => answer(entry, false)}
          >
            Dismiss
          </button>
        </span>
      </div>
    {/each}
  {/if}
</section>

<style>
  /*
    Five columns rather than the sheet's four: the reason is not decoration,
    it is the thing the reader judges the row by (#41 story 2), so it gets a
    cell of its own instead of a tooltip.
  */
  .row.sug {
    grid-template-columns: 34px 1fr 1fr 1.4fr 148px;
    cursor: default;
  }

  /* The whole title cell is the navigation target, not a link-coloured word. */
  .open {
    text-align: left;
    width: 100%;
    color: var(--text);
  }

  .open:hover {
    color: var(--link);
  }

  /* The relation, so the row reads as a sentence rather than as two names. */
  .arrow {
    color: var(--faint);
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-right: 6px;
  }

  .cls {
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-right: 6px;
  }

  /*
    A guess is marked, because the reader trusts it differently. Amber is what
    the shell already uses for "look at this before believing it".
  */
  .cls.guess {
    color: var(--amber);
  }

  .wd {
    margin-left: 6px;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--amber);
  }
</style>
