<!--
  *Run… ▾* (ADR-0060, section 9), in a file's header, the repository's header and a generated folder's bar: *Open in…*'s
  menu (`tf-appmenu`), a command a row, its name over its command as it will be run, and on the right how its last run
  on this thing ended. The first row has focus, so `Enter` runs it. Under the commands, where a run of this thing is
  kept, *Show the last run*; then *Add a command* and *Manage commands*.

  The button is always drawn where a command could apply and keeps its room, so stepping from file to file moves
  nothing; with no command for the thing its menu says so in one line. Not cloned here, it is greyed and says so.
  `X` on the Code page opens the most specific one standing (`oneoffs.openMenu`).

  In a generated folder's bar (`made`, ADR-0060 amended 2026-11-25) every row runs on the folder knobas kept, never on
  the checkout, and nothing is greyed for a repository not cloned: the file's commands on the generated file shown, a
  rule, the folder's commands on the generated folder, a rule, the file commands that take two or more of its files.
-->
<script lang="ts">
  import "../css/oneoff.css";
  import type { CheckoutView } from "../../ipc/entity";
  import type { OneOff, OneOffTarget } from "../../ipc/oneoff";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfMenu from "../TfMenu.svelte";
  import TfPath from "../TfPath.svelte";
  import { ago, endedShort, fill, shown, stateOf, takesCommit, takesFile, takesFolder, takesMadeFile, takesMadeFolder, takesRange, takesRepository } from "../../render/oneoff";
  import { BATCH_MAX } from "../../render/oneoff.svelte";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";
  import { commandPageHash } from "../settings/command-page";
  import TfRepoCommands from "./TfRepoCommands.svelte";
  import type { OneOffRepoCommands } from "../../ipc/oneoff";

  let {
    entityId,
    repo,
    target,
    checkout,
    top = null,
    own = null,
    files = null,
    made = null,
    priority = 1,
    small = true,
    store = sharedStore,
    layers = sharedLayers,
  }: {
    /** The repository the run is in. */
    entityId: string;
    /** Its name, for the dialog. */
    repo: string;
    target: OneOffTarget;
    /** The repository's checkout, once read: its path fills the command, its absence greys the button. */
    checkout: CheckoutView | null | undefined;
    /** The names at the repository's top, where read. */
    top?: readonly string[] | null;
    /** A folder's own files, where read. */
    own?: readonly string[] | null;
    /** Files a file's command may run on one at a time (section 12): a folder's files, or a diff's changed files. */
    files?: { paths: readonly string[]; changed: boolean } | null;
    /**
     * A generated folder's bar: what `renderer` made of `folder`, kept at `dir` on disk (`null` while none is), its files
     * from its top, and the generated file shown (`null` on the folder). Every row runs on it.
     */
    made?: { renderer: string; folder: string; dir: string | null; files: readonly string[]; shown: string | null } | null;
    /** Which `X` opens where several stand: the file's over the folder's over the repository's. */
    priority?: number;
    small?: boolean;
    store?: OneOffs;
    layers?: Layers;
  } = $props();

  let open = $state(false);
  /** The repository's own file, read when the menu opens (section 22); its dialog when shown. */
  let fromRepo = $state<OneOffRepoCommands | null>(null);
  let showRepo = $state(false);
  $effect(() => {
    if (!open || checkout?.path == null) return;
    let live = true;
    store.repoCommands(entityId).then(
      (read) => live && (fromRepo = read),
      () => {},
    );
    return () => (live = false);
  });
  /** What the file offers that can be added: one Save would refuse is shown in the dialog, not counted here. */
  const newFromRepo = $derived(fromRepo?.offered.filter((o) => o.copied == null && o.refused == null).length ?? 0);
  const changedFromRepo = $derived(fromRepo?.offered.filter((o) => o.copied != null && o.theirs != null).length ?? 0);
  /** The menu's row for the repository's own file: what is new to add and what changed since it was added. */
  function repoOffer(fresh: number, changed: number): string {
    const said = [fresh > 0 ? `${fresh} ${fresh === 1 ? "command" : "commands"} from this repository` : "", changed > 0 ? `${changed} changed in this repository` : ""];
    return `${said.filter((s) => s !== "").join(", ")}…`;
  }
  let face = $state<HTMLButtonElement | null>(null);

  const facts = $derived({ repo: entityId, top });
  /** A generated folder's own files at its top: what *Holding a file* reads there. */
  const madeTop = $derived(made === null ? null : made.files.filter((p) => !p.includes("/")));
  /** The generated file shown's commands, and the generated folder's. */
  const madeFile = $derived(made === null || made.shown === null ? [] : store.commands.filter((c) => takesMadeFile(c, made.shown!, facts)));
  const madeFolder = $derived(made === null ? [] : store.commands.filter((c) => takesMadeFolder(c, "", madeTop, facts)));
  const takes = $derived(
    store.commands.filter((c) =>
      target.kind === "range"
        ? takesRange(c, facts)
        : target.kind === "commit"
          ? takesCommit(c, facts)
          : target.kind === "file"
            ? takesFile(c, target.path, facts)
            : target.kind === "folder"
              ? takesFolder(c, target.path, own, facts) || (target.path === "" && takesRepository(c, facts))
              : takesRepository(c, facts) || takesFolder(c, "", own, facts),
    ),
  );
  /** The file commands that take two or more of `files` (of a generated folder: of its files), each with the files it takes. */
  const many = $derived(
    made !== null
      ? store.commands.map((c) => ({ c, paths: made.files.filter((p) => takesMadeFile(c, p, facts)) })).filter((one) => one.paths.length >= 2)
      : files === null
        ? []
        : store.commands
            .map((c) => ({ c, paths: files.paths.filter((p) => takesFile(c, p, facts)) }))
            .filter((one) => one.paths.length >= 2),
  );
  const root = $derived(made !== null ? made.dir : (checkout?.path ?? null));
  /** A generated folder is run where knobas kept it: never greyed for a repository not cloned here. */
  const notCloned = $derived(made === null && checkout !== undefined && root === null);
  const sep = $derived(root !== null && /^[A-Za-z]:\\/.test(root) ? "\\" : "/");

  /** The run of `command` on this thing, as the target it runs on. */
  function targetFor(command: OneOff): OneOffTarget {
    return target.kind === "repository" && takesFolder(command, "", own, facts) && !takesRepository(command, facts) ? { kind: "folder", path: "" } : target;
  }
  /** A generated folder's runs: on the file shown, or on the folder's top. */
  const madeTarget = (file: boolean): OneOffTarget =>
    made === null ? target : { kind: "made", renderer: made.renderer, folder: made.folder, path: file ? (made.shown ?? "") : "", dir: !file, lines: null };

  function willRun(command: OneOff, t: OneOffTarget = targetFor(command)): string {
    const file = t.kind === "file" || (t.kind === "made" && !t.dir) ? t.path : null;
    const filled = fill(command.command, {
      root: root ?? repo,
      sep,
      file,
      dir: t.kind === "folder" || (t.kind === "made" && t.dir) ? t.path : file !== null ? file.slice(0, Math.max(0, file.lastIndexOf("/"))) : null,
      line: t.kind === "file" ? (t.lines?.[0] ?? null) : null,
      sha: t.kind === "commit" ? t.sha : null,
      base: t.kind === "range" ? t.base : null,
      head: t.kind === "range" ? t.head : null,
    });
    return "argv" in filled ? shown(filled.argv) : command.command;
  }

  const key = $derived(JSON.stringify(made === null ? target : madeTarget(made.shown !== null)));
  function last(command: OneOff, t: OneOffTarget = targetFor(command)) {
    return store.rows.find((r) => r.command === command.id && r.entity_id === entityId && JSON.stringify(r.target) === JSON.stringify(t));
  }
  const lastOfThing = $derived(store.rows.find((r) => r.entity_id === entityId && JSON.stringify(r.target) === key && r.kept));

  function run(command: OneOff, t: OneOffTarget = targetFor(command)) {
    open = false;
    store.openRun(command, entityId, repo, t, root);
  }

  // A generated folder's command page tries nothing: the path it would try is no file of the checkout (Fable).
  const addHash = $derived(made !== null ? commandPageHash({ repo: entityId }) : commandPageHash({ repo: entityId, on: target.kind === "file" || target.kind === "folder" ? target.path : "" }));

  // `X` opens the most specific menu standing.
  $effect(() => {
    const p = priority;
    return store.menu(p, () => {
      if (!notCloned) open = true;
    });
  });

  const tone = (c: OneOff, t: OneOffTarget = targetFor(c)) => {
    const row = last(c, t);
    if (row === undefined || row.ended === null || row.ended === undefined) return "";
    const st = stateOf(row.ended);
    return st.tone === "muted" ? "" : `tf-${st.tone}`;
  };
