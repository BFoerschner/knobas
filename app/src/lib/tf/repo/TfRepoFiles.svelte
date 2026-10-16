<!--
  The repository page's *Files* tab (#838, R-8.5): the tree on the left, the
  file on the right, as prototype-6's `.repo`.

  The tree is the source's, a directory at a time (`entity_view` with the
  section a listing names, #711): the top directory first, then each directory
  opened, and every directory above the file the address names. A file the
  local checkout changed is marked `M`; open, it warns above its text. The
  file is drawn with its line numbers; *History* goes to Commits. *Edit*
  (#845) and *Open in…* (#844) are not built yet.

  With no file in the address the top directory's README opens.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
  import type { TreeEntry, ViewBlock } from "../../ipc/entity";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { ancestors, fileHash, fileLines, pickDefaultFile, repoHash } from "./repo";

  type FileBlock = Extract<ViewBlock, { type: "file" }>;

  let {
    ports,
    repoId,
    at,
    branch,
    path,
    changed,
  }: {
    ports: Pick<RepoPorts, "entityView">;
    repoId: string;
    /** The entity the tree is read at: the repository, or the branch picked. */
    at: string;
    /** The branch's name, for *on <branch>*. */
    branch: string | null;
    /** The file the address names, or `null`. */
    path: string | null;
    /** The paths the local checkout changed. */
    changed: ReadonlySet<string>;
  } = $props();

  /** Each directory read so far at `at`, by its path (`""` the top). */
  let listings = $state<Record<string, TreeEntry[]>>({});
  let treeFailed = $state<string | null>(null);
  const open = new SvelteSet<string>();
  let file = $state<FileBlock | null>(null);
  let fileFailed = $state<string | null>(null);
  /** Directory reads asked and not answered, at the current `at`. */
  let asked = new Set<string>();
  let generation = 0;

  const join = (dir: string, name: string) => (dir === "" ? name : `${dir}/${name}`);
  const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");
  const nameOf = (full: string) => full.slice(full.lastIndexOf("/") + 1);

  /** The entry `full` names in its parent's listing, once that is read. */
  function entryOf(full: string): TreeEntry | undefined {
    return listings[parentOf(full)]?.find((entry) => entry.name === nameOf(full));
  }

  async function readSection(section: string, where = at): Promise<ViewBlock | undefined> {
    const view = await ports.entityView(where, section);
    return view?.sections[0]?.blocks[0];
  }

  function readDir(dir: string, section: string) {
    if (asked.has(dir) || listings[dir] !== undefined) return;
    asked.add(dir);
    const mine = generation;
    readSection(section).then(
      (block) => {
        if (mine !== generation) return;
        listings[dir] = block?.type === "tree" ? block.entries : [];
      },
      (error) => {
        if (mine === generation) treeFailed = ipcErrorMessage(error);
      },
    );
  }

  // A new branch is a new tree.
  $effect(() => {
    void at;
    generation += 1;
    asked = new Set();
    listings = {};
    treeFailed = null;
    untrack(() => readDir("", "files"));
  });

  // The directories above the file the address names open.
  $effect(() => {
    if (path !== null) for (const dir of ancestors(path)) open.add(dir);
  });

  // Every open directory is read once its parent is.
  $effect(() => {
    for (const dir of open) {
      const entry = entryOf(dir);
      if (entry?.kind === "dir" && entry.section !== null) readDir(dir, entry.section);
    }
  });

  const shown = $derived(path ?? pickDefaultFile(listings[""] ?? []));

  const MISSING = "\u0000missing";

  /**
   * The section that opens the file shown, once its directory is read;
   * `missing` where the directory holds no such file.
   */
  const fileSection = $derived.by((): string | null => {
    if (shown === null || listings[parentOf(shown)] === undefined) return null;
    const entry = entryOf(shown);
    return entry === undefined || entry.section === null || entry.kind === "dir" ? MISSING : entry.section;
  });
  // The file, once its directory has said which section opens it.
  $effect(() => {
    const section = fileSection;
    const where = at;
    file = null;
    fileFailed = null;
    if (section === null) return;
    if (section === MISSING) {
      fileFailed = `There is no file ${untrack(() => shown)} on ${untrack(() => branch) ?? "this branch"}.`;
      return;
    }
    let live = true;
    readSection(section, where).then(
      (block) => {
        if (live) file = block?.type === "file" ? block : null;
      },
      (error) => {
        if (live) fileFailed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  const lines = $derived(file === null || file.binary ? [] : fileLines(file.text));

  function toggle(dir: string) {
    if (open.has(dir)) open.delete(dir);
    else open.add(dir);
  }
</script>

{#snippet listing(dir: string, depth: number)}
  {#each listings[dir] ?? [] as entry (entry.name)}
    {@const full = join(dir, entry.name)}
    {#if entry.kind === "dir"}
      <button
        type="button"
        class="tf-fdir tf-d{Math.min(depth, 8)}"
        aria-expanded={open.has(full)}
        onclick={() => toggle(full)}>{entry.name}/</button
      >
      {#if open.has(full)}
        {@render listing(full, depth + 1)}
      {/if}
    {:else if entry.section !== null}
      <a
        href={fileHash(repoId, full)}
        class="tf-d{Math.min(depth, 8)}"
        class:tf-changed={changed.has(full)}
        aria-current={full === shown ? "true" : undefined}
        title={changed.has(full) ? "Changed in your checkout" : undefined}>{entry.name}</a
      >
    {:else}
      <span class="tf-fsub tf-d{Math.min(depth, 8)}">{entry.name}</span>
    {/if}
  {/each}
{/snippet}

<div class="tf-repo">
  <nav class="tf-ftree" aria-label="Files">
    {#if treeFailed !== null}
      <p class="tf-cfail">{treeFailed}</p>
    {/if}
    {@render listing("", 0)}
  </nav>
  <div class="tf-fileview">
    {#if file !== null}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-dim tf-small"
          >{file.binary ? `${file.size} bytes` : `${lines.length} ${lines.length === 1 ? "line" : "lines"}`}{branch !== null ? `, on ${branch}` : ""}</span
        >
        <span class="tf-acts">
          <button type="button" class="tf-btn tf-sm" aria-disabled="true" title="Not built in the new shell yet">Edit</button>
          <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}>History</a>
          <button type="button" class="tf-btn tf-sm tf-ghost" aria-disabled="true" title="Not built in the new shell yet"
            >Open in…</button
          >
        </span>
      </header>
      {#if changed.has(file.path)}
        <div class="tf-banner tf-warn">
          Your checkout has an uncommitted change to this file.
          <span class="tf-acts"><a class="tf-btn tf-sm" href={repoHash(repoId, "local")}>See local changes</a></span>
        </div>
      {/if}
      {#if file.truncated}
        <div class="tf-banner tf-info">
          The file is longer than knobas shows.
          {#if file.url}<span class="tf-acts"><a class="tf-btn tf-sm" href={file.url}>The rest on the source</a></span>{/if}
        </div>
      {/if}
      {#if file.binary}
        <div class="tf-empty"><p>A binary file, not drawn.</p></div>
      {:else}
        <pre class="tf-code">{#each lines as line, n (n)}<div class="tf-l"><span>{n + 1}</span><span>{line || " "}</span></div>{/each}</pre>
      {/if}
    {:else if fileFailed !== null}
      <p class="tf-cfail tf-fpad">{fileFailed}</p>
    {:else if shown === null && listings[""] !== undefined}
      <div class="tf-empty"><p>Pick a file in the tree.</p></div>
    {/if}
  </div>
</div>
