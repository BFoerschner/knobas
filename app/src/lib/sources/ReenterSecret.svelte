<!--
  The inline *Re-enter password* strip (`signal-miller.html`'s `.src-fix`).

  Spec §3: *"401 detection → Re-enter password"*. It is a strip under the row
  rather than a dialog because the answer to "which source is this about?" is
  the row directly above it, and because the scheduler will not retry a 401 on
  its own (P7) — the reader is looking at a source that is stopped until they
  act, and a modal would hide the rest of the list while they do.

  ## The secret

  The typed value lives in this component's `$state` and in the input element,
  and goes to `set_source_secret` — which writes it to the OS keychain and
  never to Postgres (contract §3). It is never stored, never logged, never put
  in an attribute, and there is no command anywhere that reads one back. The
  sentence on screen says so because it is true and because a person typing a
  password is owed it.
-->
<script lang="ts">
  import { setSourceSecret, syncNow } from "../ipc/sources";
  import { ipcErrorMessage } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";

  let {
    sourceId,
    displayName,
    onhealth,
    oncancel,
  }: {
    sourceId: string;
    displayName: string;
    /** The health the backend answered with, so the row redraws from it. */
    onhealth: (health: CredentialHealth) => void;
    oncancel: () => void;
  } = $props();

  let value = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let field = $state<HTMLInputElement | null>(null);

  $effect(() => {
    // The strip appears *because* the reader asked to fix this source, so the
    // caret belongs in it. Without this they would have to find a 220px input
    // that just appeared under a row.
    field?.focus();
  });

  async function save() {
    if (busy || value === "") return;
    busy = true;
    error = null;
    try {
      const health = await setSourceSecret(sourceId, { value });
      // Cleared before anything else can await: the value has served its one
      // purpose and there is no reason for it to survive the round trip.
      value = "";
      onhealth(health);
      // Re-entering a password is a request to make the sync work again, not a
      // request to store a string. Doing only the storing would leave the
      // reader looking at a row that still says it last failed.
      await syncNow(sourceId).catch(() => {
        // The sync is the follow-through, not the transaction. A source that
        // is disabled or already running rejects here, and the credential is
        // stored either way.
      });
    } catch (cause) {
      error = ipcErrorMessage(cause);
    } finally {
      busy = false;
    }
  }
</script>

<div class="src-fix">
  <label class="l" for="reenter-{sourceId}">Password or token for {displayName}</label>
  <input
    id="reenter-{sourceId}"
    class="inp"
    type="password"
    autocomplete="off"
    bind:this={field}
    bind:value
    disabled={busy}
    onkeydown={(event) => {
      if (event.key === "Enter") void save();
      if (event.key === "Escape") {
        event.stopPropagation();
        oncancel();
      }
    }}
  />
  <button class="btn pri sm" disabled={busy || value === ""} onclick={() => void save()}>
    Save and retry sync
  </button>
  <button class="btn sm" disabled={busy} onclick={oncancel}>Cancel</button>
  <span class="note">Stored in the OS keychain, never in the database.</span>
  {#if error}
    <span class="fail">{error}</span>
  {/if}
</div>

<style>
  /*
    The strip's own labels. `.src-fix` in `app.css` is the mockup's geometry;
    these two are the accessible name and the promise, neither of which the
    mockup drew.
  */
  .l {
    font: 400 11px var(--mono);
    color: var(--muted);
  }

  .note {
    font: 400 11px var(--mono);
    color: var(--faint);
  }

  .fail {
    font: 400 11px var(--mono);
    color: var(--fail);
  }
</style>
