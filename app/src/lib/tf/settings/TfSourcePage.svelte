<!--
  A source's page (#830, R-13a.2): prototype-6's `stgSource`.

  - A credential the reader has to fix is said on top, with *Re-enter
    password* (*Re-enter token* for a token source, #1405). Every *Re-enter …*
    in the new shell links here.
  - **Connection**: the address and the user are edited and saved through
    `update_source`; the password or token is re-entered into the keychain through
    `set_source_secret`, which never reads one back, and its row counts down to
    the expiry the source reported (#1499); an adapter that takes a
    second credential (Uptime Kuma) has an *Account* row. *Test connection*
    tests what is stored and says what it found.
  - **What it syncs**: the schedule and the pause switch, each saved as it
    changes, through `update_source`; then the adapter's own config,
    generated from its `config_schema` (#904): for Jira, projects or a JQL
    query, and the Sprint and Epic Link field ids the sync records (#1021).
    Every config save reads the stored config again and lays only the
    reader's changes over it (`source-config.ts`), so an id the sync recorded
    after the page opened is kept.
  - **Boards and filters** (#878), for an adapter whose schema declares the
    section: which Jira boards and favourite filters come in, and every Jira
    view with *Hide* / *Show* (`TfBoardsAndFilters.svelte`).
  - **The run in flight** (#1384): while one goes, its walk and its count
    under the header, read from the channel `watch_sync` attaches. The page
    attaches when it opens on a running source and when `sync:state` says a
    run started, and stops listening when it leaves.
  - A value the address adds (*Sync WEB too*, #879) is scrolled into view,
    with *WEB added, not saved yet* beside *Save* until it is saved or the
    reader edits the field (#1404).
  - **Sync runs**: the source's last runs. A result wraps anywhere, and an
    unreachable run's transport error sits behind *Details* (#1478).
  - **Maintenance**: *Backfill* (`backfill_source`) and *Remove…*, which asks
    whether to keep what the source synced.

  The user is the config's `username`, the convention every adapter follows
  (`settings.ts`'s `IDENTITY_FIELD`), and the row is drawn only for an
  adapter whose schema declares it. Nothing here names an adapter.
-->
<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type {
    ConnectionReport,
    CredentialHealth,
    SourceDescriptor,
    SourcePatch,
    SourceSummary,
    SourceSyncStatus,
    SyncProgress,
    SyncRunRow,
  } from "../../ipc/sources";
  import { accountHalfDone, accountOf, ACCOUNT_INCOMPLETE } from "../../sources/account";
  import { connectionLine, connectionNote, failedReport } from "../../sources/connection";
  import { DEFAULT_MIN_SYNC_INTERVAL_SECS } from "../../sources/new-source";
  import { declaresSection, validate } from "../../sources/schema-form";
  import { BOARDS_SECTION, savedViewSystem } from "./boards";
  import TfBoardsAndFilters from "./TfBoardsAndFilters.svelte";
  import { push } from "../../shell/toasts.svelte";
  import type { Layers } from "../layers.svelte";
  import { configChanges, formValues, pageFields, withAdded, withChanges } from "./source-config";
  import TfSchemaForm from "./TfSchemaForm.svelte";
  import type { SettingsPorts } from "./ports";
  import { expiryNote } from "../../shell/health.svelte";
  import { storedWhere } from "../../shell/secret-store";
  import {
    BACKFILL_HINT,
    EXPIRY_CLASS,
    IDENTITY_FIELD,
    credentialAlert,
    roleMissLines,
    declaresUser,
    intervalOptions,
    intervalWords,
    reenterWords,
    runProgressWords,
    runRow,
    secretWord,
    sourceLine,
    sourceState,
    userOf,
    type ListAddition,
  } from "./settings";
  import { walkAfter, walkMax, type Walk } from "../../sources/walk";
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
    add,
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
    /** A value to add to a list field, unsaved: a Jira view's *Sync WEB too* (#879). */
    add?: ListAddition | undefined;
  } = $props();

  const name = $derived(source.display_name || source.id);
  const reading = $derived(sourceState(source, health, status, now));
  const alert = $derived(credentialAlert(name, secretWord(source.auth_kind), health, status, now));
  // The roles its records did not fill (#1199): the surfaces that leave it out.
  const missLines = $derived(roleMissLines(source));
  const word = $derived(secretWord(source.auth_kind));
  // The countdown on the credential row (#1499): the date the source reported, never typed.
  const expiry = $derived(expiryNote(health?.secret_expires_at, now));
  // Where the credential went (#1585): the keychain, unless this process keeps
  // credentials in memory (`KNOBAS_SECRET_STORE=memory`, headless QA). Until
  // `app_status` answers, or if it fails, the line names the keychain, the
  // store every other run uses.
  let secretsInMemory = $state(false);
  $effect(() => {
    let dead = false;
    ports
      .appStatus()
      .then((status) => {
        if (!dead) secretsInMemory = status.secrets_in_memory;
      })
      .catch(() => {});
    return () => {
      dead = true;
    };
  });
  const stored = $derived(storedWhere(secretsInMemory));
  const showsUser = $derived(adapter !== null && declaresUser(adapter.config_schema));
  const acceptsAccount = $derived(adapter?.accepts_account ?? false);
  // The adapter's floor (#1188); an adapter this build does not know offers the
  // 60 s every source had before adapters declared one.
  const floor = $derived(adapter?.min_sync_interval_secs ?? DEFAULT_MIN_SYNC_INTERVAL_SECS);

  // ---- Connection: what is typed, against what is stored ----
  // Read once: the page is keyed on the source, so a list re-read under the
  // reader's typing does not wipe it.
  // svelte-ignore state_referenced_locally
  let address = $state(source.base_url);
  // svelte-ignore state_referenced_locally
  let user = $state(userOf(source.config));
  const addressChanged = $derived(address.trim() !== source.base_url);
  const userChanged = $derived(showsUser && user.trim() !== userOf(source.config));
  const dirty = $derived(addressChanged || userChanged);
  let saving = $state(false);

  /**
   * The config as stored now, read again at the moment of saving: the sync
   * records field ids into it (#889), and a copy read when the page opened
   * would blank them (#1021). `update_source` replaces it whole.
   */
  async function storedConfig(): Promise<unknown> {
    const rows = await ports.listSources();
    return rows.find((row) => row.id === source.id)?.config ?? source.config;
  }

  async function update(change: SourcePatch | (() => Promise<SourcePatch>), done: string): Promise<SourceSummary | null> {
    saving = true;
    try {
      const next = await ports.updateSource(source.id, typeof change === "function" ? await change() : change);
      onchanged(next);
      push({ text: done });
      return next;
    } catch (cause) {
      push({ text: `Could not save ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return null;
    } finally {
      saving = false;
    }
  }

  async function save() {
    if (!dirty || saving) return;
    const saved = await update(async () => {
      const next: SourcePatch = {};
      if (addressChanged) next.base_url = address.trim();
      if (userChanged) {
        next.config = withChanges(await storedConfig(), {
          [IDENTITY_FIELD]: user.trim() === "" ? null : user.trim(),
        });
      }
      return next;
    }, "Saved");
    if (saved !== null) report = null;
  }

  // ---- What it syncs: the adapter's own config, generated (#904) ----
  const config = $derived(adapter === null ? { fields: [], error: null } : pageFields(adapter.config_schema));
  /** *Boards and filters* (#878): drawn for an adapter whose schema declares it. */
  const boardsSection = $derived(adapter !== null && declaresSection(adapter.config_schema, BOARDS_SECTION));
  /** The ticket system its views come from, which the section names (#2032). */
  const boardsSystem = $derived(adapter === null ? "" : savedViewSystem(adapter));
  let configValues = $state<Record<string, unknown>>({});
  /** The config the form was read from, validated as the form validates it. */
  let configLoaded = $state<Record<string, unknown>>({});
  const configEdited = $derived(validate(config.fields, configValues));
  const configChanged = $derived(
    configChanges(
      configLoaded,
      configEdited.config,
      config.fields.map((field) => field.key),
    ),
  );
  const configDirty = $derived(Object.keys(configChanged).length > 0);
  const configValid = $derived(Object.keys(configEdited.errors).length === 0);

  // The form follows what is stored (an id the sync recorded shows once the
  // list is read again) until the reader changes something; then it is theirs.
  $effect(() => {
    const fields = config.fields;
    const stored = source.config;
    untrack(() => {
      if (!configDirty) readForm(fields, stored);
    });
  });

  /**
   * The value the address added to the form (#1404), and a snapshot of the
   * field as it left it: the note beside *Save* stands while the field still reads so.
   */
  let added = $state<{ value: string; field: string; snapshot: string } | null>(null);
  /** The config's *Save* and its note, to scroll to. */
  let configActs = $state<HTMLElement>();
  const addedNote = $derived(
    added !== null && snapshotOf(configValues[added.field]) === added.snapshot ? `${added.value} added, not saved yet` : null,
  );

  function snapshotOf(value: unknown): string {
    return JSON.stringify(value ?? null);
  }

  /** Put a stored config in the form, as both what it shows and what it was read from. */
  function readForm(fields: typeof config.fields, stored: unknown) {
    configValues = formValues(fields, stored);
    configLoaded = validate(fields, configValues).config;
    // A value the address adds (#879) is the reader's edit, offered unsaved.
    const read = configValues;
    configValues = withAdded(fields, read, add);
    if (add && configValues !== read) {
      added = { ...add, snapshot: snapshotOf(configValues[add.field]) };
      // In view, so the reader sees it went in (#1404): *Save* and its note at
      // the bottom of the window, then the field, if that left it out of sight.
      // jsdom draws no layout and has no `scrollIntoView`.
      const id = `tf-src-cfg-${add.field}`;
      void tick().then(() => {
        configActs?.scrollIntoView?.({ block: "end" });
        document.getElementById(id)?.scrollIntoView?.({ block: "nearest" });
      });
    } else {
      added = null;
    }
  }

  async function saveConfig() {
    if (!configDirty || !configValid || saving) return;
    const changes = { ...configChanged };
    const saved = await update(async () => ({ config: withChanges(await storedConfig(), changes) }), "Saved");
    if (saved !== null) readForm(config.fields, saved.config);
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

  // ---- The run in flight (#1384) ----
  /** The watched run's latest count, clock and walk, or `null` when none is drawn. */
  let live = $state<{ items: number; elapsedMs: number; walk: Walk | null } | null>(null);
  const walkOf = $derived(walkMax(live?.walk ?? null));
  /** Moves when a watched run ends, so the runs are read again. */
  let ended = $state(0);

  /**
   * The run being watched, if any: what `watch_sync` answered (`null` while it
   * has not), and the signal that drops its channel. Not state: nothing draws it.
   */
  let watched: { run: number | null; stop: AbortController } | null = null;

  function unwatch() {
    watched?.stop.abort();
    watched = null;
  }

  async function watch() {
    unwatch();
    /** The run the status named when this ask went out. */
    const asked = runningId;
    const mine = { run: null as number | null, stop: new AbortController() };
    watched = mine;
    const hear = (message: SyncProgress) => {
      if (mine.stop.signal.aborted) return;
      live = { items: message.items, elapsedMs: message.elapsed_ms, walk: walkAfter(live?.walk ?? null, message) };
      if (message.phase === "finished" || message.phase === "failed") {
        live = null;
        ended += 1;
        if (watched === mine) unwatch();
      }
    };
    try {
      mine.run = await ports.watchSync(source.id, hear, mine.stop.signal);
    } catch {
      // The page still has `sync:state` and the disabled *Sync now*; a run it
      // cannot watch is a run it draws as before.
      mine.run = null;
    }
    if (watched !== mine) return;
    // The status named another run while the ask was out (a local *Sync now*
    // asks before its trigger lands, and the event with the id can beat the
    // empty answer): the effect skipped it, so ask again for that run. Only
    // for a run this ask did not know, so a status that lags an ended run
    // does not loop.
    const now = runningId;
    if (typeof now === "number" && now !== asked && now !== mine.run) {
      void watch();
      return;
    }
    // Nothing in flight: let the next `sync:state` attach afresh.
    if (mine.run === null) unwatch();
  }

  /** The run `sync:state` says is going: its id, `null` before one is known, `undefined` when idle. */
  const runningId = $derived(status?.running === true ? status.run_id : undefined);
  $effect(() => {
    const run = runningId;
    untrack(() => {
      if (run === undefined) {
        // Idle again: whatever the channel still owes, the run is over.
        unwatch();
        live = null;
        return;
      }
      // Already watching it, or already asking: the second status of one run
      // (a local start, then its event) attaches once.
      if (watched !== null && (run === null || watched.run === null || watched.run === run)) return;
      void watch();
    });
  });
  onDestroy(unwatch);

  // ---- Sync runs ----
  let runs = $state<SyncRunRow[] | null>(null);
  $effect(() => {
    void finished;
    void ended;
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

  // ---- Re-enter the credential ----
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

{#if live !== null}
  <div class="tf-stg-run" role="status">
    <!--
      Two elements rather than one with an `undefined` value, as the wizard
      draws it: a `<progress>` once given a value stays determinate.
    -->
    {#if walkOf === undefined}
      <progress></progress>
    {:else}
      <progress max={walkOf} value={live.walk?.walked}></progress>
    {/if}
    <p>{runProgressWords(live.items, live.elapsedMs, live.walk)}</p>
  </div>
{/if}

{#if alert !== null}
  <div class="tf-stg-alert" role="alert">
    <span class="tf-dot tf-err"></span><span>{alert}</span>
    <button class="tf-btn tf-sm tf-primary" onclick={openReenter}>{reenterWords(source.auth_kind)}</button>
  </div>
{/if}
{#each missLines as line (line)}
  <div class="tf-stg-alert" role="status">
    <span class="tf-dot tf-warn"></span><span>{line}</span>
  </div>
{/each}

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
        <span>{health?.state === "missing_secret" ? "None stored" : stored}</span>
        {#if expiry !== null}
          <span class="tf-stg-exp {EXPIRY_CLASS[expiry.tone]}">{expiry.text}</span>
        {/if}
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
        class="tf-stg-in"
        value={source.sync_interval_secs}
        disabled={saving}
        onchange={async (event) => {
          const select = event.currentTarget;
          const secs = Number(select.value);
          // A schedule the backend refused goes back to the stored one.
          if (!(await update({ sync_interval_secs: secs }, `${name} syncs every ${intervalWords(secs)}`))) {
            select.value = String(source.sync_interval_secs);
          }
        }}
      >
        {#each intervalOptions(source.sync_interval_secs, floor) as secs (secs)}
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
            // A write is sent the moment it is queued, paused or not (#1438): the pause holds the sync, not what the reader sends.
            source.enabled ? "Paused. Nothing syncs until you turn it on; writes you make still go." : `${name} syncs again.`,
          )}><i></i></button>
    </div>
  </div>
  {#if config.error !== null}
    <p class="tf-stg-note tf-stg-errt">{config.error}</p>
  {:else if config.fields.length > 0}
    <TfSchemaForm
      fields={config.fields}
      values={configValues}
      errors={configEdited.errors}
      disabled={saving}
      idPrefix="tf-src-cfg"
      layout="rows"
    />
    <div class="tf-stg-acts" bind:this={configActs}>
      <button class="tf-btn tf-sm tf-primary" disabled={!configDirty || !configValid || saving} onclick={() => void saveConfig()}
        >Save</button
      >
      {#if addedNote !== null}<span class="tf-stg-pend" role="status">{addedNote}</span>{/if}
    </div>
  {/if}
</section>

{#if boardsSection}
  <TfBoardsAndFilters
    {source}
    system={boardsSystem}
    {ports}
    {saving}
    {finished}
    onsave={(changes, done) => update(async () => ({ config: withChanges(await storedConfig(), changes) }), done)}
  />
{/if}

<section class="tf-stg-sec">
  <h2>Sync runs</h2>
  {#if runs !== null && runs.length === 0}
    <p class="tf-stg-p">No runs yet. The first one starts on the schedule, or with <b>Sync now</b>.</p>
  {:else if runs !== null}
    <table class="tf-tbl tf-stg-runs">
      <thead><tr><th>Started</th><th>Took</th><th>Result</th></tr></thead>
      <tbody>
        {#each runs as run (run.id)}
          {@const row = runRow(run, source, now)}
          <tr>
            <td>{row.started}</td>
            <td>{row.took}</td>
            <td class="tf-stg-res {row.tone === 'err' ? 'tf-stg-errt' : ''}"
              >{row.result}{#if row.detail !== null}<details class="tf-stg-det">
                  <summary>Details</summary>
                  <code>{row.detail}</code>
                </details>{/if}</td
            >
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
      Fetch everything again<small>{BACKFILL_HINT}</small>
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
  <TfDialog title="Re-enter the {name} {word.toLowerCase()}" {layers} onclose={() => (reentering = false)}>
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
        {stored}. Writes waiting for {name} go out once it signs in.{acceptsAccount
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
