<!--
  Settings › Sources › ⟨source⟩ › *Site* (ADR-0050, decisions §16): whether the source's pages open inside knobas, its
  sign-in kept on this computer and *Forget…*, and the addresses its site may show. Rows in the page's own grid.

  The switch is saved as it changes, with a toast. *Forget…* asks first, then closes the site (asking its page if it
  holds changes) and removes what the site stored here.
-->
<script lang="ts" module>
  import type { SiteAsked, SiteSetting } from "../../ipc/site";

  /** The bridge calls the section makes; a test's stand-ins otherwise. */
  export interface SiteSectionPorts {
    setOpenInside: (id: string, on: boolean) => Promise<SiteSetting>;
    allow: (id: string, origin: string) => Promise<SiteSetting>;
    disallow: (id: string, origin: string) => Promise<SiteSetting>;
    forget: (id: string, force: boolean) => Promise<SiteAsked>;
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { setSiteOpenInside, siteAllow, siteDisallow, siteForget } from "../../ipc/site";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { siteStore, type SiteStore } from "./site.svelte";

  const BRIDGE: SiteSectionPorts = {
    setOpenInside: (id, on) => setSiteOpenInside(id, on),
    allow: (id, origin) => siteAllow(id, origin),
    disallow: (id, origin) => siteDisallow(id, origin),
    forget: (id, force) => siteForget(id, force),
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
          <span class="tf-stg-hint">{setting.persists ? "Not signed in here yet" : "Kept until knobas quits on this macOS"}</span>
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
      </div>
    </div>
    <p class="tf-stg-note">
      On, Open in {name} shows the page here, under knobas's bar, and you sign in there once. The site cannot reach knobas, and
      knobas does not read the site. Off, it opens your browser. The sign-in is in no backup.
    </p>
  </section>
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