</script>

{#snippet row(command: OneOff, t: OneOffTarget)}
  {@const done = last(command, t)}
  <button role="menuitem" onclick={() => run(command, t)}
    ><b>{command.name}</b><TfPath path={willRun(command, t)} />{#if done !== undefined}<span class="tf-oo-last"><span class="tf-dot {tone(command, t)}"></span>{endedShort(done.ended)}, {ago(done.started_at, Date.now())}</span>{/if}</button
  >
{/snippet}

<span class="tf-openin tf-oo-runin">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-ghost"
    class:tf-sm={small}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="Run…"
    disabled={notCloned}
    title={notCloned ? "Not cloned on this machine" : "Run one of your commands on this (X)"}
    onclick={(event) => {
      event.stopPropagation();
      open = !open;
    }}
    ><!-- A narrow generated folder's bar draws the glyph instead of the word (code.css, Fable 2026-11-25). --><svg
      class="tf-oo-runglyph"
      viewBox="0 0 16 16"
      aria-hidden="true"><path d="M5 3.5v9l7.5-4.5z" /></svg
    ><span class="tf-oo-runword">Run…</span><svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
  >
  <TfMenu bind:open opener={face} label="Run" className="tf-appmenu tf-oo-menu" {layers}>
    {#if made !== null}
      {#if madeFile.length > 0}
        {@const fileAt = madeTarget(true)}
        {#each madeFile as command (command.id)}
          {@render row(command, fileAt)}
        {/each}
        {#if madeFolder.length > 0}<hr />{/if}
      {/if}
      {#each madeFolder as command (command.id)}
        {@render row(command, madeTarget(false))}
      {/each}
      {#if madeFile.length === 0 && madeFolder.length === 0}
        <span class="tf-oo-none">No command of yours runs on this generated folder. A command is a program of yours, such as a linter, run here on a press.</span>
      {/if}
    {:else}
    {#each takes as command (command.id)}
      {@const row = last(command)}
      <button role="menuitem" onclick={() => run(command)}
        ><b>{command.name}</b><TfPath path={willRun(command)} />{#if row !== undefined}<span class="tf-oo-last"><span class="tf-dot {tone(command)}"></span>{endedShort(row.ended)}, {ago(row.started_at, Date.now())}</span>{/if}</button
      >
    {:else}
      <span class="tf-oo-none">No command of yours runs on this. A command is a program of yours, such as a linter, run here on a press.</span>
    {/each}
    {/if}
    {#if many.length > 0}
      <hr />
      {#each many as { c, paths } (c.id)}
        {@const together = c.command.includes("{files}")}
        <button
          role="menuitem"
          disabled={!together && paths.length > BATCH_MAX}
          title={!together && paths.length > BATCH_MAX ? `More than ${BATCH_MAX} files: a command that takes the folder runs once instead` : undefined}
          onclick={() => {
            open = false;
            const of = made === null ? null : { renderer: made.renderer, folder: made.folder };
            if (together) store.openRun(c, entityId, repo, of === null ? { kind: "files", paths: [...paths] } : { kind: "made_files", ...of, paths: [...paths] }, root);
            else store.openBatch(c, entityId, repo, paths, root, of);
          }}
          ><b>{c.name} on {paths.length} {made !== null ? "generated files" : files?.changed ? "changed files" : `files${c.files.length > 0 ? ` that match ${c.files.join(" ")}` : ""}`}</b><span class="tf-dim">{together ? "One run, every file an argument" : "One run a file, four at a time"}</span></button
        >
      {/each}
    {/if}
    {#if lastOfThing !== undefined}
      <hr />
      <button role="menuitem" onclick={() => { open = false; void store.reopen(lastOfThing.id); }}><b>Show the last run</b><span class="tf-dim">{lastOfThing.name}, {endedShort(lastOfThing.ended)}</span></button>
    {/if}
    {#if newFromRepo > 0 || changedFromRepo > 0}
      <hr />
      <button role="menuitem" onclick={() => { open = false; showRepo = true; }}
        ><span class="tf-dim">{repoOffer(newFromRepo, changedFromRepo)}</span></button
      >
    {/if}
    <hr />
    <a role="menuitem" href={addHash} onclick={() => (open = false)}>Add a command</a>
    <a role="menuitem" href="#/settings/commands" onclick={() => (open = false)}>Manage commands</a>
  </TfMenu>
</span>
{#if showRepo && fromRepo !== null}
  <TfRepoCommands {entityId} {repo} read={fromRepo} {store} {layers} onclose={() => (showRepo = false)} />
{/if}
