<!--
  A command's page (ADR-0060, section 27): Settings › Commands › *Add a command* (`#/settings/commands/new`), an
  example of *Ready to add* (`…/example/<n>`) or a command's *Edit…* (`…/<id>`), each optionally followed by a
  repository and a file or folder to try it on. Shaped as the folder renderer's page.

  - *Name*; *Command* with the placeholder buttons, the refusal under the field as it is typed, and *It is offered on*
    as one line that holds its room; *It shows*; *Timeout*; *Which files* or *Which folders*; *How it starts*.
  - **Try it on**: a repository with a checkout here, and a file or folder in it. *Try it* saves the command (only a
    saved command runs, ADR-0016) and opens the run dialog on it.
  - The paragraph of what knobas does not protect against, then *Save command*, *Cancel*, *Remove*.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { kindOf } from "../../render/oneoff";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";
  import { push } from "../../shell/toasts.svelte";
  import type { EscTargetOf } from "../escape";
  import { backStep } from "../page-name.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { PressHold } from "../press-hold.svelte";
  import { insertAt } from "../openin/openin";
  import { lastTried, noteTried } from "../openin/app-page";
  import { realTryReads, type TryReads } from "../openin/try-reads";
  import { TryRepos } from "../openin/try-repos.svelte";
  import TfTryMore from "../openin/TfTryMore.svelte";
  import {
    COMMAND_PLACEHOLDERS,
    commandDraftFrom,
    commandWhy,
    offeredOn,
    READY_TO_ADD,
    sameCommandDraft,
    SHOWS,
    withCommandDraft,
    type CommandDraft,
    type CommandPageAt,
  } from "./command-page";

  let {
    at,
    store = sharedStore,
    reads = realTryReads,
    back = null,
  }: {
    at: CommandPageAt;
    store?: OneOffs;
    reads?: Pick<TryReads, "repos" | "checkouts" | "files">;
    back?: (Pick<EscTargetOf, "name"> & Partial<EscTargetOf>) | null;
  } = $props();

  const step = backStep();
  const SECTION = "#/settings/commands";

  $effect(() => {
    if (!store.loaded) void store.load();
  });
  const saved = $derived(at.command === null ? null : (store.commands.find((c) => c.id === at.command) ?? null));
  let draft = $state<CommandDraft>(commandDraftFrom(untrack(() => (at.example != null ? (READY_TO_ADD[at.example] ?? null) : null))));
  let seeded = false;
  $effect(() => {
    if (seeded || (at.command !== null && !store.loaded)) return;
    seeded = true;
    if (saved !== null) draft = commandDraftFrom(saved);
  });

  let error = $state<string | null>(null);
  let busy = $state(false);
  /** The id a *Try it* gave a new command, so a later *Save* changes it in place. */
  let ownId = $state<string | null>(untrack(() => at.command));
  const changed = $derived(!sameCommandDraft(draft, commandDraftFrom(saved)));
  const why = $derived(busy ? null : commandWhy(draft));
  const ready = $derived(why === null && !busy);
  const offered = $derived(offeredOn(draft));
  const kind = $derived(kindOf(draft.command, draft.stdin));

  let commandField = $state<HTMLInputElement | null>(null);
  function insert(placeholder: string) {
    const next = insertAt(draft.command, commandField?.selectionStart ?? null, placeholder);
    draft.command = next.value;
    queueMicrotask(() => {
      commandField?.focus();
      commandField?.setSelectionRange(next.cursor, next.cursor);
    });
  }

  // -- Try it on ----------------------------------------------------------------

  const tryOn = new TryRepos();
  const repo = $derived(tryOn.repo);
  let on = $state(untrack(() => at.on ?? ""));
  let gone = false;
  $effect(() => {
    void untrack(() => tryOn.read(reads, [at.repo, lastTried()], () => gone));
  });
  onDestroy(() => (gone = true));
  let fileList = $state<string[]>([]);
  $effect(() => {
    const id = tryOn.pickedId;
    if (id === null) return;
    let live = true;
    reads.files(id).then(
      (files) => live && (fileList = (files ?? []).slice(0, FILES_SHOWN)),
      () => live && (fileList = []),
    );
    return () => (live = false);
  });
  const FILES_SHOWN = 2000;

  async function keep(): Promise<string | null> {
    busy = true;
    error = null;
    try {
      const stored = await store.save(withCommandDraft(store.commands, draft, ownId));
      busy = false;
      const entry = ownId !== null ? stored.find((c) => c.id === ownId) : stored.at(-1);
      ownId = entry?.id ?? ownId;
      return ownId;
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return null;
    }
  }

  const tryHold = new PressHold();
  async function tryIt() {
    const picked = repo;
    if (!ready || picked === null || tryHold.held()) return;
    tryHold.hold();
    const id = await keep();
    const command = store.commands.find((c) => c.id === id);
    if (command === undefined) {
      tryHold.release();
      return;
    }
    noteTried(picked.entityId);
    const path = on.trim().replace(/^\/+|\/+$/g, "");
    const target =
      kind === "file" ? { kind: "file" as const, path, lines: null, at: null } : kind === "folder" ? { kind: "folder" as const, path } : { kind: "repository" as const };
    store.openRun(command, picked.entityId, picked.title, target, picked.path);
  }

  function goBack() {
    if (back?.hash !== undefined && step !== null) step(back.hash, back.opened ?? false);
    else location.hash = SECTION;
  }

  async function save() {
    if (!ready) return;
    if ((await keep()) === null) return;
    push({ text: `${at.command !== null ? "Saved" : "Added"} ${draft.name.trim()}. It is offered in Run… where it applies.` });
    goBack();
  }

  async function remove() {
    const removing = saved;
    if (removing === null) return;
    const had = store.commands.map((c) => ({ ...c }));
    busy = true;
    try {
      await store.save(had.filter((c) => c.id !== removing.id));
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return;
    }
    push({ text: `Removed ${removing.name}`, action: { label: "Undo", run: () => void store.save(had).catch(() => {}) } });
    goBack();
  }

  const missing = $derived(at.command !== null && store.loaded && saved === null && ownId === at.command);
  const title = $derived(at.command === null ? "Add a command" : (saved?.name ?? draft.name) || "A command");
  const isWindows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
