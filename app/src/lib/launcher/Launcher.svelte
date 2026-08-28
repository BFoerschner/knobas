<!--
  The ⌘K overlay (spec §4, round-3 mockup `openSearch()`).

  **Mounting it is the whole integration.** It registers its own hotkey, owns
  its own `Esc` ladder and asks the backend itself; a shell mounts it, binds
  `open`, and answers `onnavigate`. That is deliberate: a launcher whose
  keyboard lives in the shell is a launcher whose behaviour is split across two
  owners, and this one is stream E's while the shell is stream D's.

  ## The Esc ladder

  Spec §4's rung, unwound one step at a time: a non-empty query is cleared
  first, and only an already-empty box closes the overlay. Both rungs
  `stopPropagation`, so the shell's own ladder (detail slide-over → room) never
  sees the key while the launcher is up — the same rule `shell/Modal.svelte`
  follows, and the reason it is a rule is that two handlers unwinding two
  ladders on one keystroke is indistinguishable from a bug.

  ## What is reserved rather than bound

  `Tab` is M2's action chain (§4: *Change status › Comment › Link to…*). It is
  swallowed here rather than left alone, because the alternative is `Tab`
  moving focus out of the box — training a habit the app will have to break.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import { launcherHome as defaultHome, search as defaultSearch } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";
  import { hashFor } from "../shell/router.svelte";
  import Board from "./Board.svelte";
  import Chips from "./Chips.svelte";
  import Help from "./Help.svelte";
  import QueryBox from "./QueryBox.svelte";
  import Results from "./Results.svelte";
  import { type LauncherAction, filterActions, navigationActions } from "./actions";
  import type { LauncherRow } from "./rows";
  import { Session, type SessionPorts } from "./session.svelte";

  let {
    open = $bindable(false),
    sources,
    actions = [],
    onnavigate,
    onclose,
    ports,
    now,
  }: {
    open?: boolean;
    /**
     * Credential health from a shell that subscribes to `source:health`.
     *
     * `undefined` means *unsupplied* and falls back to the board's own copy;
     * an empty array means *supplied, and nothing to complain about*. The two
     * are different answers and the launcher must not collapse them — see
     * `health` below.
     */
    sources?: CredentialHealth[] | undefined;
    /** Extras appended to `>` — things only the shell knows it can offer. */
    actions?: LauncherAction[];
    onnavigate: (hash: string) => void;
    onclose: () => void;
    /**
     * The IPC, injectable. Production passes nothing and gets the real
     * bridge; a test passes fakes and needs no `window.__TAURI_INTERNALS__`.
     */
    ports?: SessionPorts;
    /** Injectable clock, so the age column is testable. */
    now?: Date | undefined;
  } = $props();

  // svelte-ignore state_referenced_locally
  // The bridge is read once, on purpose: a `Session` owns a debounce timer and
  // a request counter, so swapping its ports mid-life would drop both. Nothing
  // changes this prop -- production omits it and tests pass fakes at mount.
  const session = new Session(ports ?? { search: defaultSearch, launcherHome: defaultHome });

  /**
   * The credential health the chips and the result rows are drawn from.
   *
   * **Two rounds of review live in this one line, so both are recorded.**
   *
   * Round 1 (#27): the prop alone was not enough. Nothing in the shell polled
   * credential health, so on the one production mount (`App.svelte`) it was
   * `[]` unconditionally — `Row.svelte` always drew the sync age, `Chips.svelte`
   * never drew the failure dot, and the component test was green about a path
   * production did not take. The fix was a fallback to `LauncherHome.sources`,
   * which the launcher already fetches on every opening.
   *
   * Round 2 (issue #36): that fallback was written `sources.length > 0`, which
   * reads a **supplied but empty** list as *unsupplied*. Correct while nothing
   * supplied the prop; wrong the moment `App.svelte` passes the live
   * `source:health` store, because a store that has just been told the only
   * source is fine is supplied and empty — and falling back to a per-opening
   * board fetch would redraw a 401 the backend has already retracted. Worse for
   * a shell that polls a *subset*: the fallback would silently swap the subset
   * for a different list.
   *
   * So the distinction is carried by `undefined` versus `[]`, which is what
   * those two values already mean everywhere else. A supplied list always wins,
   * because a live subscription is fresher than a per-opening fetch.
   */
  const health = $derived(sources ?? session.home?.sources ?? []);

  let box = $state<ReturnType<typeof QueryBox> | null>(null);

  /**
   * The `>` palette: navigation plus whatever the shell appended, narrowed by
   * what has been typed after the marker.
   *
   * Narrowed against `interpreted.text` — the backend's parse with the `>`
   * already stripped — rather than against the raw box, which still has the
   * marker in it (ruling P2 again: the frontend does not strip grammar).
   */
  $effect(() => {
    const typed = session.response?.interpreted.text ?? "";
    session.actions = filterActions([...navigationActions(), ...actions], typed);
  });

  /** The board is fetched once per opening, not once per mount. */
  $effect(() => {
    if (open) {
      void session.loadHome();
      void tick().then(() => box?.focus());
    } else {
      session.dispose();
    }
  });

  onMount(() => {
    function onkeydown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        // Opens, never toggles, and that is the difference between converging
        // and fighting. The shell binds ⌘K too (`shell/keys.ts`, and its top
        // strip's search field calls the same handler), so on one keystroke
        // two listeners run: two idempotent "open" writes land on open, while
        // an open and a toggle land on whichever ran last. `Esc` and the × are
        // the ways out, which is also what the round-3 mockup did.
        open = true;
      }
    }
    window.addEventListener("keydown", onkeydown);
    return () => {
      window.removeEventListener("keydown", onkeydown);
      session.dispose();
    };
  });

  function close() {
    open = false;
    session.dispose();
    onclose();
  }

  /** Where a row goes. Every M1 row is an address (spec §2). */
  function addressOf(row: LauncherRow): string | null {
    switch (row.kind) {
      case "hit":
        return hashFor({
          view: "room",
          ctx: "all",
          detail: { kind: row.hit.kind, entityId: row.hit.entity_id },
        });
      case "recent":
        return hashFor({
          view: "room",
          ctx: "all",
          detail: { kind: row.row.kind, entityId: row.row.entity_id },
        });
      case "action":
        return row.action.hash;
      // A list and a syntax row do not navigate: they rewrite the box, which
      // is what makes them a *filter* rather than a destination.
      case "list":
      case "syntax":
        return null;
    }
  }

  function activate(row: LauncherRow) {
    if (row.kind === "list") {
      session.set(`list:${row.list.id}`);
      box?.focus();
      return;
    }
    if (row.kind === "syntax") {
      session.set(row.entry.insert);
      box?.focus();
      return;
    }
    const hash = addressOf(row);
    if (!hash) return;
    close();
    onnavigate(hash);
  }

  function onkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        session.move(1);
        return;
      case "ArrowUp":
        event.preventDefault();
        session.move(-1);
        return;
      case "Enter": {
        event.preventDefault();
        const row = session.current;
        if (row) activate(row);
        return;
      }
      case "Tab":
        // Reserved for M2's action chain. Swallowed, never forwarded: the
        // default would move focus out of the box.
        event.preventDefault();
        return;
      case "Escape":
        // One rung per press, and the key never reaches the shell's ladder.
        event.preventDefault();
        event.stopPropagation();
        if (session.raw !== "") {
          session.set("");
        } else {
          close();
        }
        return;
      default:
    }
  }

  const placeholder =
    "Search everything, or type > for actions, # tickets, @ people, / source, ? help";
  const footnote = $derived(
    `local index · ${session.home?.pending_writes ?? 0} pending writes`,
  );
  /** The chips render from the answer; before one lands there is nothing to
      say about a query nobody has interpreted yet. */
  const interpreted = $derived(session.response?.interpreted ?? null);
