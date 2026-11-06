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
  app or a write it has today, carried out by `SheetMode`
  (`sheet-mode.svelte.ts`), which holds the mode. *Link to ⟨job⟩*
  comes first on every entity, with the relation picked inline by ← and → and
  written on Enter through the job's own mapping (`drawn`), against the job
  the shell names (`openEntity`: the ticket on screen, else the chosen job),
  from any page. This reverses the 2026-10-21 ruling on #1416, which trimmed
  the chain to two rows and handed the link to the job's dialog
  (`docs/decisions/2026-11-02-launcher-sheet.md`).

  *Start timer* is a ticket's row alone, and the shell does the rest: it
  stops the running timer first and makes the ticket the chosen job.

  ## A section's filter (#2425)

  `/`, ⌘/ and the top bar's button open the box on the section's filter. The
  query holds its grammar (`#rp `), and the box does not: it draws a chip named
  for the section (*Code*) in front of the words, removed by its × or by
  Backspace on the empty box. `Esc` takes the words first and then closes, so
  an opening with no words closes in one press wherever it was opened. Text
  that leads with its own panel (`>`, `?`, a pasted link) replaces the filter.

  ## Flows (#2427)

  An action that asks something is a flow (`does.kind === "flow"`): the
  sheet's `flow` holds its steps, the picks so far and the current step's
  choices once read. The box, labelled with the step's title, narrows them;
  Enter picks and the next step is read; the last pick runs what the flow's
  `finish` makes of the picks, like any other action. Esc or
  Backspace on the empty box step back one step, and from the first back to
  the actions; the row stays pinned throughout.

  ## Twins (#1476)

  Two sources can hold one key, and those are two tickets. A row whose key another
  row in the list has names its source at its end (`twinTickets`), as *Link to…*
  does; no other row does.

  ## Keys (#2429)

  ⌘B on a selected action, or *Assign key…* on a saved list, opens the key
  panel in the sheet (the box labelled *Keys*): every press is drawn at once,
  the modifiers while held; Enter or OK keeps the chord, Backspace clears it,
  Esc and Cancel leave things as they were (`key-panel.svelte.ts`). The box's
  own chords, the shell's and the system's are refused there with a message
  (`keys.ts`). A
  bound action chord runs that action on the selected row of its kind
  (`runChord`), opening the sheet first only for a flow, a rename, the panel
  or the link; a list chord puts the list's text in the box. With the
  launcher closed the shell answers a list chord (`TfShell`).

