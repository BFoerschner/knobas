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

  ## The Tab action chain

  Two rows over the selected entity result, in this order, and nothing else
  (#1500; the 2026-10-21 ruling on #1416 in
  `docs/decisions/2026-10-unattended-rulings.md`):

  * **Link to ⟨key⟩**, only while the breadcrumb names a job (the job page, or
    a view opened from it: 'PAY-231 › Time') and the result is not that job's
    own ticket. Never on Tickets, Inbox or Settings. Choosing it closes the
    launcher and the shell opens the job's own link dialog with the result
    preset, so the relation is picked where every other link from the job is;
    the launcher never writes a link;
  * **Start timer on ⟨title⟩** (#278), on any result that can be a timer
    target. A stored context cannot (`shell/timer.ts`), so the row is absent
    there rather than offered and then refused by the backend.

  The rest of spec §4's old chain is retired by that ruling: Change status,
  Comment and Start work are the job page's, the "do it here" timer and
  comment rows are ⌘T's and the job page's, and *Add to context* went with
  contexts made by hand (CONTEXT.md, Context). There is no *Open* row: `Enter`
  opens. A saved smart list keeps its own two rows, Rename and Delete (#506).

  `Tab` still swallows the keystroke when the chain would be empty, because the
  alternative is `Tab` moving focus out of the box.

  The launcher acts on neither. It says *which* result the reader chose and
  lets the shell act, for the same reason it does not navigate: the job, the
  IPC and the toast are the shell's. For the timer that division carries a
  rule with it: *Start timer* on another entity **stops the running one
  first**, so the block the reader was in is closed rather than lost (story
  11), and the shell is where the stop, and #280's draft, belong.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import {
    createSmartList as defaultCreateSmartList,
    deleteSmartList as defaultDeleteSmartList,
    filterSuggestions as defaultFilterSuggestions,
    launcherHome as defaultHome,
    ipcErrorMessage,
    renameSmartList as defaultRenameSmartList,
    resolveUrl as defaultResolveUrl,
    search as defaultSearch,
  } from "../ipc";
  import type { CredentialHealth, SourceSyncStatus } from "../ipc/sources";
  import type { HealthPhase } from "../shell/health.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { entityAddress } from "../tf/address";
  import type { LinkTarget } from "../tf/job/current.svelte";
  import { canBeTarget } from "../shell/timer";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import Board from "./Board.svelte";
  import Chips from "./Chips.svelte";
  import Help from "./Help.svelte";
  import QueryBox from "./QueryBox.svelte";
  import Results from "./Results.svelte";
  import { type LauncherAction, filterActions, navigationActions } from "./actions";
  import { counted } from "../tf/counted";
  import type { InboxPreview, LauncherRow, TodayTime } from "./rows";
  import { opener } from "../tf/inbox/inbox";
  import { Session, type SessionPorts } from "./session.svelte";
  import { type Suggestion, activeToken, complete, suggest } from "./suggest";

  let {
    open = $bindable(false),
    sources,
    runs = [],
    sourcesRead,
    inbox,
    today,
    actions = [],
    onnavigate,
    onclose,
    openEntity,
    onlink,
    ontimer,
    ports,
    now,
    openExternal = realOpenExternal,
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
    /**
     * Each source's last run, from the shell's `sync:state` reading (#1581):
     * what the board's strip reads beside credential health.
     */
    runs?: SourceSyncStatus[];
    /**
     * How the shell's read of `sources` stands (#746). Omitted, the list is
     * taken as read. An empty `sources` is "none configured" only when `phase`
     * is `ready`; while it waits or has failed the board and the chips say
     * so, and `retry` is offered.
     */
    sourcesRead?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    /**
     * The inbox the board previews (#1418, spec §4): the live store's count
     * and its newest items, from the shell. Omitted or empty, the board draws
     * no inbox block.
     */
    inbox?: InboxPreview | undefined;
    /**
     * Today's tracked and to-log minutes, the Day view's figures, from the
     * shell (#1418). Omitted or nothing tracked, the board draws no time block.
     */
    today?: TodayTime | undefined;
    /** Extras appended to `>` — things only the shell knows it can offer. */
    actions?: LauncherAction[];
    onnavigate: (hash: string) => void;
    onclose: () => void;
    /**
     * The job the breadcrumb names, or `undefined` (#1500): the job page's,
     * or the job a view opened from it names ('PAY-231 › Time'). Never on
     * Tickets, Inbox or Settings, and never the newest open job.
     *
     * The presence of this is what puts *Link to <key>* in a result's `Tab`
     * chain: the one action about the pair (the result and the job). `label`
     * is what the row says, the job's key.
     */
    openEntity?: { entityId: string; label: string } | undefined;
    /**
     * Link the chosen result to that job. The launcher closes and the shell
     * opens the job's own link dialog with the result preset, where the
     * relation is picked and the write is the dialog's.
     */
    onlink?: ((target: LinkTarget) => void) | undefined;
    /**
     * Start the timer on the chosen result, **stopping whatever is running
     * first** (#278, story 11).
     *
     * The stop, the start, the acknowledgement and the failure message are all
     * the shell's, exactly as `onlink`'s write is. `undefined` — a caller with
     * no timer to offer — leaves the row out of the chain entirely, the same
     * distinction `onlink` draws.
     *
     * **Required, unlike `onlink`**, and for the reason `switcherContexts`
     * records after #238: this prop is the only join between the launcher's
     * *Start timer* row and the shell that acts on it, and an optional one
     * can be dropped from `App.svelte`, type-check clean, and remove the row
     * from the product without failing a test. `onlink` keeps its `?` because
     * it is already witnessed at the App seam; this one was not.
     */
    ontimer: ((targetId: string, targetTitle: string) => void) | undefined;
    /**
     * The IPC, injectable. Production passes nothing and gets the real
     * bridge; a test passes fakes and needs no `window.__TAURI_INTERNALS__`.
     *
     * **Partial**, since #496: a caller overrides the ports it cares about and
     * the rest stay the real ones. What that costs is a test that forgets a
     * port reaching the bridge, which throws where it is used; what it buys is
     * that a port added here is a *compile* error in the defaults below rather
     * than a silently missing capability, which is the failure `ontimer`
     * records above.
     */
    ports?: Partial<SessionPorts>;
    /** Injectable clock, so the age column is testable. */
    now?: Date | undefined;
    /**
     * How *Open in browser* on an unresolved paste reaches the OS. Injectable
     * for the same reason `AssetsView`'s is: the assertion is that the button
     * hands the **raw URL** to the shared opener, and a test cannot watch a
     * real one.
     */
    openExternal?: (url: string) => Promise<void>;
  } = $props();

  // svelte-ignore state_referenced_locally
  // The bridge is read once, on purpose: a `Session` owns a debounce timer and
  // a request counter, so swapping its ports mid-life would drop both. Nothing
  // changes this prop -- production omits it and tests pass fakes at mount.
  const session = new Session({
    search: defaultSearch,
    launcherHome: defaultHome,
    resolveUrl: defaultResolveUrl,
    createSmartList: defaultCreateSmartList,
    renameSmartList: defaultRenameSmartList,
    deleteSmartList: defaultDeleteSmartList,
    filterSuggestions: defaultFilterSuggestions,
    ...ports,
  });

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
  /** Whether the list above is an answer, for the claims drawn from it. */
  const sourcesPhase = $derived(sourcesRead?.phase ?? "ready");

  let box = $state<ReturnType<typeof QueryBox> | null>(null);

  /** One row of a result's `Tab` chain. */
  interface ChainAction {
    id: string;
    label: string;
    run: () => void;
  }

  /**
   * The open action chain, or `null`.
   *
   * `rowId` is remembered so the chain closes when the selection moves off the
   * row it belongs to: a chain acting on a row nobody is looking at any more
   * is a chain that acts on the wrong thing.
   */
  let chain = $state<{ rowId: string; actions: ChainAction[]; selected: number } | null>(null);

  /**
   * The saved list whose name is being edited on the board, or `null` (#506).
   *
   * Here rather than in `Board.svelte` because both ways of starting a rename
   * end here: the row's own *Rename* button, and the `Tab` chain, which is the
   * launcher's.
   */
  let renaming = $state<string | null>(null);

  /**
   * The value suggestions under the box (#601): the `kind:`, `source:` or
   * `under:` token being typed, and the values it can still become.
   *
   * `dismissedAt` is the box text `Esc` closed the list on. The list stays
   * closed until the text changes, and `Esc` is one rung of the ladder like
   * every other: it closes the list before it clears the box.
   */
  const typing = $derived(activeToken(session.raw));
  let dismissedAt = $state<string | null>(null);
  let suggestionPicked = $state(0);
  const offer = $derived<Suggestion[]>(
    typing && session.suggestions && session.raw !== dismissedAt
      ? suggest(typing, session.suggestions)
      : [],
  );

  // Read the lists the first time a suggestible token is being typed, and
  // not before: a reader who never types one never pays for the read. An
  // `under:` value is asked for as it is typed, because the engine answers
  // the parents by prefix (#761); the session asks each value once.
  $effect(() => {
    if (open && typing) void session.loadSuggestions(typing.key === "under" ? typing.partial : "");
  });

  // A new keystroke is a new list, so the cursor starts on its first value.
  $effect(() => {
    void session.raw;
    suggestionPicked = 0;
  });

  /** Complete the token being typed to `value`, and run the query. */
  function pickSuggestion(value: string) {
    if (!typing) return;
    session.set(complete(session.raw, typing, value));
    box?.focus();
  }

  /**
   * The keyboard while the list is open: the arrows walk it, `Enter` and
   * `Tab` pick, `Esc` closes it. `true` when the key was the list's.
   */
  function suggestionKey(event: KeyboardEvent): boolean {
    if (offer.length === 0) return false;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        suggestionPicked = Math.min(offer.length - 1, suggestionPicked + 1);
        return true;
      case "ArrowUp":
        event.preventDefault();
        suggestionPicked = Math.max(0, suggestionPicked - 1);
        return true;
      case "Enter":
      case "Tab": {
        event.preventDefault();
        const picked = offer[suggestionPicked];
        if (picked) pickSuggestion(picked.value);
        return true;
      }
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        dismissedAt = session.raw;
        return true;
      default:
        return false;
    }
  }

  /** The entity a row stands for, if it stands for one. */
  function entityOf(row: LauncherRow): { entityId: string; title: string; kind: string } | null {
    switch (row.kind) {
      case "hit":
        return { entityId: row.hit.entity_id, title: row.hit.title, kind: row.hit.kind };
      case "recent":
        return { entityId: row.row.entity_id, title: row.row.title, kind: row.row.kind };
      // A list, a syntax card and a navigation action are not entities, so
      // there is nothing to link. An inbox line is *about* an entity but is
      // not one (a credential expiry or an alert has none), and the day is a
      // view: both open, and neither has a chain.
      case "list":
      case "action":
      case "syntax":
      case "inbox":
      case "today":
        return null;
    }
  }

  /**
   * What can be done to `row` right now.
   *
   * Empty, and therefore no chain at all, when the row is not an entity and
   * nothing can be done to it. *Link to <key>* is left out on the job's own
   * ticket: an entity cannot be linked to itself, and the backend refuses it,
   * so offering the row would be offering an error.
   */
  function chainFor(row: LauncherRow): ChainAction[] {
    // A saved smart list is not an entity and has its own two things to do
    // (#506, story 58). They are in the chain because the chain is where every
    // per-row action in this box lives, and because a launcher driven from the
    // keyboard must be able to reach them without the pointer. A **built-in**
    // list has no chain at all: it is code, and there is nothing to rename.
    //
    // *Delete* here goes straight through, where the row's own button arms
    // first: reaching this row took `Tab`, an arrow and `Enter`, which is
    // already the deliberate act the button's second press stands in for.
    if (row.kind === "list") {
      if (!row.list.saved) return [];
      return [
        { id: "rename", label: `Rename ${row.list.label}`, run: () => startRename(row.list.id) },
        {
          id: "delete",
          label: `Delete ${row.list.label}`,
          run: () => {
            chain = null;
            void session.deleteList(row.list.id);
          },
        },
      ];
    }
    const entity = entityOf(row);
    if (!entity) return [];
    const actions: ChainAction[] = [];

    const open = openEntity;
    if (open && onlink && entity.entityId !== open.entityId) {
      const link = onlink;
      actions.push({
        id: "link",
        label: `Link to ${open.label}`,
        // The relation is picked in the job's dialog, which the shell opens
        // over the job, so the launcher steps out of its way.
        run: () => {
          chain = null;
          close();
          link({ entity_id: entity.entityId, title: entity.title, kind: entity.kind });
        },
      });
    }

    // Refused rather than absent: a context is a row in `knobas.entity` and
    // reaches this list like anything else, so the row has to be taken *out*
    // — see `shell/timer.ts`. Offering it would offer a row whose only
    // outcome is the backend's `invalid`.
    if (ontimer && canBeTarget({ entityId: entity.entityId })) {
      const start = ontimer;
      actions.push({
        id: "timer",
        label: `Start timer on ${entity.title}`,
        run: () => {
          start(entity.entityId, entity.title);
          chain = null;
        },
      });
    }
    return actions;
  }

  // The chain belongs to one row. Moving the cursor abandons it.
  $effect(() => {
    if (chain && session.current?.id !== chain.rowId) chain = null;
  });

  /**
   * The ids the options carry, so the box can name the active one (#1411,
   * #1665).
   *
   * By index in the flat list rather than by row id: an index is what
   * `session.selected` is, and a row id (`confluence:ENG:SEPA design`) is
   * not a valid element id.
   */
  const uid = $props.id();
  const optionId = (index: number) => `${uid}-opt-${index}`;
  const chainOptionId = (index: number) => `${uid}-chain-${index}`;
  const suggestionId = (index: number) => `${uid}-sug-${index}`;
  /** The three listboxes, which the box names as the one it controls (#1665). */
  const resultsId = `${uid}-results`;
  const chainId = `${uid}-actions`;
  const suggestionsId = `${uid}-suggestions`;

  /**
   * The result the box names as current, or `undefined` when no option
   * is drawn for it: no row, a failed read that draws its reason instead, or
   * the saved list whose name is a text field while it is being renamed.
   */
  const activeOption = $derived.by(() => {
    const row = session.current;
    if (!row || session.error) return undefined;
    if (row.kind === "list" && row.list.id === renaming) return undefined;
    return optionId(session.selected);
  });
  const activeChainOption = $derived(chain ? chainOptionId(chain.selected) : undefined);

  /**
   * The list the box controls and the option it names: whichever list the
   * arrows walk, in the order `onkeydown` asks -- the value suggestions while
   * they are open, then the Tab chain, then the results.
   */
  const controlled = $derived.by(() => {
    if (offer.length > 0) return { list: suggestionsId, option: suggestionId(suggestionPicked) };
    if (chain) return { list: chainId, option: activeChainOption };
    return { list: resultsId, option: activeOption };
  });

  /**
   * The selected row stays on screen (#1411, spec §4's ↑↓): whenever it
   * changes — an arrow, a hover, the reset to the first row that typing
   * makes — it is scrolled just far enough to be seen. `nearest` leaves a
   * row that is already visible where it is. `rows` is read so a new answer
   * under an unchanged index still brings its first row back into view.
   *
   * Runs after the DOM update, so the element is the one just drawn.
   */
  $effect(() => {
    void session.rows;
    if (activeOption) scrollToOption(activeOption);
  });
  $effect(() => {
    if (activeChainOption) scrollToOption(activeChainOption);
  });

  function scrollToOption(id: string) {
    // Guarded: jsdom has no `scrollIntoView`.
    document.getElementById(id)?.scrollIntoView?.({ block: "nearest" });
  }

  /** The shell's inbox and today's time, as rows of the board (#1418). */
  $effect.pre(() => {
    session.extras = { inbox: inbox ?? null, today: today ?? null };
  });

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
      // Selected, not cleared: the session keeps the last query on purpose,
      // and selecting it is what lets typing replace it (#1480).
      void tick().then(() => box?.focus({ select: true }));
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

  /**
   * Where a row goes. Every M1 row is an address (spec §2); an entity's is
   * the ticket-first shell's, through the one rule in `tf/address.ts` (#1085).
   */
  function addressOf(row: LauncherRow): string | null {
    switch (row.kind) {
      // A ticket whose key another source's row holds opens by its source (#1476).
      case "hit":
        return entityAddress(row.hit.kind, row.hit.entity_id, row.hit.title);
      case "recent":
        return entityAddress(row.row.kind, row.row.entity_id, row.row.title);
      case "action":
        return row.action.hash;
      // The Inbox page's own link for the line, and the Inbox page for a line
      // that has none to open.
      case "inbox":
        return opener(row.item)?.href ?? "#/inbox";
      // Time › Day, which is today when the address names no day.
      case "today":
        return "#/time";
      // A list and a syntax row do not navigate: they rewrite the box, which
      // is what makes them a *filter* rather than a destination.
      case "list":
      case "syntax":
        return null;
    }
  }

  /**
   * A pasted link that resolved goes straight to the entity (#496, spec #491
   * story 9: *"one paste away from its detail"*).
   *
   * An effect rather than a call inside the session, for the division this
   * component's header states: the session says *what the box holds*, and
   * navigating is the shell's, reached through `onnavigate` exactly as a
   * chosen row is. The launcher closes first so the address lands on a shell
   * with nothing over it.
   *
   * `session.dispose()` inside `close()` forgets the paste, which is what
   * stops this effect from firing a second time on the same answer.
   */
  $effect(() => {
    const found = session.urlAnswer?.match;
    if (!found) return;
    const hash = entityAddress(found.kind, found.entity_id);
    close();
    onnavigate(hash);
  });

  /**
   * *Open in browser* on a link the mirror does not hold — the other half of
   * story 10, so that a paste never dead-ends.
   *
   * The **raw** URL, the one the reader pasted, and not the normalised value
   * the resolver compared: normalisation exists to make two spellings match a
   * stored row, and handing a stripped fragment to the browser would open the
   * page somewhere other than where the link pointed.
   */
  async function openTheMiss(url: string) {
    try {
      await openExternal(url);
      close();
    } catch (cause) {
      session.error = ipcErrorMessage(cause);
    }
  }

  /** Open the row's name as a field, and close the chain that asked for it. */
  function startRename(id: string) {
    chain = null;
    renaming = id;
  }

  /**
   * End a rename. `label` is `null` for Escape and for the blur behind it,
   * which abandons: a half-typed name is not a name anybody chose.
   */
  function finishRename(id: string, label: string | null) {
    renaming = null;
    if (label !== null && label.trim() !== "") void session.renameList(id, label);
    box?.focus();
  }

  /**
   * *Save as list* (#506, story 56: *"a search I run every day is one click"*).
   *
   * The name is the query itself, because one click is the story and the
   * launcher has nowhere to ask for a name that is not a second dialog over
   * this one. *Rename* on the row is what turns it into a word — which is why
   * the two shipped together.
   */
  function saveAsList() {
    void session.saveCurrent(session.raw.trim());
  }

  function activate(row: LauncherRow) {
    if (row.kind === "list") {
      // A list whose saved query today's grammar cannot run has no rows to
      // show, and the reason is already on the row. Opening it would put the
      // backend's refusal in the error line as though the reader had done
      // something wrong.
      if (row.list.needs_attention) return;
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
    if (suggestionKey(event)) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        // While a chain is open the arrows walk *it*: it is the thing in front
        // of the reader, and moving the result selection underneath would pull
        // the chain out from under its own row.
        if (chain) {
          chain.selected = Math.min(chain.actions.length - 1, chain.selected + 1);
        } else {
          session.move(1);
        }
        return;
      case "ArrowUp":
        event.preventDefault();
        if (chain) {
          chain.selected = Math.max(0, chain.selected - 1);
        } else {
          session.move(-1);
        }
        return;
      case "Enter": {
        event.preventDefault();
        if (chain) {
          chain.actions[chain.selected]?.run();
          return;
        }
        // A miss has one thing to do and no row to do it from, so the key
        // that opens everything else opens it too -- otherwise a paste that
        // missed would be the one state in this box the keyboard cannot leave.
        const missed = session.mode === "url" && session.urlAnswer?.match === null;
        if (missed && session.url) {
          void openTheMiss(session.url);
          return;
        }
        const row = session.current;
        if (row) activate(row);
        return;
      }
      case "Tab": {
        // Swallowed either way, never forwarded: the default would move focus
        // out of the box.
        event.preventDefault();
        if (chain) {
          chain = null;
          return;
        }
        const row = session.current;
        if (!row) return;
        const actions = chainFor(row);
        // No chain rather than an empty one: a strip with nothing in it says
        // there is something here.
        if (actions.length === 0) return;
        chain = { rowId: row.id, actions, selected: 0 };
        return;
      }
      case "Escape":
        // One rung per press, and the key never reaches the shell's ladder.
        event.preventDefault();
        event.stopPropagation();
        if (chain) {
          chain = null;
        } else if (session.raw !== "") {
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
    `mirror · ${counted(session.home?.pending_writes ?? 0, "pending write", "pending writes")}`,
  );
  /** The chips render from the answer; before one lands there is nothing to
      say about a query nobody has interpreted yet. */
  const interpreted = $derived(session.response?.interpreted ?? null);
</script>

{#if open}
  <!--
    The ticket-first dialog shell (#1422): `TfDialog` draws the scrim, takes
    the click on it as the third way out beside `Esc` and the ×, keeps `Tab`
    inside, and registers the launcher on the shell's layer stack. Its title
    names the dialog for a screen reader; the prototype draws no header, so
    `tf/css/launcher.css` hides it from sight only. `TfDialog` leaves `Tab`
    alone here: the box takes it for the action chain and says so with
    `preventDefault`.
  -->
  <TfDialog title="Search and actions" className="tf-launch" onclose={close}>
    {#snippet body()}
      <QueryBox
        bind:this={box}
        value={session.raw}
        {placeholder}
        pending={session.pending}
        {footnote}
        controls={controlled.list}
        active={controlled.option}
        oninput={(value) => session.type(value)}
        {onkeydown}
        onclose={close}
      />

      {#if offer.length > 0}
        <!--
          Directly under the box, over the chips and the results: it completes
          the word being typed, and it is what the keyboard drives while open.
        -->
        <div id={suggestionsId} class="tf-ln-suggest" role="listbox" aria-label="Suggestions">
          {#each offer as suggestion, index (`${suggestion.value}\u0000${suggestion.detail}`)}
            <button
              id={suggestionId(index)}
              class="tf-ln-opt tf-ln-sug"
              class:tf-ln-on={index === suggestionPicked}
              role="option"
              aria-selected={index === suggestionPicked}
              onclick={() => pickSuggestion(suggestion.value)}
              onmouseenter={() => (suggestionPicked = index)}
            >
              <span class="tf-ln-nm">{suggestion.value}</span>
              <!-- The separator is an expression: Svelte trims an element's leading space (#1515). -->
              {#if suggestion.detail}<span class="tf-ln-s">{" · "}{suggestion.detail}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if interpreted}
        <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} />
      {/if}

      <div id={resultsId} class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Results">
        {#if session.error}
          <p class="tf-ln-none tf-ln-err">{session.error}</p>
          <!-- A board that failed to load has nothing else to offer; a failed
               search is retried by typing. -->
          {#if !session.home}
            <button class="tf-ln-opt" onclick={() => void session.loadHome()}>
              <span class="tf-ln-nm">Retry</span>
              <span class="tf-ln-s">Read the board again.</span>
            </button>
          {/if}
        {:else if session.mode === "url"}
          <!--
            A pasted link. A hit never draws: the effect above navigates on the
            answer, so what is left here is the wait and the miss.
          -->
          {#if !session.urlAnswer}
            <p class="tf-ln-none">Looking up the link…</p>
          {:else}
            <div class="tf-ln-miss">
              <!--
                The pasted URL missed, not necessarily the item (#1475): a
                source serves a page under more URLs than its adapter declares.
              -->
              <p class="tf-ln-none">Not in the mirror as this address; it may be stored under another form.</p>
              <p class="tf-ln-none tf-ln-raw">{session.url}</p>
              <button class="tf-ln-opt" onclick={() => session.url && void openTheMiss(session.url)}>
                <span class="tf-ln-nm">Open in browser</span>
                <span class="tf-ln-s">Open it where it lives.</span>
              </button>
            </div>
          {/if}
        {:else if session.mode === "board"}
          {#if session.home}
            <Board
              home={session.home}
              inboxCount={inbox?.count ?? 0}
              sources={health}
              {runs}
              reading={sourcesRead}
              rows={session.rows}
              selected={session.selected}
              {renaming}
              {optionId}
              {now}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
              onstartrename={startRename}
              onrename={finishRename}
              ondelete={(id) => void session.deleteList(id)}
            />
          {:else}
            <p class="tf-ln-none">Reading the mirror…</p>
          {/if}
        {:else if session.mode === "help"}
          <Help
            rows={session.rows}
            selected={session.selected}
            {optionId}
            onopen={activate}
            onhover={(index) => (session.selected = index)}
          />
        {:else if session.mode === "actions"}
          <div class="tf-ln-h"><span class="tf-ln-lab">Go to</span></div>
          {#each session.rows as row, i (row.id)}
            {#if row.kind === "action"}
              <button
                id={optionId(i)}
                class="tf-ln-opt"
                class:tf-ln-on={i === session.selected}
                role="option"
                aria-selected={i === session.selected}
                onclick={() => activate(row)}
                onmouseenter={() => (session.selected = i)}
              >
                <span class="tf-ln-nm">{row.action.label}</span>
                <span class="tf-ln-s">{row.action.detail}</span>
              </button>
            {/if}
          {/each}
          {#if session.rows.length === 0}
            <p class="tf-ln-none">No action matches.</p>
          {/if}
        {:else if session.response}
          <Results
            response={session.response}
            rows={session.rows}
            selected={session.selected}
            {optionId}
            sources={health}
            {now}
            onopen={activate}
            onhover={(index) => (session.selected = index)}
          />
        {:else}
          <p class="tf-ln-none">Searching…</p>
        {/if}
      </div>

      {#if chain}
        <div
          id={chainId}
          class="tf-ln-chain"
          role="listbox"
          tabindex="-1"
          aria-label="Actions on the selected result"
        >
          {#each chain.actions as action, index (action.id)}
            <button
              id={chainOptionId(index)}
              class="tf-ln-opt tf-ln-chain-a"
              class:tf-ln-on={index === chain.selected}
              role="option"
              aria-selected={index === chain.selected}
              onclick={() => action.run()}
              onmouseenter={() => chain && (chain.selected = index)}
            >
              <span class="tf-ln-nm">{action.label}</span>
            </button>
          {/each}
        </div>
      {/if}
    {/snippet}

    {#snippet footer()}
      <span><kbd>↑↓</kbd> move</span>
      <span><kbd>↵</kbd> open</span>
      <span title="Actions on the selected result"><kbd>Tab</kbd> actions</span>
      <span><kbd>Esc</kbd> back one step</span>
      {#if session.canSave}
        <!--
          On a query and never on the board: an empty box is the board, and
          there is nothing there to save. `canSave` is the session's, so the
          control and the write agree about when there is a query.
        -->
        <button class="tf-btn tf-sm tf-ln-save" onclick={saveAsList} disabled={session.savedId !== null}>
          {session.savedId === null ? "Save as list" : "Saved as list"}
        </button>
      {/if}
      <span class="tf-spacer"></span>
      {#if session.response}
        <span class="tf-ln-took">{session.response.total} matches · {session.response.took_ms} ms</span>
      {/if}
    {/snippet}
  </TfDialog>
{/if}
