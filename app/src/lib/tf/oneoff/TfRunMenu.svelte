<!--
  *Run… ▾* (ADR-0060, section 9), in a file's header, the repository's header and a generated folder's bar: *Open in…*'s
  menu (`tf-appmenu`), a command a row, its name over its command as it will be run, and on the right how its last run
  on this thing ended. The first row has focus, so `Enter` runs it. Under the commands, where a run of this thing is
  kept, *Show the last run*; then *Add a command* and *Manage commands*.

  The button is always drawn where a command could apply and keeps its room, so stepping from file to file moves
  nothing; with no command for the thing its menu says so in one line. Not cloned here, it is greyed and says so.
  `X` on the Code page opens the most specific one standing (`oneoffs.openMenu`).
-->
<script lang="ts">
  import "../css/oneoff.css";
  import type { CheckoutView } from "../../ipc/entity";
  import type { OneOff, OneOffTarget } from "../../ipc/oneoff";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfMenu from "../TfMenu.svelte";
  import TfPath from "../TfPath.svelte";
  import { ago, endedShort, fill, shown, stateOf, takesFile, takesFolder, takesRepository } from "../../render/oneoff";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";

  let {
    entityId,
    repo,
    target,
    checkout,
    top = null,
    own = null,
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
    /** Which `X` opens where several stand: the file's over the folder's over the repository's. */
    priority?: number;
    small?: boolean;
    store?: OneOffs;
    layers?: Layers;
  } = $props();

  let open = $state(false);
  let face = $state<HTMLButtonElement | null>(null);

  const facts = $derived({ repo: entityId, top });
  const takes = $derived(
    store.commands.filter((c) =>
      target.kind === "file" ? takesFile(c, target.path, facts) : target.kind === "folder" ? takesFolder(c, target.path, own, facts) || (target.path === "" && takesRepository(c, facts)) : takesRepository(c, facts) || takesFolder(c, "", own, facts),
    ),
  );
  const root = $derived(checkout?.path ?? null);
  const notCloned = $derived(checkout !== undefined && root === null);
  const sep = $derived(root !== null && /^[A-Za-z]:\\/.test(root) ? "\\" : "/");

  /** The run of `command` on this thing, as the target it runs on. */
  function targetFor(command: OneOff): OneOffTarget {
    return target.kind === "repository" && takesFolder(command, "", own, facts) && !takesRepository(command, facts) ? { kind: "folder", path: "" } : target;
  }

  function willRun(command: OneOff): string {
    const t = targetFor(command);
    const filled = fill(command.command, {
      root: root ?? repo,
      sep,
      file: t.kind === "file" ? t.path : null,
      dir: t.kind === "folder" ? t.path : t.kind === "file" ? t.path.slice(0, Math.max(0, t.path.lastIndexOf("/"))) : null,
      line: t.kind === "file" ? (t.lines?.[0] ?? null) : null,
    });
    return "argv" in filled ? shown(filled.argv) : command.command;
  }

  const key = $derived(JSON.stringify(target));
  function last(command: OneOff) {
    return store.rows.find((r) => r.command === command.id && r.entity_id === entityId && JSON.stringify(r.target) === JSON.stringify(targetFor(command)));
  }
  const lastOfThing = $derived(store.rows.find((r) => r.entity_id === entityId && JSON.stringify(r.target) === key && r.kept));

  function run(command: OneOff) {
    open = false;
    store.openRun(command, entityId, repo, targetFor(command), root);
  }

  const addHash = $derived(`#/settings/commands/new?repo=${encodeURIComponent(entityId)}${target.kind === "file" || target.kind === "folder" ? `&on=${encodeURIComponent(target.path)}` : ""}`);

  // `X` opens the most specific menu standing.
  $effect(() => {
    const p = priority;
    return store.menu(p, () => {
      if (!notCloned) open = true;
    });
  });

  const tone = (c: OneOff) => {
    const row = last(c);
    if (row === undefined || row.ended === null || row.ended === undefined) return "";
    const st = stateOf(row.ended);
    return st.tone === "muted" ? "" : `tf-${st.tone}`;
  };
</script>

<span class="tf-openin tf-oo-runin">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-ghost"
    class:tf-sm={small}
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={notCloned}
    title={notCloned ? "Not cloned on this machine" : "Run one of your commands on this (X)"}
    onclick={(event) => {
      event.stopPropagation();
      open = !open;
    }}>Run…<svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
  >
  <TfMenu bind:open opener={face} label="Run" className="tf-appmenu tf-oo-menu" {layers}>
    {#each takes as command (command.id)}
      {@const row = last(command)}
      <button role="menuitem" onclick={() => run(command)}
        ><b>{command.name}</b><TfPath path={willRun(command)} />{#if row !== undefined}<span class="tf-oo-last"><span class="tf-dot {tone(command)}"></span>{endedShort(row.ended)}, {ago(row.started_at, Date.now())}</span>{/if}</button
      >
    {:else}
      <span class="tf-oo-none">No command of yours runs on this. A command is a program of yours, such as a linter, run here on a press.</span>
    {/each}
    {#if lastOfThing !== undefined}
      <hr />
      <button role="menuitem" onclick={() => { open = false; void store.reopen(lastOfThing.id); }}><b>Show the last run</b><span class="tf-dim">{lastOfThing.name}, {endedShort(lastOfThing.ended)}</span></button>
    {/if}
    <hr />
    <a role="menuitem" href={addHash} onclick={() => (open = false)}>Add a command</a>
    <a role="menuitem" href="#/settings/commands" onclick={() => (open = false)}>Manage commands</a>
  </TfMenu>
</span>
