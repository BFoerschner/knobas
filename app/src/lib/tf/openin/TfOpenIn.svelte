<!--
  *Open in… ▾* and *Clone* (#844, #846; R-8.22, R-8.25), prototype-6's
  `openIn`: one component for every place a checkout is offered (the
  repository header, the file view, a Repositories row, the job's Code line,
  Local changes).

  - **Cloned here**: one button, "Open in… ▾", and the whole of it opens the
    menu: every application with its command filled in for this checkout, the
    default first, marked *(default)* and focused, so `Enter` runs it; then
    *Add an application…* and *Manage applications…*. Arrow keys move, `Esc`
    closes. At a file, an application's file command is used.
  - **Not on this machine**: *Clone*, its tooltip naming the folder; while it
    runs, "Cloning… 52 %" and *Stop*; a failure said in place with *Try
    again*. With no clones root set it first asks where clones go.
  - Nothing while the checkout has not been read.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { setClonesRoot, type CheckoutView } from "../../ipc/entity";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "./apps.svelte";
  import { clones as sharedClones, type Clones } from "./clones.svelte";
  import { cloneTip, commandFor, fill, menuOrder, repoName, type Sample } from "./openin";
  import TfAppDialog from "./TfAppDialog.svelte";
  import TfAppsManage from "./TfAppsManage.svelte";
  import TfCloneRoot from "./TfCloneRoot.svelte";

  let {
    entityId,
    title,
    checkout,
    file = null,
    line = null,
    size = "sm",
    variant = null,
    apps = sharedApps,
    clones = sharedClones,
    saveRoot = (path: string) => setClonesRoot(path),
    layers = sharedLayers,
  }: {
    /** The repo or branch the checkout was read for: what opens. */
    entityId: string;
    /** The repository's `owner/name`. */
    title: string;
    /** `undefined` while `entity_checkout` has not answered. */
    checkout: CheckoutView | undefined;
    /** A file inside the checkout, relative to it. */
    file?: string | null;
    line?: number | null;
    size?: "sm" | "md";
    variant?: "primary" | "ghost" | null;
    apps?: Apps | undefined;
    clones?: Clones | undefined;
    saveRoot?: (path: string) => Promise<void>;
    layers?: Layers | undefined;
  } = $props();

  let open = $state(false);
  let dialog = $state<"add" | "manage" | "root" | null>(null);
  let face = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const cls = $derived(
    ["tf-btn", size === "sm" ? "tf-sm" : "", variant === "primary" ? "tf-primary" : variant === "ghost" ? "tf-ghost" : ""]
      .filter(Boolean)
      .join(" "),
  );
  const name = $derived(repoName(title));
  const repoId = $derived(checkout?.repo_entity_id ?? entityId);
  const mark = $derived(clones.marks[repoId]);
  const path = $derived(checkout?.path ?? null);
  const entries = $derived(apps.settings === null ? [] : menuOrder(apps.settings));
  const sample = $derived<Sample | null>(path === null ? null : { entityId, name, path, file });
  const fileAt = $derived(path !== null && file !== null ? `${path}/${file}` : null);

  $effect(() => {
    void apps.ensure();
  });

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "dialog", close: () => close() }));
    queueMicrotask(() => menu?.querySelector<HTMLButtonElement>("button")?.focus());
    const outside = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && !menu?.contains(target) && !face?.contains(target)) open = false;
    };
    window.addEventListener("click", outside, true);
    return () => {
      off();
      window.removeEventListener("click", outside, true);
    };
  });

  function close() {
    open = false;
    face?.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" && items.length > 0) {
      event.preventDefault();
      items[(at + 1) % items.length]?.focus();
    } else if (event.key === "ArrowUp" && items.length > 0) {
      event.preventDefault();
      items[(at - 1 + items.length) % items.length]?.focus();
    }
  }

  function run(id: string, appName: string) {
    open = false;
    void apps.run(entityId, appName, { application: id, file, line });
  }

  function clone() {
    if (checkout === undefined) return;
    if (checkout.clones_root === null) {
      dialog = "root";
      return;
    }
    void clones.start({ entityId: repoId, name, repoUrl: checkout.repo_url, root: checkout.clones_root });
  }

  async function setRoot(root: string) {
    await saveRoot(root);
    void apps.reload();
  }
</script>

{#if checkout !== undefined}
  {#if path !== null}
    <span class="tf-openin">
      <button
        bind:this={face}
        type="button"
        class={cls}
        aria-haspopup="menu"
        aria-expanded={open}
        title="Open {path} in an application"
        onclick={(event) => {
          event.stopPropagation();
          open = !open;
        }}>Open in… <span class="tf-dim" aria-hidden="true">▾</span></button
      >
      {#if open}
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="tf-menu tf-appmenu" role="menu" aria-label="Open in" bind:this={menu} {onkeydown}>
          {#each entries as entry (entry.app.id)}
            <button role="menuitem" onclick={() => run(entry.app.id, entry.app.name)}
              ><b>{entry.app.name}{#if entry.isDefault}{" "}<span class="tf-dim tf-plain">(default)</span>{/if}</b><span
                class="tf-mono">{fill(commandFor(entry.app, file), { path, file: fileAt, line })}</span
              ></button
            >
          {/each}
          {#if entries.length > 0}<hr />{/if}
          <button role="menuitem" onclick={() => ((open = false), (dialog = "add"))}>Add an application…</button>
          <button role="menuitem" onclick={() => ((open = false), (dialog = "manage"))}>Manage applications…</button>
        </div>
      {/if}
    </span>
  {:else if mark?.kind === "running"}
    <span class="tf-openin">
      <button type="button" class={cls} disabled aria-live="polite">Cloning… {mark.percent} %</button>
      <button type="button" class="{cls} tf-ghost" aria-label="Stop cloning {name}" onclick={() => void clones.stop(repoId)}
        >Stop</button
      >
    </span>
  {:else}
    <span class="tf-openin">
      <button type="button" class={cls} title={cloneTip(checkout.clones_root, name)} onclick={clone}
        >{mark?.kind === "failed" ? "Try again" : "Clone"}</button
      >
      {#if mark?.kind === "failed"}<span class="tf-clonefail" role="alert">{mark.text}</span>{/if}
    </span>
  {/if}
{/if}

{#if dialog === "add"}
  <TfAppDialog {apps} {sample} {layers} onclose={() => (dialog = null)} />
{:else if dialog === "manage"}
  <TfAppsManage {apps} {sample} {layers} onclose={() => (dialog = null)} />
{:else if dialog === "root"}
  <TfCloneRoot
    save={setRoot}
    {layers}
    onclose={() => (dialog = null)}
    ondone={() => void clones.start({ entityId: repoId, name, repoUrl: checkout?.repo_url ?? null, root: null })}
  />
{/if}
