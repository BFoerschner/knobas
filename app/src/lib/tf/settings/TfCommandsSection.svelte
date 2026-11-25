<!--
  Settings › Commands (ADR-0060, section 27): a person's own programs, run on a press and read in a dialog.

  - **The table**: a row a command, every cell one line: the name, the command (`TfCommand`), what it is offered on,
    what it shows, *Edit…*, ↑ and *Remove*. The order is the menus' order.
  - **Ready to add**: six examples that save nothing until pressed; a press opens a new command's page filled in.
  - **Where programs are found**: the login shell's `PATH`, read once a session (macOS and Linux).
  - **Last runs**: when, which command, on what, how it ended and how long it took, with *Show* where the lines are
    still kept, *Run again*, and *Forget every run*.

  No command is installed by itself: the menus are empty until the person adds one.
-->
<script lang="ts">
  import "../css/oneoff.css";
  import { ipcErrorMessage } from "../../ipc";
  import type { OneOff, OneOffRunRow, OneOffShell } from "../../ipc/oneoff";
  import { ago, endedShort, stateOf, targetLabel, took } from "../../render/oneoff";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { PressHold } from "../press-hold.svelte";
  import TfCommand from "../TfCommand.svelte";
  import { commandPageHash, countLine, draftOf, onWords, READY_TO_ADD, SHOWS } from "./command-page";

  let { store = sharedStore }: { store?: OneOffs } = $props();

  // Read again each time the section opens: the list is what the menus offer, and the runs move.
  $effect(() => {
    void store.load();
  });

  let shell = $state<OneOffShell | null>(null);
  $effect(() => {
    void store.shell(false).then((answer) => (shell = answer), () => {});
  });

  const shows = (c: OneOff) => SHOWS.find((s) => s.value === c.shows)?.label ?? "As the bytes say";

  /** ↑ pressed twice moves one command one place (the press rule). */
  const upHold = new PressHold();
  async function moveUp(index: number) {
    const list = store.commands.map(draftOf);
    if (index < 1 || index >= list.length || upHold.held()) return;
    upHold.hold("", 1000);
    const [moved] = list.splice(index, 1);
    list.splice(index - 1, 0, moved!);
    await store.save(list).catch((cause: unknown) => {
      upHold.release();
      push({ text: `Could not save the order: ${ipcErrorMessage(cause)}`, tone: "err" });
    });
  }

  async function remove(c: OneOff) {
    const had = store.commands.map(draftOf);
    try {
      await store.save(had.filter((d) => d.id !== c.id));
    } catch (cause) {
      push({ text: `Could not remove ${c.name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return;
    }
    push({ text: `Removed ${c.name}`, action: { label: "Undo", run: () => void store.save(had).catch(() => {}) } });
  }

  /** An example's page: the new command's page with the example filled in, saved only there. */
  function exampleHash(index: number): string {
    return `#/settings/commands/example/${index}`;
  }

  const RUNS_SHOWN = 200;
  const now = Date.now();

  function again(row: OneOffRunRow) {
    const command = store.commands.find((c) => c.id === row.command);
    if (command === undefined) {
      push({ text: `${row.name} is no longer one of your commands.`, tone: "err" });
      return;
    }
    store.openRun(command, row.entity_id, row.entity_id.split("/").pop() ?? row.entity_id, row.target);
  }

  async function readShell(again: boolean, on: boolean | null) {
    shell = await store.shell(again, on).catch((cause: unknown) => {
      push({ text: `Could not read your shell's PATH: ${ipcErrorMessage(cause)}`, tone: "err" });
      return shell;
    });
  }
</script>

<header class="tf-stg-h">
  <h1>Commands</h1>
  <p>Your own programs, such as a linter, run on a file, a folder or the repository when you press it. What one prints opens in a dialog.</p>
</header>

<div class="tf-stg-sec">
  <p class="tf-stg-p">{countLine(store.commands)}</p>
  {#if store.commands.length > 0}
    <table class="tf-tbl tf-rnd-tbl tf-oo-tbl">
      <thead><tr><th>Command</th><th>Runs</th><th>On</th><th>Shows</th><th><span class="tf-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each store.commands as c, index (c.id)}
          <tr>
            <td title={c.name}><b>{c.name}</b></td>
            <td><TfCommand command={c.command} /></td>
            <td title={onWords(c)}>{onWords(c)}</td>
            <td>{shows(c)}</td>
            <td>
              <button class="tf-btn tf-sm tf-ghost" disabled={index === 0} title={index === 0 ? "It is first" : `Move ${c.name} up`} aria-label="Move {c.name} up" onclick={() => void moveUp(index)}>↑</button>
              <a class="tf-btn tf-sm" href={commandPageHash({ command: c.id })}>Edit…</a>
              <button class="tf-btn tf-sm tf-ghost tf-danger" onclick={() => void remove(c)}>Remove</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="tf-stg-acts tf-stg-gap"><a class="tf-btn tf-sm" href={commandPageHash()}>Add a command</a></div>
</div>

<div class="tf-stg-sec">
  <h2>Ready to add</h2>
  <p class="tf-stg-p">Nothing is added until you press one and save it.</p>
  <div class="tf-oo-examples">
    {#each READY_TO_ADD as example, index (example.name)}
      <a class="tf-oo-ex" href={exampleHash(index)}><b>{example.name}</b><span title={example.command}>{example.command}</span></a>
    {/each}
  </div>
</div>

{#if shell === null || shell.offered}
  <div class="tf-stg-sec">
    <h2>Where programs are found</h2>
    <div class="tf-stg-row">
      <div class="tf-stg-l">
        Use my shell's PATH<small
          >knobas started from the Dock or a launcher has a short PATH, without cargo or a project's tools. With this on, your login shell is asked for its PATH once a session, at the first run.</small
        >
      </div>
      <div class="tf-stg-c tf-stg-inl">
        <label class="tf-stg-tick"><input type="checkbox" checked={shell?.on ?? true} disabled={shell === null} onchange={(event) => void readShell(false, (event.currentTarget as HTMLInputElement).checked)} /> On</label>
        {#if shell?.read}
          <span class="tf-dim" title={shell.read.path ?? undefined}
            >{shell.read.failed ?? `${shell.read.folders} folders from ${shell.read.shell}, ${ago(shell.read.read_at, now)}`}</span
          >
        {:else if shell !== null}
          <span class="tf-dim">Not read yet this session.</span>
        {/if}
        <button class="tf-btn tf-sm" disabled={shell === null || !shell.on} onclick={() => void readShell(true, null)}>Read again</button>
      </div>
    </div>
  </div>
{/if}

<div class="tf-stg-sec">
  <h2>Last runs</h2>
  <p class="tf-stg-p">
    The last 200 runs, kept in a file of knobas's own that no backup carries. What a run printed is kept only until knobas quits.
  </p>
  {#if store.rows.length > 0}
    <table class="tf-tbl tf-rnd-tbl tf-oo-runs">
      <thead><tr><th>When</th><th>Command</th><th>On</th><th>Ended</th><th>Took</th><th><span class="tf-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each store.rows.slice(0, RUNS_SHOWN) as row (row.id)}
          {@const st = stateOf(row.ended ?? null)}
          <tr>
            <td>{ago(row.started_at, now)}</td>
            <td title={row.argv.join(" ")}>{row.name}</td>
            <td title={targetLabel(row.target, row.entity_id)}>{targetLabel(row.target, row.entity_id.split("/").pop() ?? row.entity_id)}</td>
            <td><span class="tf-dot {st.tone === 'muted' ? '' : st.tone === 'info' ? 'tf-run' : `tf-${st.tone}`}"></span> {endedShort(row.ended)}</td>
            <td>{row.duration_ms != null ? took(row.duration_ms) : ""}</td>
            <td>
              {#if row.kept || row.ended == null}<button class="tf-btn tf-sm tf-ghost" onclick={() => void store.reopen(row.id)}>Show</button>{/if}
              <button class="tf-btn tf-sm tf-ghost" onclick={() => again(row)}>Run again</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn tf-sm tf-ghost tf-danger" onclick={() => void store.forget(null)}>Forget every run</button></div>
  {:else}
    <p class="tf-dim">None yet.</p>
  {/if}
</div>
