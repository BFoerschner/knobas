<!--
  The **Clones root** section of the settings view (issue #499, spec #491
  story 25).

  One directory: where knobas looks for the clones on this machine. It scans two
  levels under it when a repo or a branch detail opens and matches each clone's
  `origin` to the repo by host and owner/repo, so `~/src/payout-service` and
  `~/src/gitea.example.com/payout-service` are both reached and nothing deeper
  is walked.

  **knobas never writes to a working tree** (ADR-0016): no clone, no checkout,
  no fetch. The section says so, because a directory setting handed to an app
  is exactly the moment a person wants to know what it will do with it.

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
    clonesRoot as realRead,
    setClonesRoot as realWrite,
  } from "../ipc/entity";
  import { latestRead } from "../shell/latest-read";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `PassiveSection`'s shape. */
    ports?: Partial<{
      clonesRoot: typeof realRead;
      setClonesRoot: typeof realWrite;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = { clonesRoot: realRead, setClonesRoot: realWrite, ...ports };

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
    <div class="row">
      <label class="lab" for="clones-root">Directory</label>
      <input
        id="clones-root"
        class="inp"
        bind:value={draft}
        disabled={saving}
        placeholder="/Users/you/src"
      />
      <div class="acts">
        <button class="btn" disabled={saving || draft.trim() === (stored ?? "")} onclick={() => void save(draft)}>
          Save
        </button>
        {#if stored !== null}
          <button class="btn" disabled={saving} onclick={() => void save(null)}>Clear</button>
        {/if}
      </div>
      {#if stored === null}
        <p class="sub">Not set — no checkout is looked for until it is.</p>
      {/if}
    </div>
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

  .row {
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
