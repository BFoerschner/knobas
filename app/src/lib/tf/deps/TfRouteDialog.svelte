<!--
  The route dialog (#876, R-11.21): the address (with a scheme), a name (from
  the address when left empty), *Exposed by*, *Lands on* with "Nothing knobas
  knows" as a real choice, and who can reach it. From a ticket it offers to
  link the new route.

  Editing is the same dialog over `get_route`: *Exposed by* is fixed (a route
  exposed elsewhere is a new route), *Save* sends one `edit_route` with only
  what changed, and *Delete…* asks once more before `delete_route`.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { Dependencies, RouteRow, Visibility } from "../../ipc/assets";
  import TfDialog from "../settings/TfDialog.svelte";
  import { hasScheme, linkToTicket, routeEditsOf, routeNameOf, type RouteForm } from "./add";
  import { assetPaths } from "./estate-paths";
  import type { AddPorts } from "./ports";

  let {
    read,
    routeId = null,
    exposer = null,
    ticket = null,
    ports,
    onclose,
    onsaved,
    ondeleted,
  }: {
    /** The whole estate: where *Exposed by* and *Lands on* pick from. */
    read: Dependencies;
    /** The route to edit, or `null` for a new one. */
    routeId?: string | null;
    /** The asset a new route starts exposed by; else the first reverse proxy. */
    exposer?: string | null;
    ticket?: { key: string; id: string } | null;
    ports: AddPorts;
    onclose: () => void;
    onsaved: (done: { id: string; name: string; created: boolean; unlinked: string | null }) => void;
    ondeleted: (name: string) => void;
  } = $props();

  const uid = $props.id();
  const places = $derived(assetPaths(read).places);
  // svelte-ignore state_referenced_locally
  const firstProxy = read.items.find((item) => item.kind === "reverse_proxy")?.id ?? null;

  let row = $state.raw<RouteRow | null>(null);
  // svelte-ignore state_referenced_locally
  let form = $state<RouteForm>({ url: "https://", name: "", target: null, visibility: "internal" });
  // svelte-ignore state_referenced_locally
  let exposedBy = $state<string>(exposer ?? firstProxy ?? "");
  let link = $state(true);
  let failure = $state<string | null>(null);
  let writing = $state(false);
  let sure = $state(false);

  async function load(id: string) {
    try {
      const detail = await ports.getRoute(id);
      row = detail.route;
      form = { url: row.url, name: row.name, target: row.target_id, visibility: row.visibility };
    } catch (error) {
      failure = `The route could not be read: ${ipcErrorMessage(error)}`;
    }
  }
  // svelte-ignore state_referenced_locally
  if (routeId !== null) void load(routeId);
  // svelte-ignore state_referenced_locally
  if (exposedBy === "" && places.length > 0) exposedBy = places[0]!.id;

  async function save() {
    if (writing) return;
    const url = form.url.trim();
    if (!hasScheme(url)) {
      failure = "The address needs a scheme, like https://";
      return;
    }
    writing = true;
    failure = null;
    try {
      if (row !== null) {
        const edits = routeEditsOf(row, form);
        const saved = edits.length > 0 ? await ports.editRoute(row.id, edits) : row;
        onsaved({ id: saved.id, name: saved.name, created: false, unlinked: null });
        return;
      }
      if (exposedBy === "") {
        failure = "A route needs the asset that exposes it.";
        return;
      }
      const name = form.name.trim() || routeNameOf(url);
      const made = await ports.createRoute(exposedBy, name, url, form.target, form.visibility);
      const unlinked = await linkToTicket(ports.createLink, ticket, link, made.id);
      onsaved({ id: made.id, name: made.name, created: true, unlinked });
    } catch (error) {
      failure = ipcErrorMessage(error);
    } finally {
      writing = false;
    }
  }

  async function remove() {
    if (row === null) return;
    if (!sure) {
      sure = true;
      return;
    }
    try {
      await ports.deleteRoute(row.id);
      ondeleted(row.name);
    } catch (error) {
      failure = ipcErrorMessage(error);
      sure = false;
    }
  }
</script>

<TfDialog title={routeId !== null ? "Edit route" : "New route"} {onclose}>
  {#snippet body()}
    <div class="tf-formgrid">
      <label for="{uid}-url">Address</label>
      <input id="{uid}-url" type="url" class="tf-mono" placeholder="https://payouts.tidewater.internal" autocomplete="off" bind:value={form.url} />
      <label for="{uid}-name">Name</label>
      <input id="{uid}-name" type="text" placeholder="From the address, if left empty" autocomplete="off" bind:value={form.name} />
      {#if routeId !== null}
        <span>Exposed by</span>
        <span class="tf-mono">{row?.asset_name ?? ""}</span>
      {:else}
        <label for="{uid}-exp">Exposed by</label>
        <select id="{uid}-exp" bind:value={exposedBy}>
          {#each places as place (place.id)}<option value={place.id}>{place.path}</option>{/each}
        </select>
      {/if}
      <label for="{uid}-to">Lands on</label>
      <select id="{uid}-to" value={form.target ?? ""} onchange={(e) => (form.target = e.currentTarget.value || null)}>
        <option value="">Nothing knobas knows (a health path, a port)</option>
        {#each places as place (place.id)}<option value={place.id}>{place.path}</option>{/each}
      </select>
      <label for="{uid}-vis">Who can reach it</label>
      <select id="{uid}-vis" value={form.visibility} onchange={(e) => (form.visibility = e.currentTarget.value as Visibility)}>
        <option value="internal">internal</option>
        <option value="public">public</option>
      </select>
    </div>
    {#if routeId === null}
      <p class="tf-dim tf-note">The asset that exposes a route never changes; exposing it elsewhere is a new route.</p>
      {#if ticket !== null}
        <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link it to {ticket.key}</label>
        <p class="tf-dim tf-note">Links stay in knobas and are never written to a source.</p>
      {/if}
    {/if}
    {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
  {/snippet}
  {#snippet footer()}
    {#if row !== null}
      <button type="button" class="tf-btn tf-ghost tf-dp-del" class:tf-danger={sure} onclick={() => void remove()}
        >{sure ? `Delete ${row.name}? Links to it stay, marked deleted.` : "Delete…"}</button
      >
    {/if}
    <span class="tf-dp-gap"></span>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    {#if routeId !== null}
      <button type="button" class="tf-btn tf-primary" disabled={writing || row === null} onclick={() => void save()}>Save</button>
    {:else}
      <button type="button" class="tf-btn tf-primary" disabled={writing} onclick={() => void save()}>Create route</button>
    {/if}
  {/snippet}
</TfDialog>
