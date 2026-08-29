<!--
  The room's heading strip (`signal-miller.html:2519-2525`).

  The `.acts` slot is deliberately **empty** in M1. Spec §2's adaptive action
  bar is *Start work*, *Trigger build*, *Log time* — write-backs, every one of
  them M2. A bar of disabled buttons teaches a reader nothing except that the
  app is unfinished, so the element stays and its contents wait.
-->
<script lang="ts">
  import type { RoomContext } from "./contexts";

  let {
    context,
    count = null,
    inboxHere = null,
    onnewnote,
  }: {
    context: RoomContext;
    /** How many items this room holds, or `null` while it is still counting. */
    count?: number | null;
    /**
     * How many inbox items are about this room's members (#47, spec §7's
     * "3 here") — `null` for a derived room, whose inbox is the global one.
     * Zero is not drawn: a chip that says "0 here" on every quiet context is
     * a chip nobody reads.
     */
    inboxHere?: number | null;
    /**
     * Start a new note (#46).
     *
     * Its own slot rather than the `.acts` one below: `.acts` is spec §2's
     * adaptive action bar -- *Start work*, *Trigger build*, *Log time* -- which
     * is write-backs, chosen by what the room is about. Writing a note is
     * neither. Putting it there would make the bar's rule "actions, plus this
     * one", which is how a slot with a rule becomes a slot with a list.
     */
    onnewnote: () => void;
  } = $props();
</script>

<div class="room-bar">
  <h1>{context.label}</h1>
  <span class="kind">{context.kindWord}</span>
  {#if count !== null}
    <span class="mono faint">{count} item{count === 1 ? "" : "s"}</span>
  {/if}
  {#if inboxHere !== null && inboxHere > 0}
    <!-- An address, not a handler: the inbox is a place (spec §2). -->
    <a class="here mono" href="#/inbox" title="Inbox items about this context's members"
      >{inboxHere} here</a
    >
  {/if}
  <span class="own">
    <button class="btn sm" onclick={onnewnote}>New note</button>
  </span>
  <!-- M2: the adaptive action bar. See the comment above. -->
  <span class="acts"></span>
</div>
