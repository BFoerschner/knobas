<!--
  The Add-source dialog — spec §3's *"type → URL → auth → Test connection →
  sync schedule → save"*, as the mockup's five `.steps`.

  The one change from the mockup is the whole point: **step 2's configuration
  form is generated** from the adapter's `config_schema` (§3a), so a fourth
  adapter is offered, configured and saved without a line of this file
  changing.

  ## Order of operations is the backend's

  Contract §3: the secret is written to the keychain first, then the config
  row, rolled back on failure. This dialog therefore sends **one**
  `add_source` call carrying both and does not attempt its own two-phase
  dance — a frontend that stored the secret and then failed to write the row
  would leave a credential in the keychain with nothing pointing at it.

  ## The secret

  It lives in this component's `$state` and in a `type="password"` input, goes
  out in exactly two requests (the test and the save), and is dropped when the
  dialog closes. There is no command that reads one back (contract §2.2).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    addSource,
    listAdapters,
    testSource,
    type AuthMethod,
    type ConnectionReport,
    type SourceDescriptor,
    type SourceSummary,
  } from "../ipc/sources";
  import Modal from "../shell/Modal.svelte";
  import SchemaForm from "./SchemaForm.svelte";
  import { defaultValues, schemaFields, validate, type SchemaField } from "./schema-form";

  let {
    onclose,
    onsaved,
  }: {
    onclose: () => void;
    onsaved: (source: SourceSummary) => void;
  } = $props();

  /** The breadcrumb, and the order the flow walks. */
  const STEPS = ["Type", "Connection", "Auth", "Test", "Schedule"] as const;

  /** How an `AuthMethod` reads to a person. */
  const AUTH_LABEL: Record<AuthMethod, string> = {
    UserPassword: "Username and password",
    Pat: "Personal access token",
    ApiToken: "API token",
    OAuth: "OAuth",
  };

  /** Contract §4.1. The id is the entity namespace and is immutable (P10). */
  const ID_PATTERN = /^[a-z][a-z0-9-]{0,31}$/;

  /** The schedules the sources view can describe in one line. */
  const INTERVALS = [
    { secs: 300, label: "every 5 minutes" },
    { secs: 900, label: "every 15 minutes" },
    { secs: 3600, label: "hourly" },
    { secs: 21_600, label: "every 6 hours" },
    { secs: 86_400, label: "daily" },
  ];

  let stepIndex = $state(0);
  let adapters = $state<SourceDescriptor[]>([]);
  let adaptersError = $state<string | null>(null);
  let chosen = $state<SourceDescriptor | null>(null);

  let id = $state("");
  let displayName = $state("");
  let baseUrl = $state("");
  let authKind = $state<AuthMethod | null>(null);
  let secret = $state("");
  let interval = $state(900);
  let enabled = $state(true);

  let configValues = $state<Record<string, unknown>>({});
  let report = $state<ConnectionReport | null>(null);
  let testing = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  $effect(() => {
    let dead = false;
    void listAdapters()
      .then((rows) => {
        if (!dead) adapters = rows;
      })
      .catch((cause) => {
        // An empty picker with a Next button under it is a dialog claiming
        // there are no adapters, which is never true — every build has at
        // least the mock.
        if (!dead) adaptersError = ipcErrorMessage(cause);
      });
    return () => {
      dead = true;
    };
  });

  /**
   * The adapter's config fields, or the reason there are none.
   *
   * `schemaFields` **throws** for a schema that declares a secret (contract
   * §3). Catching it here turns an adapter bug into a sentence on the step it
   * belongs to, rather than an exception that takes the dialog down.
   */
  const configFields = $derived.by((): { fields: SchemaField[]; error: string | null } => {
    if (!chosen) return { fields: [], error: null };
    try {
      return { fields: schemaFields(chosen.config_schema), error: null };
    } catch (cause) {
      return { fields: [], error: cause instanceof Error ? cause.message : String(cause) };
    }
  });

  const validated = $derived(validate(configFields.fields, configValues));

  const idError = $derived.by(() => {
    if (id === "") return "An id is required.";
    if (!ID_PATTERN.test(id)) {
      return "Lower-case letters, digits and hyphens, starting with a letter — the id is the entity namespace and cannot be changed later.";
    }
    return null;
  });

  function choose(descriptor: SourceDescriptor) {
    chosen = descriptor;
    id = descriptor.adapter_kind;
    displayName = descriptor.name;
    // The config belongs to the adapter that declared it. Carrying values
    // across would write, say, Jira's `flavor` into a Gitea source's config
    // column, where it means nothing and nothing will ever remove it.
    let fields: SchemaField[] = [];
    try {
      fields = schemaFields(descriptor.config_schema);
    } catch {
      fields = [];
    }
    configValues = defaultValues(fields);
    authKind = descriptor.auth_methods[0] ?? null;
    report = null;
  }

  /** Whether the step on screen has everything it needs. */
  const canAdvance = $derived.by(() => {
    switch (stepIndex) {
      case 0:
        return chosen !== null;
      case 1:
        return (
          idError === null &&
          baseUrl.trim() !== "" &&
          configFields.error === null &&
          Object.keys(validated.errors).length === 0
        );
      case 2:
        return authKind !== null && secret !== "";
      case 3:
        // A source whose credential has just been refused would go into the
        // database as a row the scheduler can never run.
        return report?.ok === true;
      default:
        return true;
    }
  });

  /**
   * The config property an adapter puts its identity in, by convention.
   *
   * All three shipped adapters spell it `username` — and so must a fourth, or
   * `@me`, *My items* and *Mine, untouched* have nothing to resolve against on
   * that source. Keying on the property **name** is what keeps this dialog
   * adapter-agnostic: it is a convention every adapter follows, not a table of
   * what each one's config keys mean, and the schema descriptions state it
   * where an adapter author will read it (#82).
   */
  const IDENTITY_FIELD = "username";

  /**
   * Put the account the source just reported into the identity field.
   *
   * `username` is the only source of the identity the three identity lists
   * read, and author matching is case-sensitive by construction — so the
   * account is written in the server's own spelling and never normalised.
   * That is the whole advantage over retyping it: the dialog has just printed
   * *"Connected as …"* on screen, and a reader copying it back by hand is one
   * slip away from an identity that matches nothing.
   *
   * **Only when the field is empty.** A value already there was typed by
   * somebody who meant it — a service account, or a login that is not the
   * account the API reports — and a test run afterwards must not quietly
   * replace it. The filled value lands in the generated form, visible and
   * editable one *Back* away.
   *
   * An **absent** account is not a failure: a source whose API has no "who am
   * I" endpoint is a working source (`ConnectionInfo::account` is optional by
   * design), and nothing here assumes it is present. Nor is a missing
   * `username` property — an adapter that declares none simply gets no fill.
   */
  function fillIdentity(account: string | null) {
    if (account === null || account === "") return;
    const field = configFields.fields.find((f) => f.key === IDENTITY_FIELD);
    // A text control, or there is nothing a plain account string can be put
    // into: an adapter that typed `username` as, say, a number is not one this
    // convention covers, and guessing at its shape would write a config value
    // it never asked for.
    if (!field || field.control.kind !== "text") return;
    const current = configValues[IDENTITY_FIELD];
    if (typeof current === "string" && current.trim() !== "") return;
    configValues[IDENTITY_FIELD] = account;
  }

  async function test() {
    if (!chosen || !authKind) return;
    testing = true;
    report = null;
    try {
      report = await testSource({
        // An unsaved draft: the backend tests what is being typed, not
        // something already stored.
        source_id: null,
        adapter_kind: chosen.adapter_kind,
        base_url: baseUrl.trim(),
        auth_kind: authKind,
        config: validated.config,
        secret: { value: secret },
      });
      // A refused credential reports no account worth keeping, and a source
      // that cannot be saved has no config to fill in either.
      if (report.ok) fillIdentity(report.account);
    } catch (cause) {
      report = {
        ok: false,
        account: null,
        server_version: null,
        secret_expires_at: null,
        error: ipcErrorMessage(cause),
        code: null,
        elapsed_ms: 0,
      };
    } finally {
      testing = false;
    }
  }

  async function save() {
    if (!chosen || !authKind || saving) return;
    saving = true;
    saveError = null;
    try {
      const source = await addSource({
        id,
        adapter_kind: chosen.adapter_kind,
        display_name: displayName.trim() === "" ? chosen.name : displayName.trim(),
        base_url: baseUrl.trim(),
        auth_kind: authKind,
        config: validated.config,
        secret: { value: secret },
        sync_interval_secs: interval,
        enabled,
      });
      secret = "";
      onsaved(source);
    } catch (cause) {
      // The dialog stays open with the draft intact: closing on failure would
      // throw away five steps of typing over a name collision.
      saveError = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  /** *"Connected as ‹account› · ‹version› · ‹ms› ms"*, minus what is absent. */
  const reportLine = $derived.by(() => {
    if (!report) return "";
    if (!report.ok) return report.error ?? "The connection failed.";
    // The three readings depend on P4 and may be `None`. An absent one is
    // simply not written — never "null", never an orphaned separator.
    return [
      report.account ? `Connected as ${report.account}` : "Connected",
      report.server_version,
      `${report.elapsed_ms} ms`,
    ]
      .filter((part): part is string => Boolean(part))
      .join(" · ");
  });
</script>

<Modal title="Add a source" wide onclose={onclose}>
  {#snippet body()}
    <div class="steps">
      {#each STEPS as label, index (label)}
        <span class={index === stepIndex ? "on" : index < stepIndex ? "done" : ""}>{label}</span>
      {/each}
    </div>

    {#if stepIndex === 0}
      {#if adaptersError}
        <p class="msg fail">Could not read the adapter registry: {adaptersError}</p>
      {:else}
        <div class="modules">
          {#each adapters as adapter (adapter.id)}
            <button
              class="mod {chosen?.id === adapter.id ? 'on' : ''}"
              aria-pressed={chosen?.id === adapter.id}
              onclick={() => choose(adapter)}
            >
              <span class="mg">{adapter.adapter_kind.slice(0, 2).toUpperCase()}</span>
              <span>
                <span class="nm">{adapter.name}</span>
                <span class="sub">
                  {adapter.adapter_kind} · {adapter.entity_kinds.map((k) => k.plural).join(", ") ||
                    "no declared kinds"}
                </span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {:else if stepIndex === 1}
      <div class="form">
        <label class="l" for="add-id">Instance id</label>
        <div class="cell">
          <input class="inp" id="add-id" bind:value={id} aria-invalid={idError ? "true" : undefined} />
          {#if idError}
            <span class="msg fail">{idError}</span>
          {:else}
            <span class="msg">Permanent. Every entity from this source is namespaced with it.</span>
          {/if}
        </div>

        <label class="l" for="add-name">Display name</label>
        <div class="cell"><input class="inp" id="add-name" bind:value={displayName} /></div>

        <label class="l" for="add-url">Base URL</label>
        <div class="cell"><input class="inp" id="add-url" bind:value={baseUrl} /></div>
      </div>

      <div class="cfg">
        {#if configFields.error}
          <p class="msg fail">{configFields.error}</p>
        {:else}
          <SchemaForm
            fields={configFields.fields}
            values={configValues}
            errors={validated.errors}
            idPrefix="add-cfg"
          />
        {/if}
      </div>
    {:else if stepIndex === 2}
      <div class="radios">
        {#each chosen?.auth_methods ?? [] as method (method)}
          <label>
            <input type="radio" name="auth" value={method} bind:group={authKind} />
            {AUTH_LABEL[method] ?? method}
          </label>
        {/each}
      </div>
      <div class="form secret">
        <label class="l" for="add-secret">
          {authKind === "UserPassword" ? "Password" : "Token"}
        </label>
        <div class="cell">
          <input class="inp" id="add-secret" type="password" autocomplete="off" bind:value={secret} />
          <span class="msg">Stored in the OS keychain, never in the database.</span>
        </div>
      </div>
    {:else if stepIndex === 3}
      <p class="msg">
        knobas will connect to {baseUrl} and report what answered. Nothing is written yet — not to
        the database, not to the keychain.
      </p>
      <button class="btn pri" disabled={testing} onclick={() => void test()}>
        {testing ? "Testing…" : "Test connection"}
      </button>
      {#if report}
        <!-- Text: `error` is a line an upstream server wrote (gotcha 7). -->
        <p class="test-res {report.ok ? '' : 'fail'}">{reportLine}</p>
      {/if}
    {:else}
      <div class="form">
        <label class="l" for="add-interval">Sync</label>
        <div class="cell">
          <select class="sel-inline" id="add-interval" bind:value={interval}>
            {#each INTERVALS as option (option.secs)}
              <option value={option.secs}>{option.label}</option>
            {/each}
          </select>
          <span class="msg">Counted from the end of the previous run (P7).</span>
        </div>

        <span class="l">Enabled</span>
        <div class="cell">
          <label class="chk">
            <input type="checkbox" bind:checked={enabled} />
            Sync on the schedule above
          </label>
        </div>
      </div>
      {#if saveError}
        <p class="msg fail">{saveError}</p>
      {/if}
    {/if}
  {/snippet}

  {#snippet footer()}
    <span class="l">
      {#if chosen}{chosen.name} · {id}{/if}
    </span>
    {#if stepIndex > 0}
      <button class="btn" onclick={() => (stepIndex -= 1)}>Back</button>
    {/if}
    {#if stepIndex < STEPS.length - 1}
      <button class="btn pri" disabled={!canAdvance} onclick={() => (stepIndex += 1)}>Next</button>
    {:else}
      <button class="btn pri" disabled={saving} onclick={() => void save()}>
        {saving ? "Saving…" : "Save"}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .cell {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .form {
    align-items: start;
  }

  .form .l {
    padding-top: 6px;
  }

  .inp {
    width: 100%;
  }

  .msg {
    font: 400 11px/1.45 var(--mono);
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  .cfg {
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--hair);
  }

  .secret {
    margin-top: 12px;
  }

  .chk {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
  }

  .chk input {
    accent-color: var(--text);
  }

  .mod .mg {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 16px;
    border: 1px solid var(--hair2);
    border-radius: 2px;
    font: 500 10px/1 var(--mono);
    color: var(--muted);
  }
</style>
