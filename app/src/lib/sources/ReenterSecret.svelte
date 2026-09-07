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

  ## The optional account (#452)

  A source whose adapter declares `accepts_account` — Uptime Kuma, and nothing
  else — can carry a second credential in the same keychain item, and this
  strip is where one is added. **Both fields keep what is stored when left
  empty**, which is the rule `SecretInput` states: a reader adding an account
  to a Kuma cannot retype an API key Kuma showed them once, and a reader
  replacing an expired key must not lose the account beside it. That is why the
  key field's *Save* is enabled with nothing typed in it once an account is —
  and why this strip cannot *remove* an account: there is no credential it may
  read back, so there is nothing it could show a reader to remove.
-->
<script lang="ts">
  import { setSourceSecret, syncNow } from "../ipc/sources";
  import { ipcErrorMessage } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";

  let {
    sourceId,
    displayName,
    acceptsAccount = false,
    onhealth,
    oncancel,
  }: {
    sourceId: string;
    displayName: string;
    /**
     * Whether this source's adapter can use a second credential (#452) —
     * `SourceDescriptor.accepts_account`, passed in by the row rather than
     * looked up here, so this strip knows no adapter kinds.
     */
    acceptsAccount?: boolean;
    /** The health the backend answered with, so the row redraws from it. */
    onhealth: (health: CredentialHealth) => void;
    oncancel: () => void;
  } = $props();

  let value = $state("");
  let accountUser = $state("");
  let accountPassword = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let field = $state<HTMLInputElement | null>(null);

  $effect(() => {
    // The strip appears *because* the reader asked to fix this source, so the
    // caret belongs in it. Without this they would have to find a 220px input
    // that just appeared under a row.
    field?.focus();
  });

  /**
   * Both halves of the account, or `null` for a submission that is not about
   * one. Half an account is refused by {@link saveable} rather than stored.
   */
  function account() {
    return accountUser !== "" && accountPassword !== ""
      ? { username: accountUser, password: accountPassword }
      : null;
  }

  const accountHalfDone = $derived(
    (accountUser === "") !== (accountPassword === ""),
  );

  /**
   * Whether there is anything to save.
   *
   * **Either half is enough**, which is the whole of "adding the account
   * without re-entering the key": a submission with only an account keeps the
   * stored secret, and one with only a secret keeps the stored account.
   */
  const saveable = $derived(
    !busy && !accountHalfDone && (value !== "" || account() !== null),
  );

  async function save() {
    if (!saveable) return;
    busy = true;
    error = null;
    try {
      const health = await setSourceSecret(sourceId, {
        // `null` and not `""`: absent means keep, and an empty string is a
        // credential of no characters.
        value: value === "" ? null : value,
        account: account(),
      });
      // Cleared before anything else can await: the values have served their
      // one purpose and there is no reason for them to survive the round trip.
      value = "";
      accountUser = "";
      accountPassword = "";
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
  {#if acceptsAccount}
    <input
      id="reenter-{sourceId}-user"
      class="inp"
      type="text"
      autocomplete="off"
      placeholder="Account username"
      bind:value={accountUser}
      disabled={busy}
      onkeydown={(event) => {
        if (event.key === "Enter") void save();
      }}
    />
    <input
      id="reenter-{sourceId}-password"
      class="inp"
      type="password"
      autocomplete="off"
      placeholder="Account password"
      bind:value={accountPassword}
      disabled={busy}
      onkeydown={(event) => {
        if (event.key === "Enter") void save();
      }}
    />
  {/if}
  <button class="btn pri sm" disabled={!saveable} onclick={() => void save()}>
    Save and retry sync
  </button>
  <button class="btn sm" disabled={busy} onclick={oncancel}>Cancel</button>
  <span class="note">
    {#if accountHalfDone}
      An account needs both a username and a password.
    {:else if acceptsAccount}
      Stored in the OS keychain, never in the database. Anything left empty keeps what
      is stored.
    {:else}
      Stored in the OS keychain, never in the database.
    {/if}
  </span>
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
