<!--
  A source's page (#830, R-13a.2): prototype-6's `stgSource`.

  - A credential the reader has to fix is said on top, with *Re-enter
    password*. Every *Re-enter password* in the new shell links here.
  - **Connection**: the address and the user are edited and saved through
    `update_source`; the password is re-entered into the keychain through
    `set_source_secret`, which never reads one back; an adapter that takes a
    second credential (Uptime Kuma) has an *Account* row. *Test connection*
    tests what is stored and says what it found.
  - **What it syncs**: the schedule and the pause switch, each saved as it
    changes, through `update_source`.
  - **Sync runs**: the source's last runs.
  - **Maintenance**: *Backfill* (`backfill_source`) and *Remove…*, which asks
    whether to keep what the source synced.

  The user is the config's `username`, the convention every adapter follows
  (`settings.ts`'s `IDENTITY_FIELD`), and the row is drawn only for an
  adapter whose schema declares it. Nothing here names an adapter.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type {
    ConnectionReport,
    CredentialHealth,
    SourceDescriptor,
    SourcePatch,
    SourceSummary,
    SourceSyncStatus,
    SyncRunRow,
  } from "../../ipc/sources";
  import { accountHalfDone, accountOf, ACCOUNT_INCOMPLETE } from "../../sources/account";
  import { connectionLine, connectionNote, failedReport } from "../../sources/connection";
  import { push } from "../../shell/toasts.svelte";
  import type { Layers } from "../layers.svelte";
  import type { SettingsPorts } from "./ports";
  import {
    IDENTITY_FIELD,
    credentialAlert,
    declaresUser,
    intervalOptions,
    intervalWords,
    runRow,
    secretWord,
    sourceLine,
    sourceState,
    userOf,
  } from "./settings";
  import TfDialog from "./TfDialog.svelte";

  let {
    source,
    adapter,
    health,
    status,
    finished,
    now,
    ports,
    layers,
    onhealth,
    onchanged,
    onstarted,
    onstopped,
    onremoved,
  }: {
    source: SourceSummary;
    /** The adapter's descriptor, or `null` until `list_adapters` answers. */
    adapter: SourceDescriptor | null;
    health: CredentialHealth | null;
    status: SourceSyncStatus | null;
    /** Moves on every finished run, so the runs are read again. */
    finished: number;
    now: Date;
    ports: SettingsPorts;
    layers: Layers;
    onhealth: (next: CredentialHealth) => void;
    onchanged: (next: SourceSummary) => void;
    onstarted: () => void;
    onstopped: () => void;
    onremoved: () => void;
  } = $props();

  const name = $derived(source.display_name || source.id);
  const reading = $derived(sourceState(source, health, status, now));
  const alert = $derived(credentialAlert(name, health, status, now));
  const word = $derived(secretWord(source.auth_kind));
  const keychain = /Mac/.test(navigator.userAgent) ? "the macOS keychain" : "the system keychain";
  const showsUser = $derived(adapter !== null && declaresUser(adapter.config_schema));
  const acceptsAccount = $derived(adapter?.accepts_account ?? false);

  // ---- Connection: what is typed, against what is stored ----
  // Read once: the page is keyed on the source, so a list re-read under the
  // reader's typing does not wipe it.
  // svelte-ignore state_referenced_locally
  let address = $state(source.base_url);
  // svelte-ignore state_referenced_locally
  let user = $state(userOf(source.config));
  const patch = $derived.by(() => {
    const next: SourcePatch = {};
    if (address.trim() !== source.base_url) next.base_url = address.trim();
    if (showsUser && user.trim() !== userOf(source.config)) {
      const config = typeof source.config === "object" && source.config !== null ? source.config : {};
      next.config = { ...config, [IDENTITY_FIELD]: user.trim() === "" ? null : user.trim() };
    }
    return next;
  });
  const dirty = $derived(Object.keys(patch).length > 0);
  let saving = $state(false);

  async function update(change: SourcePatch, done: string): Promise<boolean> {
    saving = true;
    try {
      const next = await ports.updateSource(source.id, change);
      onchanged(next);
      push({ text: done });
      return true;
    } catch (cause) {
      push({ text: `Could not save ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    } finally {
      saving = false;
    }
  }

  async function save() {
    if (!dirty || saving) return;
    if (await update(patch, "Saved")) report = null;
  }

  // ---- Test connection: what is stored, never what is typed ----
  let report = $state<ConnectionReport | null>(null);
  let testing = $state(false);
  async function test() {
    testing = true;
    try {
      report = await ports.testSource({
        source_id: source.id,
        adapter_kind: source.adapter_kind,
        base_url: source.base_url,
        auth_kind: source.auth_kind ?? "Pat",
        config: source.config,
        secret: null,
      });
    } catch (cause) {
      report = failedReport(cause);
    } finally {
      testing = false;
    }
  }

  // ---- syncing ----
  async function syncNow() {
    onstarted();
    try {
      await ports.syncNow(source.id);
      push({ text: `Syncing ${name}…` });
    } catch (cause) {
      onstopped();
      push({ text: `${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function backfill() {
    onstarted();
    try {
      await ports.backfillSource(source.id);
      push({ text: `Backfilling ${name}: everything is read again, nothing is lost meanwhile` });
    } catch (cause) {
      onstopped();
      push({ text: `Could not backfill ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  // ---- Sync runs ----
  let runs = $state<SyncRunRow[] | null>(null);
  $effect(() => {
    void finished;
    let dead = false;
    ports
      .listSyncRuns(source.id, 8)
      .then((rows) => {
        if (!dead) runs = rows;
      })
      .catch(() => {
        if (!dead) runs = [];
      });
    return () => {
      dead = true;
    };
  });

  // ---- Re-enter password ----
  let reentering = $state(false);
  let secret = $state("");
  let accountUser = $state("");
  let accountPassword = $state("");
  let signingIn = $state(false);
  let secretError = $state<string | null>(null);
  const half = $derived(accountHalfDone({ username: accountUser, password: accountPassword }));
  const account = $derived(accountOf({ username: accountUser, password: accountPassword }));
  const canSignIn = $derived(!signingIn && !half && (secret !== "" || account !== null));

  function openReenter() {
    secret = "";
    accountUser = "";
    accountPassword = "";
    secretError = null;
    reentering = true;
  }

  async function signIn() {
    if (!canSignIn) return;
    signingIn = true;
    secretError = null;
    try {
      const next = await ports.setSourceSecret(source.id, { value: secret === "" ? null : secret, account });
      secret = "";
      accountUser = "";
      accountPassword = "";
      onhealth(next);
      if (next.state === "unauthorized") {
        secretError = `${name} refused it${next.detail ? `: ${next.detail}` : ""}.`;
        return;
      }
      reentering = false;
      push({ text: `Signed in to ${name}. Syncing now.` });
      onstarted();
      // The sync is the follow-through, not the transaction: a disabled or
      // running source refuses it, and the credential is stored either way.
      await ports.syncNow(source.id).catch(() => onstopped());
    } catch (cause) {
      secretError = ipcErrorMessage(cause);
    } finally {
      signingIn = false;
    }
  }

  // ---- Remove… ----
  let removing = $state(false);
  let purge = $state(false);
  async function remove() {
    removing = false;
    try {
      await ports.deleteSource(source.id, purge);
      push({ text: `Removed ${name}` });
      onremoved();
    } catch (cause) {
      push({ text: `Could not remove ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h tf-stg-srch">
  <div>
    <h1><span class="tf-dot tf-{reading.tone}"></span> {name}</h1>
    <p>{sourceLine(source, adapter?.name ?? source.adapter_kind, status, now)}</p>
  </div>
  <div class="tf-stg-acts tf-stg-flush">
    <button class="tf-btn" disabled={status?.running === true} onclick={() => void syncNow()}>Sync now</button>
  </div>
</header>

{#if alert !== null}
  <div class="tf-stg-alert" role="alert">
    <span class="tf-dot tf-err"></span><span>{alert}</span>
    <button class="tf-btn tf-sm tf-primary" onclick={openReenter}>Re-enter password</button>
  </div>
{/if}

<section class="tf-stg-sec">
  <h2>Connection</h2>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-src-address">Address</label>
    <div class="tf-stg-c"><input id="tf-src-address" type="text" class="tf-stg-in" bind:value={address} /></div>
  </div>
  {#if showsUser}
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-src-user">User</label>
      <div class="tf-stg-c"><input id="tf-src-user" type="text" class="tf-stg-in" bind:value={user} /></div>
    </div>
  {/if}
  {#if source.auth_kind !== null}
    <div class="tf-stg-row">
      <div class="tf-stg-l">{word}</div>
      <div class="tf-stg-c tf-stg-inl">
        <span>{health?.state === "missing_secret" ? "None stored" : `Stored in ${keychain}`}</span>
        <button class="tf-btn tf-sm" onclick={openReenter}>Re-enter…</button>
      </div>
    </div>
  {/if}
  {#if acceptsAccount}
    <div class="tf-stg-row">
      <div class="tf-stg-l">Account<small>A second sign-in, for what the API key cannot do.</small></div>
      <div class="tf-stg-c tf-stg-inl">
        <span>Kept beside the {word.toLowerCase()}, never shown</span>
        <button class="tf-btn tf-sm" onclick={openReenter}>Change…</button>
      </div>
    </div>
  {/if}
  {#if report !== null}
    {@const note = connectionNote(report)}
    <div class="tf-stg-note {report.ok ? '' : 'tf-stg-errt'}" role="status">
      <b>Found when tested:</b> {connectionLine(report)}{note ? `. ${note}` : ""}
    </div>
  {/if}
  <div class="tf-stg-acts">
    <button class="tf-btn tf-sm" disabled={testing} onclick={() => void test()}>Test connection</button>
    <button class="tf-btn tf-sm tf-primary" disabled={!dirty || saving} onclick={() => void save()}>Save</button>
  </div>
</section>

<section class="tf-stg-sec">
  <h2>What it syncs</h2>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-src-every">Every</label>
    <div class="tf-stg-c">
      <select
        id="tf-src-every"
        class="tf-stg-in tf-stg-narrow"
        value={source.sync_interval_secs}
        disabled={saving}
        onchange={(event) => {
          const secs = Number(event.currentTarget.value);
          void update({ sync_interval_secs: secs }, `${name} syncs every ${intervalWords(secs)}`);
        }}
      >
        {#each intervalOptions(source.sync_interval_secs) as secs (secs)}
          <option value={secs}>{intervalWords(secs)}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">Syncing</div>
    <div class="tf-stg-c">
      <button
        class="tf-stg-sw"
        role="switch"
        aria-checked={source.enabled}
        aria-label="Syncing"
        disabled={saving}
        onclick={() =>
          void update(
            { enabled: !source.enabled },
            source.enabled ? "Paused. Nothing syncs until you turn it on." : `${name} syncs again.`,
          )}><i></i></button>
    </div>
  </div>
</section>

<section class="tf-stg-sec">
  <h2>Sync runs</h2>
  {#if runs !== null && runs.length === 0}
    <p class="tf-stg-p">No runs yet. The first one starts on the schedule, or with <b>Sync now</b>.</p>
  {:else if runs !== null}
    <table class="tf-tbl">
      <thead><tr><th>Started</th><th>Took</th><th>Result</th></tr></thead>
      <tbody>
        {#each runs as run (run.id)}
          {@const row = runRow(run, now)}
          <tr>
            <td>{row.started}</td>
            <td>{row.took}</td>
            <td class={row.tone === "err" ? "tf-stg-errt" : ""}>{row.result}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<section class="tf-stg-sec">
  <h2>Maintenance</h2>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      Fetch everything again<small>After a knobas update reads more fields. Takes a few minutes; the app stays usable.</small>
    </div>
    <div class="tf-stg-c">
      <button class="tf-btn tf-sm" disabled={status?.running === true} onclick={() => void backfill()}>Backfill</button>
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">Remove this source<small>Asks whether to keep what it synced.</small></div>
    <div class="tf-stg-c">
      <button
        class="tf-btn tf-sm tf-danger-t"
        onclick={() => {
          purge = false;
          removing = true;
        }}>Remove…</button>
    </div>
  </div>
</section>

{#if reentering}
  <TfDialog title="Re-enter the {name} password" {layers} onclose={() => (reentering = false)}>
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-reenter"
        onsubmit={(event) => {
          event.preventDefault();
          void signIn();
        }}
      >
        <label for="tf-reenter-secret">{word}</label>
        <input id="tf-reenter-secret" type="password" autocomplete="off" placeholder={word} bind:value={secret} />
        {#if acceptsAccount}
          <label for="tf-reenter-user">Account</label>
          <input id="tf-reenter-user" type="text" autocomplete="off" placeholder="Username" bind:value={accountUser} />
          <label for="tf-reenter-pass">Account password</label>
          <input id="tf-reenter-pass" type="password" autocomplete="off" placeholder="Password" bind:value={accountPassword} />
        {/if}
      </form>
      {#if half}<p class="tf-dlg-note tf-stg-errt">{ACCOUNT_INCOMPLETE}</p>{/if}
      {#if secretError !== null}<p class="tf-dlg-note tf-stg-errt" role="alert">{secretError}</p>{/if}
      <p class="tf-dlg-note">
        Stored in {keychain}. Writes waiting for {name} go out once it signs in.{acceptsAccount
          ? " A field left empty keeps what is stored."
          : ""}
      </p>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (reentering = false)}>Cancel</button>
      <button class="tf-btn tf-primary" type="submit" form="tf-reenter" disabled={!canSignIn}>Sign in</button>
    {/snippet}
  </TfDialog>
{/if}

{#if removing}
  <TfDialog title="Remove {name}?" {layers} onclose={() => (removing = false)}>
    {#snippet body()}
      <div class="tf-radio" role="radiogroup" aria-label="What it synced">
        <label>
          <input type="radio" name="tf-rm" checked={!purge} onchange={() => (purge = false)} />
          <span>Keep what it synced<small>Items stay searchable and linked, and stop updating.</small></span>
        </label>
        <label>
          <input type="radio" name="tf-rm" checked={purge} onchange={() => (purge = true)} />
          <span>Delete what it synced<small>Links to those items go too. Your links between other items stay.</small></span>
        </label>
      </div>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (removing = false)}>Cancel</button>
      <button class="tf-btn tf-primary tf-danger" onclick={() => void remove()}>Remove</button>
    {/snippet}
  </TfDialog>
{/if}
