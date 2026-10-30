<!--
  *Open in… ▾* on a code object (#2274, spec #2234 step 6c): a commit row, a
  commit's page, a range's diff address, a diff file header at its line, a
  pull request's header and Compare. A ghost button, as every "Open in" is
  (#2238); the menu is `TfOpenIn`'s, keys and all.

  - The applications that accept the object (`objectMenu`: the ones with a
    command for its kind), the default first, each with its command filled
    in, when the repository is on this machine. The checkout is read when the
    menu opens, so a clone made since counts.
  - A pull request's applications open its worktree under the clones root
    (#2285, ADR-0035), so the line each draws names that folder.
  - *Diff tool* (#2275), for a range or a pull request whose two shas the
    checkout holds (`diff_tool`): `git difftool --dir-diff --gui`, the tool the
    person's git configuration names. A range's ends are its own; a pull
    request's come from `ends`, read when the menu opens.
  - Then *Browser*: the object's page at its source (`object_page`, built
    from the source's typed base URL), handed to the opener. Offered on every
    platform whenever the source declares its pages.
  - With neither, one line says why nothing opens it here.
-->
<script module lang="ts">
  import { diffTool, entityCheckout, objectPage, openDiffTool, type CheckoutView, type OpenObject } from "../../ipc/entity";
  import { openExternal } from "../../shell/open-external";

  /** What the menu reads and the one thing it hands the OS. */
  export interface OpenObjectPorts {
    entityCheckout: (entityId: string) => Promise<CheckoutView>;
    objectPage: (entityId: string, object: OpenObject) => Promise<string | null>;
    openExternal: (url: string) => Promise<void>;
    diffTool: (entityId: string, base: string, head: string) => Promise<string[] | null>;
    openDiffTool: (entityId: string, base: string, head: string) => Promise<string[]>;
  }

  /** Two ends of a diff, as shas. */
  export interface DiffEnds {
    base: string;
    head: string;
  }

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: OpenObjectPorts = {
    entityCheckout: (entityId) => entityCheckout(entityId),
    objectPage: (entityId, object) => objectPage(entityId, object),
    openExternal: (url) => openExternal(url),
    diffTool: (entityId, base, head) => diffTool(entityId, base, head),
    openDiffTool: (entityId, base, head) => openDiffTool(entityId, base, head),
  };
</script>

<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "./apps.svelte";
  import { argvText, fillObject, objectCommand, objectMenu, worktreeOf } from "./openin";

  let {
    entityId,
    object,
    size = "sm",
    ends,
    apps = sharedApps,
    ports = BRIDGE,
    layers = sharedLayers,
  }: {
    /** The repository (or a branch of it) the object is in. */
    entityId: string;
    object: OpenObject;
    size?: "sm" | "md";
    /** A pull request's two shas for the Diff tool (its files' range), read when the menu opens. */
    ends?: () => Promise<DiffEnds | null>;
    apps?: Apps;
    ports?: OpenObjectPorts;
    layers?: Layers;
  } = $props();

  const WHAT: Record<OpenObject["kind"], string> = {
    repository: "the repository",
    file: "this file",
    commit: "this commit",
    range: "this range",
    pull_request: "this pull request",
  };

  let open = $state(false);
  let reading = $state(false);
  let path = $state<string | null>(null);
  let page = $state<string | null>(null);
  /** Where a pull request's applications run: its worktree. */
  let worktree = $state<string | null>(null);
  /** The Diff tool's ends and argv, when it is offered. */
  let tool = $state<{ ends: DiffEnds; argv: string[] } | null>(null);
  let face = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const entries = $derived(
    path === null || apps.settings === null ? [] : objectMenu(apps.settings, object.kind),
  );

  /** The checkout and the page, read each time the menu opens. */
  async function read() {
    reading = true;
    const asked = object;
    const key = JSON.stringify([entityId, asked]);
    const [view, url, offered] = await Promise.all([
      ports.entityCheckout(entityId).catch(() => null),
      ports.objectPage(entityId, asked).catch(() => null),
      readTool(asked),
      apps.ensure(),
    ]);
    // A row redrawn with another object while this read ran: that object's menu reads its own.
    if (key !== JSON.stringify([entityId, object])) return;
    path = view?.path ?? null;
    worktree =
      asked.kind === "pull_request" && view !== null ? worktreeOf(view.clones_root, view.repo_entity_id, asked.number) : null;
    page = url;
    tool = offered;
    reading = false;
  }

  /** The Diff tool for a range or a pull request, when the checkout holds both shas. */
  async function readTool(asked: OpenObject): Promise<{ ends: DiffEnds; argv: string[] } | null> {
    try {
      const at: DiffEnds | null =
        asked.kind === "range" ? { base: asked.base, head: asked.head } : asked.kind === "pull_request" && ends ? await ends() : null;
      if (at === null) return null;
      const argv = await ports.diffTool(entityId, at.base, at.head);
      return argv === null ? null : { ends: at, argv };
    } catch {
      return null;
    }
  }

  $effect(() => {
    if (!open) return;
    void untrack(() => read());
    const off = untrack(() => layers.add({ kind: "dialog", close: () => close() }));
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

  // The first entry takes the focus once the reads have drawn it, so `Enter` runs the default.
  $effect(() => {
    if (open && !reading) queueMicrotask(() => menu?.querySelector<HTMLButtonElement>("button")?.focus());
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

  function run(id: string, name: string) {
    open = false;
    void apps.runObject(entityId, name, id, object);
  }

  function diff(at: DiffEnds) {
    open = false;
    ports.openDiffTool(entityId, at.base, at.head).then(
      (argv) => push({ text: `Started the Diff tool: ${argvText(argv)}` }),
      (cause: unknown) => push({ text: `Could not start the Diff tool: ${ipcErrorMessage(cause)}`, tone: "err" }),
    );
  }

  function browse(url: string) {
    open = false;
    ports.openExternal(url).catch((cause: unknown) => {
      push({ text: `Could not open the browser: ${ipcErrorMessage(cause)}`, tone: "err" });
    });
  }
</script>

<span class="tf-openin">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-ghost"
    class:tf-sm={size === "sm"}
    aria-haspopup="menu"
    aria-expanded={open}
    title="Open {WHAT[object.kind]} in an application or the browser"
    onclick={(event) => {
      event.stopPropagation();
      open = !open;
    }}>Open in… <span class="tf-dim" aria-hidden="true">▾</span></button
  >
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="tf-menu tf-appmenu" role="menu" aria-label="Open in" bind:this={menu} {onkeydown}>
      {#if reading}
        <span class="tf-dim tf-appmenu-note" aria-live="polite">Reading…</span>
      {:else}
        {#each entries as entry (entry.app.id)}
          <button role="menuitem" onclick={() => run(entry.app.id, entry.app.name)}
            ><b>{entry.app.name}{#if entry.isDefault}{" "}<span class="tf-dim tf-plain">(default)</span>{/if}</b><span
              class="tf-mono">{fillObject(objectCommand(entry.app, object.kind) ?? "", { path: worktree ?? path ?? "", object, url: page })}</span
            ></button
          >
        {/each}
        {#if tool !== null}
          {@const at = tool.ends}
          {#if entries.length > 0}<hr />{/if}
          <button role="menuitem" onclick={() => diff(at)}><b>Diff tool</b><span class="tf-mono">{argvText(tool.argv)}</span></button>
        {/if}
        {#if page !== null}
          {#if entries.length > 0 || tool !== null}<hr />{/if}
          <button role="menuitem" onclick={() => browse(page!)}><b>Browser</b><span class="tf-mono">{page}</span></button>
        {/if}
        {#if entries.length === 0 && page === null && tool === null}
          <span class="tf-dim tf-appmenu-note"
            >{path === null
              ? "The repository is not on this machine, and its source has no web page for this."
              : `No application opens ${WHAT[object.kind]}: add one in Settings › Checkouts.`}</span
          >
        {/if}
      {/if}
    </div>
  {/if}
</span>
