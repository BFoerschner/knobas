<!--
  Settings › Sources › ⟨source⟩ › *Link handoff* (ADR-0061, #2530): whether
  matching external links to this source open in knobas, whether that covers
  known items only or the whole source (non-item pages routing to the source's
  site when enabled), and installing or removing the Firefox Native Messaging
  host manifest on this machine.
-->
<script lang="ts" module>
  import type { LinkHandoffScope, LinkHandoffSetting, NativeHostStatus } from "../../ipc/app";

  /** The bridge calls this section makes; a test's stand-ins otherwise. */
  export interface LinkHandoffPorts {
    read: (id: string) => Promise<LinkHandoffSetting>;
    save: (id: string, enabled: boolean, scope: LinkHandoffScope) => Promise<LinkHandoffSetting>;
    hostStatus: () => Promise<NativeHostStatus>;
    installHost: () => Promise<NativeHostStatus>;
    uninstallHost: () => Promise<NativeHostStatus>;
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    firefoxHostStatus,
    installFirefoxHost,
    setSourceLinkHandoff,
    sourceLinkHandoff,
    uninstallFirefoxHost,
  } from "../../ipc/app";
  import { push } from "../../shell/toasts.svelte";

  const BRIDGE: LinkHandoffPorts = {
    read: (id) => sourceLinkHandoff(id),
    save: (id, enabled, scope) => setSourceLinkHandoff(id, enabled, scope),
    hostStatus: () => firefoxHostStatus(),
    installHost: () => installFirefoxHost(),
    uninstallHost: () => uninstallFirefoxHost(),
  };

  let {
    source,
    name,
    openInside = false,
    ports = BRIDGE,
  }: {
    source: string;
    name: string;
    /** Whether *Open its pages inside knobas* is on for this source. */
    openInside?: boolean;
    ports?: LinkHandoffPorts;
  } = $props();

  let setting = $state<LinkHandoffSetting>({
    source: "",
    enabled: false,
    scope: "known_items",
  });
  let loaded = $state(false);
  let saving = $state(false);
  let host = $state<NativeHostStatus | null>(null);
  let hostBusy = $state(false);

  $effect(() => {
    const id = source;
    let dead = false;
    loaded = false;
    setting = { source: id, enabled: false, scope: "known_items" };
    void ports
      .read(id)
      .then((next) => {
        if (!dead) {
          setting = next;
          loaded = true;
        }
      })
      .catch(() => {
        if (!dead) loaded = true;
      });
    void ports
      .hostStatus()
      .then((next) => {
        if (!dead) host = next;
      })
      .catch(() => {});
    return () => {
      dead = true;
    };
  });

  async function save(enabled: boolean, scope: LinkHandoffScope, done: string) {
    if (saving) return;
    saving = true;
    try {
      setting = await ports.save(source, enabled, scope);
      push({ text: done, subject: `link-handoff:${source}` });
    } catch (cause) {
      push({ text: `Could not save link handoff for ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }

  function toggleEnabled() {
    const next = !setting.enabled;
    const scope = setting.scope;
    const message = next
      ? scope === "whole_source"
        ? `Matching ${name} links open in knobas (whole source).`
        : `Matching ${name} links open in knobas (known items only).`
      : `${name} links stay in your browser.`;
    void save(next, scope, message);
  }

  function pickScope(scope: LinkHandoffScope) {
    if (setting.scope === scope || !setting.enabled) return;
    const message =
      scope === "whole_source"
        ? `All ${name} links hand off to knobas.`
        : `Only known ${name} items hand off to knobas.`;
    void save(setting.enabled, scope, message);
  }

  async function installHost() {
    if (hostBusy) return;
    hostBusy = true;
    try {
      host = await ports.installHost();
      push({ text: "Firefox Native Messaging host installed.", subject: "firefox-host" });
    } catch (cause) {
      push({ text: `Could not install Firefox host: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      hostBusy = false;
    }
  }

  async function uninstallHost() {
    if (hostBusy) return;
    hostBusy = true;
    try {
      host = await ports.uninstallHost();
      push({ text: "Firefox Native Messaging host removed.", subject: "firefox-host" });
    } catch (cause) {
      push({ text: `Could not remove Firefox host: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      hostBusy = false;
    }
  }
</script>

<section class="tf-stg-sec">
  <h2>Link handoff</h2>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      Open matching links in knobas
      <small>Open {name} links from Firefox or knobas:// in knobas</small>
    </div>
    <div class="tf-stg-c">
      <button
        type="button"
        class="tf-stg-sw"
        role="switch"
        aria-checked={setting.enabled}
        aria-label="Open matching links in knobas"
        disabled={saving || !loaded}
        onclick={toggleEnabled}><i></i></button>
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      Scope
      <small>Which links under {name}'s address hand off to knobas</small>
    </div>
    <div class="tf-stg-c">
      <div class="tf-seg" role="group" aria-label="Matching link scope">
        <button
          type="button"
          aria-pressed={setting.scope === "known_items"}
          disabled={saving || !loaded || !setting.enabled}
          onclick={() => pickScope("known_items")}>Known items only</button>
        <button
          type="button"
          aria-pressed={setting.scope === "whole_source"}
          disabled={saving || !loaded || !setting.enabled}
          onclick={() => pickScope("whole_source")}>Whole source</button>
      </div>
      {#if setting.enabled && setting.scope === "whole_source" && !openInside}
        <span class="tf-stg-hint">Turn on Open its pages inside knobas above so other pages open in {name}'s site.</span>
      {/if}
    </div>
  </div>
  {#if host !== null}
    <div class="tf-stg-row">
      <div class="tf-stg-l">
        Firefox host
        <small>Native Messaging manifest for the Firefox extension</small>
      </div>
      <div class="tf-stg-c tf-stg-inl">
        {#if !host.supported}
          <span class="tf-stg-hint">Not supported on this system</span>
        {:else if host.installed && host.matches_current}
          <span>Installed</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" disabled={hostBusy} onclick={() => void uninstallHost()}>Remove</button>
        {:else if host.installed}
          <span class="tf-stg-hint">Points to another knobas copy</span>
          <button type="button" class="tf-btn tf-sm" disabled={hostBusy} onclick={() => void installHost()}>Update host</button>
          <button type="button" class="tf-btn tf-sm tf-ghost" disabled={hostBusy} onclick={() => void uninstallHost()}>Remove</button>
        {:else}
          <span class="tf-stg-hint">Not installed</span>
          <button type="button" class="tf-btn tf-sm" disabled={hostBusy} onclick={() => void installHost()}>Install host</button>
        {/if}
      </div>
    </div>
  {/if}
  <p class="tf-stg-note">
    Known items only opens mirrored tickets, pages, runs, repositories and pull requests in knobas and leaves other pages in
    your browser. Whole source also sends other {name} pages to its site inside knobas when Open its pages inside knobas is
    on. Open in browser records a one-shot bypass so Firefox opens that page instead of handing it back.
  </p>
</section>
