<!--
  The boot screen.

  The M0 carry-over, discharged: bring-up used to block the event loop behind a
  hidden window, so a first run — a PostgreSQL download plus an `initdb` — was
  a minute of nothing on screen. The window now appears immediately and says
  what it is doing.

  No spinner. A spinner is the same picture for "downloading 60 MB" and "wedged
  on a port that is already taken", and the difference between those is the
  only thing worth showing here.
-->
<script lang="ts">
  import type { DbState } from "../ipc/app";

  let {
    db,
    version,
    demo,
    onretry,
  }: {
    db: DbState;
    version: string;
    demo: boolean;
    onretry: () => void;
  } = $props();

  /** The line under the heading, or `null` when there is nothing true to say. */
  const detail = $derived(db.state === "starting" ? db.detail : null);
</script>

<div class="boot">
  <div class="boot-b">
    {#if db.state === "failed"}
      <h1>knobas cannot start its database</h1>
      <!-- Text, always: the message is whatever PostgreSQL or the OS said. -->
      <p class="boot-mono boot-fail">{db.message}</p>
      <p>knobas {version} · profile {demo ? "demo" : "default"}</p>
      <div class="acts">
        <button class="tf-btn tf-primary" onclick={onretry}>Retry</button>
      </div>
    {:else if db.state === "migrating"}
      <h1>Bringing the schema up to date</h1>
      <p>This takes a moment the first time after an update.</p>
    {:else}
      <h1>Starting the local database</h1>
      {#if detail}
        <p>{detail}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  /*
    The design system's look, drawn outside the shell (#1402). This renders before
    `TfShell` mounts and is not wrapped in `.tf`: the app's tests take a `.tf`
    in the document to mean "the shell drew". The tokens are on `:root` and
    `.tf-btn` is unscoped, so neither needs the wrapper; what `.tf` would have
    given (its paper, its focus ring) is set here. The class names are the
    component's own: `unported.css` styles bare `.mono` and `.fail` globally.
  */
  .boot {
    display: flex;
    align-items: center;
    justify-content: center;
    /*
      Viewport units, not `100%`: this renders straight into `#app`, which has
      no height of its own, so a percentage would resolve against an
      auto-height box and collapse. `html, body { overflow: hidden }` means
      `100vh` is exactly the window.
    */
    height: 100vh;
    background: var(--paper);
    color: var(--ink);
  }

  .boot-b {
    max-width: 560px;
    padding: 0 24px;
  }

  /* The page title step. */
  .boot-b h1 {
    font-size: var(--t-3xl);
    line-height: 1.15;
    font-weight: 700;
    font-variation-settings: "wdth" 85;
  }

  /* Archivo, the row step, secondary ink: these are sentences, not code. */
  .boot-b p {
    margin-top: 10px;
    font-size: var(--t-sm);
    line-height: 1.45;
    color: var(--ink-2);
    /*
      The failure message is a raw error string with no line breaks of its own
      and often a long path in it. Wrapping it is what keeps it inside the
      window instead of running off the right-hand edge.
    */
    overflow-wrap: anywhere;
  }

  /* The one code value here: whatever PostgreSQL or the OS said. */
  .boot-mono {
    font-family: var(--mono);
    font-size: 0.9em;
  }

  .boot-fail {
    color: var(--err);
  }

  .acts {
    margin-top: 16px;
    display: flex;
    gap: var(--control-gap);
  }

  .acts :focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
    border-radius: 3px;
  }
</style>
