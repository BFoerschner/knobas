<!--
  The **Logs** section of the settings view (issue #747).

  One button: open the folder the app writes its rotating log to. Release
  builds have no console, so when something hangs this folder is the only
  evidence there is.

  No new IPC command: the folder comes from Tauri's path API (`appLogDir`, the
  same directory the Rust side writes to) and is opened with the opener plugin,
  whose `open-path` permission in `capabilities/default.json` is scoped to
  `$APPLOG` alone. Both are imported when the button is pressed, so a view that
  never shows this section never loads them.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";

  let {
    ports,
  }: {
    /** Injectable so a test needs no Tauri — `CaptureSection`'s shape. */
    ports?: Partial<{ openLogs: () => Promise<void> }>;
  } = $props();

  async function realOpenLogs() {
    const { appLogDir } = await import("@tauri-apps/api/path");
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(await appLogDir());
  }

  // svelte-ignore state_referenced_locally
  const openLogs = ports?.openLogs ?? realOpenLogs;

  let failure = $state<string | null>(null);

  async function show() {
    failure = null;
    try {
      await openLogs();
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Logs</span>
</div>

<div class="sec-b">
  <p>
    knobas keeps its log on disk: five files of up to 5 MB, newest first. Send
    them along when something hangs.
  </p>
  <div class="acts">
    <button class="btn" aria-label="Show logs" onclick={() => void show()}>Show logs</button>
  </div>
  {#if failure}
    <p class="fail">{failure}</p>
  {/if}
</div>

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .acts {
    margin-top: 8px;
    display: flex;
    gap: 6px;
  }

  .fail {
    margin-top: 8px;
    color: var(--fail);
  }
</style>