-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import {
    addFavourite as defaultAddFavourite,
    createSmartList as defaultCreateSmartList,
    deleteSmartList as defaultDeleteSmartList,
    filterSuggestions as defaultFilterSuggestions,
    launcherHome as defaultHome,
    ipcErrorMessage,
    removeFavourite as defaultRemoveFavourite,
    renameSmartList as defaultRenameSmartList,
    resolveUrl as defaultResolveUrl,
    search as defaultSearch,
    EVENTS,
  } from "../ipc";
  import { listSources, type CredentialHealth, type KindStateColours } from "../ipc/sources";
  import type { HealthPhase } from "../shell/health.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { entityAddress } from "../tf/address";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { docsSearchHash } from "../tf/docs/docs";
  import { aimed, watchPointer } from "./aim";
  import Board from "./Board.svelte";
  import Chips from "./Chips.svelte";
  import Help from "./Help.svelte";
  import { type LegendPorts, legendOf, realLegendPorts } from "./legend";
  import Preview, { PREVIEW_DEBOUNCE_MS } from "./Preview.svelte";
  import QueryBox from "./QueryBox.svelte";
  import Pinned from "./Pinned.svelte";
  import Results from "./Results.svelte";
  import KeyPanel from "./KeyPanel.svelte";
  import SheetList from "./SheetList.svelte";
  import { savedListOf } from "./sheet";
  import { SheetMode } from "./sheet-mode.svelte";
  import { type SheetPorts, realSheetPorts } from "./sheet-ports";
  import { type LauncherAction, filterActions, navigationActions, placeActions } from "./actions";
  import { type InboxPreview, type LauncherRow, holdsAnswer, narrowed, rowItem, stepOf, twinTickets, walk } from "./rows";
  import { modeName, seedKinds, seedName } from "./section";
  import { pastedUrl } from "./url";
  import { opener } from "../tf/inbox/inbox";
  import { Session, type SessionPorts } from "./session.svelte";
  import { type ScopeChoice, launcherScope } from "./scope.svelte";
  import { type Suggestion, activeToken, complete, suggest } from "./suggest";
  import { addFilter, narrowFilters, overlayFilters } from "./syntax";
  import { type Chord, isBoxChord, isHelpPress } from "./keys";
  import { type KeyStore, launcherKeys } from "./keys.svelte";
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
    legendPorts,
    openExternal = realOpenExternal,
    scope = launcherScope,
    keys = launcherKeys,
    places = placeActions(),
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
    /** The colour legend's read and the event that re-reads it (#2467), injectable as `ports` is. */
    legendPorts?: Partial<LegendPorts>;
    /**
     * *Titles* or *Everything* (#2430): the app's one choice, kept across
     * closing and a restart. Injectable so a test can keep it in a storage of
     * its own and read it back as a restart would.
     */
    scope?: ScopeChoice;
    /** The keys bound (#2429): the window's store, which the shell reads too; injectable for a test. */
    keys?: KeyStore;
    /** The app's places, which *Go to* offers by name over a search's answer; injectable for a test about something else. */
    places?: readonly LauncherAction[];
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
    addFavourite: defaultAddFavourite,
    removeFavourite: defaultRemoveFavourite,
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

  /** What an empty answer's *Search everything* does: the same words, with the filters gone and every field searched. */
  function widen(words: string) {
    if (scope.titles) scope.toggle();
    session.type(words);
    session.rerun();
  }

  /**
   * The section's filter the box opened on, while the query still starts with
   * it: drawn as a chip, never as text. The query keeps it, because it is what
   * the backend parses and what a saved list saves.
   */
  let seed = $state<string | null>(null);
  const seeded = $derived(seed !== null && session.raw.startsWith(seed) ? seed : null);
  const seedLabel = $derived(seeded === null ? null : seedName(seeded));
  /** What the box shows of the query: the words after the chip. */
  const boxWords = $derived(seeded === null ? session.raw : session.raw.slice(seeded.length));

  /** The query for what the box now holds. Text that leads with its own panel does not go under the section's filter. */
  function queryFor(words: string): string {
    if (seeded === null) return words;
    const panel = session.syntax.some((row) => (row.prefix === "action" || row.prefix === "help") && words.startsWith(row.token));
    return panel || pastedUrl(words) !== null ? words : `${seeded}${words}`;
  }

  /** The chip's ×, or Backspace on the empty box: the search is everything again. */
  function dropSeed() {
    seed = null;
    session.set("");
    box?.focus();
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
   * The Tab sheet, its flows and the bound chords (`sheet-mode.svelte.ts`); the
   * key panel is one opening held on the sheet (`key-panel.svelte.ts`). This
   * component only asks them in the order the box's keys are answered.
   */
  const sheet = new SheetMode({
    session,
    io,
    get keys() {
      return keys;
    },
    get job() {
      return openEntity;
    },
    get ontimer() {
      return ontimer;
    },
    get now() {
      return now;
    },
    inboxOps: (key) => inbox?.actions?.[key] ?? [],
    focus: (how) => box?.focus(how),
    close: () => close(),
    navigate: (hash) => onnavigate(hash),
  });

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

  // svelte-ignore state_referenced_locally
  // Read once, as the session's ports are.
  const legendIo: LegendPorts = { ...realLegendPorts, ...legendPorts };
  /**
   * The colour legend under the `?` card and the ⌘? overlay (#2467): the
   * merged map the *state colours* read answers, read while either is on
   * screen. A set or a clear announces itself on `activity:new` (#2465
   * Ruling 1), so while the legend is drawn every activity line re-reads it.
   * Not filtered by verb: the event is coalesced, so the line it carries may
   * be another one than the set's. A failed read keeps what was drawn.
   */
  const legendShown = $derived(open && (overlay !== null || session.mode === "help"));
  let colours = $state<KindStateColours[]>([]);
  const legend = $derived(legendOf(colours));
  $effect(() => {
    if (!legendShown) return;
    let live = true;
    let asked = 0;
    let off: (() => void) | null = null;
    const read = () => {
      const mine = ++asked;
      legendIo
        .stateColours()
        .then((answer) => {
          if (live && mine === asked) colours = answer;
        })
        .catch(() => {});
    };
    legendIo
      .listen(EVENTS.activityNew, read)
      .then((stop) => {
        if (live) off = stop;
        else stop();
      })
      .catch(() => {
        // No event channel: the legend is the one read at opening.
      });
    read();
    return () => {
      live = false;
      off?.();
    };
  });

  /**
   * The list's name while ⌘S asks for it (#2424), or `null`. The box holds
   * the name; the query being saved waits in `session.raw`.
   */
  let naming = $state<string | null>(null);

  /**
   * Whether *Titles* or *Everything* changes what the box answers: the board (as the choice to type under) and a search.
   * In the sheet, `>`, `?`, a paste and the prompts it changes nothing, so the chip and ⌘D's hint are gone there and ⌘D
   * does nothing but keep the webview's bookmark away (F-643).
   */
  const scopeAnswers = $derived(!overlay && naming === null && !sheet.current && (session.mode === "board" || session.mode === "results"));

  /** The tickets in this list whose key another ticket here holds (#1476): they say their source, and nothing else does. */
  const twins = $derived(twinTickets(session.rows.flatMap((row) => rowItem(row) ?? [])));
  /** Source id → name, read when a twin is drawn; until it answers a row says the id, which is a true name. */
  let sourceNames = $state<Record<string, string>>({});
  $effect(() => {
    if (!open || twins.size === 0) return;
    let live = true;
    listSources().then(
      (rows) => {
        if (live) sourceNames = Object.fromEntries(rows.map((row) => [row.id, row.display_name]));
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });
  function sourceOf(row: LauncherRow): string | null {
    const item = rowItem(row);
    return item !== null && twins.has(item.entity_id) ? (sourceNames[item.source_id] || item.source_id) : null;
  }

  /** Whether Tab has something to open on the selected row: the footer says so only then. */
  const tabOpens = $derived(!overlay && naming === null && !sheet.current && sheet.opensOn(session.current));

  /**
   * *Everything* with a preview (#2431): the dialog grows, and over a search
   * the body splits, the results at the left and the selected row's
   * excerpts at the right. Titles is the one list at the usual size.
   */
  const deep = $derived(scope.titles === false);
  const split = $derived(deep && overlay === null && naming === null && sheet.current === null && session.mode === "results" && holdsAnswer(session.rows));

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
      sheet.reset();
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
        : sheet.current
          ? sheet.label
          : null,
  );
  const placeholder = $derived(
    naming !== null
      ? "The list's name"
      : overlay
        ? "Narrow the filters"
        : sheet.current
          ? sheet.placeholder
          : seedLabel !== null
            ? `Search ${seedLabel.toLowerCase()}`
            : "Search tickets, code, builds and docs",
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
    typing && session.suggestions && session.raw !== dismissedAt && !overlay && naming === null && !sheet.current
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
    const to = walk(event, suggestionPicked, offer.length);
    if (to !== null) {
      suggestionPicked = to;
      return true;
    }
    switch (event.key) {
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
  const activeSheetOption = $derived(sheet.active === null ? undefined : sheetOptionId(sheet.active));

  /**
   * The list the box controls and the option it names: whichever list the
   * arrows walk, in the order `onkeydown` asks -- the value suggestions while
   * they are open, then the Tab sheet, then the results.
   */
  const controlled = $derived.by(() => {
    if (overlay) return { list: resultsId, option: overlayRows.length > 0 ? optionId(overlay.selected) : undefined };
    if (offer.length > 0) return { list: suggestionsId, option: suggestionId(suggestionPicked) };
    if (sheet.current) return { list: sheetId, option: activeSheetOption };
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
    session.extras = { inbox: inbox ?? null, places, widens: true };
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
      // A share import or another window may have bound keys since.
      void keys.load();
      // Every opening starts on what the opener asked for (#2425): empty for
      // ⌘K, never the last query (that was #1480's, reversed by spec #2419),
      // and a section's filter for its keys, with the caret after it. Read
      // untracked, so typing into the box does not run this again.
      untrack(() => {
        seed = seedName(query) === null ? null : query;
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
    const stopWatching = watchPointer();
    return () => {
      window.removeEventListener("keydown", onkeydown);
      stopWatching();
      session.dispose();
    };
  });

  /**
   * What held focus when the launcher opened, and the address it stood on: its box takes focus before the dialog shell
   * looks, so the shell's restore finds the box. A close that went nowhere puts focus back (F-632); a jump is the shell's.
   */
  let before: { el: Element | null; hash: string } | null = null;
  $effect.pre(() => {
    if (open && before === null) before = { el: document.activeElement, hash: location.hash };
  });

  /** When `Enter` was last pressed in the box. */
  let enterAt = Number.NEGATIVE_INFINITY;

  /**
   * The second of two quick presses must not fall through the closed box (F-340's trip): focus went back to what held it
   * and the pointer rests over the page, so it would open a ticket card nobody aimed at. Only a real press counts, and only
   * for the moment a double press takes.
   */
  function holdTheSecondPress() {
    const byEnter = performance.now() - enterAt < 2000;
    const stop = () => {
      window.removeEventListener("keydown", onpress, true);
      window.removeEventListener("click", onpress, true);
      clearTimeout(timer);
    };
    const onpress = (event: Event) => {
      if (!event.isTrusted) return;
      if (event instanceof KeyboardEvent ? byEnter && event.key === "Enter" : true) {
        event.preventDefault();
        event.stopPropagation();
      }
      stop();
    };
    window.addEventListener("keydown", onpress, true);
    window.addEventListener("click", onpress, true);
    const timer = setTimeout(stop, 400);
  }

  function close() {
    const was = before;
    before = null;
    open = false;
    holdTheSecondPress();
    session.dispose();
    onclose();
    void tick().then(() => {
      if (was !== null && was.el instanceof HTMLElement && was.el.isConnected && was.el !== document.body && location.hash === was.hash) was.el.focus({ preventScroll: true });
    });
  }

  /**
   * Where a row goes. Every M1 row is an address (spec §2); an entity's is
   * the ticket-first shell's, through the one rule in `tf/address.ts` (#1085).
   */
  function addressOf(row: LauncherRow): string | null {
    switch (row.kind) {
      // A ticket opens by its entity id, which names its source (#1476, #1977).
      // A saved list, a `#l` hit or a favourite, does not navigate: it is
      // applied in the box (#2423, #2428).
      case "hit":
        return savedListOf(row) ? null : entityAddress(row.hit.kind, row.hit.entity_id, row.hit.title);
      case "recent":
      case "favourite":
        return savedListOf(row) ? null : entityAddress(row.row.kind, row.row.entity_id, row.row.title);
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
      case "widen":
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
    if (!session.canSave || session.savedId !== null || sheet.current) return;
    naming = boxWords.trim() || seedLabel || session.raw.trim();
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
    const to = walk(event, current.selected, overlayRows.length);
    if (to !== null) {
      current.selected = to;
      return;
    }
    switch (event.key) {
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

  /** ⌘S, with no other modifier. */
  const isSaveChord = (event: KeyboardEvent) => isBoxChord(event, "s");

  function activate(row: LauncherRow) {
    if (row.kind === "widen") {
      widen(row.words);
      box?.focus();
      return;
    }
    const subject = savedListOf(row);
    if (subject) {
      // A saved list, its `#l` hit or a favourite, applies by its own text
      // (#2423, spec #2419 story 14): the filters and words go back into the
      // box, with the space after them, and run; the reader stays in the box
      // to see and change them. One whose text today's grammar refuses
      // carries the reason as its state (`saved::plan`), and says it instead
      // of applying.
      if (subject.refusal !== null) {
        push({ text: subject.refusal });
        return;
      }
      session.set(`${subject.text} `);
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
    if (event.key === "Enter") enterAt = performance.now();
    // The key panel takes every key first: its chords are what is being bound.
    if (sheet.current?.panel) return sheet.current.panel.press(event);
    if (isBoxChord(event, "d")) {
      // Before the webview's own ⌘D (a bookmark) and anything else the box does.
      event.preventDefault();
      if (scopeAnswers) toggleScope();
      return;
    }
    if (naming !== null) {
      // ⌘S again is the prompt's own key, not the browser's Save.
      if (isSaveChord(event)) event.preventDefault();
      else namingKey(event, naming);
      return;
    }
    // A bound chord (#2429), from the results or the sheet's actions; never in a flow, a rename or the filters.
    if (!overlay && sheet.runChord(event)) return;
    if (sheet.current) return sheet.key(event);
    if (isHelpPress(event)) {
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
    const step = stepOf(event);
    if (step !== 0) {
      event.preventDefault();
      session.move(step);
      return;
    }
    switch (event.key) {
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
        sheet.open();
        return;
      case "Escape":
        // One rung per press, and the key never reaches the shell's ladder.
        event.preventDefault();
        event.stopPropagation();
        // The chip stays with the words gone, and the next press closes.
        if (boxWords !== "") {
          session.set(seeded ?? "");
        } else {
          close();
        }
        return;
      case "Backspace":
        if (seeded !== null && boxWords === "") {
          event.preventDefault();
          dropSeed();
        }
        return;
      default:
    }
  }

  /** The chord drawn on a saved list's row in the results and on the board. */
  function listChord(row: LauncherRow): Chord | null {
    const list = savedListOf(row);
    return list && keys.chordOfList(list.id);
  }

  /** The keys the box answers, at the bottom of the ⌘? overlay. */
  const boxKeys: [string, string][] = [
    ["↵", "open"],
    ["Tab", "actions"],
    ["⌘B", "bind a key to the action"],
    ["⌘S", "save as list"],
    ["⌘?", "help"],
    // #2430, joined in the merge with #2424: the box answers it too.
    ["⌘D", "titles or everything"],
    ["Esc", "back"],
  ];
  /** The chips render from the answer; before one lands there is nothing to
      say about a query nobody has interpreted yet. */
  const interpreted = $derived(session.response?.interpreted ?? null);
  /** The prefix the box opened with, by its name: drawn in front of the box, where the other modes say theirs (F-626). */
  const typedMode = $derived(
    boxLabel === null && session.raw.trim() !== "" && interpreted?.prefix ? modeName(interpreted.prefix) : null,
  );
  /** Whether the footer's *Save as list* is on offer: an answer to save, and no mode over it that has its own keys. */
  const saveShown = $derived(session.canSave && holdsAnswer(session.rows) && overlay === null && naming === null && sheet.current === null);

  /**
   * The keys the footer names, for what the box answers right now: one list per
   * mode, so `↵` always says what it does. The ⌘? overlay has its own legend and
   * the key panel its own hint, so they have none here.
   */
  const footerKeys = $derived.by((): [string, string, string?][] => {
    if (overlay !== null) return [];
    if (naming !== null) return [["↵", "save"], ["Esc", "keep searching"]];
    const open = sheet.current;
    if (open) {
      if (open.panel) return [];
      if (open.renaming !== null) return [["↵", "keep the name"], ["Esc", "leave it as it was"]];
      if (open.flow) return [["↑↓", "move"], ["↵", "pick"], ["Esc", "back one step"]];
      return [["↑↓", "move"], ["↵", "run"], ["⌘B", "bind a key"], ["Esc", "back to the results"]];
    }
    const row = session.current;
    const enter: [string, string, string?][] =
      session.mode === "url"
        ? session.urlAnswer?.match === null
          ? [["↵", "open in browser"]]
          : []
        : session.mode === "help"
          ? [["↑↓", "move"], ["↵", "put in the box"]]
          : row === null
            ? []
            : [["↑↓", "move"], ["↵", row.kind === "widen" ? "search everything" : row.kind === "action" ? `go to ${row.action.label}` : "open"]];
    return [
      ...enter,
      ...(tabOpens ? ([["Tab", "actions", "Actions on the selected result"]] as [string, string, string][]) : []),
      ["Esc", boxWords === "" ? "close" : "clear the words"],
      ...(scopeAnswers ? ([["⌘D", scope.titles ? "everything" : "titles only"]] as [string, string][]) : []),
      ["⌘?", "help"],
    ];
  });
</script>

{#snippet keysAndSave()}
  <!-- Only what a key does here. The band is gone when it has nothing to say. -->
  {#each footerKeys as [key, what, title] (key)}
    <span {title}><kbd>{key}</kbd> {what}</span>
  {/each}
  {#if saveShown}
    <!--
      On a query and never on the board: an empty box is the board, and
      there is nothing there to save. `canSave` is the session's, so the
      control and the write agree about when there is a query.
    -->
    <button class="tf-btn tf-sm tf-ln-save" onclick={startNaming} disabled={session.savedId !== null}>
      {session.savedId === null ? "Save as list…" : "Saved as list"}
    </button>
  {/if}
{/snippet}

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
  <TfDialog
    title="Search and actions"
    className={deep ? "tf-launch tf-launch-deep" : "tf-launch"}
    onclose={close}
    scrimCloses
    footer={footerKeys.length > 0 || saveShown ? keysAndSave : undefined}
  >
    {#snippet body()}
      <!--
        The label is its own element in front of the box (#2424): what the box
        asks for, highlighted, never part of the text and so never deletable.
      -->
      <div class="tf-ln-boxrow">
        {#if boxLabel !== null}
          <span class="tf-ln-mode">{boxLabel}</span>
        {:else if seedLabel !== null}
          <!-- The section's filter, by the section's name: a chip like the bar's, with its own way off. -->
          <span class="tf-ln-fchip tf-ln-set tf-ln-seed" title="Searching {seedLabel} only. Backspace on the empty box searches everything">
            {seedLabel}<button type="button" class="tf-ln-seedx" aria-label="Search everything, not only {seedLabel}" onclick={dropSeed}>
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
            </button>
          </span>
        {/if}
        {#if typedMode !== null}<span class="tf-ln-mode">{typedMode}</span>{/if}
        <QueryBox
          bind:this={box}
          value={naming !== null ? naming : overlay ? overlay.filter : sheet.current ? sheet.text : boxWords}
          {placeholder}
          pending={session.pending}
          controls={controlled.list}
          active={controlled.option}
          oninput={(value) => {
            if (naming !== null) naming = value;
            else if (overlay) overlay = { filter: value, selected: 0 };
            else if (sheet.current) sheet.type(value);
            else session.type(queryFor(value));
          }}
          {onkeydown}
          onclose={close}
        >
          {#snippet trailing()}
            <!-- A pill toggle (Controls): its words are the mode in force; ⌘D and the footer name the other. -->
            {#if scopeAnswers}
              <span class="tf-fchips tf-ln-scope">
                <button
                  type="button"
                  title={scope.titles ? "Titles and keys only. ⌘D searches everything" : "Bodies too. ⌘D searches titles only"}
                  onclick={() => {
                    toggleScope();
                    box?.focus();
                  }}>{scope.titles ? "Titles" : "Everything"}</button
                >
              </span>
            {/if}
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
              onpointermove={aimed(() => (suggestionPicked = index))}
            >
              <span class="tf-ln-nm">{suggestion.value}</span>
              <!-- The separator is an expression: Svelte trims an element's leading space (#1515). -->
              {#if suggestion.detail}<span class="tf-ln-s">{" · "}{suggestion.detail}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if interpreted && !overlay && !sheet.current}
        <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} hiddenKinds={seeded === null ? [] : seedKinds(seeded, session.syntax)} />
      {/if}

      {#if sheet.current}
        {@const current = sheet.current}
        <!-- The row the sheet acts on, lifted under the box; the box keeps the search words aside. -->
        <Pinned row={current.row} id={`${uid}-pinned`} {now} source={sourceOf(current.row)} />
        {#if current.renaming !== null}
          <p class="tf-ln-none">Type the new name in the box. Enter keeps it, Esc leaves the name as it was.</p>
        {:else if current.panel}
          <KeyPanel panel={current.panel} />
        {:else}
          <SheetList {sheet} id={sheetId} optionId={sheetOptionId} {onkeydown} onclick={() => box?.focus()} />
        {/if}
      {:else}
      <div class="tf-ln-split" class:tf-ln-two={split}>
      <div id={resultsId} class="tf-ln-list" class:tf-ln-palette={session.mode === "actions" && !overlay} role="listbox" tabindex="-1" aria-label="Results">
        {#if overlay}
          <Help
            rows={overlayRows}
            selected={overlay.selected}
            heading="Filters"
            keys={boxKeys}
            {legend}
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
              chordOf={listChord}
              {sourceOf}
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
            {legend}
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
                onpointermove={aimed(() => (session.selected = i))}
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
            chordOf={listChord}
            {sourceOf}
            within={seedLabel}
            filtered={narrowed(session.raw, session.response.interpreted.text)}
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
  </TfDialog>
{/if}
