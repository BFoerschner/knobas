<!--
  The boot screen.

  The M0 carry-over, discharged: bring-up used to block the event loop behind a
  hidden window, so a first run — a PostgreSQL download plus an `initdb` — was
  a minute of nothing on screen. The window now appears immediately and says
  what it is doing.

  No spinner. A spinner is the same picture for "downloading 60 MB" and "wedged
  on a port that is already taken", and the difference between those is the
  only thing worth showing here.

  When the database cannot start, one line under *Retry* offers a newer knobas (ADR-0052, decisions D14): the updater
  needs no database, so a pre-release build whose migration fails can still be replaced. Its room is kept whether or not
  there is an offer. *Download and restart* downloads, checks and installs in one go, saying each step in the line.
-->
<script lang="ts">
  import type { DbState } from "../ipc/app";
  import type { Updates } from "../tf/update.svelte";
  import { bootLine, bootStep } from "../tf/settings/updates";

  let {
    db,
    version,
    demo,
    onretry,
    update = null,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: {
    db: DbState;
    version: string;
    demo: boolean;
    onretry: () => void;
    /** The updater, for the line that offers a newer knobas when the database cannot start. */
    update?: Pick<Updates, "state" | "check" | "download" | "install"> | null;
    /** Puts text on the clipboard; a test's own. */
    copy?: (text: string) => Promise<void>;
  } = $props();

  const line = $derived(bootLine(update?.state ?? null));
  /** Pressed *Download and restart*: the download's end installs at once. */
  let going = $state(false);
  const step = $derived(going && update?.state ? bootStep(update.state.phase) : null);

  $effect(() => {
    if (going && update?.state?.phase.phase === "ready") void update.install();
  });

  async function press() {
    if (update === null || line === null) return;
    if (line.does === "check") return void update.check();
    going = true;
    if (update.state?.phase.phase === "ready") await update.install();
    else await update.download();
  }

  /** The line under the heading, or `null` when there is nothing true to say. */
  const detail = $derived(db.state === "starting" ? db.detail : null);

  /**
   * What *Copy details* copies (ux28): the backend's report of the failed
   * start, or the message alone when there is none (a server that died after
   * `ready`), with the version line a report needs either way.
   */
  const details = $derived(
    db.state === "failed"
      ? (db.details ?? `${db.message}\nknobas ${version} · profile ${demo ? "demo" : "default"}`)
      : "",
  );

  /** The last lines the server wrote on a failed start (ux28's Windows pass): the screen keeps to the last six, *Copy details* has them all. */
  const said = $derived(db.state === "failed" ? (db.said ?? []).slice(-6) : []);

  /** The button's own word for what the last press did; it says *Copy details* again after a moment. */
  let copied = $state<"idle" | "copied" | "failed">("idle");
  let settle: ReturnType<typeof setTimeout> | undefined;

  async function copyDetails() {
    clearTimeout(settle);
    try {
      await copy(details);
      copied = "copied";
    } catch {
      copied = "failed";
    }
    settle = setTimeout(() => (copied = "idle"), 2000);
  }

  $effect(() => () => clearTimeout(settle));
</script>

<div class="boot">
  <div class="boot-b">
    {#if db.state === "failed"}
      <h1>knobas cannot start its database</h1>
      {#if db.blocked}
        <!-- ux28: the operating system refused knobas its own server. The cause first, in words; then what was tried and what an administrator can allow, as paths to copy. -->
        <p class="boot-cause">{db.blocked.cause}</p>
        {#if db.blocked.tried.length > 0}
          <p>knobas tried</p>
          <ul class="boot-paths boot-mono">
            {#each db.blocked.tried as route (route)}<li>{route}</li>{/each}
          </ul>
        {/if}
        {#if db.blocked.allow.length > 0}
          <p>An administrator can allow these programs:</p>
          <ul class="boot-paths boot-mono">
            {#each db.blocked.allow as program (program)}<li>{program}</li>{/each}
          </ul>
        {/if}
      {/if}
      <!-- Text, always: the message is whatever PostgreSQL or the OS said. -->
      <p class="boot-mono boot-fail">{db.message}</p>
      {#if said.length > 0}
        <!-- The Windows pass on ux28: the server's own last lines, where the cause sits before its FATAL summary. -->
        <p>PostgreSQL said</p>
        <ul class="boot-paths boot-mono" data-testid="boot-said">
          {#each said as line, n (n)}<li>{line}</li>{/each}
        </ul>
      {/if}
      <p>knobas {version} · profile {demo ? "demo" : "default"}</p>
      <div class="acts">
        <button class="tf-btn tf-primary" onclick={onretry}>Retry</button>
        <button class="tf-btn" onclick={copyDetails}
          >{copied === "copied" ? "Copied" : copied === "failed" ? "Could not copy" : "Copy details"}</button
        >
      </div>
      <div class="boot-upd" data-testid="boot-update">
        {#if line !== null}
          <span>{step ?? line.text}</span>
          <button class="tf-btn" disabled={going && step !== null && !step.startsWith("Not installed")} onclick={() => void press()}>{line.button}</button>
        {/if}
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
    font-size: calc(var(--t-3xl) * var(--fs));
    line-height: 1.15;
    font-weight: 600;
    --fs: var(--size-head); letter-spacing: var(--ls-head); font-family: var(--head); font-variation-settings: "wdth" 85; font-stretch: 85%;
  }

  /* The interface face, the row step, secondary ink: these are sentences, not code. */
  .boot-b p {
    margin-top: 10px;
    font-size: calc(var(--t-sm) * var(--fs));
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
    --fs: var(--size-code); font-family: var(--mono);
    font-size: calc(var(--t-xs) * var(--fs));
    font-variant-ligatures: var(--mono-ligatures);
  }

  .boot-fail {
    color: var(--err);
  }

  /* The cause is the sentence the reader came for: the body's ink, not the secondary one. */
  .boot-b .boot-cause {
    color: var(--ink);
  }

  /* Paths and addresses, one to a line, selectable for an administrator. */
  .boot-paths {
    margin-top: 4px;
    padding-left: 0;
    list-style: none;
    color: var(--ink);
    overflow-wrap: anywhere;
    user-select: text;
  }

  .boot-paths li + li {
    margin-top: 2px;
  }

  .acts {
    margin-top: 16px;
    display: flex;
    gap: var(--control-gap);
  }

  /* The update's line: its room is kept, offer or none, so Retry never moves (ADR-0052). */
  .boot-upd {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--rule);
    display: flex;
    gap: 12px;
    align-items: center;
    min-height: 45px;
  }

  .boot-upd span {
    flex: 1;
    font-size: calc(var(--t-sm) * var(--fs));
    color: var(--ink);
  }

  .acts :focus-visible, .boot-upd :focus-visible {
    outline: var(--focus-w) solid var(--focus);
    outline-offset: var(--focus-off);
  }
</style>
