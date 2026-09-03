<!--
  The **Standup protocols** section of the settings view (issue #289, spec
  #272 story 67).

  ## Why this exists at all

  The first *Publish* already asks where protocols go, and stores the answer.
  This is the other half of the same sentence — *"asked for the first time and
  changeable in settings, so that ENG › Standup protocols is a setting, not a
  constant"* — and it is the half a reader needs on the day the team moves the
  parent page. Without it the only way to change the target would be to find
  the setting in a database.

  It writes the **same key** the dialog writes, so the two cannot disagree
  about where the next page lands. A protocol already published is not moved:
  it has a page and a link, and this is about the next one, which the section
  says out loud rather than leaving to be discovered.

  ## What it shows when nothing is stored

  The stored target, or the sentence that there is none — never a guess at
  which Confluence is meant. `null` is the ordinary state before the first
  publish and reads as itself.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    setStandupPublishTarget as realWrite,
    standupPublishTarget as realRead,
    type PublishTarget,
  } from "../ipc/entity";
  import { listAdapters as realListAdapters, listSources as realListSources } from "../ipc/sources";
  import { publishableSources } from "../standup/protocol";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `PassiveSection`'s shape. */
    ports?: Partial<{
      standupPublishTarget: typeof realRead;
      setStandupPublishTarget: typeof realWrite;
      listSources: typeof realListSources;
      listAdapters: typeof realListAdapters;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = {
    standupPublishTarget: realRead,
    setStandupPublishTarget: realWrite,
    listSources: realListSources,
    listAdapters: realListAdapters,
    ...ports,
  };

  /** `undefined` until the first read answers — *unknown*, which is not *none*. */
  let stored = $state<PublishTarget | null | undefined>(undefined);
  let sources = $state<{ id: string; name: string }[]>([]);
  let failure = $state<string | null>(null);
  let saving = $state(false);

  /** What the fields hold while the reader is editing them. */
  let sourceId = $state("");
  let parent = $state("");

  $effect(() => {
    void load();
  });

  async function load() {
    try {
      const [target, configured, descriptors] = await Promise.all([
        io.standupPublishTarget(),
        io.listSources(),
        io.listAdapters(),
      ]);
      stored = target;
      sources = publishableSources(configured, descriptors);
      sourceId = target?.source_id ?? "";
      parent = target?.parent ?? "";
      failure = null;
    } catch (cause) {
      // Not "none": "nothing is stored" is a claim about the database, and a
      // section that could not ask has not earned it. The rule
      // `PassiveSection` records.
      failure = ipcErrorMessage(cause);
    }
  }

  async function save() {
    saving = true;
    try {
      stored = await io.setStandupPublishTarget({ source_id: sourceId, parent });
      failure = null;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Standup protocols</span>
</div>

<div class="sec-b">
  <p>
    Where <em>Publish to Confluence</em> puts the next protocol: which
    Confluence, and the page it goes under. The first publish asks for this and
    stores the answer here; changing it moves where the <em>next</em> page
    lands and leaves the ones already published where they are.
  </p>

  {#if failure}
    <p class="fail">{failure}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  {:else if stored !== undefined}
    <p class="sub">
      {#if stored === null}
        Nothing is stored yet — the first publish will ask.
      {:else}
        Currently {stored.parent} in {stored.source_id}.
      {/if}
    </p>

    <label class="fld">
      Confluence
      <select
        value={sourceId}
        disabled={saving}
        onchange={(event) => (sourceId = event.currentTarget.value)}
      >
        <option value="">Choose…</option>
        {#each sources as source (source.id)}
          <option value={source.id}>{source.name}</option>
        {/each}
      </select>
    </label>

    <label class="fld">
      Parent page
      <input
        type="text"
        placeholder="confluence:98400"
        value={parent}
        disabled={saving}
        oninput={(event) => (parent = event.currentTarget.value)}
      />
    </label>

    <button
      class="btn"
      disabled={saving || sourceId === "" || parent.trim() === ""}
      onclick={() => void save()}
    >
      Save
    </button>
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

  .sub {
    margin-top: 8px;
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  .fld {
    display: grid;
    gap: 4px;
    max-width: 42ch;
    margin-top: 10px;
    color: var(--muted);
  }
</style>
