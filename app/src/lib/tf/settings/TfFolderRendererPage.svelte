<!--
  A folder renderer's page (ADR-0044): Settings › Renderers › *Add a folder
  renderer* (`#/settings/renderers/folder/add`) or a renderer's *Edit*
  (`…/folder/<id>`), each optionally followed by a repository and a folder to
  try it on. Shaped as the *Open in…* application page
  (`openin/TfAppPage.svelte`, the 2026-11-13 ruling).

  - The fields: name, command (with `{path}` and `{out}`, inserted by their
    buttons), the files a folder must hold and the folder names it takes
    (globs separated by spaces, both empty for every folder), what it makes
    and a timeout. A placeholder other than the two, or a command without
    `{path}`, is said under the field and the backend refuses it too.
  - **Try it on**: a repository with a checkout on this disk and one of its
    folders. *Try it* saves the draft (only a saved command runs, ADR-0016)
    and draws the folder under it with `TfFolderRender`, running it again.
    Leaving without *Save* takes the try back: the entry as it was before
    the first try, or gone if it was new.
  - *Save*, *Cancel* and *Remove* go back where the page was opened from.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { FolderRenderer } from "../../ipc/app";
  import { folderRenderers, type FolderRenderers } from "../../render/folders.svelte";
  import { push } from "../../shell/toasts.svelte";
  import type { EscTargetOf } from "../escape";
  import { pageVisible } from "../kept-pages.svelte";
  import { backStep } from "../page-name.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { PressHold } from "../press-hold.svelte";
  import { insertAt } from "../openin/openin";
  import { lastTried, noteTried } from "../openin/app-page";
  import { realTryReads, type TryReads } from "../openin/try-reads";
  import { TryRepos } from "../openin/try-repos.svelte";
  import TfTryMore from "../openin/TfTryMore.svelte";
  import { LAND_ADD, landOn } from "./landing";
  import TfFolderRender, { ipcFolderRenderPorts, type FolderRenderPorts } from "../render/TfFolderRender.svelte";
  import { fileHash } from "../repo/repo";
  import {
    commandWhy,
    FOLDER_PLACEHOLDERS,
    folderDraftFrom,
    OUTPUTS,
    sameFolderDraft,
    withFolderDraft,
    type FolderDraft,
    type FolderPageAt,
  } from "./folder-page";

  let {
    at,
    folders = folderRenderers,
    reads = realTryReads,
    ports = ipcFolderRenderPorts,
    back = null,
    leave = undefined,
  }: {
    at: FolderPageAt;
    folders?: FolderRenderers;
    reads?: Pick<TryReads, "repos" | "checkouts" | "files">;
    ports?: FolderRenderPorts;
    back?: (Pick<EscTargetOf, "name"> & Partial<EscTargetOf>) | null;
    leave?: (() => void) | undefined;
  } = $props();

  const visible = pageVisible();
  const step = backStep();
  /** How many folders the picker offers. */
  const FOLDERS_SHOWN = 2000;

  let read = $state(false);
  $effect(() => {
    folders.ensure().then(
      () => (read = true),
      () => (read = true),
    );
  });
  const saved = $derived(at.renderer === null ? null : (folders.list.find((r) => r.id === at.renderer) ?? null));
  let draft = $state<FolderDraft>(folderDraftFrom(null));
  let seeded = false;
  $effect(() => {
    if (seeded || (at.renderer !== null && !read)) return;
    seeded = true;
    if (saved !== null) draft = folderDraftFrom(saved);
  });

  let error = $state<string | null>(null);
  let busy = $state(false);
  const changed = $derived(!sameFolderDraft(draft, folderDraftFrom(saved)));
  const commandSays = $derived(commandWhy(draft.command));
  const why = $derived(
    busy
      ? null
      : draft.name.trim() === ""
        ? draft.command.trim() === ""
          ? "Name it, and give its command."
          : "Name it first."
        : draft.command.trim() === ""
          ? "Give its command."
          : commandSays,
  );
  const ready = $derived(why === null && !busy);

  let commandField = $state<HTMLInputElement | null>(null);
  function insert(placeholder: string) {
    const next = insertAt(draft.command, commandField?.selectionStart ?? null, placeholder);
    draft.command = next.value;
    queueMicrotask(() => {
      commandField?.focus();
      commandField?.setSelectionRange(next.cursor, next.cursor);
    });
  }

  // -- Try it on ---------------------------------------------------------------

  /** The repositories with a checkout here, read in batches; a batch adds to the list and never picks again (`TryRepos`). */
  const tryOn = new TryRepos();
  const repo = $derived(tryOn.repo);
  let folderList = $state<string[] | null>(null);
  let folder = $state(untrack(() => at.folder ?? ""));
  let gone = false;

  $effect(() => {
    void untrack(() => tryOn.read(reads, [at.repo, lastTried()], () => gone));
  });

  // The picked repository's folders, every folder that holds a file.
  // Read again when another is picked, and only then: a batch of checkouts coming in leaves them be (2026-11-17).
  $effect(() => {
    const id = tryOn.pickedId;
    if (id === null) return;
    const picked = untrack(() => tryOn.repo!);
    folderList = null;
    let live = true;
    reads.files(picked.entityId).then(
      (files) => {
        if (!live) return;
        const all = new Set<string>([""]);
        for (const file of files ?? []) {
          let cut = file.lastIndexOf("/");
          while (cut > 0) {
            const dir = file.slice(0, cut);
            if (all.has(dir)) break;
            all.add(dir);
            cut = dir.lastIndexOf("/");
          }
        }
        folderList = [...all].sort();
      },
      () => live && (folderList = []),
    );
    return () => {
      live = false;
    };
  });

  // -- saving, trying, and taking a try back -----------------------------------

  /** A try's save: the entry as it was before the first one, and the id the save gave it. */
  let tried = $state<{ before: FolderRenderer | null; id: string } | null>(null);
  /** What *Try it* draws: the renderer saved for it, the repository and folder, a new key per press. */
  let trying = $state<{ renderer: FolderRenderer; repo: string; title: string; folder: string; press: number } | null>(null);
  let presses = 0;

  async function store(list: ReturnType<typeof withFolderDraft>) {
    return folders.save(list);
  }

  /** A *Try it* pressed twice runs once (the press rule): the press holds while it saves and a while after. */
  const tryHold = new PressHold();

  async function tryIt() {
    const picked = repo;
    if (!ready || picked === null || tryHold.held()) return;
    tryHold.hold();
    busy = true;
    error = null;
    const own = tried?.id ?? at.renderer;
    let stored: FolderRenderer[];
    try {
      stored = await store(withFolderDraft(folders.list, draft, own));
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      // A refused save isn't held, so the corrected command can go at once.
      tryHold.release();
      return;
    }
    busy = false;
    const entry = own !== null ? stored.find((r) => r.id === own) : stored.at(-1);
    if (entry === undefined) return;
    if (tried === null) tried = { before: saved, id: entry.id };
    noteTried(picked.entityId);
    if (gone) return;
    presses += 1;
    trying = { renderer: entry, repo: picked.entityId, title: picked.title, folder: folder.trim().replace(/^\/+|\/+$/g, ""), press: presses };
  }

  /** Put the list back as it was before the first try. */
  function takeBack() {
    const was = tried;
    tried = null;
    if (was === null) return;
    const name = draft.name.trim();
    const list = folders.list.flatMap((r) => (r.id === was.id ? (was.before === null ? [] : [was.before]) : [r]));
    folders.save(list.map((r) => ({ ...r }))).catch((cause: unknown) => push({ text: `Could not take back the try of ${name}: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }

  let done = false;
  /** The section's own address: the list this page hands back to when it was opened from it. */
  const SECTION = "#/settings/renderers";

  /**
   * Back where the page was opened from: the list, focused on `landing` (an entry's id, or `LAND_ADD`). Opened from the
   * list, the step is back through history (the page is a page of its own, `opener.ts`), so Back doesn't come here again.
   */
  function goBack(landing: string) {
    done = true;
    gone = true;
    if (leave !== undefined || back?.hash === undefined || back.hash === SECTION || step === null) landOn("folder-renderers", landing);
    if (leave) leave();
    else if (back?.hash !== undefined && step !== null) step(back.hash, back.opened ?? false);
    else location.hash = SECTION;
  }

  async function save() {
    if (!ready) return;
    busy = true;
    error = null;
    const own = tried?.id ?? at.renderer;
    try {
      await store(withFolderDraft(folders.list, draft, own));
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return;
    }
    tried = null;
    push({ text: `${at.renderer !== null ? "Saved" : "Added"} ${draft.name.trim()}. The Code page draws the folders it takes.` });
    // A new entry is the list's last: its id is the one the save gave it.
    goBack(own ?? folders.list.at(-1)?.id ?? LAND_ADD);
  }

  async function remove() {
    const removing = saved;
    if (removing === null) return;
    const had = folders.list.map((r) => ({ ...r }));
    const place = had.findIndex((r) => r.id === removing.id);
    /** The entry that takes its place in the list, else the one before it, else *Add a folder renderer*. */
    const after = had[place + 1]?.id ?? had[place - 1]?.id ?? LAND_ADD;
    busy = true;
    try {
      await store(had.filter((r) => r.id !== removing.id));
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return;
    }
    tried = null;
    push({
      text: `Removed ${removing.name}`,
      action: {
        label: "Undo",
        run: () => {
          folders.save(had).catch((cause: unknown) => push({ text: `Could not put ${removing.name} back: ${ipcErrorMessage(cause)}`, tone: "err" }));
        },
      },
    });
    goBack(after);
  }

  function cancel() {
    takeBack();
    goBack(at.renderer ?? LAND_ADD);
  }

  $effect(() => {
    if (!visible()) untrack(() => !done && takeBack());
    else gone = false;
  });
  onDestroy(() => {
    gone = true;
    if (!done) takeBack();
  });

  const missing = $derived(at.renderer !== null && read && saved === null && tried === null);
  const title = $derived(at.renderer === null ? "Add a folder renderer" : (saved?.name ?? draft.name) || "A folder renderer");
</script>

{#if missing}
  <header class="tf-stg-h"><h1>A folder renderer</h1></header>
  <TfNothingHere message="There is no such folder renderer. It may have been removed." href="#/settings/renderers" action="Renderers" />
{:else}
  <header class="tf-stg-h">
    <h1>{title}</h1>
    <p>The Code page draws every folder it takes with it, in the file’s place, and offers it on the folder’s row.</p>
  </header>
  <form
    class="tf-apppage"
    id="tf-frp-form"
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
  >
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-frp-name">Name <small class="tf-req">required</small><small>What the Code page calls it.</small></label>
      <div class="tf-stg-c"><input id="tf-frp-name" aria-label="Name" class="tf-stg-in" type="text" autocomplete="off" placeholder="Low-code preview" bind:value={draft.name} /></div>
    </div>

    <div class="tf-stg-sec">
      <h2>Command</h2>
      <p class="tf-stg-p">
        It runs as you, with no shell, in an empty folder of its own. knobas writes a copy of the folder, at the commit the Code page shows, and
        passes its path as <span class="tf-mono">{"{path}"}</span>. Print what it makes, or write it to <span class="tf-mono">{"{out}"}</span>: text after
        it stays, so <span class="tf-mono">{"{out}.png"}</span> names a PNG. To start in the folder, run a shell yourself:
        <span class="tf-mono">sh -c "cd '{"{path}"}' &amp;&amp; lowcode render"</span>, or on Windows <span class="tf-mono">cmd /c "cd /d {"{path}"} &amp;&amp; lowcode render"</span>.
      </p>
      <div class="tf-phs" aria-label="Placeholders knobas fills in">
        {#each FOLDER_PLACEHOLDERS as placeholder (placeholder.name)}
          <button type="button" class="tf-phb" onclick={() => insert(placeholder.name)}><span class="tf-mono">{placeholder.name}</span> {placeholder.says}</button>
        {/each}
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-cmd">Command <small class="tf-req">required</small></label>
        <div class="tf-stg-c tf-appcmd">
          <input
            id="tf-frp-cmd"
            aria-label="Command"
            class="tf-stg-in tf-mono"
            type="text"
            autocomplete="off"
            spellcheck="false"
            placeholder={"lowcode render {path} --png {out}.png"}
            bind:value={draft.command}
            bind:this={commandField}
          />
          {#if commandSays !== null}<small class="tf-stg-errt" role="status">{commandSays}</small>{/if}
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-out">It makes<small>A page is drawn without its scripts. It scrolls in its frame even where it hides its overflow, but not inside a box of its own that clips.</small></label>
        <div class="tf-stg-c">
          <select id="tf-frp-out" class="tf-stg-in" bind:value={draft.output}>
            {#each OUTPUTS as one (one.value)}<option value={one.value}>{one.label}</option>{/each}
          </select>
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-time">Timeout<small>1 to 600 seconds; the command is stopped after it. Copying the folder is not counted.</small></label>
        <div class="tf-stg-c tf-stg-inl"><input id="tf-frp-time" class="tf-stg-in tf-stg-n90" type="number" min="1" max="600" bind:value={draft.timeout} /><span>seconds</span></div>
      </div>
    </div>

    <div class="tf-stg-sec">
      <h2>Which folders</h2>
      <p class="tf-stg-p">Both lists must hold; an empty one takes any folder. Globs are separated by spaces, <span class="tf-mono">*</span> any run of characters.</p>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-holds">Holding a file<small>One of the folder’s own files matches, e.g. the low-code tool’s project file.</small></label>
        <div class="tf-stg-c"><input id="tf-frp-holds" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="*.lcproj" bind:value={draft.holds} /></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-folders">Named<small>The folder’s name, or with a <span class="tf-mono">/</span> its whole path.</small></label>
        <div class="tf-stg-c"><input id="tf-frp-folders" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="flows apps/*" bind:value={draft.folders} /></div>
      </div>
    </div>

    <div class="tf-stg-sec">
      <h2>Try it on</h2>
      <!-- The rows are drawn while the checkouts are read, and with no checkout anywhere they stay and say so: the answer moves nothing (2026-11-17). -->
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-repo">Repository<small>One with a checkout on this disk.</small></label>
        <div class="tf-stg-c tf-stg-inl">
          <select id="tf-frp-repo" class="tf-stg-in" bind:value={tryOn.picked} disabled={tryOn.list === null || tryOn.none} title={tryOn.failed ?? undefined}>
            {#if tryOn.list === null}<option value={null}>Reading…</option>{:else if tryOn.none}<option value={null}
                >{tryOn.failed !== null ? `Could not read the repositories: ${tryOn.failed}` : "No repository is cloned on this computer"}</option
              >{/if}
            {#each tryOn.list ?? [] as one (one.entityId)}<option value={one.entityId}>{one.title}</option>{/each}
          </select>
          <TfTryMore {tryOn} />
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-frp-folder">Folder<small>A path from the top; empty for the top folder.</small></label>
        <div class="tf-stg-c tf-stg-inl">
          <input id="tf-frp-folder" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" list="tf-frp-folders-list" placeholder="flows/orders" bind:value={folder} />
          <datalist id="tf-frp-folders-list">{#each (folderList ?? []).slice(0, FOLDERS_SHOWN) as one (one)}<option value={one}></option>{/each}</datalist>
          <button type="button" class="tf-btn tf-sm" disabled={!ready || repo === null} title={ready && repo !== null ? `Saves it, then runs it on ${folder.trim() || "the top folder"} of ${repo.title}` : (why ?? undefined)} onclick={() => void tryIt()}>Try it</button>
        </div>
      </div>
      {#if trying !== null}
        {#key trying.press}
          <div class="tf-stg-gap">
            <TfFolderRender
              entityId={trying.repo}
              renderer={trying.renderer}
              folder={trying.folder}
              title={trying.title}
              {ports}
              fileLink={(to) => fileHash(trying!.repo, to)}
            />
          </div>
        {/key}
      {/if}
    </div>

    {#if error !== null}<div class="tf-banner tf-err tf-stg-gap" role="alert">{error}</div>{/if}
    <p class="tf-stg-p tf-stg-gap">
      The program runs the folder’s files with your rights: a tool that builds or runs what is in it (<span class="tf-mono">npm install</span>,
      <span class="tf-mono">gradle</span>) runs what was pushed. It runs only when you open a folder’s drawing or press <b>Run again</b>, never on its own.
      The copy holds what git holds at the commit: LFS files are their pointers, and links are files holding their target.
    </p>

    <div class="tf-stg-acts tf-appacts">
      <button type="submit" class="tf-btn tf-sm tf-primary" disabled={!ready}>{at.renderer !== null ? "Save" : "Add folder renderer"}</button>
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={cancel}>Cancel</button>
      {#if why}<p class="tf-dwhy" role="status">{why}</p>{:else if changed && !busy}<span class="tf-stg-pend" role="status">Not saved</span>{/if}
      {#if saved !== null}
        <button type="button" class="tf-btn tf-sm tf-ghost tf-danger tf-appremove" disabled={busy} onclick={() => void remove()}>Remove</button>
      {/if}
    </div>
  </form>
{/if}
