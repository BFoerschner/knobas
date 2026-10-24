<!--
  *Add a source* in the ticket-first look (#903): prototype-6's
  `stgAddSource`, in the `TfDialog` frame.

  *What is it?* is one radio per adapter, described by what it holds. Then the
  address, the adapter's config form, **generated** from its `config_schema`
  (spec §3a, `TfSchemaForm`, no per-adapter branch), and the credential. *Test
  connection* tests the draft and fills what it found into empty fields: the
  account into the user (#82), the ids the source reports into theirs (#297).
  *Add source* sends one `add_source` with the secret and the row, as the
  rooms UI's `AddSource.svelte` did (contract §3: the backend orders the two
  writes), and only after a passing test of the draft as it stands.

  Settings › Sources mounts it, and so does the first-run wizard's *Connect a
  real system* (#1400), with the same ports: there is one add-source flow.

  *Install an adapter* (#1202, #1209, ADR-0028) takes a GitHub repository
  URL (its latest release), a manifest URL, or the path of a local
  `knobas-adapter.json`. *Install…* runs every check and writes nothing
  (#2031): a refusal is shown with its reason and offers no install, and a
  release that passes opens *Install <name>?*, which names its version, the
  roles it claims, every host it may reach (#1203: a fixed host as written, a
  template by the setting it is read from, since no source exists yet) and
  the limits it asks for. Only its *Install* installs, held to the artifact
  that was shown; *Cancel* leaves nothing behind. An installed adapter joins
  the radios, chosen, so its generated form is there to fill.

  The rooms dialog's five steps are one page here, as prototype-6 draws it.
  Its *Schedule* step is not drawn: a new source syncs every 15 minutes, its
  default, and its page changes that.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { AdapterPreview, AuthMethod, ConnectionReport, SourceDescriptor, SourceSummary } from "../../ipc/sources";
  import { ACCOUNT_INCOMPLETE, accountHalfDone, accountOf } from "../../sources/account";
  import { connectionLine, connectionNote, failedReport, noteAfterFill } from "../../sources/connection";
  import { defaultValues, validate } from "../../sources/schema-form";
  import type { Layers } from "../layers.svelte";
  import type { SettingsPorts } from "./ports";
  import { AUTH_LABEL, defaultIntervalFor, fillFound, SOURCE_ID_PATTERN } from "../../sources/new-source";
  import { pageFields } from "./source-config";
  import { reach } from "./adapter-preview";
  import TfAdapterFacts from "./TfAdapterFacts.svelte";
  import TfDialog from "./TfDialog.svelte";
  import TfSchemaForm from "./TfSchemaForm.svelte";

  let {
    ports,
    layers,
    onclose,
    onsaved,
  }: {
    ports: Pick<SettingsPorts, "listAdapters" | "testSource" | "addSource" | "previewInstallAdapter" | "installAdapter">;
    layers: Layers;
    onclose: () => void;
    onsaved: (source: SourceSummary) => void;
  } = $props();

  let adapters = $state<SourceDescriptor[] | null>(null);
  let adaptersError = $state<string | null>(null);
  let chosen = $state<SourceDescriptor | null>(null);
  let id = $state("");
  let displayName = $state("");
  let baseUrl = $state("");
  let authKind = $state<AuthMethod | null>(null);
  let secret = $state("");
  let accountUser = $state("");
  let accountPassword = $state("");
  let configValues = $state<Record<string, unknown>>({});
  let report = $state<ConnectionReport | null>(null);
  /** What the last passing test wrote into the form, by field key (#1479). */
  let filled = $state<Record<string, string>>({});
  /** The draft the passing test was of; *Add source* waits for it to match. */
  let tested = $state<string | null>(null);
  let testing = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let manifest = $state("");
  /** What the install row is doing: checking a release, or installing the confirmed one. */
  let installing = $state<"checking" | "installing" | null>(null);
  /** The checked release *Install <name>?* asks about, and the input it was read from. */
  let confirming = $state<{ input: string; preview: AdapterPreview } | null>(null);
  /** The last install's answer: what was installed, or why it was refused. */
  let installNote = $state<{ ok: boolean; text: string } | null>(null);

  $effect(() => {
    let dead = false;
    ports
      .listAdapters()
      .then((rows) => {
        if (dead) return;
        adapters = rows;
        // The first one is chosen, as prototype-6 opens, so the form is there to fill.
        if (chosen === null && rows[0]) choose(rows[0]);
      })
      .catch((cause) => {
        if (!dead) adaptersError = ipcErrorMessage(cause);
      });
    return () => {
      dead = true;
    };
  });

  const config = $derived(chosen === null ? { fields: [], error: null } : pageFields(chosen.config_schema, { keepUser: true }));
  const validated = $derived(validate(config.fields, configValues));
  const accountHalf = $derived(accountHalfDone({ username: accountUser, password: accountPassword }));
  const account = $derived(accountOf({ username: accountUser, password: accountPassword }));
  const secretWord = $derived(authKind === "UserPassword" ? "Password" : "Token");

  const idError = $derived.by(() => {
    if (id === "") return "An id is required.";
    if (!SOURCE_ID_PATTERN.test(id)) return "Lower-case letters, digits and hyphens, starting with a letter.";
    return null;
  });

  /** The draft as a test sends it, so a passing test is tied to what was typed. */
  const draft = $derived(
    JSON.stringify([chosen?.adapter_kind, baseUrl.trim(), authKind, validated.config, secret, account]),
  );
  const ready = $derived(
    chosen !== null &&
      authKind !== null &&
      baseUrl.trim() !== "" &&
      secret !== "" &&
      !accountHalf &&
      config.error === null &&
      Object.keys(validated.errors).length === 0,
  );
  const canAdd = $derived(ready && idError === null && report?.ok === true && tested === draft && !saving);

  /** What an adapter holds, as prototype-6 describes each one. */
  function kindsHeld(adapter: SourceDescriptor): string {
    return adapter.entity_kinds.map((kind, index) => (index === 0 ? kind.plural : kind.plural.toLowerCase())).join(", ");
  }

  function choose(adapter: SourceDescriptor) {
    chosen = adapter;
    id = adapter.adapter_kind;
    displayName = adapter.name;
    // The config belongs to the adapter that declared it; nothing carries over.
    configValues = defaultValues(config.fields);
    authKind = adapter.auth_methods[0] ?? null;
    report = null;
    tested = null;
  }

  /** A refusal's note: the backend's reason opens with knobas' own word, so only that is capitalised. */
  function refused(cause: unknown) {
    const reason = ipcErrorMessage(cause);
    installNote = { ok: false, text: reason.charAt(0).toUpperCase() + reason.slice(1) };
  }

  /** *Install…*: every check, nothing written; a release that passes is asked about. */
  async function check() {
    const path = manifest.trim();
    if (path === "" || installing !== null) return;
    installing = "checking";
    installNote = null;
    try {
      confirming = { input: path, preview: await ports.previewInstallAdapter(path) };
    } catch (cause) {
      refused(cause);
    } finally {
      installing = null;
    }
  }

  /** *Install* in *Install <name>?*: the release that was shown, and no other. */
  async function install() {
    const asked = confirming;
    if (asked === null || installing !== null) return;
    confirming = null;
    installing = "installing";
    try {
      const answer = await ports.installAdapter(asked.input, asked.preview.artifact_sha256);
      const rows = await ports.listAdapters();
      adapters = rows;
      const row = rows.find((adapter) => adapter.adapter_kind === answer.adapter_kind);
      if (row) choose(row);
      manifest = "";
      installNote = {
        ok: true,
        text: `Installed ${answer.template.name} ${answer.adapter_version}. ${reach(answer.hosts)}`,
      };
    } catch (cause) {
      refused(cause);
    } finally {
      installing = null;
    }
  }

  async function test() {
    if (!chosen || !authKind || !ready) return;
    testing = true;
    report = null;
    filled = {};
    try {
      report = await ports.testSource({
        source_id: null,
        adapter_kind: chosen.adapter_kind,
        base_url: baseUrl.trim(),
        auth_kind: authKind,
        config: validated.config,
        secret: { value: secret, account },
      });
      if (report.ok) {
        filled = fillFound(config.fields, configValues, report.account, report.discovered);
        // Read after the fill: the filled values are part of what was tested.
        tested = draft;
      }
    } catch (cause) {
      report = failedReport(cause);
    } finally {
      testing = false;
    }
  }

  async function add() {
    if (!chosen || !authKind || !canAdd) return;
    saving = true;
    saveError = null;
    try {
      const source = await ports.addSource({
        id,
        adapter_kind: chosen.adapter_kind,
        display_name: displayName.trim() === "" ? chosen.name : displayName.trim(),
        base_url: baseUrl.trim(),
        auth_kind: authKind,
        config: validated.config,
        secret: { value: secret, account },
        sync_interval_secs: defaultIntervalFor(chosen),
        enabled: true,
      });
      secret = "";
      accountPassword = "";
      onsaved(source);
    } catch (cause) {
      // The dialog stays open with the draft: a name collision is not a reason to retype it.
      saveError = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<TfDialog title="Add a source" {layers} {onclose}>
  {#snippet body()}
    <div class="tf-pane-h">What is it?</div>
    {#if adaptersError !== null}
      <p class="tf-fail">Could not read the adapters: {adaptersError}</p>
    {:else if adapters !== null}
      <div class="tf-radio" role="radiogroup" aria-label="What is it?">
        {#each adapters as adapter (adapter.id)}
          <label>
            <input type="radio" name="tf-add-adapter" checked={chosen?.id === adapter.id} onchange={() => choose(adapter)} />
            <span>{adapter.name}<small>{kindsHeld(adapter)}</small></span>
          </label>
        {/each}
      </div>
    {/if}
    <div class="tf-formgrid">
      <label for="tf-add-manifest">Install an adapter</label>
      <div class="tf-add-inst">
        <input
          id="tf-add-manifest"
          type="text"
          autocomplete="off"
          placeholder="A repository URL, a manifest URL or a file"
          aria-describedby="tf-add-manifest-note"
          bind:value={manifest}
        />
        <button class="tf-btn" disabled={manifest.trim() === "" || installing !== null} onclick={() => void check()}>
          {installing === "checking" ? "Checking…" : installing === "installing" ? "Installing…" : "Install…"}
        </button>
      </div>
    </div>
    {#if installNote !== null}
      <!-- Text: a refusal's reason quotes what the manifest and the adapter wrote. -->
      <p class="tf-dlg-note {installNote.ok ? '' : 'tf-stg-errt'}" id="tf-add-manifest-note" role={installNote.ok ? "status" : "alert"}>
        {installNote.text}
      </p>
    {:else}
      <p class="tf-dlg-note" id="tf-add-manifest-note">
        A GitHub repository, a manifest URL or a local manifest. Install checks it and shows what it may reach before anything is installed.
      </p>
    {/if}
    {#if chosen !== null}
      <div class="tf-formgrid">
        <label for="tf-add-address">Address</label>
        <input id="tf-add-address" type="text" placeholder="https://" autocomplete="off" bind:value={baseUrl} />
      </div>
      {#if config.error !== null}
        <p class="tf-fail">{config.error}</p>
      {:else}
        <TfSchemaForm fields={config.fields} values={configValues} errors={validated.errors} idPrefix="tf-add-cfg" layout="grid" />
      {/if}
      {#if chosen.auth_methods.length > 1}
        <div class="tf-radio" role="radiogroup" aria-label="Sign in with">
          {#each chosen.auth_methods as method (method)}
            <label>
              <input type="radio" name="tf-add-auth" value={method} bind:group={authKind} />
              <span>{AUTH_LABEL[method]}</span>
            </label>
          {/each}
        </div>
      {/if}
      <div class="tf-formgrid">
        <label for="tf-add-secret">{secretWord}</label>
        <input id="tf-add-secret" type="password" autocomplete="off" bind:value={secret} />
        {#if chosen.accepts_account}
          <label for="tf-add-account-user">Account</label>
          <input id="tf-add-account-user" type="text" autocomplete="off" placeholder="Username (optional)" bind:value={accountUser} />
          <label for="tf-add-account-pass">Account password</label>
          <input id="tf-add-account-pass" type="password" autocomplete="off" placeholder="Password" bind:value={accountPassword} />
        {/if}
        <label for="tf-add-name">Name</label>
        <input id="tf-add-name" type="text" autocomplete="off" bind:value={displayName} />
        <label for="tf-add-id">Id</label>
        <input id="tf-add-id" type="text" autocomplete="off" aria-describedby="tf-add-id-note" bind:value={id} />
      </div>
      <p class="tf-dlg-note {idError ? 'tf-stg-errt' : ''}" id="tf-add-id-note">
        {idError ?? "Permanent: everything from this source is named with it."}
      </p>
      {#if accountHalf}<p class="tf-dlg-note tf-stg-errt">{ACCOUNT_INCOMPLETE}</p>{/if}
      {#if report !== null}
        {@const note = noteAfterFill(connectionNote(report), filled)}
        <!-- Text: `error` is a line an upstream server wrote. -->
        <p class="tf-dlg-note {report.ok ? '' : 'tf-stg-errt'}" role="status">
          {connectionLine(report)}{note ? `. ${note}` : ""}
        </p>
      {:else}
        <p class="tf-dlg-note">Test connection fills in what it can find, such as your user and the field ids.</p>
      {/if}
      {#if saveError !== null}<p class="tf-dlg-note tf-stg-errt" role="alert">{saveError}</p>{/if}
    {/if}
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button class="tf-btn" disabled={!ready || testing} onclick={() => void test()}>Test connection</button>
    <button class="tf-btn tf-primary" disabled={!canAdd} onclick={() => void add()}>Add source</button>
  {/snippet}
</TfDialog>

{#if confirming !== null}
  {@const preview = confirming.preview}
  <TfDialog title="Install {preview.name}?" {layers} onclose={() => (confirming = null)}>
    {#snippet body()}
      <TfAdapterFacts {preview} />
      <p class="tf-dlg-note">It runs with no disk and no keychain. Nothing is installed until you press Install.</p>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (confirming = null)}>Cancel</button>
      <button class="tf-btn tf-primary" onclick={() => void install()}>Install</button>
    {/snippet}
  </TfDialog>
{/if}