</script>

{#if open}
  <!--
    The scrim is a click target, not a control: `Esc` and the × are the
    keyboard and pointer routes out, and this is the third. `role="presentation"`
    plus the keyboard handler on the dialog is what keeps a11y honest about
    that.
  -->
  <div
    class="scrim"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div class="dlg search" role="dialog" aria-modal="true" aria-label="Search and actions">
      <QueryBox
        bind:this={box}
        value={session.raw}
        {placeholder}
        pending={session.pending}
        {footnote}
        oninput={(value) => session.type(value)}
        {onkeydown}
        onclose={close}
      />

      {#if interpreted}
        <Chips {interpreted} sources={health} />
      {/if}

      <div class="rlist" role="listbox" tabindex="-1" aria-label="Results">
        {#if session.error}
          <p class="none err">{session.error}</p>
        {:else if session.mode === "board"}
          {#if session.home}
            <Board
              home={session.home}
              sources={health}
              rows={session.rows}
              selected={session.selected}
              {now}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
            />
          {:else}
            <p class="none">Reading the local index…</p>
          {/if}
        {:else if session.mode === "help"}
          <Help
            rows={session.rows}
            selected={session.selected}
            onopen={activate}
            onhover={(index) => (session.selected = index)}
          />
        {:else if session.mode === "actions"}
          <div class="secl"><span class="lab">Go to</span></div>
          {#each session.rows as row, i (row.id)}
            {#if row.kind === "action"}
              <button
                class="pfx"
                class:on={i === session.selected}
                role="option"
                aria-selected={i === session.selected}
                onclick={() => activate(row)}
                onmouseenter={() => (session.selected = i)}
              >
                <span class="nm">{row.action.label}</span>
                <span class="s">{row.action.detail}</span>
              </button>
            {/if}
          {/each}
          {#if session.rows.length === 0}
            <p class="none">No action matches.</p>
          {/if}
        {:else if session.response}
          <Results
            response={session.response}
            rows={session.rows}
            selected={session.selected}
            sources={health}
            {now}
            onopen={activate}
            onhover={(index) => (session.selected = index)}
          />
        {:else}
          <p class="none">Searching…</p>
        {/if}
      </div>

      <div class="search-f">
        <span><kbd>↑↓</kbd> move</span>
        <span><kbd>↵</kbd> open</span>
        <span title="Actions on the selected result arrive in M2"><kbd>Tab</kbd> actions (M2)</span>
        <span><kbd>Esc</kbd> back one step</span>
        <span class="sp"></span>
        {#if session.response}
          <span class="faint">{session.response.total} matches · {session.response.took_ms} ms</span>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /*
    `.search` — the launcher box (round 3, lines 363-377). `.scrim`, `.dlg` and
    `kbd` are the shell's and live in `app.css`; everything the launcher adds
    is here, which is what the stylesheet's header says should happen.
  */
  .search {
    width: 960px;
    max-width: 94vw;
    height: 588px;
    max-height: 84vh;
  }
  .rlist {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  .search-f {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 26px;
    padding: 0 12px;
    border-top: 1px solid var(--hair);
    background: var(--panel);
    font: 400 11px var(--mono);
    color: var(--faint);
    flex: none;
    overflow: hidden;
    white-space: nowrap;
  }
  .search-f .sp {
    flex: 1;
  }
  .secl {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid var(--hair);
    background: var(--panel);
  }
  .pfx {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 8px;
    align-items: baseline;
    width: 100%;
    text-align: left;
    padding: 7px 12px;
    font-size: 12px;
    color: var(--muted);
    border-bottom: 1px solid var(--hair);
  }
  .pfx:hover,
  .pfx.on {
    background: var(--raised);
    color: var(--text);
  }
  .pfx.on {
    box-shadow: inset 2px 0 0 var(--amber);
  }
  .pfx .nm {
    color: var(--text);
  }
  .none {
    padding: 14px 12px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.5;
  }
  .none.err {
    color: var(--fail);
  }
</style>
