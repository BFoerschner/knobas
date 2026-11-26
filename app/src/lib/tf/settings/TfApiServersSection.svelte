<!--
  Settings › API servers (ux31, section 15; ADR-0057): servers a person saved for the API explorer's requests, each a
  name, an address and, where its certificate comes from a root of its own, a CA file to trust for it only. No
  secret: Björn ruled that a secret is typed on the explorer's page and held in memory until knobas quits. *Try it*
  sends one `GET` to the address and draws the status line. One settings row; a backup carries it, a share export not.
-->
<script lang="ts">
  import { apiSend, apiServers, apiSetServers, type SavedServer, type SavedServers } from "../../ipc/api";
  import { ipcErrorMessage } from "../../ipc";

  let list = $state<SavedServers | null>(null);
  let failed = $state<string | null>(null);
  let saving = $state(false);
  let saved = $state<string | null>(null);
  let tried = $state<Record<number, string>>({});

  $effect(() => {
    void apiServers().then(
      (got) => (list = got),
      (cause: unknown) => (failed = ipcErrorMessage(cause)),
    );
  });

  function change(i: number, field: keyof SavedServer, value: string) {
    if (list === null) return;
    const servers = list.servers.map((s, n) => (n === i ? { ...s, [field]: field === "ca_file" ? value || null : value } : s));
    list = { ...list, servers };
    saved = null;
  }

  function add() {
    if (list === null) return;
    list = { ...list, servers: [...list.servers, { id: "", name: "", url: "https://", ca_file: null }] };
  }

  function remove(i: number) {
    if (list === null) return;
    list = { ...list, servers: list.servers.filter((_, n) => n !== i) };
    saved = null;
  }

  async function save() {
    if (list === null) return;
    saving = true;
    try {
      list = await apiSetServers(list);
      saved = "Saved.";
      failed = null;
    } catch (cause) {
      failed = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  async function tryIt(i: number, server: SavedServer) {
    tried = { ...tried, [i]: "Sending…" };
    try {
      const answer = await apiSend({ id: Date.now() % 1_000_000_000, method: "GET", url: server.url, headers: [], body: null, auth: null, timeout_s: 15, confirmed_host: null, server: server.id || null, file: "settings", operation: "try" });
      tried = { ...tried, [i]: answer.kind === "answered" ? `${answer.status} ${answer.reason}, ${answer.total_ms} ms` : answer.words };
    } catch (cause) {
      tried = { ...tried, [i]: ipcErrorMessage(cause) };
    }
  }
</script>

<header class="tf-stg-h">
  <h1>API servers</h1>
  <p>Where the API explorer's requests can go besides the servers a spec names. A server keeps no secret: you type it on the explorer's page, and knobas holds it until it quits.</p>
</header>
{#if failed !== null}<p class="tf-stg-p tf-stg-errt" role="alert">{failed}</p>{/if}
{#if list === null && failed === null}
  <p class="tf-stg-p" role="status">Reading the servers…</p>
{:else if list !== null}
  {#if list.servers.length === 0}<p class="tf-stg-p">No server saved. A spec with no server, or a relative one, needs one of yours.</p>{/if}
  {#each list.servers as server, i (i)}
    <section class="tf-stg-sec" aria-label={server.name || "A new server"}>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="api-srv-name-{i}">Name</label>
        <div class="tf-stg-c"><input class="tf-stg-in" id="api-srv-name-{i}" value={server.name} oninput={(e) => change(i, "name", (e.currentTarget as HTMLInputElement).value)} /></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="api-srv-url-{i}">Address<small>http or https, a host and maybe a path. No user name, password or query.</small></label>
        <div class="tf-stg-c"><input class="tf-stg-in" id="api-srv-url-{i}" spellcheck="false" value={server.url} oninput={(e) => change(i, "url", (e.currentTarget as HTMLInputElement).value)} /></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="api-srv-ca-{i}">Trust this CA file<small>A PEM file of roots, trusted for this server only, beside the system's. Optional.</small></label>
        <div class="tf-stg-c"><input class="tf-stg-in" id="api-srv-ca-{i}" spellcheck="false" value={server.ca_file ?? ""} oninput={(e) => change(i, "ca_file", (e.currentTarget as HTMLInputElement).value)} /></div>
      </div>
      <div class="tf-acts">
        <button type="button" class="tf-btn tf-sm" onclick={() => void tryIt(i, server)}>Try it</button>
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => remove(i)}>Remove</button>
        {#if tried[i] !== undefined}<span class="tf-dim" role="status">{tried[i]}</span>{/if}
      </div>
    </section>
  {/each}
  <div class="tf-acts">
    <button type="button" class="tf-btn" onclick={add}>Add a server</button>
    <button type="button" class="tf-btn tf-primary" disabled={saving} onclick={() => void save()}>Save</button>
    {#if saved !== null}<span class="tf-dim" role="status">{saved}</span>{/if}
  </div>
{/if}