</script>

{#if missing}
  <header class="tf-stg-h"><h1>A command</h1></header>
  <TfNothingHere message="There is no such command. It may have been removed." href={SECTION} action="Commands" />
{:else}
  <header class="tf-stg-h">
    <h1>{title}</h1>
    <p>A program of yours, run in the repository's checkout when you press it. What it prints opens in a dialog; it never changes how a file is drawn.</p>
  </header>
  <form
    class="tf-apppage"
    id="tf-ocp-form"
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
  >
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-ocp-name">Name <small class="tf-req">required</small><small>What the menus call it.</small></label>
      <div class="tf-stg-c"><input id="tf-ocp-name" aria-label="Name" class="tf-stg-in" type="text" autocomplete="off" placeholder="ESLint" bind:value={draft.name} /></div>
    </div>

    <div class="tf-stg-sec">
      <h2>Command</h2>
      <p class="tf-stg-p">
        It runs as you, with no shell, in the repository's top folder. A file's name is always one argument of its own. To pipe or chain, run a shell
        yourself and pass the path after the script: <span class="tf-mono">sh -c 'eslint "$1" | head -40' _ {"{file}"}</span>.
      </p>
      <div class="tf-phs" aria-label="Placeholders knobas fills in">
        {#each COMMAND_PLACEHOLDERS as placeholder (placeholder.name)}
          <button type="button" class="tf-phb" onclick={() => insert(placeholder.name)}><span class="tf-mono">{placeholder.name}</span> {placeholder.says}</button>
        {/each}
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-cmd">Command <small class="tf-req">required</small></label>
        <div class="tf-stg-c tf-appcmd">
          <input
            id="tf-ocp-cmd"
            aria-label="Command"
            class="tf-stg-in tf-mono"
            type="text"
            autocomplete="off"
            spellcheck="false"
            placeholder={"npx eslint --max-warnings 0 {file}"}
            bind:value={draft.command}
            bind:this={commandField}
          />
          {#if draft.command.trim() !== "" && why !== null && draft.name.trim() !== ""}<small class="tf-stg-errt" role="status">{why}</small>{/if}
          <p class="tf-oo-offered" role="status">{#if offered !== null}<b>It is offered on</b> {offered}{/if}</p>
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-shows">It shows<small>What its output is drawn as. The bytes overrule a label.</small></label>
        <div class="tf-stg-c">
          <select id="tf-ocp-shows" class="tf-stg-in" bind:value={draft.shows}>
            {#each SHOWS as one (one.value)}<option value={one.value}>{one.label}</option>{/each}
          </select>
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-time">Timeout<small>1 to 3,600 seconds; it is stopped after it, with everything it started.</small></label>
        <div class="tf-stg-c tf-stg-inl"><input id="tf-ocp-time" class="tf-stg-in tf-stg-n90" type="number" min="1" max="3600" bind:value={draft.timeout} /><span>seconds</span></div>
      </div>
    </div>

    {#if kind === "file"}
      <div class="tf-stg-sec">
        <h2>Which files</h2>
        <p class="tf-stg-p">Globs separated by spaces, <span class="tf-mono">*</span> any run of characters; empty for any file.</p>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-ocp-files">Files<small>The file's name, or with a <span class="tf-mono">/</span> its whole path.</small></label>
          <div class="tf-stg-c"><input id="tf-ocp-files" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="*.ts *.tsx" bind:value={draft.files} /></div>
        </div>
      </div>
    {:else if kind === "folder"}
      <div class="tf-stg-sec">
        <h2>Which folders</h2>
        <p class="tf-stg-p">Both lists must hold; an empty one takes any folder. The repository is its top folder.</p>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-ocp-folders">Folders named<small>The folder's name, or with a <span class="tf-mono">/</span> its whole path.</small></label>
          <div class="tf-stg-c"><input id="tf-ocp-folders" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="src packages/*" bind:value={draft.folders} /></div>
        </div>
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-ocp-has">Holding a file<small>One of the folder's own files matches.</small></label>
          <div class="tf-stg-c"><input id="tf-ocp-has" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="*.py" bind:value={draft.has} /></div>
        </div>
      </div>
    {/if}

    <div class="tf-stg-sec">
      <h2>How it starts</h2>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-holds">Only where the repository holds<small>A file at its top, such as package.json; empty for every repository.</small></label>
        <div class="tf-stg-c"><input id="tf-ocp-holds" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="package.json" bind:value={draft.repoHolds} /></div>
      </div>
      <div class="tf-stg-row">
        <div class="tf-stg-l">Starts in<small>A monorepo whose packages hold their own settings starts in the folder it runs on.</small></div>
        <div class="tf-stg-c tf-stg-inl">
          <label class="tf-stg-tick"><input type="radio" name="tf-ocp-start" value="root" bind:group={draft.startIn} /> The repository's top</label>
          <label class="tf-stg-tick"><input type="radio" name="tf-ocp-start" value="target" bind:group={draft.startIn} /> The folder it runs on</label>
        </div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-path">Add to PATH<small>A folder a line, put in front of PATH; <span class="tf-mono">{"{root}"}</span> is the repository.</small></label>
        <div class="tf-stg-c"><textarea id="tf-ocp-path" class="tf-stg-in tf-mono" rows="2" spellcheck="false" placeholder={"{root}/node_modules/.bin"} bind:value={draft.path}></textarea></div>
      </div>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-env">Environment<small><span class="tf-mono">NAME=value</span> a line; it wins over knobas's own.</small></label>
        <div class="tf-stg-c"><textarea id="tf-ocp-env" class="tf-stg-in tf-mono" rows="2" spellcheck="false" placeholder="NODE_OPTIONS=--max-old-space-size=4096" bind:value={draft.env}></textarea></div>
      </div>
      <div class="tf-stg-row">
        <div class="tf-stg-l">Colours<small>Asks the program for colours as a terminal would. A program that needs a flag takes it in its command, such as <span class="tf-mono">--color=always</span>.</small></div>
        <div class="tf-stg-c"><label class="tf-stg-tick"><input type="checkbox" bind:checked={draft.colours} /> Ask the program for colours</label></div>
      </div>
      {#if isWindows}
        <div class="tf-stg-row">
          <div class="tf-stg-l">WSL<small>For a tool in WSL, run as <span class="tf-mono">wsl -e ruff check {"{file}"}</span>.</small></div>
          <div class="tf-stg-c"><label class="tf-stg-tick"><input type="checkbox" bind:checked={draft.wslPaths} /> Paths as WSL sees them</label></div>
        </div>
      {/if}
    </div>

    <div class="tf-stg-sec">
      <h2>Try it on</h2>
      <div class="tf-stg-row">
        <label class="tf-stg-l" for="tf-ocp-repo">Repository<small>One with a checkout on this disk.</small></label>
        <div class="tf-stg-c tf-stg-inl">
          <select id="tf-ocp-repo" class="tf-stg-in" bind:value={tryOn.picked} disabled={tryOn.list === null || tryOn.none} title={tryOn.failed ?? undefined}>
            {#if tryOn.list === null}<option value={null}>Reading…</option>{:else if tryOn.none}<option value={null}
                >{tryOn.failed !== null ? `Could not read the repositories: ${tryOn.failed}` : "No repository is cloned on this computer"}</option
              >{/if}
            {#each tryOn.list ?? [] as one (one.entityId)}<option value={one.entityId}>{one.title}</option>{/each}
          </select>
          <TfTryMore {tryOn} />
        </div>
      </div>
      {#if kind === "file" || kind === "folder"}
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-ocp-on">{kind === "file" ? "File" : "Folder"}<small>A path from the top{kind === "folder" ? "; empty for the top folder" : ""}.</small></label>
          <div class="tf-stg-c tf-stg-inl">
            <input id="tf-ocp-on" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" list="tf-ocp-files-list" placeholder={kind === "file" ? "src/index.ts" : "src"} bind:value={on} />
            <datalist id="tf-ocp-files-list">{#each fileList.slice(0, FILES_SHOWN) as one (one)}<option value={one}></option>{/each}</datalist>
          </div>
        </div>
      {/if}
      <div class="tf-stg-acts">
        <button type="button" class="tf-btn tf-sm" disabled={!ready || repo === null || (kind === "file" && on.trim() === "")} title={ready && repo !== null ? `Saves it, then runs it on ${repo.title}` : (why ?? undefined)} onclick={() => void tryIt()}>Try it</button>
      </div>
    </div>

    {#if error !== null}<div class="tf-banner tf-err tf-stg-gap" role="alert">{error}</div>{/if}
    <p class="tf-stg-p tf-stg-gap">
      A command runs in your checkout as you, and may change files there, as a program you start in a terminal does. A linter runs the
      repository's own settings and plugins, and a build tool its scripts: running a command runs what was pushed to that checkout. It runs only
      when you press it, never on its own. On a generated folder it runs in the folder knobas kept, not in your checkout: it reads any settings
      the renderer wrote there and not your checkout's, and a generated file it changes has the folder rendered again.
    </p>

    <div class="tf-stg-acts tf-appacts">
      <button type="submit" class="tf-btn tf-sm tf-primary" disabled={!ready}>{at.command !== null ? "Save command" : "Add command"}</button>
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={goBack}>Cancel</button>
      {#if why}<p class="tf-dwhy" role="status">{why}</p>{:else if changed && !busy}<span class="tf-stg-pend" role="status">Not saved</span>{/if}
      {#if saved !== null}
        <button type="button" class="tf-btn tf-sm tf-ghost tf-danger tf-appremove" disabled={busy} onclick={() => void remove()}>Remove</button>
      {/if}
    </div>
  </form>
{/if}
