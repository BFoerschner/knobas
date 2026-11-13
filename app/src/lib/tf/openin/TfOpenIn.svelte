<!--
  *Open in… ▾* and *Clone* (#844, #846; R-8.22, R-8.25), prototype-6's
  `openIn`: one component for every place a checkout is offered (the
  repository header, the file view, a Repositories row, the job's Code line,
  Local changes).

  - **Cloned here**: one button, "Open in… ▾", and the whole of it opens the
    menu: every application with its command filled in for this checkout, the
    default first, marked *(default)* and focused, so `Enter` runs it; then
    *Browser*, the repository's or the file's page at its source, handed to
    the browser (`openInBrowser`: never the app's window, Björn 2026-11-13);
    then *Add an application* and *Manage applications*, links to Settings ›
    Checkouts and Open in (the application page, PR #2479, 2026-11-13: the
    dialog drawn here was caught in a row's fading group, so on a Repositories
    row it stood beside the row and faded out). `TfMenu` draws it. At a file,
    an application's file command is used.
  - **Not on this machine**: *Clone*, its tooltip naming the folder; while it
    runs, "Cloning… 52 %" and *Stop*; a failure said in place with *Try
    again*. With no clones root set it first asks where clones go.
  - Nothing while the checkout has not been read.
-->
<script lang="ts">
  import { objectPage, setClonesRoot, type CheckoutView } from "../../ipc/entity";
  import { openInBrowser } from "../../shell/external-links";
  import { openExternal } from "../../shell/open-external";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfMenu from "../TfMenu.svelte";
  import { apps as sharedApps, type Apps } from "./apps.svelte";
  import { clones as sharedClones, type Clones } from "./clones.svelte";
  import { appPageHash } from "./app-page";
  import { cloneTip, commandFor, fill, joinInside, menuOrder, repoName } from "./openin";
  import TfCloneRoot from "./TfCloneRoot.svelte";

  let {
    entityId,
    title,
    checkout,
    file = null,
    line = null,
    size = "sm",
    variant = "ghost",
    apps = sharedApps,
    clones = sharedClones,
    saveRoot = (path: string) => setClonesRoot(path),
    page = undefined,
    readPage = (id: string) => objectPage(id, { kind: "repository" }),
    browse = openExternal,
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
    /** Ghost by default: *Open in…* is always a ghost, in a header or a row (design system, Header). */
    variant?: "primary" | "ghost" | null;
    apps?: Apps | undefined;
    clones?: Clones | undefined;
    saveRoot?: (path: string) => Promise<void>;
    /** The page *Browser* opens: given by the file view, which knows the file's; read for the repository when the menu opens if not given. */
    page?: string | null | undefined;
    readPage?: (entityId: string) => Promise<string | null>;
    browse?: (url: string) => Promise<void>;
    layers?: Layers | undefined;
  } = $props();

  let open = $state(false);
  /** The root *Set and clone* just saved: the `checkout` prop is re-read later (#1442). */
  let savedRoot = $state<string | null>(null);
  let dialog = $state<"root" | null>(null);
  /** The repository's page, read when the menu first opens; `undefined` until then. */
  let readUrl = $state<string | null | undefined>(undefined);
  let face = $state<HTMLButtonElement | null>(null);

  const cls = $derived(
    ["tf-btn", size === "sm" ? "tf-sm" : "", variant === "primary" ? "tf-primary" : variant === "ghost" ? "tf-ghost" : ""]
      .filter(Boolean)
      .join(" "),
  );
  const name = $derived(repoName(title));
  const repoId = $derived(checkout?.repo_entity_id ?? entityId);
  const root = $derived(checkout?.clones_root ?? savedRoot);
  const mark = $derived(clones.marks[repoId]);
  const path = $derived(checkout?.path ?? null);
  const entries = $derived(apps.settings === null ? [] : menuOrder(apps.settings));
  const fileAt = $derived(path !== null && file !== null ? joinInside(path, file) : null);
  /** A file's page is only the one the file view gives: the repository's would be the wrong page. */
  const browserUrl = $derived(page !== undefined ? page : file !== null ? null : (readUrl ?? null));
  /** The application page, to try the new one on this repository and this file. */
  const addHash = $derived(appPageHash({ repo: repoId, kind: file !== null ? "file" : null, value: file }));

  $effect(() => {
    void apps.ensure();
  });

  $effect(() => {
    if (!open || page !== undefined || file !== null || readUrl !== undefined) return;
    // Called through a promise: a test's mock of the bridge may not carry `object_page`.
    Promise.resolve(entityId).then(readPage).then(
      (url) => (readUrl = url),
      () => (readUrl = null),
    );
  });

  function toBrowser(url: string) {
    open = false;
    openInBrowser(url, browse);
  }

  function run(id: string, appName: string) {
    open = false;
    void apps.run(entityId, appName, { application: id, file, line });
  }

  function clone() {
    if (checkout === undefined) return;
    if (root === null) {
      dialog = "root";
      return;
    }
    void clones.start({ entityId: repoId, name, repoUrl: checkout.repo_url, root });
  }

  async function setRoot(path: string) {
    await saveRoot(path);
    savedRoot = path;
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
        }}>Open in…<svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
      >
      <TfMenu bind:open opener={face} label="Open in" className="tf-appmenu" {layers}>
          {#each entries as entry (entry.app.id)}
            <button role="menuitem" onclick={() => run(entry.app.id, entry.app.name)}
              ><b>{entry.app.name}{#if entry.isDefault}{" "}<span class="tf-dim tf-plain">(default)</span>{/if}</b><span
                class="tf-mono" title={fill(commandFor(entry.app, file), { path, file: fileAt, line })}>{fill(commandFor(entry.app, file), { path, file: fileAt, line })}</span
              ></button
            >
          {/each}
          {#if browserUrl !== null}
            {#if entries.length > 0}<hr />{/if}
            <button role="menuitem" onclick={() => toBrowser(browserUrl)}><b>Browser</b><span class="tf-mono" title={browserUrl}>{browserUrl}</span></button>
          {/if}
          {#if entries.length > 0 || browserUrl !== null}<hr />{/if}
          <a role="menuitem" href={addHash} onclick={() => (open = false)}>Add an application</a>
          <a role="menuitem" href="#/settings/checkouts" onclick={() => (open = false)}>Manage applications</a>
      </TfMenu>
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
      <button type="button" class={cls} title={cloneTip(root, name)} onclick={clone}
        >{mark?.kind === "failed" ? "Try again" : "Clone"}</button
      >
      {#if mark?.kind === "failed"}<span class="tf-clonefail" role="alert">{mark.text}</span>{/if}
    </span>
  {/if}
{/if}

{#if dialog === "root"}
  <TfCloneRoot
    save={setRoot}
    {layers}
    onclose={() => (dialog = null)}
    ondone={() => void clones.start({ entityId: repoId, name, repoUrl: checkout?.repo_url ?? null, root: savedRoot })}
  />
{/if}
