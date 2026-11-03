<!--
  One asset's own page: `#/asset/<id>`, and `#/route/<id>` for the asset that
  exposes the route, with the route picked.

  Split out of the asset tree's pane, which held all of this in a column a
  third of the window wide beside the columns: a long property wrapped to a
  word a line, and filling an asset in meant one click, one field and one
  *Save* per value. The tree now finds and arranges (`assets/AssetsView`),
  and this page reads and edits.

  **The layout is the shell's detail page.** The canonical header (title
  block, then `.tf-acts`: the primary, the other actions, *Show in tree* as
  the ghost), then two columns: what is stored on the asset and how it is
  reached in the wide one (*Properties*, *Routes*, *Holds*), and what is in
  force on it, what breaks with it, what watches it, what it is linked to and
  what happened to it in the narrow one.

  **Editing is one gesture.** *Edit*, or a click on any value, turns the name,
  *In force* and *Properties* into one form in place, the same grid, so
  nothing moves under the reader; `Tab` walks every field, `Enter` (`⌘Enter`
  in a box) saves them all as one `edit_asset` and `Esc` puts the form away.
  The rules are the asset editor's (`deps/asset-edit.ts`): only what changed
  is sent, an emptied field clears its property, and a refusal is shown in
  the backend's words with the form left as typed. Where it sits is a move,
  not a field, so it stays *Move…*, the tree's own picker.

  **Every write is followed by a re-read**, never a patch: the history line a
  write makes is the backend's, and arrives only by reading.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { AssetDetail, AssetRow, AssetType, AttachedMonitor, DependsOnThis, Environment, RouteDetail, RouteRow } from "../../ipc/assets";
  import type { LinkEntry } from "../../ipc/entity";
  import CreateDialog from "../../assets/CreateDialog.svelte";
  import MonitorDialog from "../../assets/MonitorDialog.svelte";
  import MoveDialog from "../../assets/MoveDialog.svelte";
  import RouteDialog from "../../assets/RouteDialog.svelte";
  import { inputTypeFor, PROPERTY_KINDS } from "../../assets/editing";
  import { historyLine, nameIn } from "../../assets/history";
  import { addressOf, landingOf, pageOf, routeAddressOf, sourceOf } from "../../assets/tree";
  import LinkDialog from "../../detail/LinkDialog.svelte";
  import LinksPanel from "../../detail/LinksPanel.svelte";
  import { readingOf as relationReading } from "../../detail/relations";
  import { latestRead } from "../../shell/latest-read";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import { entityAddress } from "../address";
  import { deleteConfirm, editsOf, formOf, type AssetForm } from "../deps/asset-edit";
  import { ENVIRONMENTS } from "../deps/views";
  import type { EscTarget } from "../escape";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import {
    blankRow,
    filledOf,
    groupsOf,
    inheritedOf,
    isCode,
    monitorTone,
    readingOf,
    shownValue,
    STATUSES,
    wantsBox,
  } from "./asset-page";
  import { REAL_PORTS, type AssetPagePorts } from "./ports";

  let {
    assetId,
    routeId = null,
    ready = true,
    back = null,
    ports,
    now = () => new Date(),
    go = (hash: string) => {
      location.hash = hash;
    },
  }: {
    /** The asset the address names; `null` on a route's address until the route is read. */
    assetId: string | null;
    /** The route `#/route/<id>` names, picked on its asset's page. */
    routeId?: string | null;
    /** Whether the database answers yet: the reads wait for it. */
    ready?: boolean;
    /** Where `Esc` goes, for the breadcrumb's hint. */
    back?: Pick<EscTarget, "name"> | null;
    ports?: Partial<AssetPagePorts>;
    /** What the history's *ago* readings are relative to. */
    now?: () => Date;
    /** Follow an address; the shell's is `location.hash`. */
    go?: (hash: string) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init: a bridge swapped mid-life would leave what is drawn
  // read through one set of ports and re-read through another.
  const io: AssetPagePorts = { ...REAL_PORTS, ...ports };
  const uid = $props.id();

  // -- the reads ------------------------------------------------------------

  let routeDetail = $state.raw<RouteDetail | null>(null);
  let detail = $state.raw<AssetDetail | null>(null);
  let failure = $state<string | null>(null);
  let reading = $state(false);
  let types = $state.raw<AssetType[]>([]);
  let dependsOn = $state.raw<DependsOnThis | null>(null);
  let dependsFailure = $state<string | null>(null);
  /** Bumped by every write; the reads below re-run on it. */
  let revision = $state(0);

  const detailRead = latestRead<AssetDetail>();
  const dependsRead = latestRead<DependsOnThis>();
  const routeRead = latestRead<RouteDetail>();

  /** The asset drawn: the address's, or the one exposing the address's route. */
  const shownId = $derived.by(() => {
    if (assetId !== null) return assetId;
    if (routeId === null || routeDetail === null) return null;
    return routeDetail.route.id === routeId ? routeDetail.route.asset_id : null;
  });

  $effect(() => {
    const id = routeId;
    void revision;
    if (id === null || !ready) {
      routeDetail = null;
      return;
    }
    void routeRead(() => io.getRoute(id), {
      ok: (answer) => (routeDetail = answer),
      fail: (cause) => {
        routeDetail = null;
        failure = ipcErrorMessage(cause);
      },
    });
  });

  $effect(() => {
    if (!ready) return;
    void io
      .assetTypes()
      .then((answer) => (types = answer))
      .catch(() => {
        // *New asset inside* is the one thing the table is for here; its
        // button says the table is missing by being disabled.
      });
  });

  /** Which asset the open form and dialogs belong to: they go when the page draws another. */
  let editorsFor: string | null = null;

  $effect(() => {
    const id = shownId;
    void revision;
    if (id !== editorsFor) {
      editorsFor = id;
      closeEditors();
      dependsOn = null;
      dependsFailure = null;
    }
    if (id === null || !ready) return;
    reading = true;
    void detailRead(() => io.getAsset(id), {
      ok: (answer) => {
        detail = answer;
        failure = null;
        reading = false;
      },
      fail: (cause) => {
        detail = null;
        failure = ipcErrorMessage(cause);
        reading = false;
      },
    });
    void dependsRead(() => io.dependsOnThis(id), {
      ok: (answer) => {
        dependsOn = answer;
        dependsFailure = null;
      },
      fail: (cause) => {
        dependsOn = null;
        dependsFailure = ipcErrorMessage(cause);
      },
    });
  });

  /** A read for another asset than the address names is not drawn: the frame between two addresses. */
  const shown = $derived(detail !== null && detail.asset.id === shownId ? detail : null);
  const phase = $derived(waitPhase({ drawn: shown !== null, reading, failed: failure !== null }));

  const crumbs = $derived<Crumb[]>([
    { name: "Dependencies", href: "#/deps" },
    { name: "Asset tree", href: shown === null ? "#/assets/tree" : addressOf(shown.asset) },
    { name: shown?.asset.name ?? "Asset" },
  ]);

  const holdsWindow = new Windowed({ count: () => detail?.holds.length ?? 0, key: (at) => detail!.holds[at]!.id, estimate: 30, gap: 0 });
  const dependsWindow = new Windowed({
    count: () => dependsOn?.assets.length ?? 0,
    key: (at) => dependsOn!.assets[at]!.asset.id,
    estimate: 30,
    gap: 0,
  });

  // -- writes ---------------------------------------------------------------

  let writing = $state(false);
  /** What the last write refused with, under the header rather than as a toast. */
  let writeFailure = $state<string | null>(null);

  let moving = $state(false);
  let deleting = $state(false);
  let linking = $state(false);
  let creatingMonitor = $state(false);
  let creating = $state(false);
  let exposing = $state(false);
  let editingRoute = $state<RouteRow | null>(null);

  /** Run one write and read again; `null` when it was refused or one is already out. */
  async function write<T>(action: () => Promise<T>, reread = true): Promise<T | null> {
    if (writing) return null;
    writing = true;
    writeFailure = null;
    try {
      const answer = await action();
      if (reread) revision += 1;
      return answer;
    } catch (cause) {
      writeFailure = ipcErrorMessage(cause);
      return null;
    } finally {
      writing = false;
    }
  }

  function closeEditors() {
    editing = false;
    moving = false;
    deleting = false;
    linking = false;
    creatingMonitor = false;
    creating = false;
    exposing = false;
    editingRoute = null;
    writeFailure = null;
  }

  // -- the form -------------------------------------------------------------

  let editing = $state(false);
  let form = $state<AssetForm | null>(null);

  /** Turn the page into its form, focusing the field a click named (the name when none did). */
  async function beginEdit(focus: string = `${uid}-name`) {
    const current = shown;
    if (current === null) return;
    form = formOf(current, types);
    editing = true;
    writeFailure = null;
    await tick();
    const field = document.getElementById(focus);
    if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement || field instanceof HTMLSelectElement) {
      field.focus();
      if (field instanceof HTMLInputElement && field.type === "text") field.select();
    }
  }

  /** Put the form away. `form` stays until the next *Edit* rebuilds it, so a keystroke in flight still finds it. */
  function cancelEdit() {
    editing = false;
    writeFailure = null;
  }

  async function save() {
    const current = shown;
    if (current === null || form === null) return;
    let edits;
    try {
      edits = editsOf(current, form);
    } catch (error) {
      writeFailure = error instanceof Error ? error.message : String(error);
      return;
    }
    if (edits.length === 0) {
      cancelEdit();
      return;
    }
    const done = await write(() => io.editAsset(current.asset.id, edits));
    if (done !== null) editing = false;
  }

  async function addRow() {
    if (form === null) return;
    form.added.push(blankRow());
    await tick();
    if (form === null) return;
    document.getElementById(`${uid}-a${form.added.length - 1}`)?.focus();
  }

  /** `Enter` in a line saves the form; in a box it is a new line, and `⌘Enter` saves. */
  function fieldKey(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.isComposing) return;
    const box = event.currentTarget instanceof HTMLTextAreaElement;
    if (box && !(event.metaKey || event.ctrlKey)) return;
    event.preventDefault();
    void save();
  }

  /** `Esc` puts the form away and goes no further: the shell's `Esc` would leave the page. */
  function onkeydown(event: KeyboardEvent) {
    if (!editing || event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    cancelEdit();
  }

  const inherited = $derived(shown === null ? { environment: null, owner: null } : inheritedOf(shown));

  // -- the rest -------------------------------------------------------------

  async function confirmDelete() {
    const current = shown;
    if (current === null) return;
    const parent = current.held_by.at(-1) ?? null;
    const done = await write(async () => {
      await io.deleteAsset(current.asset.id);
      return true;
    }, false);
    if (done === null) return;
    deleting = false;
    push({ text: `Deleted ${current.asset.name}` });
    // Where it was, in the tree: the asset's own address would answer
    // `not_found` over a deletion that worked.
    go(parent === null ? "#/assets/tree" : addressOf(parent));
  }

  async function openInBrowser(url: string) {
    try {
      await io.openExternal(url);
    } catch (rejection) {
      writeFailure = `Could not open the link: ${ipcErrorMessage(rejection)}`;
    }
  }

  async function removeLink(entry: LinkEntry) {
    await write(() => io.unlink(entry.link.id));
  }

  function monitorAddress(watch: AttachedMonitor): string {
    return entityAddress("monitor", watch.entity_id);
  }

  /** The names the estate file kept that have not found a monitor yet. */
  const namedAndWaiting = $derived(
    shown === null ? [] : shown.monitors.filter((name) => !shown.monitoring.some((watch) => watch.name === name)),
  );

  const nameOf = (value: unknown) => nameIn(shown, value);
  const typeOf = $derived(shown === null ? undefined : types.find((type) => type.id === shown.asset.type_id));
</script>

<!--
  The page reads `Esc` while its form is open, before the shell's handler on
  `window` would take it as *go back*. It is not a control and takes no focus.
-->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tf-full tf-as" {onkeydown}>
  <TfCrumbs parts={crumbs} {back} />
  <TfWaiting {phase} error={failure} shape="lines" count={10}>
    {#if shown !== null}
      {@const asset = shown.asset}
      {@const health = readingOf(asset.health)}
      <header class="tf-fhead tf-as-head">
        <span class="tf-dp-mono tf-as-mg" title={asset.type_label}>{asset.monogram}</span>
        <div class="tf-as-titles">
          <div class="tf-as-titleline">
            {#if editing && form !== null}
              <input
                id="{uid}-name"
                class="tf-as-name"
                type="text"
                aria-label="Name"
                autocomplete="off"
                spellcheck="false"
                bind:value={form.name}
                onkeydown={fieldKey}
              />
            {:else}
              <h1>{asset.name}</h1>
            {/if}
            <!-- State only where there is some: an unrated asset says so in *In force*, not beside every title. -->
            {#if health.tone !== "muted"}
              <span class="tf-chip tf-{health.tone}" title="Health: the worst of its own status, its monitors and what it holds">{health.word}</span>
            {/if}
          </div>
          <p class="tf-sub">
            {asset.type_label}
            {#if shown.held_by.length === 0}
              at the top of the estate
            {:else}
              in
              {#each shown.held_by as held, index (held.id)}
                {#if index > 0}{" "}<span class="tf-as-sep" aria-hidden="true">›</span>{/if}
                <a href={pageOf(held)}>{held.name}</a>
              {/each}
            {/if}
          </p>
        </div>
        <div class="tf-acts">
          {#if editing}
            <button class="tf-btn tf-primary" disabled={writing} onclick={() => void save()}>{writing ? "Saving…" : "Save"}</button>
            <button class="tf-btn tf-ghost" onclick={cancelEdit}>Cancel</button>
          {:else}
            <button class="tf-btn tf-primary" onclick={() => void beginEdit()}>Edit</button>
            <button class="tf-btn" onclick={() => (moving = true)}>Move…</button>
            <button
              class="tf-btn"
              onclick={() => {
                linking = true;
                writeFailure = null;
              }}>Link to…</button
            >
            <button class="tf-btn tf-danger" onclick={() => (deleting = true)}>Delete…</button>
            <a class="tf-btn tf-ghost" href={addressOf(asset)}>Show in tree</a>
          {/if}
        </div>
      </header>

      {#if writeFailure !== null && !deleting}
        <p class="tf-banner tf-err" role="alert">{writeFailure}</p>
      {/if}

      <div class="tf-as-body">
        <div class="tf-as-col">
          {@render properties(shown)}
          {@render routes(shown)}
          {@render holds(shown)}
        </div>
        <div class="tf-as-col">
          {@render inForce(shown)}
          {@render breaks()}
          {@render monitoring(shown)}
          <LinksPanel
            entityId={asset.id}
            links={shown.links}
            onopen={go}
            onunlink={(entry) => void removeLink(entry)}
            onlink={() => {
              linking = true;
              writeFailure = null;
            }}
          />
          {@render history(shown)}
        </div>
      </div>
    {/if}
  </TfWaiting>
</div>

<!-- What is stored on it: the type's properties, then the rest. -->
{#snippet properties(on: AssetDetail)}
  {@const groups = groupsOf(on.properties)}
  {@const filled = filledOf(on.properties)}
  <section class="tf-panel tf-as-props" aria-label="Properties">
    <header>
      <h3>Properties</h3>
      {#if filled.of > 0}<span class="tf-as-n">{filled.set} of {filled.of} set</span>{/if}
      {#if !editing}
        <span class="tf-trail"><button class="tf-btn tf-sm" onclick={() => void beginEdit(`${uid}-p0`)}>Edit</button></span>
      {/if}
    </header>
    {#if editing && form !== null}
      {#if form.properties.length > 0}
        <div class="tf-formgrid tf-as-form">
          {#each form.properties as field, index (field.key)}
            <label for="{uid}-p{index}" class:tf-as-own={field.custom}>{field.label}</label>
            {#if wantsBox(field.kind, String(field.value ?? ""))}
              <textarea id="{uid}-p{index}" rows="3" bind:value={field.value} onkeydown={fieldKey}></textarea>
            {:else}
              <input
                id="{uid}-p{index}"
                type={inputTypeFor(field.kind)}
                class:tf-mono={field.kind === "url"}
                autocomplete="off"
                spellcheck="false"
                placeholder={field.custom ? "" : `Not set`}
                bind:value={field.value}
                onkeydown={fieldKey}
              />
            {/if}
          {/each}
        </div>
      {/if}
      {#each form.added as row, index (index)}
        <div class="tf-as-addrow">
          <input
            id="{uid}-a{index}"
            type="text"
            aria-label="New property key"
            placeholder="backup window"
            autocomplete="off"
            spellcheck="false"
            bind:value={row.key}
            onkeydown={fieldKey}
          />
          <!-- The kind is asked for rather than guessed: `"8080"` and `8080` are different properties. -->
          <select aria-label="New property kind" bind:value={row.kind}>
            {#each PROPERTY_KINDS as offered (offered.kind)}
              <option value={offered.kind}>{offered.label}</option>
            {/each}
          </select>
          <input
            type={inputTypeFor(row.kind ?? "text")}
            aria-label="New property value"
            autocomplete="off"
            spellcheck="false"
            bind:value={row.value}
            onkeydown={fieldKey}
          />
          <button class="tf-iconbtn tf-sm" aria-label="Remove this row" title="Remove this row" onclick={() => form!.added.splice(index, 1)}>×</button>
        </div>
      {/each}
      <div class="tf-as-formfoot">
        <button class="tf-btn tf-sm" onclick={() => void addRow()}>Add a property</button>
        <span class="tf-dim">Empty a field to clear it. Enter saves, Esc cancels.</span>
      </div>
    {:else}
      {#if groups.typed.length > 0}
        {@render values(groups.typed, 0)}
      {/if}
      {#if groups.other.length > 0}
        <h4 class="tf-as-sub">Other properties</h4>
        {@render values(groups.other, groups.typed.length)}
      {/if}
      {#if on.properties.length === 0}
        <p class="tf-as-none">
          {typeOf?.properties.length === 0 || typeOf === undefined ? "Its type declares no properties, and none were added." : "Nothing set yet."}
          <button class="tf-linkish" onclick={() => void beginEdit().then(addRow)}>Add a property</button>
        </p>
      {/if}
    {/if}
  </section>
{/snippet}

<!--
  The value is the control: a click turns the page into its form with that
  field focused. An unset one says so and is still a button, because the
  empty one is exactly the property a reader came to fill in.
-->
{#snippet values(list: AssetDetail["properties"], offset: number)}
  <dl class="tf-as-kv">
    {#each list as property, index (property.key)}
      {@const value = shownValue(property)}
      <div>
        <dt>{property.label}</dt>
        <dd>
          <button
            class="tf-as-val"
            class:tf-mono={isCode(property)}
            class:tf-as-unset={value === null}
            title="Edit {property.label}"
            onclick={() => void beginEdit(`${uid}-p${offset + index}`)}>{value ?? "Not set"}</button
          >
        </dd>
      </div>
    {/each}
  </dl>
{/snippet}

<!-- What it offers and how it is reached: two lists, two questions. -->
{#snippet routes(on: AssetDetail)}
  <section class="tf-panel" aria-label="Routes">
    <header>
      <h3>Routes</h3>
      <span class="tf-trail">
        <button
          class="tf-btn tf-sm"
          onclick={() => {
            exposing = true;
            writeFailure = null;
          }}>Add a route</button
        >
      </span>
    </header>
    <h4 class="tf-as-sub">Exposes</h4>
    {#if on.exposes.length === 0}
      <p class="tf-as-none">No routes.</p>
    {:else}
      <ul class="tf-as-routes">
        {#each on.exposes as route (route.id)}
          {@render routeRow(route, true, lands)}
          {#snippet lands()}
            {#if route.target_id !== null}
              {@const to = route.target_id}
              lands on <button class="tf-linkish" onclick={() => go(pageOf({ id: to }))}>{route.target_name}</button>
            {:else}
              lands on nothing knobas knows
            {/if}
          {/snippet}
        {/each}
      </ul>
    {/if}
    <h4 class="tf-as-sub">Reachable via</h4>
    {#if on.reachable_via.length === 0}
      <p class="tf-as-none">Nothing lands here.</p>
    {:else}
      <ul class="tf-as-routes">
        {#each on.reachable_via as route (route.id)}
          {@render routeRow(route, false, arrives)}
          {#snippet arrives()}
            {@const landing = landingOf(on, route, pageOf)}
            {#if landing.goTo}
              {@const there = landing.goTo}
              <button class="tf-linkish" onclick={() => go(there)}>{landing.note}</button>
            {:else}
              {landing.note}
            {/if}, exposed by
            <button class="tf-linkish" onclick={() => go(pageOf({ id: route.asset_id }))}>{route.asset_name}</button>
          {/snippet}
        {/each}
      </ul>
    {/if}
  </section>
{/snippet}

{#snippet routeRow(route: RouteRow, own: boolean, where: import("svelte").Snippet)}
  {@const picked = route.id === routeId}
  <li class="tf-as-route" class:tf-sel={picked} aria-current={picked ? "true" : undefined}>
    <div class="tf-as-rline">
      <button class="tf-as-rname" onclick={() => go(routeAddressOf(route))}>{route.name}</button>
      {#if route.visibility === "public"}<span class="tf-chip">public</span>{/if}
      <button class="tf-as-url tf-mono" title="Open {route.url}" onclick={() => void openInBrowser(route.url)}>{route.url}</button>
      {#if own}
        <span class="tf-trail">
          <button
            class="tf-btn tf-sm tf-ghost"
            aria-label="Edit {route.name}"
            onclick={() => {
              editingRoute = route;
              writeFailure = null;
            }}>Edit…</button
          >
        </span>
      {/if}
    </div>
    <p class="tf-as-rmeta">{@render where()}</p>
    {#if route.properties.length > 0}
      <dl class="tf-as-rprops">
        {#each route.properties as property (property.key)}
          <div><dt>{property.label}</dt> <dd>{shownValue(property) ?? "not set"}</dd></div>
        {/each}
      </dl>
    {/if}
    <!-- A route exposed here and landing inside is in both lists: its history goes under the first. -->
    {#if picked && (own || route.asset_id !== shownId) && routeDetail !== null && routeDetail.route.id === route.id}
      <div class="tf-as-rhist">
        <h4 class="tf-as-sub">{route.name} history</h4>
        {#if routeDetail.history.length === 0}
          <p class="tf-as-none">Nothing recorded.</p>
        {:else}
          <ol class="tf-as-hist">
            {#each routeDetail.history as entry (entry.id)}
              <li><span>{historyLine(entry.verb, entry.detail, nameOf)}</span><span class="tf-dim">{ago(entry.at, now())}</span></li>
            {/each}
          </ol>
        {/if}
      </div>
    {/if}
  </li>
{/snippet}

{#snippet holds(on: AssetDetail)}
  <section class="tf-panel" aria-label="Holds">
    <header>
      <h3>Holds</h3>
      {#if on.holds.length > 0}<span class="tf-as-n">{on.holds.length}</span>{/if}
      <span class="tf-trail">
        <button class="tf-btn tf-sm" disabled={types.length === 0} onclick={() => (creating = true)}>New asset inside</button>
      </span>
    </header>
    {#if on.holds.length === 0}
      <p class="tf-as-none">Nothing inside it.</p>
    {:else}
      <ul class="tf-as-list" {@attach holdsWindow.list}>
        {#each holdsWindow.slice(on.holds) as held (held.id)}
          {@const tone = readingOf(held.health).tone}
          <li {@attach holdsWindow.row(held.id)}>
            <span class="tf-dp-mono" title={held.type_label}>{held.monogram}</span>
            <button class="tf-as-to" onclick={() => go(pageOf(held))}>{held.name}</button>
            <span class="tf-dim">{held.type_label}</span>
            {#if tone !== "muted"}<span class="tf-dot tf-{tone}" title={readingOf(held.health).word}></span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/snippet}

<!--
  What the asset *has*, against what is stored on it: environment and owner
  walk up to the nearest asset that sets them, and health is the worst of
  this asset and everything under it, so each says where it comes from. In
  the form it is what is set *here*: *Inherit* clears it.
-->
{#snippet inForce(on: AssetDetail)}
  <section class="tf-panel" aria-label="In force">
    <header><h3>In force</h3></header>
    {#if editing && form !== null}
      <div class="tf-formgrid tf-as-form">
        <label for="{uid}-env">Environment</label>
        <select
          id="{uid}-env"
          value={form.environment ?? ""}
          onchange={(e) => (form!.environment = (e.currentTarget.value || null) as Environment | null)}
        >
          <option value="">Inherit{inherited.environment ? ` (${inherited.environment})` : ""}</option>
          {#each ENVIRONMENTS as env (env)}<option value={env}>{env}</option>{/each}
        </select>
        <label for="{uid}-owner">Owner</label>
        <input
          id="{uid}-owner"
          type="text"
          autocomplete="off"
          placeholder={inherited.owner ? `Inherit (${inherited.owner})` : "Nobody"}
          bind:value={form.owner}
          onkeydown={fieldKey}
        />
        <label for="{uid}-status">Own status</label>
        <select id="{uid}-status" bind:value={form.status}>
          {#each STATUSES as status (status.value)}<option value={status.value}>{status.label}</option>{/each}
        </select>
      </div>
    {:else}
      <dl class="tf-as-kv tf-as-force">
        {@render forced("Environment", on.effective_environment, `${uid}-env`)}
        {@render forced("Owner", on.effective_owner, `${uid}-owner`)}
        <div>
          <dt>Health</dt>
          <dd>
            <span class="tf-chip tf-{readingOf(on.asset.health).tone}">{readingOf(on.asset.health).word}</span>
            {#if on.asset.health !== on.asset.status}
              <span class="tf-dim">own status {readingOf(on.asset.status).word}</span>
            {/if}
          </dd>
        </div>
      </dl>
    {/if}
  </section>
{/snippet}

{#snippet forced(label: string, held: AssetDetail["effective_owner"] | AssetDetail["effective_environment"], field: string)}
  <div>
    <dt>{label}</dt>
    <dd>
      {#if held}
        {@const from = sourceOf(shown!, held, pageOf)}
        <button class="tf-as-val" title="Edit {label}" onclick={() => void beginEdit(field)}>{held.value}</button>
        {#if from.goTo}
          {@const there = from.goTo}
          <button class="tf-linkish tf-as-from" onclick={() => go(there)}>{from.note}</button>
        {:else}
          <span class="tf-dim tf-as-from">{from.note}</span>
        {/if}
      {:else}
        <button class="tf-as-val tf-as-unset" title="Edit {label}" onclick={() => void beginEdit(field)}>Not set anywhere</button>
      {/if}
    </dd>
  </div>
{/snippet}

<!--
  **Depends on this**: what breaks if this asset goes down (spec §12.2,
  #505). Drawn on every asset: *nothing* is a real answer, the one a reader
  wants before pulling a machine out. The routes come beneath and outside the
  count, as `CONTEXT.md` has it.
-->
{#snippet breaks()}
  <section class="tf-panel tf-as-breaks" aria-label="Depends on this">
    <header>
      <h3>Depends on this</h3>
      {#if dependsOn !== null && dependsOn.assets.length > 0}<span class="tf-as-n">{dependsOn.assets.length}</span>{/if}
    </header>
    {#if dependsFailure !== null}
      <p class="tf-as-none tf-ifail">{dependsFailure}</p>
    {:else if dependsOn === null}
      <p class="tf-as-none">Reading…</p>
    {:else if dependsOn.assets.length === 0}
      <p class="tf-as-none">Nothing depends on this.</p>
    {:else}
      <ul class="tf-as-list tf-as-wrap" {@attach dependsWindow.list}>
        {#each dependsWindow.slice(dependsOn.assets) as row (row.asset.id)}
          <li {@attach dependsWindow.row(row.asset.id)}>
            <button class="tf-as-to" onclick={() => go(pageOf(row.asset))}>{row.asset.name}</button>
            <!-- Containment is a parent field, not a link (ADR-0014): no relation, and the word is *inside*. -->
            <span class="tf-dim">{row.relation === null ? "inside" : relationReading(row.relation, true)}</span>
            {#if row.path !== null}<span class="tf-dim tf-as-path">{row.path}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if dependsOn !== null && dependsOn.routes.length > 0}
      <h4 class="tf-as-sub">Reachable via routes that would break</h4>
      <ul class="tf-as-list tf-as-wrap">
        {#each dependsOn.routes as route (route.id)}
          <li>
            <button class="tf-as-to" onclick={() => go(routeAddressOf(route))}>{route.name}</button>
            <span class="tf-dim tf-mono tf-as-path">{route.url}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/snippet}

<!--
  **Monitoring**: what watches it (#445), and the names the estate file gave
  that have not found a monitor yet (#439), two facts in two lists.
  *Create monitor* is drawn only where a source could take it (#453): with no
  monitoring source configured nothing is being kept from the reader.
-->
{#snippet monitoring(on: AssetDetail)}
  <section class="tf-panel tf-as-watch" aria-label="Monitoring">
    <header>
      <h3>Monitoring</h3>
      {#if on.monitor_targets.length > 0}
        <span class="tf-trail"><button class="tf-btn tf-sm" onclick={() => (creatingMonitor = true)}>Create monitor</button></span>
      {/if}
    </header>
    {#if on.monitoring.length === 0 && namedAndWaiting.length === 0}
      <p class="tf-as-none">Nothing watches it.</p>
    {/if}
    {#if on.monitoring.length > 0}
      <ul class="tf-as-list">
        {#each on.monitoring as watch (watch.entity_id)}
          <li>
            <button class="tf-as-to" onclick={() => go(monitorAddress(watch))}>{watch.name}</button>
            <!-- A monitor that left `/metrics` is paused or deleted (#442): no state to show and no page to open. -->
            {#if watch.tombstoned}
              <span class="tf-dim">Paused or gone from Kuma</span>
            {:else if watch.state !== null}
              <span class="tf-chip tf-{monitorTone(watch.state)}">{watch.state}</span>
            {:else}
              <span class="tf-dim">no reading</span>
            {/if}
            {#if watch.web_url !== null}
              {@const url = watch.web_url}
              <span class="tf-trail"><button class="tf-btn tf-sm tf-ghost" onclick={() => void openInBrowser(url)}>Open in Kuma</button></span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if namedAndWaiting.length > 0}
      <h4 class="tf-as-sub">Named by the import, not in Kuma yet</h4>
      <ul class="tf-as-list tf-as-named">
        {#each namedAndWaiting as name (name)}
          <li class="tf-mono">{name}</li>
        {/each}
      </ul>
    {/if}
  </section>
{/snippet}

{#snippet history(on: AssetDetail)}
  <section class="tf-panel" aria-label="History">
    <header><h3>History</h3></header>
    {#if on.history.length === 0}
      <p class="tf-as-none">Nothing recorded.</p>
    {:else}
      <ol class="tf-as-hist">
        {#each on.history as entry (entry.id)}
          <li><span>{historyLine(entry.verb, entry.detail, nameOf)}</span><span class="tf-dim">{ago(entry.at, now())}</span></li>
        {/each}
      </ol>
    {/if}
  </section>
{/snippet}

{#if creating && shown !== null}
  {@const parent = shown.asset}
  <CreateDialog
    at={{ id: parent.id, parent }}
    {types}
    create={io.createAsset}
    edit={io.editAsset}
    onclose={() => (creating = false)}
    oncreated={(row: AssetRow, more: boolean) => {
      revision += 1;
      if (more) return;
      creating = false;
      go(pageOf(row));
    }}
  />
{/if}

{#if linking && shown !== null}
  <LinkDialog
    fromId={shown.asset.id}
    fromTitle={shown.asset.name}
    onclose={() => (linking = false)}
    oncreated={() => {
      linking = false;
      revision += 1;
    }}
  />
{/if}

{#if creatingMonitor && shown !== null}
  {@const on = shown}
  <MonitorDialog
    detail={on}
    record={io.editAsset}
    queue={io.submitWrite}
    onclose={() => (creatingMonitor = false)}
    oncreated={({ name, source }) => {
      creatingMonitor = false;
      // What knobas did: the monitor itself is up to a poll away.
      push({ text: `${name} queued for ${source} — the next poll attaches it` });
      revision += 1;
    }}
  />
{/if}

{#if moving && shown !== null}
  <MoveDialog
    asset={shown.asset}
    heldBy={shown.held_by}
    tree={io.assetTree}
    move={io.moveAsset}
    onclose={() => (moving = false)}
    onmoved={() => {
      moving = false;
      revision += 1;
    }}
  />
{/if}

{#if (exposing || editingRoute !== null) && shown !== null}
  {@const on = shown.asset}
  <RouteDialog
    asset={on}
    heldBy={shown.held_by}
    route={editingRoute}
    tree={io.assetTree}
    create={io.createRoute}
    edit={io.editRoute}
    remove={io.deleteRoute}
    onclose={() => {
      exposing = false;
      editingRoute = null;
    }}
    onsaved={(row) => {
      exposing = false;
      editingRoute = null;
      go(routeAddressOf(row));
      revision += 1;
    }}
    ondeleted={() => {
      exposing = false;
      editingRoute = null;
      go(pageOf(on));
      revision += 1;
    }}
  />
{/if}

{#if deleting && shown !== null}
  {@const doomed = shown.asset}
  {@const ask = deleteConfirm(shown)}
  <TfDialog closeButton title="Delete asset" subtitle={doomed.name} onclose={() => (deleting = false)}>
    {#snippet body()}
      <!-- The Dependencies editor's own sentence (#1463): links stay marked deleted, landing routes are counted. -->
      <p class="tf-as-ask">{ask}</p>
      <!-- What it holds is not counted here: `assets::delete` refuses a branch by name, with the count. -->
      {#if writeFailure}
        <p class="tf-ifail" role="alert">{writeFailure}</p>
      {/if}
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (deleting = false)}>Cancel</button>
      <button class="tf-btn tf-primary tf-danger" disabled={writing} onclick={() => void confirmDelete()}>
        {writing ? "Deleting…" : "Delete"}
      </button>
    {/snippet}
  </TfDialog>
{/if}
