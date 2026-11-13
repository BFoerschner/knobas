<!--
  *Add an application* and an application's own page, in Settings › Checkouts
  and Open in (PR #2479, 2026-11-13; R-8.23, #844, #2274, ADR-0016). It was a
  dialog drawn where *Open in…* stood, which a Repositories row's fading group
  caught (Björn: "the open in... popup where you can add an application is
  broken"). Its address is `app-page.ts`'s; *Open in… › Add an application*
  comes here with the repository and the object it was pressed on, and
  Settings' list with an application's *Edit*.

  - **Name**, then one row per object, each with its command: the repository
    (required), a file (falls back to the repository's), a commit, a range, a
    pull request. An application is offered for the objects it has a command
    for. The placeholders are buttons that insert into the field last in.
  - **It will run**, under each command, filled from **Try it on**: a
    repository with a checkout on this disk (the one it came from, else the
    last one tried, else the first), and the file, commit or pull request
    picked from it. A range is the commit picked and the one before it.
    With no checkout anywhere it says so and how to get one.
  - **Try it** beside each line saves the draft and runs that command by the
    id the save gave it: only a saved template ever runs (ADR-0016, #1643).
    Leaving without *Save* (Cancel, Esc, Back, any other address) takes the
    try back: the entry is put back as it was before the first try, or gone if
    it was new, and nothing else in the list is touched. What was typed stays
    for the session, so coming back finds it; *Cancel* drops it.
  - **Save** writes the whole list (`set_checkout_applications`) and goes back
    where the page was opened from. *Remove* is at once, with *Undo* on its
    toast (the removal rule).
-->
<script module lang="ts">
  import type { AppDraft } from "./app-page";

  /** What was typed and not saved, per page (an application's id, or `add`), for the session. */
  const typedDrafts = new Map<string, AppDraft>();

  /** Tests only: forget what was typed. */
  export function forgetAppDrafts(): void {
    typedDrafts.clear();
  }
</script>

<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutApplication, OpenObject, OpenObjectKind } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import type { EscTargetOf } from "../escape";
  import { pageVisible } from "../kept-pages.svelte";
  import { backStep } from "../page-name.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { apps as sharedApps, type Apps } from "./apps.svelte";
  import {
    draftCommand,
    draftFrom,
    entryOf,
    lastTried,
    noteTried,
    sameDraft,
    tryObject,
    withEntry,
    withoutTry,
    type AppPageAt,
  } from "./app-page";
  import { insertAt, PLACEHOLDERS, runPreview, warnings, worktreeOf } from "./openin";
  import { CHECKOUT_BATCH, realTryReads, type TryCommit, type TryPr, type TryReads, type TryRepo } from "./try-reads";

  let {
    at,
    apps = sharedApps,
    reads = realTryReads,
    back = null,
    leave = undefined,
  }: {
    at: AppPageAt;
    apps?: Apps;
    reads?: TryReads;
    /** Where the page goes once it is done: the shell's way back. */
    back?: (Pick<EscTargetOf, "name"> & Partial<EscTargetOf>) | null;
    /** Tests: how it goes there. */
    leave?: (() => void) | undefined;
  } = $props();

  const key = untrack(() => at.app ?? "add");
  /** How many of each the pickers offer: the newest, which is what a person tries a command on. */
  const FILES_SHOWN = 2000;
  const COMMITS_SHOWN = 50;
  const PRS_SHOWN = 100;
  const visible = pageVisible();
  const step = backStep();

  const saved = $derived(at.app === null ? null : (apps.settings?.applications.find((app) => app.id === at.app) ?? null));

  let draft = $state<AppDraft>(typedDrafts.get(untrack(() => at.app ?? "add")) ?? draftFrom(null));
  let seeded = typedDrafts.has(untrack(() => at.app ?? "add"));
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    void apps.ensure();
  });
  // An application's fields once the list has answered, unless words typed before are waiting.
  $effect(() => {
    if (seeded || (at.app !== null && apps.settings === null)) return;
    seeded = true;
    if (saved !== null) draft = draftFrom(saved);
  });

  const changed = $derived(!sameDraft(draft, draftFrom(saved)));
  const ready = $derived(draft.name.trim() !== "" && draft.command.trim() !== "" && !busy);
  const why = $derived(
    busy ? null : draft.name.trim() === "" ? (draft.command.trim() === "" ? "Name it, and give its command." : "Name it first.") : draft.command.trim() === "" ? "Give its command." : null,
  );
  const warned = $derived(warnings(draft.command.trim()));

  // -- the fields and the placeholders ----------------------------------------

  type Field = "command" | "file" | "commit" | "range" | "pr";
  const ROWS: readonly { field: Field; kind: OpenObjectKind; label: string; says: string; example: string }[] = [
    { field: "command", kind: "repository", label: "Command", says: "Opens the repository. Every Open in… on a repository or a branch offers it.", example: "MyGitViewer {path}" },
    { field: "file", kind: "file", label: "For a file", says: "Without one, a file opens with the command above.", example: "MyGitViewer --open {file}" },
    { field: "commit", kind: "commit", label: "For a commit", says: "Offered on a commit only when it has one.", example: "tig -C {path} show {sha}" },
    { field: "range", kind: "range", label: "For a range", says: "Two commits, as Compare and a diff's address name them.", example: "git -C {path} difftool -d {base} {head}" },
    { field: "pr", kind: "pull_request", label: "For a pull request", says: "Runs in the pull request's own worktree under the clones root.", example: "MyGitViewer --pr {pr} {path}" },
  ];
  const fields: Partial<Record<Field, HTMLInputElement>> = {};
  /** The field a placeholder button inserts into: the one last focused. */
  let last: Field = "command";

  function insert(placeholder: string) {
    const field = fields[last];
    const next = insertAt(draft[last], field?.selectionStart ?? null, placeholder);
    draft[last] = next.value;
    queueMicrotask(() => {
      field?.focus();
      field?.setSelectionRange(next.cursor, next.cursor);
    });
  }

  // -- Try it on ---------------------------------------------------------------

  /** The repositories with a checkout here, in the order found; `null` while the first batch is out. */
  let repos = $state<TryRepo[] | null>(null);
  /** Still reading checkouts after the first batch. */
  let more = $state(false);
  let readFailed = $state<string | null>(null);
  let repoId = $state<string | null>(null);
  const repo = $derived(repos?.find((one) => one.entityId === repoId) ?? null);

  let files = $state<string[] | null>(null);
  let file = $state<string>("");
  let commits = $state<TryCommit[] | null>(null);
  let commit = $state<string | null>(null);
  let prs = $state<TryPr[] | null>(null);
  let prPick = $state<number | null>(null);
  let prEnds = $state<{ base: string; head: string } | null>(null);
  /** `{url}` for each object, read only when a command names it. */
  let pages = $state<Partial<Record<OpenObjectKind, string | null>>>({});

  $effect(() => {
    void untrack(() => readRepos());
  });

  async function readRepos() {
    let rows;
    try {
      rows = await reads.repos();
    } catch (cause) {
      readFailed = ipcErrorMessage(cause);
      repos = [];
      return;
    }
    const preferred = [at.repo, lastTried()].filter((id): id is string => id !== null);
    const order = [...preferred.filter((id) => rows.some((row) => row.entity_id === id)), ...rows.map((row) => row.entity_id).filter((id) => !preferred.includes(id))];
    const byId = new Map(rows.map((row) => [row.entity_id, row]));
    const found: TryRepo[] = [];
    more = true;
    for (let start = 0; start < order.length && !gone; start += CHECKOUT_BATCH) {
      try {
        const answers = await reads.checkouts(order.slice(start, start + CHECKOUT_BATCH));
        for (const answer of answers) {
          const row = byId.get(answer.entityId);
          if (answer.path !== null && row) found.push({ entityId: row.entity_id, title: row.title, sourceId: row.source_id, path: answer.path, clonesRoot: answer.clonesRoot });
        }
      } catch {
        // A batch that can't be read leaves its repositories out rather than claiming a checkout.
      }
      repos = [...found];
      if (repoId === null) repoId = preferred.find((id) => found.some((one) => one.entityId === id)) ?? found[0]?.entityId ?? null;
    }
    repos = [...found];
    more = false;
  }

  // The picked repository's objects, read again when another is picked.
  $effect(() => {
    const picked = repo;
    if (picked === null) return;
    untrack(() => readObjects(picked));
  });

  function readObjects(picked: TryRepo) {
    const still = () => repoId === picked.entityId;
    const asked = at.repo === picked.entityId ? at : null;
    files = null;
    commits = null;
    prs = null;
    prEnds = null;
    pages = {};
    file = asked?.kind === "file" && asked.value !== null ? asked.value : "README.md";
    commit = null;
    prPick = null;
    reads.files(picked.entityId).then(
      (list) => {
        if (!still()) return;
        files = list;
        if (list !== null && list.length > 0 && !list.includes(file)) file = list.find((one) => /^readme/i.test(one)) ?? list[0]!;
      },
      () => {},
    );
    reads.commits(picked.entityId).then(
      (list) => {
        if (!still()) return;
        commits = list;
        const wanted = asked?.kind === "commit" ? asked.value : null;
        commit = list.find((one) => wanted !== null && one.sha.startsWith(wanted))?.sha ?? (wanted !== null && /^[0-9a-f]{7,64}$/i.test(wanted) ? wanted : (list[0]?.sha ?? null));
      },
      () => still() && (commits = []),
    );
    reads.prs(picked).then(
      (list) => {
        if (!still()) return;
        prs = list;
        const wanted = asked?.kind === "pull_request" && asked.value !== null ? Number(asked.value) : null;
        prPick = list.find((one) => one.number === wanted)?.number ?? list[0]?.number ?? null;
      },
      () => still() && (prs = []),
    );
  }

  // A pull request's branches, read when one is picked and a command needs it.
  $effect(() => {
    const picked = prs?.find((one) => one.number === prPick) ?? null;
    if (picked === null || draft.pr.trim() === "") return;
    prEnds = null;
    reads.prEnds(picked.id).then(
      (ends) => prPick === picked.number && (prEnds = ends),
      () => {},
    );
  });

  const prItem = $derived(prs?.find((one) => one.number === prPick) ?? null);
  const objects = $derived.by((): Record<OpenObjectKind, OpenObject | null> => {
    const picks = {
      file: file.trim() === "" ? null : file.trim(),
      at: null,
      commits: commits?.map((one) => one.sha) ?? [],
      commit,
      pr: prItem !== null && prEnds !== null ? { number: prItem.number, base: prEnds.base, head: prEnds.head, item: prItem.id } : null,
    };
    return {
      repository: tryObject("repository", picks),
      file: tryObject("file", picks),
      commit: tryObject("commit", picks),
      range: tryObject("range", picks),
      pull_request: tryObject("pull_request", picks),
    };
  });

  // `{url}`: the object's page, read only for a command that names it.
  $effect(() => {
    const picked = repo;
    if (picked === null) return;
    for (const row of ROWS) {
      const object = objects[row.kind];
      if (object === null || !draftCommand(draft, row.kind).includes("{url}")) continue;
      const wanted = JSON.stringify(object);
      untrack(() => reads.page(picked.entityId, object)).then(
        (url) => {
          if (repoId === picked.entityId && JSON.stringify(objects[row.kind]) === wanted) pages = { ...pages, [row.kind]: url };
        },
        () => {},
      );
    }
  });

  /** What a row's line says: the command filled in, or what it waits for. */
  function runLine(kind: OpenObjectKind): { run: string | null; wait: string | null } {
    // A file with no command of its own opens with the repository's, which its row says in words rather than twice.
    const command = kind === "file" ? draft.file.trim() : draftCommand(draft, kind);
    if (command === "") return { run: null, wait: kind === "repository" ? "Type the command, and this shows what it starts." : null };
    if (repo === null) return { run: null, wait: repos === null || more ? "Reading the repositories on this disk…" : "Pick a repository below to see it filled in." };
    const object = objects[kind];
    if (object === null) {
      const waits: Record<OpenObjectKind, string> = {
        repository: "",
        file: "Name a file of the repository to try it on.",
        commit: commits === null ? "Reading its commits…" : "The repository has no commits to try it on.",
        range: commits === null ? "Reading its commits…" : "A range needs two commits; the repository has fewer.",
        pull_request: prs === null || (prItem !== null && prEnds === null) ? "Reading its pull requests…" : "The repository has no pull request to try it on.",
      };
      return { run: null, wait: waits[kind] };
    }
    const path = object.kind === "pull_request" ? (worktreeOf(repo.clonesRoot, repo.entityId, object.number) ?? repo.path) : repo.path;
    const run = runPreview(command, { path, object, url: pages[kind] ?? null });
    return run === null ? { run: null, wait: "A quote in the command is never closed." } : { run, wait: null };
  }

  // -- saving, trying, and taking a try back ---------------------------------------

  /** A try's save: the entry as it was before the first one, and the id the save gave it. */
  let tried = $state<{ before: CheckoutApplication | null; id: Promise<string | null> } | null>(null);
  /** The page was left: a try whose save lands after that starts nothing. */
  let gone = false;
  /** An application's page whose application the list doesn't hold. */
  const missing = $derived(at.app !== null && apps.settings !== null && saved === null && tried === null);

  /** Save `list`, keeping the default where it still names an entry; rejects with the refusal. */
  async function store(list: CheckoutApplication[]) {
    const current = apps.settings?.default_application ?? null;
    await apps.save(list, list.some((app) => app.id === current) ? current : null);
  }

  /** The id this page's entry has in the saved list: its own, or the one a try's save gave it. */
  async function entryId(): Promise<string> {
    return (await tried?.id) ?? at.app ?? "";
  }

  async function tryIt(kind: OpenObjectKind) {
    const object = objects[kind];
    const picked = repo;
    if (!ready || object === null || picked === null) return;
    busy = true;
    error = null;
    const first = tried === null;
    const own = await entryId();
    const entry = entryOf(draft, own);
    const list = withEntry(apps.settings?.applications ?? [], entry);
    const saving = store(list).then(() => (own !== "" ? own : (apps.settings?.applications.at(-1)?.id ?? null)));
    if (first) tried = { before: saved, id: saving.catch(() => null) };
    else tried = { ...tried!, id: saving.catch(() => null) };
    let id: string | null;
    try {
      id = await saving;
    } catch (cause) {
      error = ipcErrorMessage(cause);
      if (first) tried = null;
      busy = false;
      return;
    }
    busy = false;
    noteTried(picked.entityId);
    if (gone || id === null) return;
    await apps.runObject(picked.entityId, entry.name, id, object);
  }

  /** Put the list back as it was before the first try. */
  function takeBack() {
    const was = tried;
    tried = null;
    if (was === null) return;
    const name = draft.name.trim();
    void was.id
      .then((id) => (id === null ? undefined : store(withoutTry(apps.settings?.applications ?? [], id, was.before))))
      .catch((cause: unknown) => push({ text: `Could not take back the try of ${name}: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }

  /** Saved, removed or cancelled: nothing typed is kept. */
  let done = false;

  function goBack() {
    done = true;
    gone = true;
    if (leave) leave();
    else if (back?.hash !== undefined && step !== null) step(back.hash, back.opened ?? false);
    else location.hash = "#/settings/checkouts";
  }

  async function save() {
    if (!ready) return;
    busy = true;
    error = null;
    const list = withEntry(apps.settings?.applications ?? [], entryOf(draft, await entryId()));
    try {
      await store(list);
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return;
    }
    tried = null;
    typedDrafts.delete(key);
    push({ text: `${at.app !== null ? "Saved" : "Added"} ${draft.name.trim()}. It is in every Open in… menu.` });
    goBack();
  }

  async function remove() {
    // Held here: once the save lands the list no longer has it, and `saved` follows the list.
    const removing = saved;
    if (removing === null) return;
    const had = [...(apps.settings?.applications ?? [])];
    const wasDefault = apps.settings?.default_application ?? null;
    busy = true;
    try {
      await store(had.filter((app) => app.id !== removing.id));
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
      return;
    }
    tried = null;
    typedDrafts.delete(key);
    // The removal rule (Controls, *Disabled*): gone at once, with *Undo* on its toast.
    push({
      text: `Removed ${removing.name}`,
      action: {
        label: "Undo",
        run: () => {
          apps.save(had, wasDefault).catch((cause: unknown) => push({ text: `Could not put ${removing.name} back: ${ipcErrorMessage(cause)}`, tone: "err" }));
        },
      },
    });
    goBack();
  }

  function cancel() {
    typedDrafts.delete(key);
    draft = draftFrom(saved);
    takeBack();
    goBack();
  }

  /** Left without *Save*: the words stay for the session, the try goes. */
  function left() {
    if (!done && !sameDraft(draft, draftFrom(saved))) typedDrafts.set(key, $state.snapshot(draft));
    else typedDrafts.delete(key);
    takeBack();
  }

  $effect(() => {
    if (!visible()) untrack(left);
    else gone = false;
  });
  onDestroy(() => {
    gone = true;
    left();
  });

  const title = $derived(at.app === null ? "Add an application" : (saved?.name ?? draft.name) || "An application");
</script>

{#if missing}
  <header class="tf-stg-h"><h1>An application</h1></header>
  <TfNothingHere message="Open in… has no such application. It may have been removed." href="#/settings/checkouts" action="Checkouts and Open in" />
{:else}
  <header class="tf-stg-h">
    <h1>{title}</h1>
    <p>Open in… lists it on every repository, file, commit, range and pull request it has a command for, and runs it at the checkout on this disk.</p>
  </header>
  <form
    class="tf-apppage"
    id="tf-app-form"
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
  >
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-app-name">Name <small class="tf-req">required</small><small>What Open in… calls it.</small></label>
      <div class="tf-stg-c">
        <input id="tf-app-name" aria-label="Name" class="tf-stg-in" type="text" autocomplete="off" placeholder="MyGitViewer" bind:value={draft.name} />
      </div>
    </div>

    <div class="tf-stg-sec">
      <h2>Try it on</h2>
      {#if repos !== null && repos.length === 0 && !more}
        <p class="tf-stg-p" role="status">
          {#if readFailed !== null}Could not read the repositories: {readFailed}.{:else}No repository has a checkout on this disk yet, so there is nothing to try a command on. Open a repository and press
            <b>Clone</b>, or <a href="#/code">Code</a>’s <b>Clone all…</b>; it is here once it is cloned.{/if}
        </p>
      {:else}
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-app-repo">Repository<small>One with a checkout on this disk. It and the objects below fill each command in.</small></label>
          <div class="tf-stg-c tf-stg-inl">
            <select id="tf-app-repo" class="tf-stg-in" bind:value={repoId} disabled={repos === null}>
              {#if repos === null}<option value={null}>Reading…</option>{/if}
              {#each repos ?? [] as one (one.entityId)}
                <option value={one.entityId}>{one.title}</option>
              {/each}
            </select>
            {#if more}<small class="tf-dim" role="status">Finding more…</small>{/if}
          </div>
        </div>
        {#if repo !== null}
          {#if draft.file.trim() !== "" || draft.command.includes("{file}")}
            <div class="tf-stg-row">
              <label class="tf-stg-l" for="tf-app-tryfile">File<small>A path inside the checkout.</small></label>
              <div class="tf-stg-c">
                <input id="tf-app-tryfile" class="tf-stg-in tf-mono" type="text" autocomplete="off" spellcheck="false" list="tf-app-files" bind:value={file} />
                <datalist id="tf-app-files">
                  {#each (files ?? []).slice(0, FILES_SHOWN) as one (one)}<option value={one}></option>{/each}
                </datalist>
              </div>
            </div>
          {/if}
          {#if draft.commit.trim() !== "" || draft.range.trim() !== ""}
            <div class="tf-stg-row">
              <label class="tf-stg-l" for="tf-app-trycommit">Commit<small>A range is this commit and the one before it.</small></label>
              <div class="tf-stg-c">
                <select id="tf-app-trycommit" class="tf-stg-in" bind:value={commit} disabled={commits === null || commits.length === 0}>
                  {#if commits === null}<option value={commit}>Reading…</option>{:else if commits.length === 0}<option value={null}>No commits</option>{/if}
                  {#if commit !== null && commits !== null && !commits.some((one) => one.sha === commit)}<option value={commit}>{commit.slice(0, 12)}</option>{/if}
                  {#each (commits ?? []).slice(0, COMMITS_SHOWN) as one (one.sha)}
                    <option value={one.sha}>{one.sha.slice(0, 7)} {one.title}</option>
                  {/each}
                </select>
              </div>
            </div>
          {/if}
          {#if draft.pr.trim() !== ""}
            <div class="tf-stg-row">
              <label class="tf-stg-l" for="tf-app-trypr">Pull request</label>
              <div class="tf-stg-c">
                <select id="tf-app-trypr" class="tf-stg-in" bind:value={prPick} disabled={prs === null || prs.length === 0}>
                  {#if prs === null}<option value={prPick}>Reading…</option>{:else if prs.length === 0}<option value={null}>No pull requests</option>{/if}
                  {#each (prs ?? []).slice(0, PRS_SHOWN) as one (one.id)}
                    <option value={one.number}>#{one.number} {one.title}</option>
                  {/each}
                </select>
              </div>
            </div>
          {/if}
        {/if}
      {/if}
    </div>

    <div class="tf-stg-sec">
      <h2>Commands</h2>
      <p class="tf-stg-p">
        The command runs as you, with no shell, so pipes and <span class="tf-mono">&amp;&amp;</span> don’t work; point it at a script instead. Quote a
        path with spaces in double quotes. On Windows, name a <span class="tf-mono">.cmd</span> or <span class="tf-mono">.bat</span> with its extension.
      </p>
      <div class="tf-phs" aria-label="Placeholders knobas fills in">
        {#each PLACEHOLDERS as placeholder (placeholder.name)}
          <button type="button" class="tf-phb" onclick={() => insert(placeholder.name)}><span class="tf-mono">{placeholder.name}</span> {placeholder.says}</button>
        {/each}
      </div>
      {#each ROWS as row (row.field)}
        {@const line = runLine(row.kind)}
        <div class="tf-stg-row">
          <label class="tf-stg-l" for="tf-app-{row.field}"
            >{row.label}{#if row.field === "command"}{" "}<small class="tf-req">required</small>{/if}<small>{row.says}</small></label
          >
          <div class="tf-stg-c tf-appcmd">
            <input
              id="tf-app-{row.field}"
              aria-label={row.label}
              class="tf-stg-in tf-mono"
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder={row.field === "command" ? row.example : `optional, e.g. ${row.example}`}
              bind:value={draft[row.field]}
              bind:this={fields[row.field]}
              onfocus={() => (last = row.field)}
            />
            {#if line.run !== null || line.wait !== null}
              <div class="tf-apprun">
                {#if line.run !== null}
                  <code class="tf-mono" aria-label="It will run, {row.label.toLowerCase()}">{line.run}</code>
                  <button type="button" class="tf-btn tf-sm" disabled={!ready} title={ready ? `Saves it, then runs this on ${repo?.title}` : (why ?? undefined)} onclick={() => void tryIt(row.kind)}>Try it</button>
                {:else}
                  <span class="tf-dim">{line.wait}</span>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#each warned as warning (warning)}
      <div class="tf-banner tf-warn tf-stg-gap" role="status">{warning}</div>
    {/each}
    {#if error !== null}
      <div class="tf-banner tf-err tf-stg-gap" role="alert">{error}</div>
    {/if}
    <p class="tf-stg-p tf-stg-gap">
      The paths come only from this disk: the checkout knobas found or the path you set. A sha, a branch or a pull request number is passed only
      when it is a plain git name, and the URL is built from the source’s address; titles and other synced values are never passed to a program.
    </p>

    <div class="tf-stg-acts tf-appacts">
      <button type="submit" class="tf-btn tf-sm tf-primary" disabled={!ready}>{at.app !== null ? "Save" : "Add application"}</button>
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={cancel}>Cancel</button>
      {#if why}<p class="tf-dwhy" role="status">{why}</p>{:else if changed && !busy}<span class="tf-stg-pend" role="status">Not saved</span>{/if}
      {#if saved !== null}
        <button type="button" class="tf-btn tf-sm tf-ghost tf-danger tf-appremove" disabled={busy} onclick={() => void remove()}>Remove</button>
      {/if}
    </div>
  </form>
{/if}
