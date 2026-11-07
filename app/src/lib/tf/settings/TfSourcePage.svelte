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
  - **Schedule**: *Every* and the pause switch, each saved as it changes, through
    `update_source`, in a group of their own (F-627): *Save sync settings* never covers them.
  - **What it syncs**: the adapter's own config, generated from its
    `config_schema` (#904), behind that *Save*: for Jira, projects or a JQL
    query, and the Sprint and Epic Link field ids the sync records (#1021).
    Every config save reads the stored config again and lays only the
    reader's changes over it (`source-config.ts`), so an id the sync recorded
    after the page opened is kept.
  - **Docs** (#2440), for a source that claims the wiki role: *Run
    embedded HTML*, saved as it changes (`set_run_embedded_html`). Off, the
    reader draws the source's embeds as placeholders.
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
  - **A draft** (F-617, `source-drafts.svelte.ts`): what is typed in Connection and
    in the adapter's config stays per source for the session, across a leave and a
    return, with *Not saved* and *Reset* beside that section's *Save*. *Every* and
    *Syncing* save as they change, so they are never part of one.

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
  import { connectionLine, connectionNote, failedReport } from "../../sources/connection";
  import { DEFAULT_MIN_SYNC_INTERVAL_SECS } from "../../sources/new-source";
  import { declaresSection, validate } from "../../sources/schema-form";
  import { BOARDS_SECTION, savedViewSystem } from "./boards";
  import TfBoardsAndFilters from "./TfBoardsAndFilters.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { createSourceSyncs } from "../role-sync.svelte";
  import type { Layers } from "../layers.svelte";
  import { sourceDrafts } from "./source-drafts.svelte";
  import { configChanges, formValues, pageFields, withAdded, withChanges } from "./source-config";
  import TfSchemaForm from "./TfSchemaForm.svelte";
  import { WIKI } from "../roles.svelte";
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
    credentialFix,
    runProgressWords,
    runRow,
    secretWord,
    sourceLine,
    sourceHash,
    sourceState,
    userOf,
    type ListAddition,
  } from "./settings";
  import { replaceAddress } from "../opener";
  import { walkAfter, walkMax, type Walk } from "../../sources/walk";
  import TfDialog from "./TfDialog.svelte";
  import TfSecretPrompt from "./TfSecretPrompt.svelte";

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
    reenter = false,
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
    /** Open the credential's prompt on arrival: a raw `…/reenter` address. A press on one opens over the page you are on (`secret-prompt.svelte.ts`). */
    reenter?: boolean;
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

  // *Run embedded HTML* (#2440), for a wiki: `null` until it is read.
  const wiki = $derived(source.roles.some((claim) => claim.role === WIKI));
  let embedsOn = $state<boolean | null>(null);
  $effect(() => {
    if (!wiki) return;
    let dead = false;
    ports
      .runEmbeddedHtml(source.id)
      .then((on) => {
        if (!dead) embedsOn = on;
      })
      .catch((cause: unknown) => {
        // The switch stays disabled: say why, as every other failure here does.
        if (!dead) push({ text: `Could not read Run embedded HTML for ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      });
    return () => {
      dead = true;
    };
  });
  async function setEmbeds(on: boolean): Promise<void> {
    saving = true;
    try {
      embedsOn = await ports.setRunEmbeddedHtml(source.id, on);
      push({ text: embedsOn ? `${name} pages run their embedded HTML.` : `${name} pages draw their embedded HTML as a placeholder.`, subject: `embeds:${source.id}` });
    } catch (cause) {
      push({ text: `Could not save ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }
  const showsUser = $derived(adapter !== null && declaresUser(adapter.config_schema));
  const acceptsAccount = $derived(adapter?.accepts_account ?? false);
  // The adapter's floor (#1188); an adapter this build does not know offers the
  // 60 s every source had before adapters declared one.
  const floor = $derived(adapter?.min_sync_interval_secs ?? DEFAULT_MIN_SYNC_INTERVAL_SECS);

  // ---- Connection: what is typed, against what is stored ----
  // Read once: the page is keyed on the source, so a list re-read under the
  // reader's typing does not wipe it.
  // What was typed here before and not saved comes back as it was left (F-617).
  // svelte-ignore state_referenced_locally
  const kept = sourceDrafts.get(source.id)?.connection;
  // svelte-ignore state_referenced_locally
  let address = $state(kept?.address ?? source.base_url);
  // svelte-ignore state_referenced_locally
  let user = $state(kept?.user ?? userOf(source.config));
  const addressChanged = $derived(address.trim() !== source.base_url);
  const userChanged = $derived(showsUser && user.trim() !== userOf(source.config));
  const dirty = $derived(addressChanged || userChanged);
  let saving = $state(false);

  // Written whatever the adapter says: until `list_adapters` answers `userChanged` is false, and that isn't a reset.
  $effect(() => {
    const edited = addressChanged || user.trim() !== userOf(source.config);
    const draft = edited ? { address, user } : null;
    untrack(() => sourceDrafts.put(source.id, "connection", draft));
  });

  /** *Reset*: back to what is stored, with *Undo*, since what was typed may be long. */
  function resetConnection() {
    const was = { address, user };
    address = source.base_url;
    user = userOf(source.config);
    report = null;
    push({
      text: `${name}'s connection is back to what is saved`,
      action: {
        label: "Undo",
        run: () => {
          // Into the store too: the toast outlives the page if the reader has left.
          sourceDrafts.put(source.id, "connection", was);
          address = was.address;
          user = was.user;
        },
      },
    });
    void tick().then(() => document.getElementById("tf-src-address")?.focus());
  }

  /**
   * The config as stored now, read again at the moment of saving: the sync
   * records field ids into it (#889), and a copy read when the page opened
   * would blank them (#1021). `update_source` replaces it whole.
   */
  async function storedConfig(): Promise<unknown> {
    const rows = await ports.listSources();
    return rows.find((row) => row.id === source.id)?.config ?? source.config;
  }

  async function update(change: SourcePatch | (() => Promise<SourcePatch>), done: string, subject?: string): Promise<SourceSummary | null> {
    saving = true;
    try {
      const next = await ports.updateSource(source.id, typeof change === "function" ? await change() : change);
      onchanged(next);
      push(subject === undefined ? { text: done } : { text: done, subject });
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
    }, `Saved ${name}'s connection`);
    if (saved !== null) report = null;
  }

  /** `Enter` in a Connection field saves it, as `Enter` in a dialog's field sends. */
  function saveOnEnter(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void save();
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

  /** Put a stored config in the form, as both what it shows and what it was read from, with what the reader left unsaved over it. */
  function readForm(fields: typeof config.fields, stored: unknown) {
    const values = formValues(fields, stored);
    configLoaded = validate(fields, values).config;
    // A value the address adds (#879) is the reader's edit, offered unsaved.
    const read = { ...values, ...sourceDrafts.get(source.id)?.config };
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

  // What the reader changed, by field, so a value the sync records meanwhile still shows (F-617, #1021). Not while no
  // field is drawn: until the adapter answers nothing reads as changed, and that isn't a reset either.
  $effect(() => {
    if (config.fields.length === 0) return;
    const changed = Object.keys(configChanged);
    const draft = changed.length === 0 ? null : Object.fromEntries(changed.map((key) => [key, $state.snapshot(configValues[key])]));
    untrack(() => sourceDrafts.put(source.id, "config", draft));
  });

  async function saveConfig() {
    if (!configDirty || !configValid || saving) return;
    const changes = { ...configChanged };
    const saved = await update(async () => ({ config: withChanges(await storedConfig(), changes) }), `Saved what ${name} syncs`);
    if (saved !== null) {
      sourceDrafts.put(source.id, "config", null);
      readForm(config.fields, saved.config);
    }
  }

  /** *Reset*: the form back to what is stored, with *Undo*. */
  function resetConfig() {
    const was = $state.snapshot(sourceDrafts.get(source.id)?.config) ?? {};
    const read = formValues(config.fields, source.config);
    sourceDrafts.put(source.id, "config", null);
    configLoaded = validate(config.fields, read).config;
    configValues = read;
    added = null;
    push({
      text: `What ${name} syncs is back to what is saved`,
      action: {
        label: "Undo",
        run: () => {
          sourceDrafts.put(source.id, "config", was);
          configValues = { ...configValues, ...was };
        },
      },
    });
    const first = config.fields[0];
    if (first) void tick().then(() => document.getElementById(`tf-src-cfg-${first.key}`)?.focus());
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
  /** Ends as every *Sync now* does, in one toast that takes *Syncing…*'s place, even if the reader has left the page (F-652). */
  async function syncNow() {
    if (status?.running) return;
    onstarted();
    const run = createSourceSyncs({
      syncStatus: ports.syncStatus,
      listen: ports.listen,
      syncNow: async (id) => {
        const started = await ports.syncNow(id);
        push({ text: `Syncing ${name}…`, subject: `sync:${id}` });
        return started;
      },
    }).of(source.id);
    await run.start(name);
    if (run.failed !== null) {
      onstopped();
      push({ text: `Could not sync ${name}: ${run.failed}`, tone: "err" });
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

  /** Nothing is stored yet, so nothing here speaks of entering it again. */
  const noneStored = $derived(health?.state === "missing_secret");

  /** The prompt shuts; opened by its address (a raw `…/reenter`), the address goes back to the page's, so a reload does not ask again. */
  function closeReenter() {
    reentering = false;
    if (reenter) replaceAddress(sourceHash(source.id));
  }

  function openReenter() {
    reentering = true;
  }
  $effect(() => {
    if (reenter) untrack(openReenter);
  });

  // ---- Remove… ----
  let removing = $state(false);
  let purge = $state(false);
  async function remove() {
    removing = false;
    try {
      await ports.deleteSource(source.id, purge);
      sourceDrafts.forget(source.id);
      push({ text: `Removed ${name}` });
      onremoved();
    } catch (cause) {
      push({ text: `Could not remove ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h tf-stg-hline">
  <div>
    <h1><span class="tf-dot tf-{reading.tone}"></span> {name}</h1>
    <p>{sourceLine(source, adapter?.name ?? source.adapter_kind, status, now)}</p>
  </div>
  <div class="tf-acts">
    <!-- `aria-disabled`, as a view's *Sync now* and *Sync all* are: a press from the keyboard stays on the button while the run goes. -->
    <button class="tf-btn" aria-disabled={status?.running ? "true" : undefined} title={status?.running ? `${name} is syncing` : undefined} onclick={() => void syncNow()}>Sync now</button>
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
    <button class="tf-btn tf-sm tf-primary" onclick={openReenter}>{credentialFix(source.auth_kind, health?.state)}…</button>
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
    <div class="tf-stg-c"><input id="tf-src-address" type="text" class="tf-stg-in" bind:value={address} onkeydown={saveOnEnter} /></div>
  </div>
  {#if showsUser}
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-src-user">User</label>
      <div class="tf-stg-c"><input id="tf-src-user" type="text" class="tf-stg-in" bind:value={user} onkeydown={saveOnEnter} /></div>
    </div>
  {/if}
  {#if source.auth_kind !== null}
    <div class="tf-stg-row">
      <div class="tf-stg-l">{word}</div>
      <div class="tf-stg-c tf-stg-inl">
        <span>{noneStored ? "None stored" : stored}</span>
        {#if expiry !== null}
          <span class="tf-stg-exp {EXPIRY_CLASS[expiry.tone]}">{expiry.text}</span>
        {/if}
        <button class="tf-btn tf-sm" onclick={openReenter}>{noneStored ? "Enter…" : "Re-enter…"}</button>
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
  <div class="tf-stg-acts">
    <button class="tf-btn tf-sm" disabled={testing} onclick={() => void test()}>Test connection</button>
    <button class="tf-btn tf-sm tf-primary" disabled={!dirty || saving} onclick={() => void save()}>Save connection</button>
    {#if dirty}
      <span class="tf-stg-pend" role="status">Not saved</span>
      <button class="tf-btn tf-sm tf-ghost" aria-label="Reset the connection" onclick={resetConnection}>Reset</button>
    {/if}
  </div>
  <!-- Under the buttons, so what a test found doesn't move them from under the pointer that pressed one (F-658). -->
  {#if report !== null}
    {@const note = connectionNote(report)}
    <div class="tf-stg-note {report.ok ? '' : 'tf-stg-errt'}" role="status">
      <b>Found when tested:</b> {connectionLine(report)}{note ? `. ${note}` : ""}
    </div>
  {/if}
</section>

<section class="tf-stg-sec">
  <h2>Schedule</h2>
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
          if (!(await update({ sync_interval_secs: secs }, `${name} syncs every ${intervalWords(secs)}`, `every:${source.id}`))) {
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
            `syncing:${source.id}`,
          )}><i></i></button>
    </div>
  </div>
</section>

{#if config.error !== null || config.fields.length > 0}
<section class="tf-stg-sec">
  <h2>What it syncs</h2>
  {#if config.error !== null}
    <p class="tf-stg-note tf-stg-errt">{config.error}</p>
  {:else}
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
        >Save sync settings</button
      >
      {#if configDirty}
        <span class="tf-stg-pend" role="status">{addedNote ?? "Not saved"}</span>
        <button class="tf-btn tf-sm tf-ghost" aria-label="Reset the sync settings" onclick={resetConfig}>Reset</button>
      {/if}
    </div>
  {/if}
</section>
{/if}

{#if wiki}
  <section class="tf-stg-sec">
    <h2>Docs</h2>
    <div class="tf-stg-row">
      <div class="tf-stg-l">Run embedded HTML</div>
      <div class="tf-stg-c">
        <button
          class="tf-stg-sw"
          role="switch"
          aria-checked={embedsOn ?? true}
          aria-label="Run embedded HTML"
          disabled={saving || embedsOn === null}
          onclick={() => void setEmbeds(!(embedsOn ?? true))}><i></i></button
        >
      </div>
    </div>
    <p class="tf-stg-note">HTML a page embeds runs in a sandboxed frame that cannot reach knobas. Off, it is a placeholder with Open in {name}.</p>
  </section>
{/if}

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
        class="tf-btn tf-sm tf-danger"
        onclick={() => {
          purge = false;
          removing = true;
        }}>Remove…</button>
    </div>
  </div>
</section>

{#if reentering}
  <TfSecretPrompt {source} {health} {acceptsAccount} {stored} {ports} {layers} {onhealth} onclose={closeReenter} {onstarted} {onstopped} />
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
