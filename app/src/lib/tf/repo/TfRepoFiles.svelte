<!--
  The repository page's *Files* tab (#838, R-8.5): the tree on the left, the
  file on the right, as prototype-6's `.repo`.

  The tree is the source's, a directory at a time (`entity_view` with the
  section a listing names, #711): the top directory first, then each directory
  opened, and every directory above the file the address names. A file the
  local checkout changed is marked `M`; open, it warns above its text. The
  file is drawn with its line numbers; *History* goes to Commits. *Open in…*
  (#844) opens the file in the checkout, with an application's file command.

  *Edit* (#845, R-8.6) is the file's address under `edit/`: the file turns
  into an editor with line numbers under the banner that the commit goes to
  the source and the checkout is never written, and *Commit change…* opens
  `TfCommitFile`. Only a file the source gave a blob sha for (#883) can be
  edited, since that is what the commit is checked against; a binary or cut
  file cannot either, and *Edit* says why. A commit queued this session is a
  banner on the file, its echo (`tf/queued.svelte.ts`, #2105): *queued*, then
  *sent*, until the source holds a newer blob, the file read again on the
  refresh event and a run's end.

  With no file in the address the top directory's README opens.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutView, TreeEntry, ViewBlock, ViewSection } from "../../ipc/entity";
  import type { WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { echoOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfCommitFile from "./TfCommitFile.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import {
    REPO_FILES,
    ancestors,
    commitKind,
    commitRefusal,
    editHash,
    fileHash,
    fileLines,
    pickDefaultFile,
    repoHash,
    sourceName,
  } from "./repo";

  type FileBlock = Extract<ViewBlock, { type: "file" }>;

  let {
    ports,
    source = null,
    repoId,
    at,
    branch,
    path,
    changed,
    editing = false,
    job = null,
    checkoutBranch = null,
    checkout = undefined,
    title = "",
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "submitWrite" | "listen">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    repoId: string;
    /** The entity the tree is read at: the repository, or the branch picked. */
    at: string;
    /** The branch's name, for *on <branch>*. */
    branch: string | null;
    /** The file the address names, or `null`. */
    path: string | null;
    /** The paths the local checkout changed. */
    changed: ReadonlySet<string>;
    /** The address is the file's edit (`edit/<path>`). */
    editing?: boolean;
    /** The job's key, for the commit's message. */
    job?: string | null;
    /** The branch the local checkout is on, `null` without one. */
    checkoutBranch?: string | null;
    /** The repository's checkout, for *Open in…*; `undefined` while it is read. */
    checkout?: CheckoutView | undefined;
    /** The repository's `owner/name`. */
    title?: string;
    /** The page's refreshes (#2135): each reads the open directories and the file again over what the tab drew. */
    refreshed?: number;
  } = $props();

  /** Each directory read so far at `at`, by its path (`""` the top). */
  let listings = $state<Record<string, TreeEntry[]>>({});
  let treeFailed = $state<string | null>(null);
  const open = new SvelteSet<string>();
  let file = $state<FileBlock | null>(null);
  /** Why the source refuses a commit straight onto the branch (#985), `null` where it takes one. */
  let refusal = $state<string | null>(null);
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

  /** The section `section` names, read at `where`: a directory's listing or a file and its actions. */
  async function readSection(section: string, where = at): Promise<ViewSection | undefined> {
    const view = await ports.entityView(where, section);
    return view?.sections[0];
  }

  function readDir(dir: string, section: string) {
    if (asked.has(dir) || listings[dir] !== undefined) return;
    asked.add(dir);
    const mine = generation;
    readSection(section).then(
      (read) => {
        if (mine !== generation) return;
        const block = read?.blocks[0];
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
    refusal = null;
    fileFailed = null;
    if (section === null) return;
    if (section === MISSING) {
      fileFailed = `There is no file ${untrack(() => shown)} on ${untrack(() => branch) ?? "this branch"}.`;
      return;
    }
    let live = true;
    readSection(section, where).then(
      (read) => {
        if (!live) return;
        const block = read?.blocks[0];
        file = block?.type === "file" ? block : null;
        refusal = commitRefusal(read);
        draft = null;
        settleCommit(file);
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

  /** Why the file shown cannot be edited, `null` where it can. */
  const notEditable = $derived(
    file === null
      ? null
      : file.binary
        ? "A binary file is not edited here"
        : file.truncated
          ? "The file is longer than knobas shows, so it is not edited here"
          : file.sha === null
            ? "The source did not say which version of the file this is, so a commit could not be checked against it"
            : null,
  );
  const editable = $derived(file !== null && notEditable === null);

  /** The editor's text, `null` until the reader types: the file as read. */
  let draft = $state<string | null>(null);
  const text = $derived(draft ?? file?.text ?? "");
  const lineCount = $derived(text.split("\n").length);
  const numbers = $derived(Array.from({ length: lineCount }, (_, n) => n + 1).join("\n"));
  let committing = $state(false);

  // Leaving the edit drops what was typed, as *Cancel* does.
  $effect(() => {
    if (!editing) draft = null;
  });

  function openCommit() {
    if (file === null || text === file.text) {
      push({ text: "Nothing changed yet." });
      return;
    }
    committing = true;
  }

  /** The file's commit echo, while the file read is still the blob it was made against. */
  const queued = $derived.by(() => {
    if (file === null || branch === null) return null;
    const echo = echoOn(repoId, commitKind(branch, file.path));
    return echo !== null && echo.base === file.sha ? echo : null;
  });

  /** A read of `read` drops the file's commit echo once the source holds a newer blob. */
  function settleCommit(read: FileBlock | null) {
    const on = untrack(() => branch);
    if (read === null || on === null) return;
    settleEchoes(repoId, (echo) => echo.kind === commitKind(on, read.path) && echo.base !== read.sha);
  }

  // The refresh after the commit, or a run of its source's end, reads the
  // file again (#2105); what the reader typed stays.
  $effect(() => untrack(() => watchEchoes(REPO_FILES, rereadFile, { listen: ports.listen })));

  function rereadFile() {
    const section = fileSection;
    const where = at;
    if (section === null || section === MISSING) return;
    readSection(section, where).then(
      (read) => {
        if (section !== fileSection || where !== at) return;
        const block = read?.blocks[0];
        if (block?.type !== "file") return;
        file = block;
        refusal = commitRefusal(read);
        settleCommit(block);
      },
      // The file keeps what it drew; the next read tries again.
      () => {},
    );
  }

  // A refresh of the page reads every directory drawn and the file again,
  // over what the tab drew; what the reader typed stays.
  let seenRefresh: number | null = null;
  $effect(() => {
    const n = refreshed;
    if (seenRefresh !== null && n !== seenRefresh) untrack(() => {
        rereadTree();
        rereadFile();
      });
    seenRefresh = n;
  });

  function rereadTree() {
    const mine = generation;
    for (const dir of Object.keys(listings)) {
      const section = dir === "" ? "files" : entryOf(dir)?.section;
      if (section == null) continue;
      readSection(section).then(
        (read) => {
          const block = read?.blocks[0];
          if (mine === generation && block?.type === "tree") listings[dir] = block.entries;
        },
        // The directory keeps what it drew.
        () => {},
      );
    }
  }

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
    {#if file !== null && editing && editable && branch !== null}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-chip tf-warn">editing on {branch}</span>
        <span class="tf-acts">
          <a class="tf-btn tf-sm tf-ghost" href={fileHash(repoId, file.path)}>Cancel</a>
          <button type="button" class="tf-btn tf-sm tf-primary" onclick={openCommit}>Commit change…</button>
        </span>
      </header>
      <div class="tf-banner tf-info">
        <span>The edit is committed to <span class="tf-mono">{branch}</span> on {sourceName(source)}. Your local checkout is never written to.</span>
      </div>
      <div class="tf-editor">
        <div class="tf-nums" aria-hidden="true">{numbers}</div>
        <textarea
          spellcheck="false"
          rows={lineCount}
          aria-label="Edit {file.path}"
          value={text}
          oninput={(event) => (draft = (event.currentTarget as HTMLTextAreaElement).value)}
        ></textarea>
      </div>
    {:else if file !== null}
      <header>
        <span class="tf-p">{file.path}</span>
        <span class="tf-dim tf-small"
          >{file.binary ? `${file.size} bytes` : `${lines.length} ${lines.length === 1 ? "line" : "lines"}`}{branch !== null ? `, on ${branch}` : ""}</span
        >
        <span class="tf-acts">
          {#if editable}
            <a class="tf-btn tf-sm" href={editHash(repoId, file.path)}>Edit</a>
          {:else}
            <button type="button" class="tf-btn tf-sm" aria-disabled="true" title={notEditable}>Edit</button>
          {/if}
          <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}>History</a>
          <TfOpenIn entityId={repoId} {title} {checkout} file={file.path} />
        </span>
      </header>
      {#if queued !== null}
        <div class="tf-banner tf-warn">
          A commit of this file {queued.state === "sent" ? "was sent" : "is queued"}: “{queued.text}”. It shows here once {sourceName(source)} has it.
        </div>
      {/if}
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

{#if committing && file !== null && file.sha !== null && branch !== null}
  <TfCommitFile
    {repoId}
    {branch}
    path={file.path}
    before={file.text}
    after={text}
    baseSha={file.sha}
    {refusal}
    {job}
    {checkoutBranch}
    {ports}
    {source}
    onclose={() => (committing = false)}
  />
{/if}
