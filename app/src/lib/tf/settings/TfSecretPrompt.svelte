<!--
  *Enter* or *Re-enter the <source> <password or token>*: the one credential prompt (F-048). A source's page opens it from
  its own buttons, and the shell mounts it over any page through `TfSecretPromptHost`. It writes through
  `set_source_secret`, which never reads one back, and a source that refuses it says so here and the prompt stays.
  Signed in, it syncs: the sync is the follow-through, not the transaction, since a disabled or running source refuses
  it and the credential is stored either way.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CredentialHealth, SourceSummary } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { accountHalfDone, accountOf, ACCOUNT_INCOMPLETE } from "../../sources/account";
  import type { Layers } from "../layers.svelte";
  import type { SettingsPorts } from "./ports";
  import { secretWord } from "./settings";
  import TfDialog from "./TfDialog.svelte";

  let {
    source,
    health,
    acceptsAccount,
    stored,
    ports,
    layers,
    onhealth,
    onclose,
    onstarted = () => {},
    onstopped = () => {},
  }: {
    source: Pick<SourceSummary, "id" | "display_name" | "auth_kind">;
    health: CredentialHealth | null;
    /** The adapter takes a second credential (Uptime Kuma's account). */
    acceptsAccount: boolean;
    /** Where a credential goes, in words (`storedWhere`). */
    stored: string;
    ports: Pick<SettingsPorts, "setSourceSecret" | "syncNow">;
    layers: Layers;
    onhealth: (next: CredentialHealth) => void;
    onclose: () => void;
    onstarted?: () => void;
    onstopped?: () => void;
  } = $props();

  const name = $derived(source.display_name || source.id);
  const word = $derived(secretWord(source.auth_kind));
  /** Nothing is stored yet, so nothing here speaks of entering it again. */
  const noneStored = $derived(health?.state === "missing_secret");

  let secret = $state("");
  let accountUser = $state("");
  let accountPassword = $state("");
  let signingIn = $state(false);
  let secretError = $state<string | null>(null);
  const half = $derived(accountHalfDone({ username: accountUser, password: accountPassword }));
  const account = $derived(accountOf({ username: accountUser, password: accountPassword }));
  const canSignIn = $derived(!signingIn && !half && (secret !== "" || account !== null));

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
      onclose();
      push({ text: `Signed in to ${name}. Syncing now.` });
      onstarted();
      await ports.syncNow(source.id).catch(() => onstopped());
    } catch (cause) {
      secretError = ipcErrorMessage(cause);
    } finally {
      signingIn = false;
    }
  }
</script>

<TfDialog why={!canSignIn && !signingIn && !half ? `Type the ${word.toLowerCase()} to sign in.` : null} title="{noneStored ? 'Enter' : 'Re-enter'} the {name} {word.toLowerCase()}" {layers} {onclose}>
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
      {noneStored ? "" : `${stored}. `}Writes waiting for {name} go out once it signs in.{acceptsAccount && !noneStored
        ? " A field left empty keeps what is stored."
        : ""}
    </p>
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button class="tf-btn tf-primary" type="submit" form="tf-reenter" disabled={!canSignIn}>Sign in</button>
  {/snippet}
</TfDialog>
