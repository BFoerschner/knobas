<!--
  *View: <name> ▾* (#873, R-11.16): the saved views, each with which opening
  it is the default for, then *Save changes*, *Discard changes*, *Save as a new
  view…*, *Rename this view…*, *Delete this view* and *Which view opens
  first…*, in prototype-6's words. The dialogs are here too. Every write goes
  to the backend at once and its answer becomes the page's state.
-->
<script lang="ts">
  import { tick } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { ViewPorts } from "./ports";
  import type { DepViewState } from "./views.svelte";
  import { viewBadge } from "./views";

  let { views, layerIds, ports }: { views: DepViewState; layerIds: readonly string[]; ports: ViewPorts } = $props();

  let open = $state(false);
  let dialog = $state<"save" | "rename" | "defaults" | null>(null);
  let name = $state("");
  let fromTicket = $state(false);
  let fromTop = $state(false);
  let pickTicket = $state("");
  let pickTop = $state("");
  let failure = $state<string | null>(null);
  let button = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);
  const uid = $props.id();

  const current = $derived(views.current);
  const saved = $derived(views.saved);

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      menu?.querySelector<HTMLElement>("button")?.focus();
    }
  }

  function pick(run: () => void) {
    open = false;
    run();
  }

  $effect(() => {
    if (!open) return;
    const away = (event: MouseEvent) => {
      if (event.target instanceof Node && (menu?.contains(event.target) || button?.contains(event.target))) return;
      open = false;
    };
    window.addEventListener("mousedown", away);
    return () => window.removeEventListener("mousedown", away);
  });

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    open = false;
    button?.focus();
  }

  async function run(write: () => Promise<void>) {
    failure = null;
    try {
      await write();
      dialog = null;
    } catch (error) {
      failure = ipcErrorMessage(error);
    }
  }

  /** *Save changes* (and the toolbar's *Save view*). */
  export async function saveChanges() {
    const view = views.current;
    const draft = views.draft;
    if (view === null || draft === null) return;
    try {
      views.kept(await ports.saveDepView(view.id, view.name, draft));
      push({ text: `Saved the view ${view.name}` });
    } catch (error) {
      push({ text: `The view was not saved: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }

  function startSaveAs() {
    name = "My view";
    fromTicket = false;
    fromTop = false;
    failure = null;
    dialog = "save";
  }

  const saveAs = () =>
    run(async () => {
      const draft = views.draft;
      if (draft === null) return;
      const view = await ports.saveDepView(null, name.trim() || "My view", draft);
      views.kept(view);
      if ((fromTicket || fromTop) && saved !== null) {
        views.take(
          await ports.setDepViewDefaults(fromTicket ? view.id : saved.opens_from_ticket, fromTop ? view.id : saved.opens_from_top),
        );
      }
      push({ text: "Saved the view" });
    });

  const rename = () =>
    run(async () => {
      const view = views.current;
      if (view === null) return;
      const draft = views.draft;
      const renamed = await ports.saveDepView(view.id, name.trim() || "View", view.settings);
      views.kept(renamed);
      // A rename keeps what the reader had changed and not yet saved.
      if (draft !== null) views.change((next) => Object.assign(next, draft));
    });

  async function remove() {
    const view = views.current;
    if (view === null) return;
    try {
      const after = await ports.deleteDepView(view.id);
      views.take(after);
      views.open(after.views[0]?.id ?? "");
      push({ text: `Deleted the view ${view.name}` });
    } catch (error) {
      push({ text: `The view was not deleted: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }

  const saveDefaults = () =>
    run(async () => {
      views.take(await ports.setDepViewDefaults(pickTicket, pickTop));
      push({ text: "Saved" });
    });
</script>

<span class="tf-dp-vmenu">
  <button
    type="button"
    class="tf-btn tf-sm tf-dp-viewbtn"
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={button}
    onclick={toggle}
    >View: <b>{current?.name ?? "Unsaved"}</b>{#if views.dirty}{" "}<span class="tf-dp-dirty" title="Changed since it was saved">changed</span>{/if} ▾</button
  >
  {#if open && saved !== null}
    <div class="tf-menu tf-appmenu tf-dp-menu" role="menu" tabindex="-1" bind:this={menu} {onkeydown}>
      {#each saved.views as view (view.id)}
        <button type="button" role="menuitem" onclick={() => pick(() => views.open(view.id))}
          ><b>{view.name}{current?.id === view.id ? " (on)" : ""}</b><span class="tf-dim">{viewBadge(saved, view.id, layerIds)}</span></button
        >
      {/each}
      <hr />
      {#if views.dirty && current !== null}
        <button type="button" role="menuitem" onclick={() => pick(() => void saveChanges())}>Save changes to {current.name}</button>
        <button type="button" role="menuitem" onclick={() => pick(() => views.open(current.id))}>Discard changes</button>
      {/if}
      <button type="button" role="menuitem" onclick={() => pick(startSaveAs)}>Save as a new view…</button>
      <button
        type="button"
        role="menuitem"
        onclick={() =>
          pick(() => {
            name = current?.name ?? "";
            failure = null;
            dialog = "rename";
          })}>Rename this view…</button
      >
      {#if saved.views.length > 1}
        <button type="button" role="menuitem" onclick={() => pick(() => void remove())}>Delete this view</button>
      {/if}
      <hr />
      <button
        type="button"
        role="menuitem"
        onclick={() =>
          pick(() => {
            pickTicket = saved.opens_from_ticket;
            pickTop = saved.opens_from_top;
            failure = null;
            dialog = "defaults";
          })}>Which view opens first…</button
      >
    </div>
  {/if}
</span>

{#if dialog === "save"}
  <TfDialog title="Save as a new view" onclose={() => (dialog = null)}>
    {#snippet body()}
      <div class="tf-formgrid">
        <label for="{uid}-n">Name</label>
        <input id="{uid}-n" type="text" autocomplete="off" bind:value={name} />
      </div>
      <label class="tf-ck"><input type="checkbox" bind:checked={fromTicket} /> Open it when I open the map from a ticket</label>
      <label class="tf-ck"><input type="checkbox" bind:checked={fromTop} /> Open it from the top bar</label>
      <p class="tf-dim tf-note">
        A view keeps which layers show, how each one groups and folds, the environment filter and tile size. The layers
        themselves are shared by every view. Stored in knobas, on this machine.
      </p>
      {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary" onclick={() => void saveAs()}>Save view</button>
    {/snippet}
  </TfDialog>
{:else if dialog === "rename"}
  <TfDialog title="Rename the view" onclose={() => (dialog = null)}>
    {#snippet body()}
      <div class="tf-formgrid">
        <label for="{uid}-r">Name</label>
        <input id="{uid}-r" type="text" autocomplete="off" bind:value={name} />
      </div>
      {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary" onclick={() => void rename()}>Rename</button>
    {/snippet}
  </TfDialog>
{:else if dialog === "defaults" && saved !== null}
  <TfDialog title="Which view opens first" onclose={() => (dialog = null)}>
    {#snippet body()}
      <div class="tf-formgrid">
        <label for="{uid}-dt">From a ticket's page</label>
        <select id="{uid}-dt" bind:value={pickTicket}>
          {#each saved.views as view (view.id)}<option value={view.id}>{view.name}</option>{/each}
        </select>
        <label for="{uid}-dn">From Dependencies in the top bar</label>
        <select id="{uid}-dn" bind:value={pickTop}>
          {#each saved.views as view (view.id)}<option value={view.id}>{view.name}</option>{/each}
        </select>
      </div>
      <p class="tf-dim tf-note">From a ticket, the view only shows what that ticket reaches. From the top bar, it shows everything.</p>
      {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (dialog = null)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary" onclick={() => void saveDefaults()}>Save</button>
    {/snippet}
  </TfDialog>
{/if}
