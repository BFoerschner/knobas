<!--
  The asset editor (#875, R-11.19, R-11.19a): one dialog over `get_asset`, in
  prototype-6's two columns. Left, where it sits (*Inside*, which moves it with
  everything it holds), environment and owner with *Inherit*, its own status,
  typed then custom properties and *Add a property*. Right, *Depends on*,
  *Needed by*, *Watched by* with *Create monitor*, and linked work.

  *Needed by* (#1457) keeps the direct `depends-on` links as chips, the part a
  link edit changes, and beneath them reads `depends_on_this`: every asset that
  breaks if this one goes down, nearest first and counted, then the routes
  landing on it or on any of those (#1553), listed and not counted, as the
  Tree's *Depends on this* does.

  *Save* runs `asset-edit.ts`' order: one `edit_asset` with only the changed
  fields, then `move_asset`, then the new `depends-on` links. A failure keeps
  the dialog open, says which part did not save and keeps what did, so the
  next *Save* sends only the rest. *Delete…* is disabled with the reason while
  the asset holds assets or exposes routes; its confirm counts the routes that
  will land on nothing, on a line of its own above the footer's buttons
  (#1458), and a `conflict` from the backend shows in place.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { AssetDetail, AssetType, DependsOnThis, Environment, Dependencies } from "../../ipc/assets";
  import { readingOf } from "../../detail/relations";
  import TfDialog from "../settings/TfDialog.svelte";
  import {
    deleteBlock,
    deleteConfirm,
    DEPENDS_ON,
    failureLine,
    formOf,
    planOf,
    runSave,
    type AssetForm,
  } from "./asset-edit";
  import { kindOf } from "./deps";
  import { assetPaths, placesOutside } from "./estate-paths";
  import { ENVIRONMENTS } from "./views";
  import type { EditorPorts } from "./ports";

  let {
    assetId,
    read,
    ports,
    onclose,
    onsaved,
    ondeleted,
    oncreatemonitor,
  }: {
    assetId: string;
    /** Dependencies' read: where *Inside* and *Depends on* pick from. */
    read: Dependencies;
    ports: EditorPorts;
    onclose: () => void;
    /** Everything saved: the asset's name, and whether it moved. */
    onsaved: (what: { name: string; moved: boolean }) => void;
    ondeleted: (name: string) => void;
    /** *Create monitor*: the page opens its dialog for this asset. */
    oncreatemonitor: (detail: AssetDetail) => void;
  } = $props();

  const STATUSES = [
    { value: "none", label: "Not set, monitors decide" },
    { value: "up", label: "Up" },
    { value: "warn", label: "Warning" },
    { value: "down", label: "Down" },
  ] as const;

  let detail = $state.raw<AssetDetail | null>(null);
  let types = $state.raw<AssetType[]>([]);
  let form = $state<AssetForm | null>(null);
  let failure = $state<string | null>(null);
  let saving = $state(false);
  let moved = false;
  let sure = $state(false);
  let deleteFailure = $state<string | null>(null);
  /** `depends_on_this`'s answer; `null` while it reads or when it failed. */
  let breaks = $state.raw<DependsOnThis | null>(null);
  let breaksFailure = $state<string | null>(null);

  const uid = $props.id();

  async function load() {
    try {
      const [read, all] = await Promise.all([ports.getAsset(assetId), ports.assetTypes()]);
      detail = read;
      types = all;
      form = formOf(read, all);
    } catch (error) {
      failure = `The asset could not be read: ${ipcErrorMessage(error)}`;
    }
  }
  void load();

  /** Its own read beside `load`: a failure here costs *Needed by*'s closure, not the editor. */
  async function readBreaks() {
    try {
      breaks = await ports.dependsOnThis(assetId);
    } catch (error) {
      breaksFailure = `What breaks could not be read: ${ipcErrorMessage(error)}`;
    }
  }
  void readBreaks();

  const paths = $derived(assetPaths(read));
  const places = $derived(placesOutside(paths, assetId));
  const type = $derived(detail ? types.find((t) => t.id === detail!.asset.type_id) : undefined);
  const kind = $derived(detail ? kindOf(read.kinds, detail.asset.type_id) : null);
  const dependsOn = $derived(
    detail?.links.filter((entry) => entry.link.relation === DEPENDS_ON && entry.link.from_id === assetId) ?? [],
  );
  const neededBy = $derived(detail?.links.filter((entry) => entry.link.relation === DEPENDS_ON && entry.link.to_id === assetId) ?? []);
  const breaksNothing = $derived(breaks !== null && breaks.assets.length === 0 && breaks.routes.length === 0);
  const work = $derived(detail?.links.filter((entry) => ["ticket", "pr", "build", "page", "commit"].includes(entry.other.kind)) ?? []);
  const candidates = $derived(
    paths.places.filter((place) => place.id !== assetId && !dependsOn.some((e) => e.other.entity_id === place.id) && !form?.dependsOn.includes(place.id)),
  );
  const blocked = $derived(detail ? deleteBlock(detail) : null);
  const health = $derived.by(() => {
    const own = read.items.find((item) => item.id === assetId);
    return own?.health ?? null;
  });

  async function save() {
    if (detail === null || form === null || saving) return;
    let plan;
    try {
      plan = planOf(detail, form);
    } catch (error) {
      failure = ipcErrorMessage(error);
      return;
    }
    saving = true;
    const outcome = await runSave(ports, assetId, plan);
    saving = false;
    if (outcome.saved.includes("move")) moved = true;
    if (outcome.failed === null) {
      onsaved({ name: form.name.trim() || detail.asset.name, moved });
      return;
    }
    failure = failureLine(outcome);
    // Keep what saved: read the asset again, and leave the form as typed, less the links that went.
    try {
      const again = await ports.getAsset(assetId);
      const left = form.dependsOn.filter((id) => !outcome.linked.includes(id));
      if (outcome.saved.includes("fields")) {
        form = { ...formOf(again, types), parent: form.parent, dependsOn: left };
      } else form.dependsOn = left;
      detail = again;
    } catch {
      // The failure line already says what did not save.
    }
  }

  async function remove() {
    if (detail === null) return;
    if (!sure) {
      sure = true;
      return;
    }
    try {
      await ports.deleteAsset(assetId);
      ondeleted(detail.asset.name);
    } catch (error) {
      deleteFailure = ipcErrorMessage(error);
      sure = false;
    }
  }

  /** What *Inherit* would give: the nearest holder that sets it. */
  const inheritEnv = $derived((detail ? [...detail.held_by].reverse() : []).find((row) => row.environment !== null) ?? null);
  const inheritOwner = $derived((detail ? [...detail.held_by].reverse() : []).find((row) => row.owner !== null) ?? null);
  /** A linked work item's own address (the shell conventions' *Routes and addresses*). */
  const workHref = (kind: string, id: string) =>
    `#/${{ ticket: "ticket", pr: "pr", build: "build", page: "page", commit: "commit" }[kind] ?? "item"}/${encodeURIComponent(id)}`;
  const nameOf = (id: string) => read.items.find((item) => item.id === id)?.name ?? id;
</script>

<TfDialog title={detail ? `${kind?.monogram ?? ""} ${detail.asset.name}`.trim() : "Asset"} {onclose} className="tf-dp-edit">
  {#snippet body()}
    {#if detail !== null && form !== null}
      <p class="tf-dim tf-dp-ehead">
        {type?.label ?? detail.asset.type_label}.
        {#if health === "err"}<span class="tf-chip tf-err">down</span>{:else if health === "warn"}<span class="tf-chip tf-warn">warning</span>{/if}
        {detail.monitoring.length > 0 ? `${detail.monitoring.length} ${detail.monitoring.length === 1 ? "check" : "checks"}` : "no check"}
      </p>
      <div class="tf-dp-ecols">
        <div>
          <div class="tf-formgrid">
            <label for="{uid}-name">Name</label>
            <input id="{uid}-name" type="text" class="tf-mono" autocomplete="off" bind:value={form.name} />
          </div>
          <h4>Where it sits</h4>
          <div class="tf-formgrid">
            <label for="{uid}-in">Inside</label>
            <select id="{uid}-in" value={form.parent ?? ""} onchange={(e) => (form!.parent = e.currentTarget.value || null)}>
              <option value="">Top of the estate</option>
              {#each places as place (place.id)}<option value={place.id}>{place.path}</option>{/each}
            </select>
          </div>
          <p class="tf-dim tf-note">Picking another place moves it, with everything it holds.</p>
          <div class="tf-formgrid">
            <label for="{uid}-env">Environment</label>
            <select id="{uid}-env" value={form.environment ?? ""} onchange={(e) => (form!.environment = (e.currentTarget.value || null) as Environment | null)}>
              <option value="">Inherit{inheritEnv ? ` (${inheritEnv.environment} from ${inheritEnv.name})` : ""}</option>
              {#each ENVIRONMENTS as env (env)}<option value={env}>{env}</option>{/each}
            </select>
            <label for="{uid}-owner">Owner</label>
            <input id="{uid}-owner" type="text" autocomplete="off" bind:value={form.owner} placeholder={inheritOwner ? `Inherit (${inheritOwner.owner})` : "Nobody"} />
            <label for="{uid}-status">Own status</label>
            <select id="{uid}-status" bind:value={form.status}>
              {#each STATUSES as status (status.value)}<option value={status.value}>{status.label}</option>{/each}
            </select>
          </div>
          <h4>Properties</h4>
          <div class="tf-formgrid">
            {#each form.properties as property, index (property.key)}
              <label for="{uid}-p{index}" class:tf-dp-ck={property.custom}>{property.label}</label>
              <input
                id="{uid}-p{index}"
                type={property.kind === "number" ? "number" : property.kind === "url" ? "url" : property.kind === "date" ? "date" : "text"}
                placeholder={property.custom ? "" : property.kind}
                bind:value={property.value}
              />
            {/each}
            {#each form.added as row, index (index)}
              <input type="text" placeholder="key" aria-label="Key" bind:value={row.key} />
              <input type="text" placeholder="value" aria-label="Value" bind:value={row.value} />
            {/each}
          </div>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => form!.added.push({ key: "", value: "" })}>+ Add a property</button>
        </div>
        <div>
          <h4>Depends on</h4>
          <div class="tf-dp-rels">
            {#each dependsOn as entry (entry.link.id)}<span class="tf-chip">{entry.other.title}</span>{/each}
            {#each form.dependsOn as id (id)}<span class="tf-chip">{nameOf(id)}</span>{/each}
            {#if dependsOn.length === 0 && form.dependsOn.length === 0}<span class="tf-dim">Nothing recorded</span>{/if}
          </div>
          <select
            aria-label="Add something it depends on"
            value=""
            onchange={(e) => {
              const id = e.currentTarget.value;
              if (id) form!.dependsOn.push(id);
              e.currentTarget.value = "";
            }}
          >
            <option value="">Add what it depends on…</option>
            {#each candidates as place (place.id)}<option value={place.id}>{place.name}</option>{/each}
          </select>
          <h4>Needed by</h4>
          {#if neededBy.length > 0}
            <div class="tf-dp-rels">{#each neededBy as entry (entry.link.id)}<span class="tf-chip">{entry.other.title}</span>{/each}</div>
          {:else if breaksNothing}
            <div class="tf-dp-rels"><span class="tf-dim">Nothing recorded</span></div>
          {/if}
          {#if breaksFailure !== null}
            <p class="tf-ifail">{breaksFailure}</p>
          {:else if breaks === null}
            <p class="tf-dim tf-note">Reading what breaks…</p>
          {:else if !breaksNothing}
            <div class="tf-dp-breaks">
              {#if breaks.assets.length > 0}
                <p class="tf-dp-breaks-h">Depends on this <span class="tf-dp-breaks-n">{breaks.assets.length}</span></p>
                <ul>
                  {#each breaks.assets as row (row.asset.id)}
                    <li>{row.asset.name} <span class="tf-dim">{row.relation === null ? "inside" : readingOf(row.relation, true)}</span></li>
                  {/each}
                </ul>
              {/if}
              {#if breaks.routes.length > 0}
                <p class="tf-dp-breaks-h">Reachable via routes that would break</p>
                <ul>
                  {#each breaks.routes as route (route.id)}
                    <li>{route.name} <span class="tf-dim tf-mono">{route.url}</span></li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}
          {#if detail.holds.length > 0}
            <p class="tf-dim tf-note">and the {detail.holds.length} {detail.holds.length === 1 ? "thing" : "things"} inside it</p>
          {/if}
          <h4>Watched by</h4>
          {#each detail.monitoring as monitor (monitor.entity_id)}
            <div class="tf-dp-watch">
              <span class="tf-chip" class:tf-ok={monitor.state === "up"} class:tf-err={monitor.state === "down"}>{monitor.state ?? "paused"}</span>
              {monitor.name}
            </div>
          {:else}
            <div class="tf-dim tf-dp-watch">
              Nothing. <button type="button" class="tf-linkish" onclick={() => oncreatemonitor(detail!)}>Create monitor</button>
            </div>
          {/each}
          <h4>Linked work</h4>
          <div class="tf-dp-rels">
            {#each work as entry (entry.link.id)}
              <a class="tf-chip tf-mono" href={workHref(entry.other.kind, entry.other.entity_id)} onclick={onclose}>{entry.other.title}</a>
            {:else}<span class="tf-dim">None</span>{/each}
          </div>
        </div>
      </div>
      {#if deleteFailure !== null}<p class="tf-ifail" role="alert">{deleteFailure}</p>{/if}
    {/if}
    {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
  {/snippet}
  {#snippet footer()}
    {#if detail !== null}
      <!-- The confirm sentence on a line of its own above the buttons, the button short (#1458). -->
      {#if sure}<p class="tf-dp-sure" id="{uid}-sure" role="status">{deleteConfirm(detail)}</p>{/if}
      <button
        type="button"
        class="tf-btn tf-ghost tf-dp-del"
        class:tf-danger={sure}
        disabled={blocked !== null}
        title={blocked ?? undefined}
        aria-describedby={sure ? `${uid}-sure` : undefined}
        onclick={() => void remove()}>{sure ? `Delete ${detail.asset.name}` : "Delete…"}</button
      >
      <a class="tf-btn tf-ghost" href="#/asset/{encodeURIComponent(assetId)}" onclick={onclose}>Open in the asset tree</a>
    {/if}
    <span class="tf-dp-gap"></span>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={form === null || saving} onclick={() => void save()}>Save</button>
  {/snippet}
</TfDialog>
