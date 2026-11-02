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
  sees the key while the launcher is up — the same rule `tf/settings/TfDialog.svelte`
  follows, and the reason it is a rule is that two handlers unwinding two
  ladders on one keystroke is indistinguishable from a bug.

  ## The Tab sheet (#2426)

  Tab on a row lifts it under the box (`Pinned.svelte`), and the box, labelled
  *Actions*, empties and narrows the row's actions: every typed letter in
  order, the tightest match first. Esc, Tab, or Backspace on the empty box
  leave it with the search words back as they were: typing in the sheet goes
  to its own `filter`, never to `session.raw`. ⌘K while open only gives the
  box its focus back.

  The actions are a table, per kind, in `sheet.ts`; each is an address of the
  app or a write it has today, carried out here by `perform`. *Link to ⟨job⟩*
  comes first on every entity, with the relation picked inline by ← and → and
  written on Enter through the job's own mapping (`drawn`), against the job
  the shell names (`openEntity`: the ticket on screen, else the chosen job),
  from any page. This reverses the 2026-10-21 ruling on #1416, which trimmed
  the chain to two rows and handed the link to the job's dialog
  (`docs/decisions/2026-11-02-launcher-sheet.md`).

  *Start timer* is a ticket's row alone, and the shell does the rest: it
  stops the running timer first and makes the ticket the chosen job.

  ## Flows (#2427)

  An action that asks something is a flow (`does.kind === "flow"`): the
  sheet's `flow` holds its steps, the picks so far and the current step's
  choices once read. The box, labelled with the step's title, narrows them;
  Enter picks and the next step is read; the last pick runs what the flow's
  `finish` makes of the picks, through `carry` like any other action. Esc or
  Backspace on the empty box step back one step, and from the first back to
  the actions; the row stays pinned throughout.

