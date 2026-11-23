<!--
  Settings › Sources › ⟨source⟩ › *Site* (ADR-0050, decisions §16): whether the source's pages open inside knobas, its
  sign-in kept on this computer and *Forget…*, and the addresses its site may show. Rows in the page's own grid.

  The switch is saved as it changes, with a toast. *Forget…* asks first, then closes the site (asking its page if it
  holds changes) and removes what the site stored here.

  *Saved sign-in* (decisions §13 (b), ux29): a username and password knobas types into the site's own sign-in form,
  kept in the source's keychain item. The row knows only that one is saved and for which name; the password is never
  read back. A site whose adapter says it signs in with the source's own credential (ux30, a password manager's web
  interface) shows that instead: *Same as ⟨source⟩'s credential*, with *Use another…* and *Turn off*; a saved one wins.
-->
<script lang="ts" module>
  import type { SiteAsked, SiteSetting } from "../../ipc/site";

  /** The bridge calls the section makes; a test's stand-ins otherwise. */
  export interface SiteSectionPorts {
    setOpenInside: (id: string, on: boolean) => Promise<SiteSetting>;
    allow: (id: string, origin: string) => Promise<SiteSetting>;
    disallow: (id: string, origin: string) => Promise<SiteSetting>;
    forget: (id: string, force: boolean) => Promise<SiteAsked>;
    saveSignIn: (id: string, username: string, password: string) => Promise<SiteSetting>;
    removeSignIn: (id: string) => Promise<SiteSetting>;
    ownSignIn: (id: string, on: boolean) => Promise<SiteSetting>;
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { setSiteOpenInside, siteAllow, siteDisallow, siteForget, siteSignInOwn, siteSignInRemove, siteSignInSave } from "../../ipc/site";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { siteStore, type SiteStore } from "./site.svelte";

  const BRIDGE: SiteSectionPorts = {
    setOpenInside: (id, on) => setSiteOpenInside(id, on),
    allow: (id, origin) => siteAllow(id, origin),
    disallow: (id, origin) => siteDisallow(id, origin),
    forget: (id, force) => siteForget(id, force),
    saveSignIn: (id, username, password) => siteSignInSave(id, username, password),
    removeSignIn: (id) => siteSignInRemove(id),
    ownSignIn: (id, on) => siteSignInOwn(id, on),
  };

  let {
    source,
    name,
    ports = BRIDGE,
    store = siteStore,
  }: {
    source: string;
    name: string;
    ports?: SiteSectionPorts;
    store?: SiteStore;
  } = $props();

  const setting = $derived(store.settings.get(source));
  let saving = $state(false);
  let forgetting = $state(false);
  /** The sign-in dialog is open. */
  let signing = $state(false);
  let signUser = $state("");
  let signPassword = $state("");
  let signError = $state<string | null>(null);
  const canSave = $derived(!saving && signUser.trim() !== "" && signPassword !== "");

  function openSignIn() {
    signUser = setting?.sign_in_user ?? "";
    signPassword = "";
    signError = null;
    signing = true;
  }

  function closeSignIn() {
    signing = false;
    signPassword = "";
  }

  async function saveSignIn() {
    if (!canSave) return;
    saving = true;
    signError = null;
    try {
      const next = await ports.saveSignIn(source, signUser.trim(), signPassword);
      store.holdOne(next);
      closeSignIn();
      push({ text: `${name}'s sign-in is saved. knobas fills it into ${name}'s own sign-in page.` });
    } catch (cause) {
      signError = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  function ownSignIn(on: boolean) {
    void write(
      () => ports.ownSignIn(source, on),
      () => (on ? `knobas signs in to ${name}'s site with ${name}'s credential.` : `knobas no longer signs in to ${name}'s site. You sign in yourself.`),
    );
  }

  function removeSignIn() {
    void write(
      () => ports.removeSignIn(source),
      () => `${name}'s saved sign-in is removed.`,
    );
  }

  const kept = $derived.by(() => {
    const since = setting?.kept_since;
    if (!since) return null;
    const at = new Date(since);
    return Number.isNaN(at.getTime()) ? null : at.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  });

  async function write(run: () => Promise<SiteSetting>, said: (setting: SiteSetting) => string | null) {
    saving = true;
    try {
      const next = await run();
      store.holdOne(next);
      const text = said(next);
      if (text !== null) push({ text });
    } catch (cause) {
      push({ text: `Could not save ${name}'s site: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }

  function toggle() {
    const on = !(setting?.open_inside ?? false);
    void write(
      () => ports.setOpenInside(source, on),
      (next) => (next.open_inside ? `${name}'s pages open inside knobas.` : `${name}'s pages open in your browser.`),
    );
  }

  async function forget(force: boolean) {
    saving = true;
    try {
      const asked = await ports.forget(source, force);
      if (!asked.done && asked.asks === "unsaved") {
        push({ text: `${name}'s page has changes it has not saved.`, action: { label: "Forget anyway", run: () => void forget(true) } });
        return;
      }
      forgetting = false;
      if (setting !== undefined) store.holdOne({ ...setting, kept_since: null });
      push({ text: `${name}'s sign-in is forgotten.` });
    } catch (cause) {
      push({ text: `Could not forget ${name}'s sign-in: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }
</script>

{#if setting !== undefined}
  <section class="tf-stg-sec">
    <h2>Site</h2>
    <div class="tf-stg-row">
      <div class="tf-stg-l">Open its pages inside knobas</div>
      <div class="tf-stg-c">
        <button
          class="tf-stg-sw"
          role="switch"
          aria-checked={setting.open_inside}
          aria-label="Open its pages inside knobas"
          disabled={saving || !setting.offered}
          onclick={toggle}><i></i></button
        >
        {#if !setting.offered}<span class="tf-stg-hint">Sites open inside knobas on Windows and macOS.</span>{/if}
      </div>
    </div>
    <div class="tf-stg-row">
      <div class="tf-stg-l">Sign-in<small>The site's own, kept on this computer</small></div>
      <div class="tf-stg-c">
        {#if kept !== null}
          <span>Kept since {kept}</span>
          <button class="tf-btn tf-sm tf-danger" disabled={saving} onclick={() => (forgetting = true)}>Forget…</button>
        {:else}
          <span class="tf-stg-hint">{!setting.offered ? "Nothing kept: no site here" : setting.persists ? "Not signed in here yet" : "Kept until knobas quits on this macOS"}</span>
        {/if}
      </div>
    </div>
    <div class="tf-stg-row">
      <div class="tf-stg-l">Addresses it may show<small>Its own, and the ones you allowed</small></div>
      <div class="tf-stg-c tf-site-origins">
        <ul>
          {#if setting.own !== ""}
            <li><span class="tf-mono">{setting.own}</span><span class="tf-lab">its own</span></li>
          {/if}
          {#each setting.allowed as origin (origin)}
            <li>
              <span class="tf-mono">{origin}</span>
              <button class="tf-btn tf-sm tf-ghost" disabled={saving} onclick={() => void write(() => ports.disallow(source, origin), () => null)}>Remove</button>
            </li>
          {/each}
          {#each setting.suggested as origin (origin)}
            <li class="tf-site-suggested">
              <span class="tf-mono">{origin}</span>
              <button class="tf-btn tf-sm tf-ghost" disabled={saving} onclick={() => void write(() => ports.allow(source, origin), () => null)}>Allow</button>
            </li>
          {/each}
        </ul>
        {#if setting.confined}
          <span class="tf-stg-hint">Content from other websites is blocked on this site. Allowed addresses still load.</span>
        {/if}
      </div>
    </div>
    <div class="tf-stg-row">
      <div class="tf-stg-l">Saved sign-in<small>Typed into {name}'s own sign-in page</small></div>
      <div class="tf-stg-c">
        {#if setting.sign_in_user}
          <span>Saved for <span class="tf-mono">{setting.sign_in_user}</span>, never shown</span>
          <button class="tf-btn tf-sm" disabled={saving} onclick={openSignIn}>Change…</button>
          <button class="tf-btn tf-sm tf-ghost" disabled={saving} onclick={removeSignIn}>Remove</button>
        {:else if setting.own_sign_in?.on}
          <span>Same as {name}'s credential, <span class="tf-mono">{setting.own_sign_in.user}</span></span>
          <button class="tf-btn tf-sm" disabled={saving || !setting.offered} onclick={openSignIn}>Use another…</button>
          <button class="tf-btn tf-sm tf-ghost" disabled={saving} onclick={() => ownSignIn(false)}>Turn off</button>
        {:else if setting.own_sign_in}
          <span class="tf-stg-hint">Off. You sign in yourself.</span>
          <button class="tf-btn tf-sm" disabled={saving} onclick={() => ownSignIn(true)}>Turn on</button>
          <button class="tf-btn tf-sm" disabled={saving || !setting.offered} onclick={openSignIn}>Use another…</button>
        {:else}
          <span class="tf-stg-hint">None</span>
          <button class="tf-btn tf-sm" disabled={saving || !setting.offered} onclick={openSignIn}>Save one…</button>
        {/if}
      </div>
    </div>
    <p class="tf-stg-note">
      On, Open in {name} shows the page here, under knobas's bar, and you sign in there once. The site cannot reach knobas, and
      knobas does not read the site. Off, it opens your browser. The sign-in is in no backup. A saved sign-in stays in this
      computer's keychain and goes only into {name}'s own sign-in form, with "remember me" ticked; a sign-in through another
      provider or a code is yours to type.
    </p>
  </section>
{/if}

{#if signing && setting !== undefined}
  <TfDialog
    why={!canSave && !saving ? "Type the username and the password to save them." : null}
    title={setting.sign_in_user ? `Change ${name}'s sign-in` : setting.own_sign_in ? `Save another sign-in for ${name}` : `Save ${name}'s sign-in`}
    onclose={closeSignIn}
  >
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-site-signin"
        onsubmit={(event) => {
          event.preventDefault();
          void saveSignIn();
        }}
      >
        <label for="tf-site-signin-user">Username</label>
        <input id="tf-site-signin-user" type="text" autocomplete="off" spellcheck="false" bind:value={signUser} />
        <label for="tf-site-signin-pass">Password</label>
        <input id="tf-site-signin-pass" type="password" autocomplete="new-password" bind:value={signPassword} />
      </form>
      {#if signError !== null}<p class="tf-dlg-note tf-stg-errt" role="alert">{signError}</p>{/if}
      <p class="tf-dlg-note">
        Kept in this computer's keychain beside {name}'s credential, never shown again. knobas types it into {name}'s own sign-in
        page when it opens there, and when you press Fill sign-in.
      </p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={closeSignIn}>Cancel</button>
      <button type="submit" class="tf-btn tf-primary" form="tf-site-signin" disabled={!canSave}>Save sign-in</button>
    {/snippet}
  </TfDialog>
{/if}

{#if forgetting}
  <TfDialog title="Forget {name}'s sign-in?" onclose={() => (forgetting = false)}>
    {#snippet body()}
      <p>knobas removes what the site stored on this computer. You sign in again the next time it opens.</p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (forgetting = false)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary tf-danger" disabled={saving} onclick={() => void forget(false)}>Forget sign-in</button>
    {/snippet}
  </TfDialog>
{/if}
