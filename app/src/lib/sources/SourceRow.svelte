<!--
  One source, as the sources view draws it (`signal-miller.html`'s `.src`).

  Six columns, and each one answers a question a reader arrives with: which
  source is this, where does it point, is its credential good, is it syncing,
  and what can I do about it. `.src.err` shades the row whose credential needs
  a human — the same set the launcher's row provenance and chip dots use
  (`shell/health.svelte`), so a red reading means one thing across the app.

  **Nothing here renders a secret.** There is no command that reads one back
  (contract §2.2) and no value on `SourceSummary` that carries one. The
  credential column shows *state*, the *kind* of credential (`auth_kind`, #74)
  and, when the server said so, the expiry countdown — three facts about a
  secret, none of them the secret.
-->
<script lang="ts">
  import Monogram from "../shell/Monogram.svelte";
  import { sourceMonogram } from "../shell/monogram";
  import { expiryNote, isActionable } from "../shell/health.svelte";
  import { ago } from "../shell/time";
  import type {
    AuthMethod,
    AuthState,
    CredentialHealth,
    SourceSummary,
    SourceSyncStatus,
  } from "../ipc/sources";

  let {
    source,
    health,
    status,
    now,
    onsync,
    onreenter,
    ondelete,
  }: {
    source: SourceSummary;
    /**
     * The live reading, when the `source:health` store has one.
     *
     * `SourceSummary.health` is the seed — it was true when `list_sources`
     * answered. The store is what a `source:health` event moves, so a row
     * redraws without the view re-listing.
     */
    health: CredentialHealth | null;
    /** The live sync status from `sync:state`, when there has been one. */
    status: SourceSyncStatus | null;
    now: Date;
    onsync: () => void;
    onreenter: () => void;
    ondelete: () => void;
  } = $props();

  const credential = $derived(health ?? source.health);
  const failing = $derived(isActionable(credential.state));

  /**
   * The states re-entering a credential can actually fix.
   *
   * `unreachable` is deliberately not one: the credential may be perfect and
   * the server down, and offering a password box for a network fault sends the
   * reader to rotate a token that was never the problem. That row keeps *Sync
   * now*, which is the gesture that tests whether the server came back.
   */
  const needsSecret = $derived(
    credential.state === "unauthorized" || credential.state === "missing_secret",
  );

  const running = $derived(status?.running ?? false);

  /**
   * `jira` → `JI`, from the **source id**, which is the same reading the top
   * strip's cluster and the launcher's board strip take.
   *
   * It was the adapter *kind* here, under a comment claiming that was "the only
   * identifier every source has" — which `source.id`, rendered two lines below
   * this in the sub-line, disproves. The cost was real: with `jira` and
   * `tidewater-jira` both configured (P10's multi-instance form), the strip
   * told them apart and this view drew `JI` twice.
   */
  const monogram = $derived(sourceMonogram(source.id));

  /**
   * What the credential column says about the state itself.
   *
   * Total over `AuthState`, like `isActionable`'s table and the top strip's:
   * a state added on the Rust side must fail `svelte-check` here rather than
   * fall through to rendering its raw enum spelling at the reader.
   */
  const CREDENTIAL_WORD: Record<AuthState, string> = {
    ok: "credential ok",
    unauthorized: "credential rejected",
    unreachable: "server unreachable",
    missing_secret: "no credential stored",
    unknown: "not checked yet",
  };

  /**
   * What *kind* of credential this source uses, in the reader's words.
   *
   * Total over `AuthMethod`, like `CREDENTIAL_WORD` above: a method added on
   * the Rust side must fail `svelte-check` here rather than reach the reader as
   * its raw enum spelling.
   *
   * `null` — a source that needs no credential, and equally an `auth_kind`
   * column the backend could not read — gets its own sentence rather than a
   * blank line, because a blank line in this column reads as a column that
   * failed to load.
   */
  const METHOD_WORD: Record<AuthMethod, string> = {
    UserPassword: "user + password",
    Pat: "personal access token",
    ApiToken: "API token",
    OAuth: "OAuth",
  };

  const method = $derived(
    source.auth_kind ? METHOD_WORD[source.auth_kind] : "no credential needed",
  );

  const expiry = $derived(expiryNote(credential.secret_expires_at, now));

  const lastRun = $derived(source.last_run);

  /** The sync column: one line saying what this source is doing, or last did. */
  const syncLine = $derived.by(() => {
    if (running) return "syncing…";
    if (!source.enabled) return "disabled";
    if (!lastRun) return "never synced";
    if (!lastRun.finished_at) return "syncing…";
    const when = ago(lastRun.finished_at, now);
    if (lastRun.outcome === "ok") return `synced ${when}`;
    return `${lastRun.outcome ?? "failed"} ${when}`;
  });
</script>

<div class="src {failing ? 'err' : ''}" data-source-id={source.id}>
  <Monogram text={monogram} tone={failing ? "err" : "ok"} label={source.adapter_kind} />

  <span>
    <span class="nm">{source.display_name}</span>
    <span class="sub">{source.id} · {source.adapter_kind}</span>
  </span>

  <span>
    <span class="url">{source.base_url}</span>
    <span class="caps">
      {#each source.kinds as kind (kind.id)}
        <span>{kind.plural}</span>
      {/each}
    </span>
  </span>

  <span>
    <span class="st">{CREDENTIAL_WORD[credential.state]}</span>
    <span class="sub">{method}</span>
    {#if expiry}
      <span class="sub {expiry.tone}">{expiry.text}</span>
    {/if}
    {#if credential.detail}
      <!-- Text, always: a credential detail is a line an upstream server wrote. -->
      <span class="sub">{credential.detail}</span>
    {/if}
  </span>

  <span>
    <span class="st">{syncLine}</span>
    <span class="sub">{source.item_count} items</span>
    {#if lastRun?.error}
      <span class="sub fail">{lastRun.error}</span>
    {/if}
  </span>

  <span class="acts">
    {#if needsSecret}
      <button class="btn sm pri" onclick={onreenter}>Re-enter</button>
    {:else if source.enabled && !running}
      <button class="btn sm" onclick={onsync}>Sync now</button>
    {/if}
    <button class="btn sm ghost" onclick={ondelete}>Delete</button>
  </span>
</div>

<style>
  /* The row's own action cluster; `.src`'s grid supplies the column. */
  .acts {
    display: flex;
    gap: 4px;
    justify-content: flex-end;
  }

  /* The expiry countdown's two loud tones. `.sub` supplies the type. */
  .sub.amber {
    color: var(--amber);
  }

  .sub.fail,
  .fail {
    color: var(--fail);
  }
</style>