-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

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
  import type { CredentialHealth } from "../ipc/sources";
  import type { HealthPhase } from "../shell/health.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { entityAddress } from "../tf/address";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { docsSearchHash } from "../tf/docs/docs";
  import Board from "./Board.svelte";
  import Chips from "./Chips.svelte";
  import Help from "./Help.svelte";
  import Preview, { PREVIEW_DEBOUNCE_MS } from "./Preview.svelte";
  import QueryBox from "./QueryBox.svelte";
  import Pinned from "./Pinned.svelte";
  import Results from "./Results.svelte";
  import {
    type Choice,
    type SheetAction,
    type SheetEffect,
    type SheetSubject,
    type Step,
    nameOf,
    narrowActions,
    sheetActions,
    subjectOf,
  } from "./sheet";
  import type { LinkSetRow, ViewAction } from "../ipc/entity";
  import { type SheetPorts, realSheetPorts } from "./sheet-ports";
  import { drawn } from "../detail/relations";
  import { JOB_LINK_RELATIONS } from "../tf/job/link-relations";
  import { linkFailureMessage } from "../detail/links";
  import { argvText, menuOrder } from "../tf/openin/openin";
  import { type LauncherAction, filterActions, navigationActions } from "./actions";
  import type { InboxPreview, LauncherRow } from "./rows";
  import { opener } from "../tf/inbox/inbox";
  import { Session, type SessionPorts } from "./session.svelte";
  import { type ScopeChoice, launcherScope } from "./scope.svelte";
  import { type Suggestion, activeToken, complete, suggest } from "./suggest";
  import { addFilter, narrowFilters, overlayFilters } from "./syntax";
  import { push } from "../shell/toasts.svelte";

  let {
    open = $bindable(false),
    query = $bindable(""),
    sources,
    sourcesRead,
    inbox,
    now,
    actions = [],
    onnavigate,
    onclose,
    openEntity,
    ontimer,
    ports,
    sheetPorts,
    openExternal = realOpenExternal,
    scope = launcherScope,
  }: {
    open?: boolean;
    /**
     * What the box holds when it opens (#2425): a section's filter (`#b `)
     * when the shell opened it with `/`, ⌘/ or the top bar's button, and
     * nothing for ⌘K, which writes `""` here itself before it opens. Read once
     * per opening; typing never writes it back.
     */
    query?: string;
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
     * How the shell's read of `sources` stands (#746). Omitted, the list is
     * taken as read. An empty `sources` is "none configured" only when `phase`
     * is `ready`; while it waits or has failed the chips say so.
     */
    sourcesRead?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    /**
     * The shell's live inbox (#1418): its count and items, newest first. The
     * empty box does not draw it (#2421); `#i` answers from it (#2423).
     */
    inbox?: (InboxPreview & { actions?: Readonly<Record<string, readonly string[]>> }) | undefined;
    /** The clock an inbox line's time cell is read against; the real one when omitted. */
    now?: Date | undefined;
    /** Extras appended to `>` — things only the shell knows it can offer. */
    actions?: LauncherAction[];
    onnavigate: (hash: string) => void;
    onclose: () => void;
    /**
     * The job the shell names (#1500, spec #2227): the ticket on screen on a
     * job or ticket page, else the chosen job; `undefined` when none is
     * chosen. The sheet's *Link to ⟨label⟩* writes against it (#2426).
     */
    openEntity?: { entityId: string; label: string } | undefined;
    /**
     * Start the timer on the chosen result, **stopping whatever is running
     * first** (#278, story 11).
     *
     * The stop, the start, the acknowledgement and the failure message are all
     * the shell's. `undefined` — a caller with no timer to offer — leaves the
     * row out of the sheet entirely.
     *
     * **Required, unlike `onlink`**, and for the reason `switcherContexts`
     * records after #238: this prop is the only join between the launcher's
     * *Start timer* row and the shell that acts on it, and an optional one
     * can be dropped from `App.svelte`, type-check clean, and remove the row
     * from the product without failing a test.
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
    /** The sheet's writes and reads (#2426), injectable as `ports` is. */
    sheetPorts?: Partial<SheetPorts>;
    /**
     * *Titles* or *Everything* (#2430): the app's one choice, kept across
     * closing and a restart. Injectable so a test can keep it in a storage of
     * its own and read it back as a restart would.
     */
    scope?: ScopeChoice;
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
    titles: () => scope.titles,
  });

  /**
   * ⌘D or the chip: Titles or Everything, and the words in the box asked
   * again at once under the other mode. An empty box is the board, which the
   * choice does not change, so nothing is sent for it.
   */
  function toggleScope() {
    scope.toggle();
    if (session.raw.trim() !== "") session.rerun();
  }

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

  // svelte-ignore state_referenced_locally
  // Read once, as the session's ports are.
  const io: SheetPorts = { ...realSheetPorts, ...sheetPorts };

  /**
   * The open sheet, or `null` (#2426). `row` is the row it lifted, `filter`
   * what the box holds while it is open (the search words wait in
   * `session.raw`), `relation` the index into `JOB_LINK_RELATIONS` the link
   * row's chips stand on, `renaming` a saved list's new name while the
   * box asks for it, and `flow` the flow an action started (#2427).
   */
  let sheet = $state<{
    rowId: string;
    row: LauncherRow;
    subject: SheetSubject;
    filter: string;
    selected: number;
    relation: number;
    renaming: string | null;
    flow: Flow | null;
  } | null>(null);

  /**
   * A flow in the sheet: `id` tells one flow's late answer from another's,
   * `picks` are the steps answered, `asked` the choices each of them offered
   * (so a step back draws them again without a read), and `choices` the
   * current step's once read, `null` while reading.
   */
  interface Flow {
    id: number;
    steps: Step[];
    finish: (picks: readonly string[]) => SheetEffect;
    picks: string[];
    asked: Choice[][];
    choices: Choice[] | null;
  }

  /**
   * Settings › Checkouts, the default first: *Open in ⟨app⟩*'s rows. Read the
   * first time a sheet opens in an opening, so a search that acts on nothing
   * costs no read; the rows join the sheet when the answer lands.
   */
  let applications = $state<{ id: string; name: string }[]>([]);
  let applicationsRead = false;
  function readApplications() {
    if (applicationsRead) return;
    applicationsRead = true;
    readLinkSets();
    void io.checkoutSettings().then(
      (settings) => (applications = menuOrder(settings).map(({ app }) => ({ id: app.id, name: app.name }))),
      () => (applications = []),
    );
  }

  /** Every link set: *Add to a set…*'s choices, read with the applications. */
  let linkSets = $state<LinkSetRow[]>([]);
  function readLinkSets() {
    void io.linkSets().then(
      (sets) => (linkSets = sets),
      () => (linkSets = []),
    );
  }

  /** The declared merge of the pull request the sheet opened on, read once per sheet: *Merge…* follows it. */
  let merge = $state<{ prId: string; action: ViewAction | null } | null>(null);
  function readMerge(subject: SheetSubject) {
    if (subject.kind !== "entity" || subject.entityKind !== "pr") return;
    const prId = subject.entityId;
    if (merge?.prId === prId) return;
    merge = { prId, action: null };
    void io.mergeAction(prId).then(
      (action) => {
        if (merge?.prId === prId) merge = { prId, action };
      },
      () => {},
    );
  }

  const sheetContext = $derived({
    job: openEntity ?? null,
    canTime: ontimer !== undefined,
    apps: applications,
    sets: linkSets,
    merge: sheet?.subject.kind === "entity" && merge?.prId === sheet.subject.entityId ? merge.action : null,
    branches: io.branches,
    now: now ?? new Date(),
  });
  const inboxOps = (key: string) => inbox?.actions?.[key] ?? [];
  /** The actions of `row`, in the table's order; empty for a row nothing can be done to. */
  function actionsOf(row: LauncherRow | null): SheetAction[] {
    const subject = row ? subjectOf(row, inboxOps) : null;
    return subject ? sheetActions(subject, sheetContext) : [];
  }
  const sheetShown = $derived(sheet && !sheet.flow ? narrowActions(sheetActions(sheet.subject, sheetContext), sheet.filter) : []);
  /** The flow's current step's choices, as the box narrows them. */
  const flowShown = $derived(sheet?.flow ? narrowActions(sheet.flow.choices ?? [], sheet.filter) : []);
  const flowStep = $derived(sheet?.flow ? sheet.flow.steps[sheet.flow.picks.length] : undefined);

  /**
   * The ⌘? overlay (#2424), or `null`: the box holds `filter`, which narrows
   * the filters, and the search words wait untouched in `session.raw`.
   */
  let overlay = $state<{ filter: string; selected: number } | null>(null);
  const overlayShown = $derived(overlay ? narrowFilters(overlayFilters(session.syntax), overlay.filter) : []);
  /** The overlay's filters as the `?` card's rows, so one component draws both. */
  const overlayRows = $derived<LauncherRow[]>(
    overlayShown.map(({ entry, word }) => ({ kind: "syntax", id: `filter:${entry.token}`, entry: { ...entry, summary: word } })),
  );

  /**
   * The list's name while ⌘S asks for it (#2424), or `null`. The box holds
   * the name; the query being saved waits in `session.raw`.
   */
  let naming = $state<string | null>(null);

  /** Whether Tab has something to open on the selected row: the footer says so only then. */
  const tabOpens = $derived(!overlay && naming === null && !sheet && actionsOf(session.current).length > 0);

  /**
   * *Everything* with a preview (#2431): the dialog grows, and over a search
   * the body splits, the results at the left and the selected row's
   * excerpts at the right. Titles is the one list at the usual size.
   */
  const deep = $derived(scope.titles === false);
  const split = $derived(deep && overlay === null && naming === null && sheet === null && session.mode === "results");

  /**
   * The row the pane draws: the selection, once it has rested for
   * {@link PREVIEW_DEBOUNCE_MS}, so running the arrows down fifty rows draws
   * one pane and not fifty. Kept by id and read back from the rows, so a new
   * answer for the same row draws its new excerpts.
   */
  let previewed = $state<string | null>(null);
  $effect(() => {
    const id = split ? (session.current?.id ?? null) : null;
    const timer = setTimeout(() => (previewed = id), PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });
  const previewRow = $derived(previewed === null ? null : (session.rows.find((row) => row.id === previewed) ?? null));

  /**
   * A close ends the mode: the launcher stays mounted, so without this the
   * next opening would reopen on the overlay or the prompt. The box's words
   * are the opener's (#2425): empty for ⌘K, a section's filter for `/`.
   */
  $effect.pre(() => {
    if (!open) {
      overlay = null;
      naming = null;
      sheet = null;
      applicationsRead = false;
      merge = null;
    }
  });

  /**
   * What the box asks for, drawn as the label in front of it: its own
   * element, never part of the text. `null` while the box is the search.
   * Later modes (the sheet's Actions, Keys, New name, a flow's step) join here.
   */
  const boxLabel = $derived(
    naming !== null
      ? "Name"
      : overlay
        ? "Filter"
        : sheet
          ? sheet.renaming !== null
            ? "New name"
            : sheet.flow
              ? (flowStep?.title ?? "Pick")
              : "Actions"
          : null,
  );
  const placeholder = $derived(
    naming !== null
      ? "the list's name"
      : overlay
        ? "narrow the filters"
        : sheet
          ? sheet.renaming !== null
            ? "the list's new name"
            : sheet.flow
              ? "narrow the choices"
              : "narrow the actions"
          : "⌘? for help",
  );

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
    typing && session.suggestions && session.raw !== dismissedAt && !overlay && naming === null && !sheet
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

  // The sheet belongs to one row: a new answer that moves the selection off it closes it.
  $effect(() => {
    if (sheet && session.current?.id !== sheet.rowId) sheet = null;
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
  const sheetOptionId = (index: number) => `${uid}-act-${index}`;
  const suggestionId = (index: number) => `${uid}-sug-${index}`;
  /** The three listboxes, which the box names as the one it controls (#1665). */
  const resultsId = `${uid}-results`;
  const sheetId = `${uid}-actions`;
  const suggestionsId = `${uid}-suggestions`;

  /**
   * The result the box names as current, or `undefined` when no option
   * is drawn for it: no row, a failed read that draws its reason instead, or
   * the saved list whose name is a text field while it is being renamed.
   */
  const activeOption = $derived.by(() => {
    const row = session.current;
    if (!row || session.error) return undefined;
    return optionId(session.selected);
  });
  /** How many rows the sheet draws: the flow's choices while one is open, else the actions. */
  const sheetCount = $derived(sheet?.flow ? flowShown.length : sheetShown.length);
  const activeSheetOption = $derived(sheet && sheet.renaming === null && sheetCount > 0 ? sheetOptionId(sheet.selected) : undefined);

  /**
   * The list the box controls and the option it names: whichever list the
   * arrows walk, in the order `onkeydown` asks -- the value suggestions while
   * they are open, then the Tab sheet, then the results.
   */
  const controlled = $derived.by(() => {
    if (overlay) return { list: resultsId, option: overlayRows.length > 0 ? optionId(overlay.selected) : undefined };
    if (offer.length > 0) return { list: suggestionsId, option: suggestionId(suggestionPicked) };
    if (sheet) return { list: sheetId, option: activeSheetOption };
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
    if (activeSheetOption) scrollToOption(activeSheetOption);
  });

  function scrollToOption(id: string) {
    // Guarded: jsdom has no `scrollIntoView`.
    document.getElementById(id)?.scrollIntoView?.({ block: "nearest" });
  }

  /** The shell's inbox, which `#i` answers from (#2423). */
  $effect.pre(() => {
    session.extras = { inbox: inbox ?? null };
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
      // Every opening starts on what the opener asked for (#2425): empty for
      // ⌘K, never the last query (that was #1480's, reversed by spec #2419),
      // and a section's filter for its keys, with the caret after it. Read
      // untracked, so typing into the box does not run this again.
      untrack(() => {
        if (session.raw !== query) session.set(query);
      });
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
        // the ways out, which is also what the round-3 mockup did. Closed, it
        // opens on an empty box (#2425); open, it leaves the box as it is.
        // While open it only gives the box its focus back (#2426), wherever a
        // click left it: Tab is the one way into the actions.
        if (open) {
          box?.focus();
          return;
        }
        query = "";
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
      // A ticket opens by its entity id, which names its source (#1476, #1977).
      // A saved list does not navigate: it is applied in the box (#2423).
      case "hit":
        return row.hit.kind === "list" ? null : entityAddress(row.hit.kind, row.hit.entity_id, row.hit.title);
      case "recent":
        return entityAddress(row.row.kind, row.row.entity_id, row.row.title);
      case "action":
        return row.action.hash;
      // The Inbox page's own link for the line, and the Inbox page for a line
      // that has none to open.
      case "inbox":
        return opener(row.item)?.href ?? "#/inbox";
      // The Docs area searching the words (#2425).
      case "docs":
        return docsSearchHash(row.words);
      // A syntax row does not navigate: it rewrites the box, which is what
      // makes it a *filter* rather than a destination.
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

  /**
   * *Save as list*, from ⌘S or the footer's button (#2424, spec #2419 story
   * 41): the box itself asks for the name, labelled *Name*, the query
   * prefilled and selected, so typing replaces it and Enter on it keeps it.
   * No second dialog over this one; Esc goes back to the search.
   */
  function startNaming() {
    if (!session.canSave || session.savedId !== null || sheet) return;
    naming = session.raw.trim();
    void tick().then(() => box?.focus({ select: true }));
  }

  /** The naming prompt's keys: Enter saves under the name, Esc keeps searching. */
  function namingKey(event: KeyboardEvent, name: string) {
    if (event.key === "Enter") {
      event.preventDefault();
      const label = name.trim();
      if (label === "") return;
      naming = null;
      box?.focus();
      void session.saveCurrent(label).then(() => {
        if (session.savedId !== null) push({ text: `Saved as list ${label}` });
      });
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      naming = null;
      box?.focus();
    }
  }

  function openOverlay() {
    overlay = { filter: "", selected: 0 };
    box?.focus();
  }

  function leaveOverlay() {
    overlay = null;
    box?.focus();
  }

  /** Enter or a click on a filter: it goes into the search that was there. */
  function pickFilter(row: LauncherRow) {
    if (row.kind !== "syntax") return;
    const picked = overlayShown.find(({ entry }) => entry.token === row.entry.token);
    overlay = null;
    if (picked) session.set(addFilter(session.raw, picked.entry, session.syntax));
    box?.focus();
  }

  /** The overlay's keys: arrows walk, Enter picks, Esc or Backspace on the empty box go back. */
  function overlayKey(event: KeyboardEvent, current: { filter: string; selected: number }) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(overlayRows.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "Enter": {
        event.preventDefault();
        const row = overlayRows[current.selected];
        if (row) pickFilter(row);
        else leaveOverlay();
        return;
      }
      case "Tab":
        event.preventDefault();
        return;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        leaveOverlay();
        return;
      case "Backspace":
        if (current.filter === "") {
          event.preventDefault();
          leaveOverlay();
        }
        return;
      default:
    }
  }

  /** ⌘? (⌘⇧/ as most keyboards send it): the overlay from any search, and back. */
  function isHelpChord(event: KeyboardEvent): boolean {
    return (event.metaKey || event.ctrlKey) && (event.key === "?" || (event.key === "/" && event.shiftKey));
  }

  /** ⌘S, with no other modifier. */
  function isSaveChord(event: KeyboardEvent): boolean {
    return (event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "s";
  }

  /** Tab on a row with actions: the sheet, the box emptied and labelled Actions. */
  function openSheet() {
    const row = session.current;
    const subject = row ? subjectOf(row, inboxOps) : null;
    if (!row || !subject || sheetActions(subject, sheetContext).length === 0) return;
    sheet = { rowId: row.id, row, subject, filter: "", selected: 0, relation: 0, renaming: null, flow: null };
    readApplications();
    readMerge(subject);
    box?.focus();
  }

  /** Back to the results, the search words in the box as they were. */
  function leaveSheet() {
    sheet = null;
    box?.focus();
  }

  /** A write or a read that answers: said in a toast and the launcher closed, or the reason said and the sheet kept. */
  async function settled(work: () => Promise<unknown>, said: () => string) {
    try {
      await work();
      push({ text: said() });
      close();
    } catch (cause) {
      push({ text: ipcErrorMessage(cause), tone: "err" });
      box?.focus();
    }
  }

  /** Carry out one action of the open sheet. */
  function perform(action: SheetAction) {
    carry(action.does);
  }

  /** Carry out what an action, or a flow's last pick, does: the one reader of `SheetEffect`. */
  function carry(does: SheetEffect) {
    if (!sheet) return;
    const { subject, relation } = sheet;
    const go = (hash: string) => {
      close();
      onnavigate(hash);
    };
    switch (does.kind) {
      case "go":
        return go(does.hash);
      case "copy":
        return void settled(() => io.copyText(does.text), () => does.said);
      case "write":
        return void settled(() => io.submitWrite(does.payload), () => does.said);
      case "ack":
        return void settled(() => io.ackAlert(does.monitorId), () => does.said);
      case "link": {
        const job = openEntity;
        if (!job || subject.kind !== "entity") return;
        const picked = JOB_LINK_RELATIONS[relation] ?? JOB_LINK_RELATIONS[0]!;
        // The job's own mapping (`TfLinkTo`): drawn from the job to the row.
        const row = drawn(picked.id, job.entityId, subject.entityId);
        void io.createLink(row.fromId, row.toId, row.relation).then(
          () => {
            push({ text: `Linked ${nameOf(subject)} to ${job.label} as ${picked.label}` });
            close();
          },
          (cause: unknown) => {
            push({ text: linkFailureMessage(cause), tone: "err" });
            box?.focus();
          },
        );
        return;
      }
      case "timer":
        if (subject.kind !== "entity" || !ontimer) return;
        ontimer(subject.entityId, subject.title);
        return close();
      case "open-in": {
        if (subject.kind !== "entity") return;
        let argv: string[] = [];
        return void settled(
          async () => {
            const where = subject.entityKind === "pr" ? await io.repoOf({ sourceId: subject.sourceId, path: subject.path }) : subject.entityId;
            if (where === null) throw new Error(`The mirror holds no repository for ${subject.title}.`);
            argv = await io.openCheckout(where, { application: does.application });
          },
          () => `Started ${does.name}: ${argvText(argv)}`,
        );
      }
      case "clone":
        if (subject.kind !== "entity") return;
        return void settled(() => io.clone({ entityId: subject.entityId, title: subject.title }), () => `Cloning ${subject.title}`);
      case "apply":
        if (subject.kind !== "list") return;
        sheet = null;
        // As Enter on the list does (#2423): its text back in the box, or why it cannot be.
        if (subject.refusal !== null) push({ text: subject.refusal });
        else session.set(`${subject.text} `);
        box?.focus();
        return;
      case "rename":
        if (subject.kind !== "list") return;
        sheet.renaming = subject.label;
        void tick().then(() => box?.focus({ select: true }));
        return;
      case "delete":
        if (subject.kind !== "list") return;
        sheet = null;
        box?.focus();
        void session.deleteList(subject.id).then((done) => done && push({ text: `Deleted ${subject.label}` }));
        return;
      case "flow":
        return startFlow(does.steps, does.finish);
      case "save-set":
        return void settled(() => io.saveLinkSet(does.draft), () => does.said);
      case "snooze":
        return void settled(() => io.inboxSnooze(does.key, does.until), () => does.said);
      case "refused":
        push({ text: does.said });
        box?.focus();
        return;
      case "inbox": {
        if (subject.kind !== "inbox") return;
        const { item } = subject;
        if (does.op === "rerun") {
          return void settled(() => io.submitWrite({ RerunBuild: { entity: item.entity_id ?? "" } }), () => `Re-run queued for ${item.title}`);
        }
        if (does.op === "ack") {
          return void settled(() => io.inboxAck(item.key), () => "Acked. The alert stays open until the monitor recovers.");
        }
        return void settled(() => io.inboxDone(item.key), () => `Done: ${item.title}`);
      }
    }
  }

  /** Start a flow in the sheet: the box empties, and the first step's choices are read. */
  let flows = 0;
  function startFlow(steps: Step[], finish: Flow["finish"]) {
    if (!sheet || steps.length === 0) return;
    const flow: Flow = { id: ++flows, steps, finish, picks: [], asked: [], choices: null };
    sheet.flow = flow;
    sheet.filter = "";
    sheet.selected = 0;
    void readStep(flow.id, 0);
    box?.focus();
  }

  /** Read step `at` of flow `id`: an answer for a flow or a step no longer open is dropped. */
  async function readStep(id: number, at: number) {
    const flow = sheet?.flow;
    const step = flow?.steps[at];
    if (!flow || !step) return;
    const picks = [...flow.picks];
    // Still the step asked: the same flow, and the same picks (a step back and another pick reads it again).
    const asked = () => {
      const live = sheet?.flow;
      return live && live.id === id && live.choices === null && live.picks.length === picks.length && live.picks.every((one, i) => one === picks[i]) ? live : null;
    };
    try {
      const choices = await step.choices(picks);
      const live = asked();
      if (live) live.choices = choices;
    } catch (cause) {
      if (!sheet || !asked()) return;
      push({ text: ipcErrorMessage(cause), tone: "err" });
      sheet.flow = null;
      sheet.filter = "";
      sheet.selected = 0;
    }
  }

  /** Take `choice` for the current step: the next step's choices, or the last pick runs the flow's finish. */
  function pick(current: NonNullable<typeof sheet>, flow: Flow, choice: Choice) {
    const picks = [...flow.picks, choice.id];
    if (picks.length >= flow.steps.length) {
      const finished = flow.finish(picks);
      current.flow = null;
      current.filter = "";
      current.selected = 0;
      carry(finished);
      return;
    }
    flow.asked = [...flow.asked, flow.choices ?? []];
    flow.picks = picks;
    flow.choices = null;
    current.filter = "";
    current.selected = 0;
    void readStep(flow.id, picks.length);
  }

  /** One step back: the step before, its choices as they were, or from the first step the actions. */
  function stepBack(current: NonNullable<typeof sheet>, flow: Flow) {
    current.filter = "";
    current.selected = 0;
    if (flow.picks.length === 0) {
      current.flow = null;
    } else {
      flow.picks = flow.picks.slice(0, -1);
      flow.choices = flow.asked.at(-1) ?? null;
      flow.asked = flow.asked.slice(0, -1);
      if (flow.choices === null) void readStep(flow.id, flow.picks.length);
    }
    box?.focus();
  }

  /** A flow's keys: the arrows walk the choices, Enter picks, Esc or Backspace on the empty box step back, Tab leaves. */
  function flowKey(event: KeyboardEvent, current: NonNullable<typeof sheet>, flow: Flow) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(flowShown.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "Enter": {
        event.preventDefault();
        const chosen = flowShown[current.selected];
        if (chosen) pick(current, flow, chosen);
        return;
      }
      case "Tab":
        event.preventDefault();
        leaveSheet();
        return;
      case "Escape":
        // Never the dialog's: a step back.
        event.preventDefault();
        event.stopPropagation();
        stepBack(current, flow);
        return;
      case "Backspace":
        if (current.filter === "") {
          event.preventDefault();
          stepBack(current, flow);
        }
        return;
      default:
    }
  }

  /** The new name's keys: Enter keeps it, Esc goes back to the list's actions. */
  function renamingKey(event: KeyboardEvent, current: NonNullable<typeof sheet>, name: string) {
    if (event.key === "Enter") {
      event.preventDefault();
      const label = name.trim();
      if (label === "" || current.subject.kind !== "list") return;
      const { id } = current.subject;
      sheet = null;
      box?.focus();
      void session.renameList(id, label).then((done) => done && push({ text: `Renamed to ${label}` }));
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      current.renaming = null;
      current.filter = "";
      box?.focus();
    }
  }

  /** The sheet's keys: the arrows walk, ← → pick the link's relation, Enter acts, Esc, Tab or Backspace on the empty box go back. */
  function sheetKey(event: KeyboardEvent, current: NonNullable<typeof sheet>) {
    if (current.renaming !== null) return renamingKey(event, current, current.renaming);
    if (current.flow) return flowKey(event, current, current.flow);
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(sheetShown.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "ArrowLeft":
      case "ArrowRight":
        if (sheetShown[current.selected]?.does.kind === "link") {
          event.preventDefault();
          const step = event.key === "ArrowRight" ? 1 : -1;
          current.relation = (current.relation + step + JOB_LINK_RELATIONS.length) % JOB_LINK_RELATIONS.length;
        }
        return;
      case "Enter": {
        event.preventDefault();
        const chosen = sheetShown[current.selected];
        if (chosen) perform(chosen);
        return;
      }
      case "Tab":
        event.preventDefault();
        leaveSheet();
        return;
      case "Escape":
        // Never the dialog's: the sheet goes back to the results.
        event.preventDefault();
        event.stopPropagation();
        leaveSheet();
        return;
      case "Backspace":
        if (current.filter === "") {
          event.preventDefault();
          leaveSheet();
        }
        return;
      default:
    }
  }

  function activate(row: LauncherRow) {
    if (row.kind === "hit" && row.hit.kind === "list") {
      // A saved list applies by its own text (#2423, spec #2419 story 14):
      // the filters and words go back into the box, with the space after
      // them, and run; the reader stays in the box to see and change them.
      // One whose text today's grammar refuses carries the reason as its
      // state (`saved::plan`), and says it instead of applying.
      if (row.hit.state) {
        push({ text: row.hit.state.text });
        return;
      }
      session.set(`${row.hit.path ?? ""} `);
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
    if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "d") {
      // Before the webview's own ⌘D (a bookmark) and anything else the box does.
      event.preventDefault();
      toggleScope();
      return;
    }
    if (naming !== null) {
      // ⌘S again is the prompt's own key, not the browser's Save.
      if (isSaveChord(event)) event.preventDefault();
      else namingKey(event, naming);
      return;
    }
    if (sheet) {
      // The box's own chords are not the sheet's: ⌘S and ⌘? do nothing here.
      if (isSaveChord(event) || isHelpChord(event)) event.preventDefault();
      else sheetKey(event, sheet);
      return;
    }
    if (isHelpChord(event)) {
      event.preventDefault();
      if (overlay) leaveOverlay();
      else openOverlay();
      return;
    }
    if (overlay) {
      if (isSaveChord(event)) event.preventDefault();
      else overlayKey(event, overlay);
      return;
    }
    if (isSaveChord(event)) {
      event.preventDefault();
      startNaming();
      return;
    }
    if (suggestionKey(event)) return;
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
      case "Tab":
        // Swallowed either way, never forwarded: the default would move focus
        // out of the box. A row with no actions opens nothing.
        event.preventDefault();
        openSheet();
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

  /** The keys the box answers, at the bottom of the ⌘? overlay. */
  const keys: [string, string][] = [
    ["↵", "open"],
    ["Tab", "actions"],
    ["⌘S", "save as list"],
    ["⌘?", "filters"],
    // #2430, joined in the merge with #2424: the box answers it too.
    ["⌘D", "titles or everything"],
    ["Esc", "back"],
  ];
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
    alone here: the box takes it for the action sheet and says so with
    `preventDefault`.
  -->
  <TfDialog title="Search and actions" className={deep ? "tf-launch tf-launch-deep" : "tf-launch"} onclose={close}>
    {#snippet body()}
      <!--
        The label is its own element in front of the box (#2424): what the box
        asks for, highlighted, never part of the text and so never deletable.
      -->
      <div class="tf-ln-boxrow">
        {#if boxLabel !== null}<span class="tf-ln-mode">{boxLabel}</span>{/if}
        <QueryBox
          bind:this={box}
          value={naming !== null ? naming : overlay ? overlay.filter : sheet ? (sheet.renaming ?? sheet.filter) : session.raw}
          {placeholder}
          pending={session.pending}
          controls={controlled.list}
          active={controlled.option}
          oninput={(value) => {
            if (naming !== null) naming = value;
            else if (overlay) overlay = { filter: value, selected: 0 };
            else if (sheet?.renaming != null) sheet.renaming = value;
            else if (sheet) {
              sheet.filter = value;
              sheet.selected = 0;
            } else session.type(value);
          }}
          {onkeydown}
          onclose={close}
        >
          {#snippet trailing()}
            <!-- A pill toggle (Controls): its words are the mode in force; ⌘D and the footer name the other. -->
            <span class="tf-fchips tf-ln-scope">
              <button
                type="button"
                title={scope.titles ? "Titles and keys only. ⌘D searches everything" : "Bodies too. ⌘D searches titles only"}
                onclick={toggleScope}>{scope.titles ? "Titles" : "Everything"}</button
              >
            </span>
          {/snippet}
        </QueryBox>
      </div>

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

      {#if interpreted && !overlay && !sheet}
        <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} />
      {/if}

      {#if sheet}
        {@const current = sheet}
        <!-- The row the sheet acts on, lifted under the box; the box keeps the search words aside. -->
        <Pinned row={current.row} id={`${uid}-pinned`} {now} />
        {#if current.renaming !== null}
          <p class="tf-ln-none">Type the new name in the box. Enter keeps it, Esc leaves the name as it was.</p>
        {:else}
          <!--
            Keys from a clicked chip or row land here, so Esc goes back to the
            results and never reaches the dialog; a click gives the box its
            focus back.
          -->
          <div
            id={sheetId}
            class="tf-ln-list tf-ln-sheet"
            role="listbox"
            tabindex="-1"
            aria-label="Actions on the selected result"
            {onkeydown}
            onclick={() => box?.focus()}
          >
            {#if current.flow}
              {@const flow = current.flow}
              <!-- A flow's step: its title, the picks so far, and its choices as the box narrows them. -->
              <div class="tf-ln-h">
                <span>{flowStep?.title ?? ""}</span>
                {#if flow.picks.length > 0}<span class="tf-ln-s">{flow.picks.join(" → ")}</span>{/if}
              </div>
              {#each flowShown as choice, index (choice.id)}
                <button
                  id={sheetOptionId(index)}
                  class="tf-ln-opt"
                  class:tf-ln-on={index === current.selected}
                  role="option"
                  aria-selected={index === current.selected}
                  onclick={() => pick(current, flow, choice)}
                  onmouseenter={() => (current.selected = index)}
                >
                  <span class="tf-ln-nm">{choice.label}</span>
                  <span class="tf-ln-s">{choice.detail}</span>
                </button>
              {/each}
              {#if flow.choices === null}
                <p class="tf-ln-none">Reading…</p>
              {:else if flowShown.length === 0}
                <p class="tf-ln-none">{current.filter === "" ? "Nothing to pick here." : `Nothing matches “${current.filter}”.`}</p>
              {/if}
            {/if}
            {#each sheetShown as action, index (action.id)}
              {#if action.does.kind === "link"}
                <!-- Not a button: the relation chips inside it are buttons of their own. -->
                <div
                  id={sheetOptionId(index)}
                  class="tf-ln-opt tf-ln-link"
                  class:tf-ln-on={index === current.selected}
                  role="option"
                  tabindex="-1"
                  aria-selected={index === current.selected}
                  onmouseenter={() => (current.selected = index)}
                >
                  <button type="button" class="tf-ln-nm" onclick={() => perform(action)}>{action.label}</button>
                  <span class="tf-fchips" role="group" aria-label="Relation">
                    {#each JOB_LINK_RELATIONS as relation, r (relation.id)}
                      <button
                        type="button"
                        tabindex="-1"
                        aria-pressed={r === current.relation}
                        onclick={() => (current.relation = r)}>{relation.label}</button
                      >
                    {/each}
                  </span>
                </div>
              {:else}
                <button
                  id={sheetOptionId(index)}
                  class="tf-ln-opt"
                  class:tf-ln-on={index === current.selected}
                  role="option"
                  aria-selected={index === current.selected}
                  onclick={() => perform(action)}
                  onmouseenter={() => (current.selected = index)}
                >
                  <span class="tf-ln-nm">{action.label}</span>
                  <span class="tf-ln-s">{action.detail}</span>
                </button>
              {/if}
            {/each}
            {#if !current.flow && sheetShown.length === 0}
              <p class="tf-ln-none">No action matches “{current.filter}”.</p>
            {/if}
          </div>
        {/if}
      {:else}
      <div class="tf-ln-split" class:tf-ln-two={split}>
      <div id={resultsId} class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Results">
        {#if overlay}
          <Help
            rows={overlayRows}
            selected={overlay.selected}
            heading="Filters"
            {keys}
            {optionId}
            onopen={pickFilter}
            onhover={(index) => overlay && (overlay.selected = index)}
          />
        {:else if session.error}
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
              rows={session.rows}
              selected={session.selected}
              {optionId}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
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
            {now}
            onopen={activate}
            onhover={(index) => (session.selected = index)}
          />
        {:else}
          <p class="tf-ln-none">Searching…</p>
        {/if}
      </div>
      {#if split}
        <Preview
          title={previewRow?.kind === "hit" ? previewRow.hit.title : null}
          excerpts={previewRow?.kind === "hit" ? previewRow.hit.excerpts : []}
          onopen={() => previewRow && activate(previewRow)}
        />
      {/if}
      </div>

      {/if}
    {/snippet}

    {#snippet footer()}
      <span><kbd>↑↓</kbd> move</span>
      <span><kbd>↵</kbd> open</span>
      {#if tabOpens}<span title="Actions on the selected result"><kbd>Tab</kbd> actions</span>{/if}
      <span><kbd>Esc</kbd> back one step</span>
      <span><kbd>⌘D</kbd> {scope.titles ? "everything" : "titles only"}</span>
      {#if session.canSave}
        <!--
          On a query and never on the board: an empty box is the board, and
          there is nothing there to save. `canSave` is the session's, so the
          control and the write agree about when there is a query.
        -->
        <button class="tf-btn tf-sm tf-ln-save" onclick={startNaming} disabled={session.savedId !== null || naming !== null}>
          {session.savedId === null ? "Save as list" : "Saved as list"}
        </button>
      {/if}
    {/snippet}
  </TfDialog>
{/if}
