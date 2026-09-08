<!--
  The **Clones root** and **Open commands** sections of the settings view
  (issues #499 and #501, spec #491 stories 25 and 29).

  One directory: where knobas looks for the clones on this machine. It scans two
  levels under it when a repo or a branch detail opens and matches each clone's
  `origin` to the repo by host and owner/repo, so `~/src/payout-service` and
  `~/src/gitea.example.com/payout-service` are both reached and nothing deeper
  is walked.

  Below it, the three commands the buttons on a repo or branch detail run:
  *Open in VS Code*, *Open in JetBrains*, *Open terminal here*. Each is a
  template with one placeholder, `{path}`, and macOS starts with a default for
  all three; on another platform they are empty and the buttons read *not
  configured* until somebody fills one in.

  **knobas never writes to a working tree** (ADR-0016): no clone, no checkout,
  no fetch. The section says so, because a directory setting handed to an app
  is exactly the moment a person wants to know what it will do with it. The
  other half of that ADR is what the placeholder is about: `{path}` is the only
  value knobas substitutes, so nothing a source mirrored can reach a program it
  starts.

  A repo the scan misses — a clone outside the root, a linked worktree — is
  answered on the repo's own detail, not here: the override is per repository
  and belongs beside the repository.

  The field draws what is **stored**, and the stored value is what the write
  answers with, which is `PassiveSection`'s rule and for its reason: a field
  that kept what was typed would tell the reader they had set a root on the one
  run where the write failed.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    checkoutCommands as realReadCommands,
    clonesRoot as realRead,
    setCheckoutCommand as realWriteCommand,
    setClonesRoot as realWrite,
    type CheckoutCommand,
  } from "../ipc/entity";
  import { latestRead } from "../shell/latest-read";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `PassiveSection`'s shape. */
    ports?: Partial<{
      clonesRoot: typeof realRead;
      setClonesRoot: typeof realWrite;
      checkoutCommands: typeof realReadCommands;
      setCheckoutCommand: typeof realWriteCommand;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = {
    clonesRoot: realRead,
    setClonesRoot: realWrite,
    checkoutCommands: realReadCommands,
    setCheckoutCommand: realWriteCommand,
    ...ports,
  };

  /** `undefined` until the first read answers — *unknown*, which is not *unset*. */
  let stored = $state<string | null | undefined>(undefined);
  let draft = $state("");
  let failure = $state<string | null>(null);
  let saving = $state(false);

  const read = latestRead<string | null>();

  function load() {
    return read(io.clonesRoot, {
      ok: (value) => {
        stored = value;
        draft = value ?? "";
        failure = null;
      },
      fail: (cause) => {
        failure = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    void load();
  });

  /** Store `value`, then re-read, so the field shows what is now stored. */
  async function save(value: string | null) {
    saving = true;
    try {
      await io.setClonesRoot(value);
      failure = null;
      await load();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  // -- the open commands (#501) ---------------------------------------------

  /** `null` until the first read answers — *unknown*, which is not *empty*. */
  let commands = $state<CheckoutCommand[] | null>(null);
  /** One draft per action, keyed by its id. */
  let drafts = $state<Record<string, string>>({});
  let commandFailure = $state<string | null>(null);
  /** The action being written, so two Saves cannot overlap. */
  let savingCommand = $state<string | null>(null);

  const readCommands = latestRead<CheckoutCommand[]>();

  /**
   * Take what the backend now says, drafts included.
   *
   * The drafts are reset from the answer rather than left alone, which is the
   * clones-root field's rule and for its reason: a field that kept what was
   * typed would tell the reader they had set a command on the one run where
   * the write was refused.
   */
  function takeCommands(answer: CheckoutCommand[]) {
    commands = answer;
    drafts = Object.fromEntries(answer.map((command) => [command.action, command.template ?? ""]));
    commandFailure = null;
  }

  function loadCommands() {
    return readCommands(io.checkoutCommands, {
      ok: takeCommands,
      fail: (cause) => {
        commandFailure = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    void loadCommands();
  });

  /** Store one action's template, or clear it back to the platform's. */
  async function saveCommand(action: string, template: string | null) {
    savingCommand = action;
    try {
      takeCommands(await io.setCheckoutCommand(action, template));
    } catch (cause) {
      commandFailure = ipcErrorMessage(cause);
    } finally {
      savingCommand = null;
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Clones root</span>
</div>

<div class="sec-b">
  <p>
    Where your clones live. When a repo or a branch opens, knobas looks two
    directory levels under this one for a clone whose <code>origin</code> is that
    repo — so both <code>~/src/payout-service</code> and
    <code>~/src/gitea.example.com/payout-service</code> are found, and nothing
    three levels down is.
  </p>
  <p class="sub">
    Reading only. knobas never clones, checks out or fetches anything: a repo
    with no clone here shows you the command to run yourself. A clone that lives
    somewhere else, or a worktree, is set on that repo's own detail.
  </p>

  {#if failure}
    <p class="fail">{failure}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  {:else if stored !== undefined}
    <div class="field">
      <label class="lab" for="clones-root">Directory</label>
      <input
        id="clones-root"
        class="inp"
        bind:value={draft}
        disabled={saving}
        placeholder="/Users/you/src"
      />
      <!--
        Every button in this component says *Save*, *Clear* or *Reset*, and
        four of them are on screen at once — so each carries an accessible name
        that says which field it belongs to. That is an accessibility fix
        first: a reader moving by control hears four identical "Save"s
        otherwise. It is also what lets the desktop witness press one of them
        by name (#501), which `just witness-unit` pins.
      -->
      <div class="acts">
        <button
          class="btn"
          aria-label="Save clones root"
          disabled={saving || draft.trim() === (stored ?? "")}
          onclick={() => void save(draft)}
        >
          Save
        </button>
        {#if stored !== null}
          <button class="btn" aria-label="Clear clones root" disabled={saving} onclick={() => void save(null)}>
            Clear
          </button>
        {/if}
      </div>
      {#if stored === null}
        <p class="sub">Not set — no checkout is looked for until it is.</p>
      {/if}
    </div>
  {/if}
</div>

<div class="tile-h">
  <span class="lab">Open commands</span>
</div>

<div class="sec-b">
  <p>
    What the buttons on a repo or a branch detail run.
    <code>&#123;path&#125;</code> stands for the checkout, and it is the
    <em>only</em> thing knobas fills in: nothing a source mirrored — a URL, a
    branch name, a title — can reach a program started from here. No shell is
    involved, so a path with a space in it is still one argument.
  </p>

  {#if commandFailure}
    <p class="fail">{commandFailure}</p>
    <button class="btn" onclick={() => void loadCommands()}>Retry</button>
  {/if}

  {#if commands}
    {#each commands as command (command.action)}
      <div class="field">
        <label class="lab" for="checkout-command-{command.action}">{command.label}</label>
        <input
          id="checkout-command-{command.action}"
          class="inp"
          bind:value={drafts[command.action]}
          disabled={savingCommand !== null}
          placeholder="not configured"
        />
        <div class="acts">
          <button
            class="btn"
            aria-label="Save {command.label}"
            disabled={savingCommand !== null ||
              drafts[command.action]?.trim() === (command.template ?? "")}
            onclick={() => void saveCommand(command.action, drafts[command.action] ?? "")}
          >
            Save
          </button>
          {#if !command.is_default}
            <button
              class="btn"
              aria-label="Reset {command.label}"
              disabled={savingCommand !== null}
              title="Forget this command and go back to what this platform starts with"
              onclick={() => void saveCommand(command.action, null)}
            >
              Reset
            </button>
          {/if}
        </div>
        {#if command.template === null}
          <p class="sub">Not configured — this button does nothing until a command is set.</p>
        {:else if command.is_default}
          <p class="sub">The default on this platform.</p>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .sec-b p {
    max-width: 78ch;
  }

  .sec-b p + p {
    margin-top: 8px;
  }

  .sub {
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  /*
    `field` and not `row`: the stylesheet's global `.row` is the list row --
    a five-column grid one line high with `overflow: hidden` on every child --
    and a form laid out in it clips its own label and its last button. Seen in
    the headless pass for this ticket.
  */
  .field {
    margin-top: 12px;
    display: grid;
    gap: 6px;
    max-width: 60ch;
  }

  .acts {
    display: flex;
    gap: 6px;
  }
</style>
