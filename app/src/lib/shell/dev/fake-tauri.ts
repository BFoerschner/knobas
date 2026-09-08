/**
 * Dev-only: answers `invoke` out of a fixture so the whole frontend runs in a
 * plain browser.
 *
 * It is **never reached in a bundle**. The only import site is guarded by
 * `import.meta.env.DEV`, which Rollup constant-folds to `false` in a
 * production build and drops together with the branch and this module;
 * `house-rules.test.ts` fails if a second, unguarded importer appears.
 *
 * ## The QA convention (roadmap §3)
 *
 * Every task's QA step drives this. `PORT = 5300 + <your agent number>` and a
 * per-agent `--user-data-dir`, because parallel agents sharing either have
 * collided before:
 *
 * ```sh
 * cd app && npx vite --port $PORT --strictPort &
 * "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
 *   --headless --disable-gpu --window-size=1440,900 \
 *   --user-data-dir=/tmp/knobas-qa-$USER-$PORT \
 *   --screenshot=/tmp/knobas-qa-$PORT.png \
 *   "http://localhost:$PORT/?fake-ipc#/ctx/all"
 * # attach the PNG to the PR; kill the dev server afterwards.
 * ```
 *
 * **The dev server, not `vite preview`** — and this is a correction, because
 * the convention as originally written could not work. `vite preview` serves
 * the *production* build, where `import.meta.env.DEV` is `false`, this module
 * is dropped from the bundle and `?fake-ipc` therefore does nothing at all.
 * The symptom is a window stuck on "Starting the local database" for ever,
 * which reads like a bug in the boot screen rather than a QA command pointed
 * at the wrong server. (`grep -c fake-ipc app/dist/assets/*.js` is `0`, which
 * is exactly the property the guard above promises.)
 *
 * A `--screenshot` is taken at the load event, and the shell is not finished
 * booting then: it awaits a dynamic import and then polls `app_status`. For a
 * screen that has to be *settled* — anything reading `db_stats` or
 * `list_sync_runs` — drive Chrome over the DevTools Protocol and wait for the
 * `.boot` element to go, rather than trusting `--virtual-time-budget`.
 *
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state so
 * it can be photographed; without it the fake answers `ready` immediately.
 *
 * **This checks layout and interaction, not the bridge.** The real end-to-end
 * check is `just dev` (Tauri + embedded PostgreSQL) or `just demo`.
 */
import type {
  LinkEnd,
  LinkRow,
  NoteLinkInput,
  SuggestionEntry,
  SuggestionPage,
} from "../../ipc/entity";
import { JIRA_SCHEMA } from "../../sources/fixtures";
// The estate itself. See `FIXTURE_ESTATE` below for why it is read and not
// copied, and for the check that it reaches no production bundle.
import ESTATE_FILE from "../../../../../testenv/hetzner/estate.json";

/** One fake command. Arguments arrive camelCased, exactly as Tauri sends them. */
export type Handler = (args: Record<string, unknown>) => unknown;

/**
 * A registered event listener, as `@tauri-apps/api/event` sets one up.
 *
 * `listen()` is not a special entry point on the internals object: it is an
 * ordinary `invoke("plugin:event|listen", ..)` whose handler has been put
 * through `transformCallback`. So a fake bridge that only implements `invoke`
 * makes every `listen()` in the app hang for ever.
 */
interface Listener {
  event: string;
  callback: (payload: unknown) => void;
}

/** What a fake bridge hands back to whoever installed it. */
export interface FakeBridge {
  /** Deliver `payload` to everything listening to `event`. */
  emit(event: string, payload: unknown): void;
}

let nextCallbackId = 0;
let nextEventId = 0;

/**
 * Define `window.__TAURI_INTERNALS__` so `invoke` and `listen` resolve against
 * `handlers` instead of against a backend that is not there.
 */
export function installFakeTauri(handlers: Record<string, Handler>): FakeBridge {
  const callbacks = new Map<number, (payload: unknown) => void>();
  const listeners = new Map<number, Listener>();

  const invoke = async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd === "plugin:event|listen") {
      const id = ++nextEventId;
      const callback = callbacks.get(args["handler"] as number);
      if (callback) {
        listeners.set(id, { event: String(args["event"]), callback });
      }
      return id;
    }
    if (cmd === "plugin:event|unlisten") {
      listeners.delete(args["eventId"] as number);
      return null;
    }
    const handler = handlers[cmd];
    if (!handler) {
      // The same shape a real rejection has, so error handling under the fake
      // exercises the code that runs against the real bridge.
      throw { code: "internal", message: `fake-tauri: no handler for ${cmd}`, source_id: null };
    }
    return handler(args);
  };

  const internals = {
    invoke,
    transformCallback(callback: (payload: unknown) => void) {
      const id = ++nextCallbackId;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc: (path: string) => path,
  };

  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: internals,
    configurable: true,
    writable: true,
  });

  // `unlisten()` is not only the `plugin:event|unlisten` command: since
  // `@tauri-apps/api` 2.x it first calls this second internals object to drop
  // the listener on the JS side, and reaches the command only afterwards. A
  // fake that defined `invoke` alone made every listener teardown -- every
  // room switch, once the tray held a subscription -- reject with a
  // `TypeError` nothing awaited. Found by mounting the real tray over this
  // bridge under the rejection guard every suite runs (#237).
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
    value: {
      unregisterListener(_event: string, eventId: number) {
        listeners.delete(eventId);
      },
    },
    configurable: true,
    writable: true,
  });

  return {
    emit(event, payload) {
      for (const [id, listener] of listeners) {
        if (listener.event === event) {
          listener.callback({ event, id, payload });
        }
      }
    },
  };
}

/**
 * Install the demo bridge when the page was opened with `?fake-ipc`.
 *
 * A query flag rather than "install whenever there is no Tauri", because
 * `just dev` serves the app from the same Vite server the browser QA uses: an
 * automatic fallback would silently answer from the fixture the first time the
 * real backend was slow to attach.
 */
export function installIfRequested(): void {
  const params = new URLSearchParams(location.search);
  if (!params.has("fake-ipc")) return;
  seedClonesRoot(params);
  seedOpenState(params);
  seedCaptureShortcut(params);
  installFakeTauri(demoHandlers(params));
}

/**
 * The fixture's answers, one per command a phase-0 screen calls.
 *
 * Grows one handler per task, as each screen learns to call something new.
 */
export function demoHandlers(params = new URLSearchParams()): Record<string, Handler> {
  return {
    ping: () => "pong",
    app_status: () => ({ db: fakeDbState(params), ...DEMO_STATUS }),
    frontend_ready: () => null,
    retry_database: () => null,
    list_entities: (args) => listEntities(args),
    mini_board: (args) => miniBoard(args),
    get_entity: (args) => getEntity(args),
    resolve_url: (args) => resolveUrl(args),
    create_note: (args) => createNote(args),
    get_note: (args) => getNote(args),
    save_note: (args) => saveNote(args),
    recent_activity: (args) => recentActivity(args),

    // Checkouts (#499). The fixture has no filesystem, so what stands in for
    // the scan is `FAKE_CHECKOUTS` -- one repo with a path and one without --
    // and the override is a variable this file keeps. That is enough for the
    // panel, which branches on `found_by` and on whether `path` is there;
    // whether a directory really holds a clone is `knobas-core`'s question and
    // is answered by its own tests over a real temporary tree.
    clones_root: () => fakeClonesRoot,
    set_clones_root: (args) => {
      const path = args["path"];
      fakeClonesRoot = typeof path === "string" && path.trim() !== "" ? path.trim() : null;
      return null;
    },
    entity_checkout: (args) => fakeCheckout(String(args["entityId"] ?? "")),
    checkout_commands: () => fakeCommands(),
    set_checkout_command: (args) =>
      fakeSetCommand(String(args["action"] ?? ""), args["template"]),
    open_checkout: (args) => fakeOpen(String(args["action"] ?? "")),

    // The capture window (#503). Its own document (`capture.html?fake-ipc`),
    // so these five are what that page's walk answers from: the shortcut the
    // settings section draws, the pair the capture reads its links from, and
    // the button that would bring the note into the main window.
    capture_shortcut: () => fakeShortcut(),
    set_capture_shortcut: (args) => fakeSetShortcut(args["accelerator"]),
    record_capture_context: () => null,
    capture_context: () => FAKE_CAPTURE_CONTEXT,
    reveal_note: () => null,
    set_checkout_override: (args) => {
      const id = String(args["entityId"] ?? "");
      const repo = fakeRepoOf(id);
      const path = args["path"];
      if (repo === null) {
        throw { code: "not_found", message: `${id} has no repository in the mirror`, source_id: null };
      }
      if (typeof path === "string" && path.trim() !== "") {
        fakeOverrides[repo] = path.trim();
      } else {
        delete fakeOverrides[repo];
      }
      return fakeCheckout(id);
    },

    // The sources cockpit. Three sources, one of them refusing its credential
    // and one with a PAT running out, because those are the two rows a person
    // has to *do* something about and they are the ones a screenshot has to be
    // able to show.
    list_sources: () => FIXTURE_SOURCES,
    list_adapters: () => FIXTURE_ADAPTERS,
    credential_health: () => FIXTURE_SOURCES.map((source) => source.health),
    sync_status: () => FIXTURE_SOURCES.map((source) => fakeStatus(source)),
    list_sync_runs: (args) => fakeRuns(args),
    db_stats: () => FIXTURE_DB_STATS,
    sync_now: () => 91,
    sync_now_with_progress: () => 91,
    sync_all: () => FIXTURE_SOURCES.map((_, index) => 91 + index),
    reindex_fts: () => null,
    delete_source: () => null,

    // The write queue (issue #42). Three rows, and the mix is the point: one
    // held write, one refused, one merely waiting -- because the whole surface
    // exists to keep those three apart, and a fixture with one of them cannot
    // show that it does.
    pending_writes: () => FIXTURE_QUEUE,
    write_queue_counts: () => ({
      pending: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
      held: FIXTURE_QUEUE.filter((row) => row.state === "held").length,
      refused: FIXTURE_QUEUE.filter((row) => row.state === "refused").length,
    }),
    // Answered, never performed -- the same rule as the credential above, and
    // it matters more here: a fixture that appeared to *send* a held write
    // would be showing a reader the one outcome this feature exists to make
    // impossible without their say-so.
    flush_writes: () => null,
    apply_held_write: () => null,
    amend_write: () => null,
    discard_write: () => null,
    // Writes are answered, never performed: the fixture has no keychain and no
    // database, and a QA pass that appeared to save a credential would be the
    // most misleading thing in this file.
    set_source_secret: (args) => ({
      source_id: String(args["id"] ?? "mock"),
      state: "ok",
      checked_at: SYNCED_AT,
      detail: null,
      secret_expires_at: null,
    }),
    test_source: () => ({
      ok: true,
      account: "mara.oyelaran",
      server_version: "9.12.4",
      secret_expires_at: null,
      error: null,
      code: null,
      elapsed_ms: 214,
      // The connection note (#326): the Epic Link clause, which is the one
      // thing a real Jira says that nothing else on the report does. The
      // demo's "saved" source has no id configured, so it is the gap arm.
      detail: "Epic Link customfield_10101 found but not configured: epic membership is not mirrored",
      // What a real Jira reports about itself (#297): the Epic Link custom
      // field id, which the Add-source dialog puts in the empty field.
      discovered: { epic_link_field: "customfield_10101" },
    }),
    add_source: (args) => {
      const input = (args["input"] ?? {}) as Record<string, unknown>;
      return {
        ...FIXTURE_SOURCES[0]!,
        id: String(input["id"] ?? "new"),
        adapter_kind: String(input["adapter_kind"] ?? "mock"),
        display_name: String(input["display_name"] ?? "New source"),
        base_url: String(input["base_url"] ?? ""),
        // Echoed, not inherited from the spread fixture: the form submits an
        // `auth_kind` and the row it draws afterwards has to be the source the
        // person just described, or the QA harness disagrees with itself one
        // render apart.
        auth_kind: input["auth_kind"] ?? null,
        item_count: 0,
        last_run: null,
      };
    },
    demo_load: () => ({ source_id: "mock", upserted: 21, deleted: 0, swept: 0, cursor: "" }),
    complete_first_run: () => null,

    // Contexts (#47). Stateful within the session, so the new-context tab and
    // *Promote* can be walked in QA; membership is empty because the fixture
    // has no link graph, and `list_entities` answers a context scope with an
    // empty page for the same reason.
    list_projects: () => listProjects(),
    list_contexts: () => FAKE_CONTEXTS.slice(),
    context_members: () => [],
    create_context: (args) => {
      const row = fakeContext("adhoc", String(args["title"] ?? "Untitled"), null);
      FAKE_CONTEXTS.unshift(row);
      return row;
    },
    promote_context: (args) => {
      const anchor = String(args["entityId"] ?? "");
      const existing = FAKE_CONTEXTS.find((row) => row.anchor_id === anchor);
      if (existing) return existing;
      const entry = CORPUS.find((candidate) => candidate.entity_id === anchor);
      if (!entry) {
        throw { code: "not_found", message: `${anchor} is not in the mirror`, source_id: null };
      }
      const row = fakeContext("ticket", entry.title, anchor);
      FAKE_CONTEXTS.unshift(row);
      return row;
    },

    // Suggestions (#237). The tray awaits `detect_suggestions` and then
    // `room_suggestions` on every room, and it renders a rejection rather than
    // an empty state -- so a table without these keys was a standing red line
    // on every room under `?fake-ipc`. Stateful within the session like the
    // contexts above: a row answered here leaves every later read for as long
    // as the page lives. No event is emitted: the real command emits
    // `activity:new`, but the tray re-reads on its own after answering, and
    // the fixture emits nothing at all.
    //
    // A pass over an unchanged mirror writes nothing, and this mirror never
    // changes, so 0 is the honest constant. The tray discards the number.
    detect_suggestions: () => 0,
    room_suggestions: (args) => roomSuggestions(args),
    // Answered, never performed: there is no link graph to accept into and no
    // tombstone to remember a dismissal by. The two are one thing here -- the
    // row stops being listed -- because that is all a QA pass can witness.
    accept_suggestion: (args) => answerSuggestion(args),
    dismiss_suggestion: (args) => answerSuggestion(args),

    // The estate (#428, writable since #429). Stateful within the session,
    // like the contexts above: an asset created, renamed, moved or deleted
    // here stays that way for as long as the page lives, so the whole create /
    // edit / move walk can be driven in a browser. Nothing is persisted --
    // reload and the estate is the fixture again.
    asset_tree: (args) => assetColumn(args),
    get_asset: (args) => assetDetail(args),
    asset_types: () => ASSET_TYPES,
    create_asset: (args) => createAsset(args),
    edit_asset: (args) => editAsset(args),
    move_asset: (args) => moveAsset(args),
    delete_asset: (args) => deleteAsset(args),
    // A route's own read, for the `#/route/<id>` address a click on a route's
    // name opens (#432). The three writes are **not** here: creating a route
    // from the dialog would need a minted id and a history of its own, and
    // what this fixture exists for is the two reads a browser looks at.
    depends_on_this: (args) => dependsOnThis(args),
    get_route: (args) => routeDetail(args),
    // The Import (#439), as a **replay** and not as a second implementation of
    // the merge rule -- see `estatePreview`. It exists because the estate this
    // fixture draws *is* the file `--demo` imports (#440), so pressing Import
    // and choosing that file is the one gesture a reader will actually make
    // here, and until now it answered "command not found" in red.
    preview_estate_import: (args) => estatePreview(args),
    apply_estate_import: (args) => estateApply(args),
    produce_estate_file: (args) => estateProduce(args),
    // The room's Assets tile (#434). Empty for `context_members`' reason: the
    // fixture has no link graph, so no context holds anything and membership
    // -- assets included -- is honestly nothing. A stored room under
    // `?fake-ipc` therefore draws the tile's empty state rather than a red
    // "command not found".
    context_assets: () => [],
    // The Monitors tab (#448). Derived from the estate file's own monitor
    // names -- see `fakeMonitorRoster` -- so the roster a browser draws is the
    // set of monitors the file asks for, with a reading invented on top.
    monitor_roster: () => fakeMonitorRoster(),
    // The Monitors tab's Pause / Resume (#452). Answers the queued row the
    // real command answers, and remembers nothing: `?fake-ipc` has no Kuma
    // behind it, so the honest fixture is "knobas took the write" and a
    // roster that still says what it said. A handler is here at all because
    // the button beside it is -- a control that answers "command not found"
    // in red is worse than no control.
    submit_write: (args: Record<string, unknown>) => fakeQueuedWrite(args),
    // The estate's open alerts (#444), derived from the same roster so the
    // strip a browser draws agrees with the tab beside it. Added with #446
    // because #444 left it out and the two surfaces it feeds -- the Assets
    // view's strip and the top strip's badge -- were the only ones under
    // `?fake-ipc` answering "command not found" in red. **The inbox is a
    // different matter and still has no handlers here at all**, so the sixth
    // category's row cannot be seen this way whatever this answers.
    open_alerts: () => fakeOpenAlerts(),
    // The ack a card offers (#446's write, #449's button). Kept in a set here
    // rather than derived, because *acked* is the one fact on this surface a
    // reader makes themselves — a fixture that answered the same list after
    // the click would make the button look broken.
    ack_alert: (args) => fakeAck(args),
    // The Monitors tab's *Not monitored* roster (#449), derived from the same
    // file: an asset the estate names no Uptime Kuma monitor for is an asset
    // nothing watches, which is exactly what the backend's read answers.
    unmonitored_assets: () => fakeUnmonitored(),
    // The launcher board (#504). Until now `launcher_home` had no handler at
    // all, so `⌘K` under `?fake-ipc` drew a red "command not found" over the
    // whole board -- which is why the three estate lists this ticket adds
    // could not be looked at in a browser.
    //
    // **Everything here is fixture-only** and none of it certifies the bridge.
    // What it is for is the walk: that the rail draws a row per list with its
    // count, and that Enter on an estate row hands the shell the Tree's
    // address. The rules themselves are witnessed at the IPC seam over a real
    // PostgreSQL, in `crates/knobas-app/tests/search_ipc.rs`.
    launcher_home: () => ({
      smart_lists: fakeSmartLists(),
      // The newest of the fixture's corpus, in the shape `list_entities`
      // answers in -- the board draws recent rows with the same component.
      recent: CORPUS.filter((entry) => entry.deleted_at === null)
        // `recent_sql!`'s order, because the comment above has to be true:
        // newest first, a row the source never dated last, and the id to
        // break a tie. CORPUS is in reading order, not date order.
        .sort(
          (left, right) =>
            (right.updated_at ?? "").localeCompare(left.updated_at ?? "") ||
            left.entity_id.localeCompare(right.entity_id),
        )
        .slice(0, 8)
        .map(mirrorRow),
      sources: FIXTURE_SOURCES.map((source) => source.health),
      pending_writes: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
    }),
    smart_lists: () => fakeSmartLists(),
    smart_list_items: (args) => fakeSmartListItems(args),
    // *Save as list* and the two writes that keep the rail tidy (#506).
    // **Fixture-only**: the rows live in a module array, not in a database,
    // and what they certify is the panel -- the rail drawing a saved row
    // beside the built-ins, the rename field, the two-press delete. The table,
    // the counts, the badge and the grammar's verdict are witnessed at the IPC
    // seam over a real PostgreSQL, in `crates/knobas-app/tests/search_ipc.rs`.
    create_smart_list: (args) => fakeCreateSmartList(args),
    rename_smart_list: (args) => fakeRenameSmartList(args),
    delete_smart_list: (args) => fakeDeleteSmartList(args),
    // The Tree's search box (#430), and **only** the Tree's: a query that is
    // not narrowed to assets is refused rather than answered from the estate,
    // because the launcher's corpus is the mirror's and this fixture has no
    // mirror to search. A handler that answered everything with assets would
    // make a QA pass over the launcher look like it worked.
    search: (args) => estateSearch(args),
  };
}

// -- the estate -------------------------------------------------------------

/**
 * A corner of the built-in type table -- `asset_types`' answer here.
 *
 * The types the estate below uses and the ones they suggest, with the ids,
 * monograms, schemas and `suggests` lists `knobas_core::asset` declares.
 * **Not all sixteen**: this is a fixture a browser is pointed at, and nine
 * types is what it takes to see a *usual here* line and a typed-property
 * editor. The `suggests` lists are nonetheless the **whole** lists the real
 * table declares, so one of them names a type this subset omits (`container`
 * suggests `runtime`). That is on purpose: `typeChoices` drops ids the list it
 * is given does not carry, so a full copy costs nothing and a trimmed one
 * would be a second list disagreeing with the first -- which is the drift this
 * fixture already had once.
 *
 * It stays a copy where the estate below stopped being one (#440). The estate
 * is a **file**, and a file can be imported; the table is a `const` in Rust,
 * and the only way for TypeScript to have it is for something to write it
 * down. What holds this copy to the real one is
 * `the fixture answers the estate file, entry for entry` in
 * `fake-tauri.test.ts`: every type the file names has to be here, with the
 * monogram and the property schema the pane draws off it -- so a table that
 * drifted from the estate it serves fails rather than draws a `??` chip.
 *
 * It has to be declared **before** the estate, because the estate is built by
 * reading it at module load rather than written out beside it: a `const` read
 * during its own file's initialisation is a temporal-dead-zone error, not a
 * `undefined`.
 */
const ASSET_TYPES: {
  id: string;
  label: string;
  monogram: string;
  properties: { key: string; label: string; kind: "text" | "number" | "date" | "url" }[];
  suggests: string[];
}[] = [
  {
    id: "site",
    label: "Site",
    monogram: "SI",
    properties: [
      { key: "location", label: "Location", kind: "text" },
      { key: "provider", label: "Provider", kind: "text" },
    ],
    suggests: ["site", "hypervisor", "vm", "network"],
  },
  {
    id: "hypervisor",
    label: "Hypervisor",
    monogram: "HV",
    properties: [
      { key: "hostname", label: "Hostname", kind: "text" },
      { key: "ip", label: "IP", kind: "text" },
      { key: "os", label: "OS", kind: "text" },
    ],
    suggests: ["vm"],
  },
  {
    id: "vm",
    label: "VM",
    monogram: "VM",
    properties: [
      { key: "hostname", label: "Hostname", kind: "text" },
      { key: "ip", label: "IP", kind: "text" },
      { key: "os", label: "OS", kind: "text" },
      { key: "size", label: "Size", kind: "text" },
    ],
    suggests: ["container_engine", "service", "database_server", "reverse_proxy"],
  },
  {
    id: "container_engine",
    label: "Container engine",
    monogram: "CE",
    properties: [
      { key: "version", label: "Version", kind: "text" },
      { key: "socket", label: "Socket", kind: "text" },
    ],
    suggests: ["container"],
  },
  {
    id: "container",
    label: "Container",
    monogram: "CT",
    properties: [
      { key: "image", label: "Image", kind: "text" },
      { key: "ports", label: "Ports", kind: "text" },
      { key: "restart_policy", label: "Restart policy", kind: "text" },
    ],
    suggests: ["service", "database", "runtime"],
  },
  {
    id: "service",
    label: "Service",
    monogram: "SV",
    properties: [
      { key: "url", label: "URL", kind: "url" },
      { key: "port", label: "Port", kind: "number" },
      { key: "health_path", label: "Health path", kind: "text" },
    ],
    suggests: ["module"],
  },
  {
    id: "database_server",
    label: "Database server",
    monogram: "DS",
    properties: [
      { key: "engine", label: "Engine", kind: "text" },
      { key: "version", label: "Version", kind: "text" },
      { key: "host", label: "Host", kind: "text" },
      { key: "port", label: "Port", kind: "number" },
    ],
    suggests: ["database"],
  },
  {
    id: "database",
    label: "Database",
    monogram: "DB",
    properties: [
      { key: "engine", label: "Engine", kind: "text" },
      { key: "size_mb", label: "Size (MB)", kind: "number" },
    ],
    suggests: ["schema"],
  },
  { id: "custom", label: "Custom", monogram: "CU", properties: [], suggests: [] },
];

/**
 * One asset in the estate file, at the shape `assets::EstateFile` parses.
 *
 * The section below is the fixture's estate, and it is the **real** one, read
 * out of the file rather than copied into this one.
 *
 * `testenv/hetzner/estate.json` is the estate as provisioned (#438), and since
 * #440 it is the file `--demo` imports -- so a browser pointed at `?fake-ipc`
 * and a person looking at `just demo` are looking at the same twenty-three
 * assets and the same nine routes. This module's own docs have said since #428
 * that *"when that file lands, this can read it instead"*, and this is that.
 *
 * The hand-copied corner it replaces is why: five levels of the estate written
 * out by hand had already drifted into a machine the estate does not have
 * (`hel1`, a site holding two VMs) and a container typed as a VM. A copy of a
 * file that is checked in beside it earns nothing and goes stale, and the
 * shortening it needed -- routes re-pointed at the assets the copy stopped
 * short of -- is exactly the kind of quiet difference a QA screenshot then
 * reports as the app's behaviour.
 *
 * Vite resolves the import and Rollup drops it together with this module in a
 * production build, where `import.meta.env.DEV` is `false` and the only door
 * to this file is behind it (`house-rules.test.ts` holds that door). Checked:
 * `grep -c "knobas test estate" dist/assets/*.js` is `0`.
 *
 * What is **not** read off the file is a **status**, because the file carries
 * none and `assets::apply_import` writes none: every row here is `"none"`, and
 * the *N problems inside* badge is therefore absent -- which is what the demo
 * profile actually shows. #431's badge has its own fixtures in
 * `AssetsView.test.svelte.ts`.
 */
interface EstateFileAsset {
  id: string;
  type: string;
  name: string;
  parent?: string;
  environment?: string;
  owner?: string;
  description?: string;
  properties?: Record<string, string | number>;
  monitors?: string[];
}

/** One route in the file: what exposes it, and what it lands on. */
interface EstateFileRoute {
  id: string;
  asset: string;
  target?: string;
  name: string;
  url: string;
  description?: string;
  properties?: Record<string, string | number>;
}

/**
 * The file, at the shape `assets::EstateFile` parses it into.
 *
 * Declared here rather than inferred. TypeScript's inference over an imported
 * JSON array is a union of one object type per *distinct set of keys* the file
 * happens to use, so `entry.parent` is a compile error on the one asset that
 * has no parent and the union's shape changes the day somebody gives a second
 * asset an owner. The cast is the seam, and it is narrow on purpose: the
 * authority on this file is the Rust parser, and what holds the file to a
 * shape is `knobas-core`'s `tests/estate_file.rs` -- eleven tests over the
 * checked-in bytes, in `just check`.
 */
const ESTATE = ESTATE_FILE as unknown as {
  name: string;
  assets: EstateFileAsset[];
  routes: EstateFileRoute[];
};

/** One asset as this fixture holds it: the wire's row, plus its properties. */
interface FixtureAsset {
  id: string;
  parent_id: string | null;
  type_id: string;
  type_label: string;
  monogram: string;
  name: string;
  status: "up" | "warn" | "down" | "none";
  environment: "dev" | "stage" | "prod" | "shared" | null;
  owner: string | null;
  properties: { key: string; label: string; value: unknown; custom: boolean }[];
  /** The Uptime Kuma names an import kept on the asset (#439). */
  monitors: string[];
}

/** One route as this fixture holds it. */
interface FixtureRoute {
  id: string;
  asset_id: string;
  target_id: string | null;
  name: string;
  url: string;
  visibility: "internal" | "public";
  properties: { key: string; label: string; value: unknown; custom: boolean }[];
}

/**
 * The file's property bag: its `properties`, plus its `description`.
 *
 * `assets::bag_of`'s rule. A description has no column of its own in this
 * model, so where it goes is the property bag -- and every asset in the real
 * estate has one, which is why the pane's first custom row is `description`
 * on nearly every asset the demo draws.
 */
function bagOf(entry: {
  description?: string;
  properties?: Record<string, string | number>;
}): Record<string, string | number> {
  const bag: Record<string, string | number> = { ...(entry.properties ?? {}) };
  if (entry.description !== undefined) bag.description = entry.description;
  return bag;
}

/**
 * One plain scalar as the tagged value the pane draws.
 *
 * `assets::property_of`'s rule: a **declared** key takes the kind its type
 * declares, an **undeclared** one takes the kind JSON already gave it. A rule
 * and not a table, for this module's stated reason -- a fixture that answered
 * with values somebody typed out would draw the right words over a pane
 * reading the wrong field.
 */
function valueOf(kind: string | null, raw: string | number | undefined) {
  if (raw === undefined) return null;
  return { kind: kind ?? (typeof raw === "number" ? "number" : "text"), value: raw };
}

/** The bag's keys, as the pane's custom rows: by key, labelled by the key. */
function customPropertiesOf(bag: Record<string, string | number>) {
  return Object.keys(bag)
    .sort()
    .map((key) => ({ key, label: key, value: valueOf(null, bag[key]), custom: true }));
}

/**
 * `assets::properties_of`: the type's declared keys in the type's own order,
 * unfilled ones included, then whatever else the bag holds, by key.
 */
function propertiesOf(typeId: string, bag: Record<string, string | number>) {
  const declared = typeOf(typeId).properties;
  const claimed = new Set(declared.map((property) => property.key));
  const rest = Object.fromEntries(
    Object.entries(bag).filter(([key]) => !claimed.has(key)),
  );
  return [
    ...declared.map((property) => ({
      key: property.key,
      label: property.label,
      value: valueOf(property.kind, bag[property.key]),
      custom: false,
    })),
    ...customPropertiesOf(rest),
  ];
}

/**
 * The table's entry for `typeId`.
 *
 * A type the table does not carry falls back to its own id rather than
 * throwing: the fixture's table is a subset, and a file naming a type it omits
 * should draw an unfamiliar chip rather than a blank screen.
 */
function typeOf(typeId: string) {
  return (
    ASSET_TYPES.find((type) => type.id === typeId) ?? {
      id: typeId,
      label: typeId,
      monogram: "??",
      properties: [] as { key: string; label: string; kind: string }[],
      suggests: [] as string[],
    }
  );
}

/** The four environments `0017` accepts. Anything else is left unset. */
const ENVIRONMENTS = ["dev", "stage", "prod", "shared"] as const;

/** One of the file's assets as the fixture holds it. */
function assetFromFile(entry: EstateFileAsset): FixtureAsset {
  const declared = typeOf(entry.type);
  return {
    id: entry.id,
    parent_id: entry.parent ?? null,
    type_id: entry.type,
    type_label: declared.label,
    monogram: declared.monogram,
    name: entry.name,
    status: "none",
    environment: ENVIRONMENTS.find((name) => name === entry.environment) ?? null,
    owner: entry.owner ?? null,
    properties: propertiesOf(entry.type, bagOf(entry)),
    monitors: entry.monitors ?? [],
  };
}

/**
 * The estate the Tree draws: twenty-three assets, five levels at the deepest.
 *
 * Stateful within the session, like the contexts: an asset created, renamed,
 * moved or deleted here stays that way for as long as the page lives, so the
 * whole create / edit / move walk can be driven in a browser. Nothing is
 * persisted -- reload and the estate is the file again.
 */
const FIXTURE_ESTATE: FixtureAsset[] = ESTATE.assets.map(assetFromFile);

/**
 * The ids the file itself names.
 *
 * What it is for is the history: an asset that arrived by import carries an
 * origin line and an asset somebody made in this session does not, and after
 * a `create_asset` the two live in the same array.
 */
const FILE_ASSET_IDS = new Set(ESTATE.assets.map((entry) => entry.id));

/**
 * One of the file's routes as the fixture holds it.
 *
 * A route declares no schema, so every property is a custom row --
 * `assets::route_row_of` calls `custom_properties` and not `properties_of`.
 *
 * **The URL is the file's, verbatim, and one of the nine does not name a
 * loopback address.** `route:tunnel-gitea-reverse` is the reverse forward, and
 * the host in its URL is `gitea` -- the docker network name the TeamCity
 * containers resolve, which is why the estate spells it that way and why it
 * resolves nowhere else. The hand-copy this replaces substituted a loopback
 * address for it, citing `house-rules.test.ts`'s *no runtime network
 * references* rule, and dropping that substitution is a deliberate change
 * rather than an oversight.
 *
 * That rule scans `.ts` and `.svelte` under `app/src/` -- **raw text,
 * comments included, which is why this paragraph does not spell the URL with
 * its scheme** -- and it exists so that a *bundle* reaches nothing but the
 * IPC. The estate file is in no bundle: `import.meta.env.DEV` is the only door
 * to this module, and `grep -c "knobas test estate" dist/assets/*.js` is `0`.
 * More to the point, a route's URL is **data**. In the running app it arrives
 * over the bridge out of `knobas.route` and is whatever a person typed or an
 * import wrote, and this fixture stands in for that table -- so a harness that
 * quietly improved one URL would be drawing a screen the app does not draw.
 */
function routeFromFile(route: EstateFileRoute): FixtureRoute {
  return {
    id: route.id,
    asset_id: route.asset,
    target_id: route.target ?? null,
    name: route.name,
    url: route.url,
    visibility: "internal",
    properties: customPropertiesOf(bagOf(route)),
  };
}

/**
 * The nine routes the estate exposes (#432, drawn as wires by #433): the five
 * ports the notebook publishes, and the four forwards
 * `testenv/hetzner/tunnel` opens -- three `-L` and the one `-R` that runs the
 * other way, from a Hetzner server back to the notebook's Gitea.
 *
 * They are what makes the Tree's wires visible in a browser at all: a wire
 * runs from a route's row to the row (or the spine) of the asset at its far
 * end, so a fixture with no routes draws none, whatever the code does. Every
 * one of these lands on an asset three or four columns away, which is the case
 * a same-machine route could not photograph.
 */
const FIXTURE_ROUTES: FixtureRoute[] = ESTATE.routes.map(routeFromFile);

/** One route on the wire -- the exposing and target names read off the estate. */
function routeRow(route: FixtureRoute) {
  const named = (id: string | null) =>
    id === null ? null : (FIXTURE_ESTATE.find((asset) => asset.id === id)?.name ?? null);
  return {
    ...route,
    asset_name: named(route.asset_id) ?? route.asset_id,
    target_name: named(route.target_id),
  };
}

/**
 * `assets::ROUTES_REACHABLE`'s rule: every route whose target is on this
 * asset's containment path, above it or below.
 *
 * The **rule** and not one of its answers, like `inForce` below: a hard-coded
 * list would draw the right route on the wrong pane, and the pane is what a
 * QA walk is looking at.
 */
function routesReaching(asset: FixtureAsset, heldBy: FixtureAsset[]) {
  const under = new Set(descendants(asset).map((held) => held.id));
  const path = new Set([asset.id, ...heldBy.map((held) => held.id)]);
  return FIXTURE_ROUTES.filter(
    (route) => route.target_id !== null && (path.has(route.target_id) || under.has(route.target_id)),
  ).map(routeRow);
}

/** The two statuses `assets::ROLLUP` counts as a problem inside. */
const PROBLEM: FixtureAsset["status"][] = ["warn", "down"];

/** `AssetStatus::severity` -- down over warn over up over none, worst first. */
const SEVERITY: Record<FixtureAsset["status"], number> = { down: 0, warn: 1, up: 2, none: 3 };

/** Everything under `asset`, over the parent field. */
function descendants(asset: FixtureAsset): FixtureAsset[] {
  const held = FIXTURE_ESTATE.filter((other) => other.parent_id === asset.id);
  return held.flatMap((child) => [child, ...descendants(child)]);
}

/** The worst of a set of statuses, `"none"` for an empty one. */
function worst(statuses: FixtureAsset["status"][]): FixtureAsset["status"] {
  return statuses.reduce(
    (so_far, next) => (SEVERITY[next] < SEVERITY[so_far] ? next : so_far),
    "none",
  );
}

/** The activity lines this session's writes have appended, newest last. */
const ASSET_HISTORY: {
  entity_id: string;
  verb: string;
  detail: unknown;
  /** `"user"` unless the line was written by an import. */
  actor?: string;
}[] = [];

/** Ids for the assets a QA walk creates. `assets::create` mints a UUID. */
let mintedAssets = 0;

/** The stored asset, or a `not_found` in the shape `IpcError` puts on the wire. */
function assetOr404(id: string): (typeof FIXTURE_ESTATE)[number] {
  const found = FIXTURE_ESTATE.find((row) => row.id === id);
  if (!found) throw { code: "not_found", message: `no asset ${id}`, source_id: null };
  return found;
}

/**
 * `create_asset`: a new asset under `parentId`, with the type's schema drawn
 * as unfilled rows.
 *
 * The id is minted here, the way `assets::create` mints one: the caller names
 * the thing and never its address (story 18).
 */
function createAsset(args: Record<string, unknown>) {
  const typeId = args.typeId as string;
  const name = String(args.name ?? "").trim();
  if (name === "") throw { code: "invalid", message: "an asset needs a name", source_id: null };
  const declared = ASSET_TYPES.find((type) => type.id === typeId);
  if (!declared) {
    throw {
      code: "invalid",
      message: `"${typeId}" is not one of the built-in asset types`,
      source_id: null,
    };
  }
  mintedAssets += 1;
  const created = {
    id: `asset:new-${mintedAssets}`,
    parent_id: (args.parentId as string | null) ?? null,
    type_id: typeId,
    type_label: declared.label,
    monogram: declared.monogram,
    name,
    status: "none" as const,
    environment: null,
    owner: null,
    properties: declared.properties.map((property) => ({
      key: property.key,
      label: property.label,
      value: null,
      custom: false,
    })),
    // None, and that is the honest answer: only an import puts a monitor name
    // on an asset (#439), and creating one by hand is the other door.
    monitors: [],
  };
  FIXTURE_ESTATE.push(created);
  ASSET_HISTORY.push({ entity_id: created.id, verb: "created", detail: {} });
  return assetRow(created);
}

/** `edit_asset`: the two edits this fixture's surface can make. */
function editAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const edits = (args.edits ?? []) as Record<string, unknown>[];
  for (const edit of edits) {
    if (edit.field === "name") {
      const from = asset.name;
      asset.name = String(edit.value ?? "").trim();
      ASSET_HISTORY.push({
        entity_id: asset.id,
        verb: "renamed",
        detail: { field: "name", from, to: asset.name },
      });
      continue;
    }
    if (edit.field === "monitors") {
      // The append `assets::AssetEdit::Monitors` makes, duplicates dropped:
      // the name is what attaches the monitor, so a fixture that stored it
      // twice would draw two waiting entries for one check (#453).
      const added = ((edit.added ?? []) as string[])
        .map((name) => name.trim())
        .filter((name) => name !== "" && !asset.monitors.includes(name));
      if (added.length === 0) continue;
      asset.monitors.push(...added);
      ASSET_HISTORY.push({
        entity_id: asset.id,
        verb: "edited",
        detail: { field: "monitors", added },
      });
      continue;
    }
    if (edit.field !== "property") continue;
    const key = String(edit.key);
    const at = asset.properties.findIndex((property) => property.key === key);
    const from = at === -1 ? null : asset.properties[at]!.value;
    if (edit.value === null) {
      // A declared key keeps its row and loses its value; a custom key goes.
      if (at !== -1 && asset.properties[at]!.custom) asset.properties.splice(at, 1);
      else if (at !== -1) asset.properties[at]!.value = null;
    } else if (at === -1) {
      asset.properties.push({ key, label: key, value: edit.value, custom: true });
    } else {
      asset.properties[at]!.value = edit.value;
    }
    ASSET_HISTORY.push({
      entity_id: asset.id,
      verb: "edited",
      detail: { field: "property", key, from, to: edit.value ?? null },
    });
  }
  return assetRow(asset);
}

/**
 * `move_asset`, cycle refusal included.
 *
 * The refusal is in `assets::move_to`'s own words, naming both ends and the
 * asset that closes the loop -- so a QA walk that tries the move a person
 * would try sees the sentence the app really answers with.
 */
function moveAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const parentId = (args.newParentId as string | null) ?? null;
  if (parentId !== null) {
    let below: string | null = null;
    for (let walk: string | null = parentId; walk !== null; ) {
      const step = FIXTURE_ESTATE.find((row) => row.id === walk);
      if (!step) break;
      if (step.id === asset.id) {
        throw {
          code: "invalid",
          message:
            `moving "${asset.name}" under "${assetOr404(parentId).name}" would make a cycle: ` +
            `"${below ?? step.name}" is already held by "${asset.name}"`,
          source_id: null,
        };
      }
      below = step.name;
      walk = step.parent_id;
    }
  }
  asset.parent_id = parentId;
  ASSET_HISTORY.push({
    entity_id: asset.id,
    verb: "moved",
    detail: { field: "parent", to: parentId },
  });
  return assetRow(asset);
}

/** `delete_asset`: a leaf goes, a branch is refused by name. */
function deleteAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const held = FIXTURE_ESTATE.filter((row) => row.parent_id === asset.id).length;
  if (held > 0) {
    throw {
      code: "conflict",
      message: `"${asset.name}" still holds ${held} asset(s) -- move or delete them first`,
      source_id: null,
    };
  }
  // `assets::delete`'s two route rules (#432), because a QA walk that deleted
  // an exposer here and not in the app would be reading the wrong app: a route
  // still exposed refuses the delete, and a route still pointing at it is
  // cleared rather than left dangling.
  const exposed = FIXTURE_ROUTES.filter((route) => route.asset_id === asset.id).length;
  if (exposed > 0) {
    throw {
      code: "conflict",
      message: `"${asset.name}" still exposes ${exposed} route(s) -- delete them first`,
      source_id: null,
    };
  }
  for (const route of FIXTURE_ROUTES) {
    if (route.target_id === asset.id) route.target_id = null;
  }
  FIXTURE_ESTATE.splice(FIXTURE_ESTATE.indexOf(asset), 1);
  return null;
}

/**
 * One asset as a column row -- everything but its properties and its path,
 * plus the rollup `assets::ROLLUP` computes (#431).
 */
function assetRow(asset: FixtureAsset) {
  const { properties: _properties, ...row } = asset;
  const under = descendants(asset);
  const inside = worst(under.map((held) => held.status));
  return {
    ...row,
    has_children: FIXTURE_ESTATE.some((other) => other.parent_id === asset.id),
    health: worst([asset.status, inside]),
    inside,
    problems_inside: under.filter((held) => PROBLEM.includes(held.status)).length,
    // Zero for the same reason `links` below is empty: no link graph here, so
    // no asset is linked to any work and no row carries the badge (#435).
    linked_work: 0,
  };
}

/**
 * `assets::inherited`: the nearest asset at or above that sets a value.
 *
 * A copy of the store's **rule**, not of one of its answers, for this module's
 * stated reason: a fixture that returned "hel1" as the source because someone
 * typed it would draw the right words over a pane reading the wrong field.
 * Answering the way the store answers is what makes a `?fake-ipc` screenshot
 * evidence about the view.
 *
 * `heldBy` arrives outermost first, so the walk is the asset and then that
 * list reversed -- nearest ancestor first.
 */
function inForce<T>(
  asset: FixtureAsset,
  heldBy: FixtureAsset[],
  of: (row: FixtureAsset) => T | null,
) {
  for (const at of [asset, ...[...heldBy].reverse()]) {
    const value = of(at);
    if (value !== null) return { value, source_id: at.id, source_name: at.name };
  }
  return null;
}

/**
 * `asset_tree`: what one asset holds, or the top of the estate.
 *
 * `name` then `id`, which is `assets::CHILDREN`'s `order by a.name asc, a.id
 * asc`: without the tiebreak two siblings sharing a name would come back in
 * whichever order the array happened to hold them, and a QA screenshot would
 * disagree with the app for a reason nobody would look for.
 */
function assetColumn(args: Record<string, unknown>) {
  const parent = (args.parentId as string | null) ?? null;
  return FIXTURE_ESTATE.filter((asset) => asset.parent_id === parent)
    .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id))
    .map(assetRow);
}

/** `get_asset`: the pane's read, ancestors walked over the parent field. */
function assetDetail(args: Record<string, unknown>) {
  const id = args.assetId as string;
  const asset = FIXTURE_ESTATE.find((row) => row.id === id);
  if (!asset) throw { code: "not_found", message: `no asset ${id}`, source_id: null };

  const heldBy: FixtureAsset[] = [];
  let walk = asset.parent_id;
  while (walk !== null) {
    const held = FIXTURE_ESTATE.find((row) => row.id === walk);
    if (!held) break;
    heldBy.unshift(held);
    walk = held.parent_id;
  }

  return {
    asset: assetRow(asset),
    properties: asset.properties,
    effective_environment: inForce(asset, heldBy, (row) => row.environment),
    effective_owner: inForce(asset, heldBy, (row) => row.owner),
    held_by: heldBy.map(assetRow),
    holds: FIXTURE_ESTATE.filter((row) => row.parent_id === asset.id).map(assetRow),
    exposes: FIXTURE_ROUTES.filter((route) => route.asset_id === asset.id).map(routeRow),
    reachable_via: routesReaching(asset, heldBy),
    // Empty for `context_assets`' reason and the mirror detail's: this fixture
    // has no link graph, so an asset is linked to nothing and the pane draws
    // the links panel's empty state. Absent rather than empty (#435 landed
    // the field and the panel; the fixture kept neither) it is `undefined`,
    // and `groupLinks` throws on the pane of every asset.
    links: [],
    // The monitor names an import kept (#439), for `links`' reason exactly:
    // the pane reads `detail.monitors.length`, so a fixture short of the key
    // throws on the pane of every asset rather than drawing nothing.
    monitors: asset.monitors,
    // Nothing is *attached*, and that is the honest answer: an attachment is a
    // `monitored-by` link to a mirrored monitor, this fixture has no link
    // graph and no mirror, so every name the file gave is still a name (#445).
    // The pane therefore draws the waiting list and no *Monitoring* section.
    monitoring: [],
    // **One target**, so `?fake-ipc` can walk *Create monitor for this asset*
    // (#453). The fixture's Uptime Kuma is the one a reader configured with an
    // account -- the absent direction has its own witness in
    // `AssetsView.monitor.test.svelte.ts`, and a fixture that answered nothing
    // here would leave the control unreachable in the browser.
    monitor_targets: [
      { source_id: "kuma", display_name: "Uptime Kuma", entity: "kuma:monitors" },
    ],
    history: [
      // This session's own writes first, newest first, which is what makes
      // *every mutation appears in the pane's history immediately* (#429)
      // something a QA walk can see rather than take on trust.
      ...ASSET_HISTORY.filter((line) => line.entity_id === asset.id)
        .map((line, index) => ({ id: 1000 + index, at: SYNCED_AT, actor: "user", ...line }))
        .reverse(),
      // The line every asset **the file names** carries, and the only one: it
      // arrived by import, and `assets::insert_asset` writes no `created` line
      // beside the origin line (#439, story 23). The actor is `import` and not
      // `user`, which is the whole of `assets::HAND_EDITED`'s question -- a
      // fixture that said `user` here would draw a pane in which every
      // property is hand-edited and frozen against the next import. An asset
      // made in this session by hand has a `created` line of its own instead,
      // pushed by `createAsset`, and no origin line, because nothing imported
      // it.
      ...(FILE_ASSET_IDS.has(asset.id)
        ? [
            {
              id: 1,
              at: SYNCED_AT,
              actor: "import",
              verb: "imported",
              entity_id: asset.id,
              detail: { estate: ESTATE.name },
            },
          ]
        : []),
    ],
  };
}

/**
 * The `depends-on` and `runs-on` links this fixture pretends somebody drew.
 *
 * **Fixture-only.** The estate file is a tree and its routes and carries no
 * link at all, so without these three rows the *Depends on this* panel (#505)
 * would show containment on every asset in the browser and the two relation
 * words would be unwalkable. They exist nowhere else: no seed writes them, no
 * import produces them, and the panel's real witness is
 * `crates/knobas-app/tests/assets_ipc.rs` over a scratch PostgreSQL, where the
 * links are drawn by the test.
 *
 * `from` depends on / runs on `to`, which is the direction the read walks:
 * what breaks when `to` goes down is `from`.
 */
const FIXTURE_DEPENDENCIES: { from: string; to: string; relation: string }[] = [
  // TeamCity's VCS roots are the Gitea repositories it reaches through
  // `route:tunnel-gitea-reverse`, which is in the file.
  { from: "asset:hetzner-teamcity", to: "asset:knobas-gitea", relation: "depends-on" },
  // An agent is no use without the server it registers with.
  { from: "asset:knobas-teamcity-agent", to: "asset:knobas-teamcity", relation: "depends-on" },
  // The mockd container answers the loopback ports the laptop's suites use.
  { from: "asset:notebook", to: "asset:knobas-mockd", relation: "runs-on" },
];

/**
 * `depends_on_this`: what breaks if this asset goes down (#505).
 *
 * `assets::depends_on_this`' **rule** and not one of its answers, the
 * discipline `inForce` above states: breadth-first over containment and over
 * {@link FIXTURE_DEPENDENCIES}, with a visited set, so a cycle drawn into that
 * list would walk once here as it does in the store. The relation is `null`
 * for a containment step, which the pane reads as *inside*.
 *
 * The routes are the ones landing on **this asset itself**, listed apart and
 * not counted -- a narrower read than `reachable_via` above, which takes the
 * whole containment path.
 */
function dependsOnThis(args: Record<string, unknown>) {
  const id = args.assetId as string;
  if (!FIXTURE_ESTATE.some((row) => row.id === id)) {
    throw { code: "not_found", message: `no asset ${id}`, source_id: null };
  }

  const seen = new Set([id]);
  let frontier = [id];
  const assets: { asset: unknown; path: string | null; relation: string | null }[] = [];
  while (frontier.length > 0) {
    const reached = new Map<string, string | null>();
    /**
     * The store's `distinct on (dst) ... order by dst, relation asc nulls
     * first`, over the **whole** step and not over one frontier entry:
     * containment beats a link, and two links between the same pair settle by
     * the relation's own text. Written out rather than first-writer-wins,
     * because the order the frontier happens to be in is not a rule.
     */
    const claim = (dst: string, relation: string | null) => {
      if (!reached.has(dst)) return void reached.set(dst, relation);
      const held = reached.get(dst) ?? null;
      if (held === null || relation === null) return void reached.set(dst, null);
      if (relation < held) reached.set(dst, relation);
    };
    for (const from of frontier) {
      for (const held of FIXTURE_ESTATE.filter((row) => row.parent_id === from)) claim(held.id, null);
      for (const edge of FIXTURE_DEPENDENCIES.filter((link) => link.to === from)) {
        claim(edge.from, edge.relation);
      }
    }
    const layer = [...reached]
      .filter(([reachedId]) => !seen.has(reachedId))
      .map(([reachedId, relation]) => ({
        // Non-null because every id in `reached` came out of `FIXTURE_ESTATE`
        // or out of a link this file wrote between two of its assets.
        asset: FIXTURE_ESTATE.find((row) => row.id === reachedId)!,
        relation,
      }))
      .sort((left, right) => left.asset.name.localeCompare(right.asset.name));
    for (const line of layer) seen.add(line.asset.id);
    for (const line of layer) {
      assets.push({
        asset: assetRow(line.asset),
        path: assetPathText(line.asset),
        relation: line.relation,
      });
    }
    frontier = layer.map((line) => line.asset.id);
  }

  return {
    assets,
    routes: FIXTURE_ROUTES.filter((route) => route.target_id === id).map(routeRow),
  };
}

/** `get_route`: one route and its history, which this fixture has none of. */
function routeDetail(args: Record<string, unknown>) {
  const id = args.routeId as string;
  const route = FIXTURE_ROUTES.find((candidate) => candidate.id === id);
  if (!route) throw { code: "not_found", message: `no route ${id}`, source_id: null };
  // Empty rather than invented: every line in a route's history is written by
  // a write, and this fixture answers none of the three that write one.
  return { route: routeRow(route), history: [] };
}

/**
 * The file an import was handed, or a refusal.
 *
 * **Two files and no others** since #509: the estate this fixture was built
 * from, and whatever its own importer last produced ({@link PRODUCED_FILE},
 * matched as text). The parse is `JSON.parse` and a shape check and nothing
 * else, where `assets::plan` is a versioned parser with a closed key
 * vocabulary, an id namespace check, a cycle check and a property-kind rule.
 * Re-implementing any of that here would be a second answer to *is this file
 * legal*, and the second answer is the one that goes stale -- so anything else
 * is refused by name.
 */
function estateFileOf(args: Record<string, unknown>) {
  let parsed: unknown;
  try {
    parsed = JSON.parse(String(args.file ?? ""));
  } catch {
    throw { code: "invalid", message: "that file is not JSON", source_id: null };
  }
  const file = parsed as { name?: string; assets?: EstateFileAsset[]; routes?: EstateFileRoute[] };
  const ours = file.name === ESTATE.name || String(args.file ?? "") === PRODUCED_FILE;
  if (!ours || !Array.isArray(file.assets) || !Array.isArray(file.routes)) {
    throw {
      code: "invalid",
      message:
        `this harness replays two imports — "${ESTATE.name}", the estate file it ` +
        `draws its own Tree from, and the one its own importer just produced. ` +
        `Any other file is the real command's to parse.`,
      source_id: null,
    };
  }
  return { assets: file.assets, routes: file.routes };
}

/** The file's entries as the dialog's rows, assets before routes. */
function importEntries(file: { assets: EstateFileAsset[]; routes: EstateFileRoute[] }) {
  return [
    ...file.assets.map((entry) => ({
      id: entry.id,
      kind: "asset",
      name: entry.name,
      type_label: typeOf(entry.type).label,
      parent_id: entry.parent ?? null,
    })),
    ...file.routes.map((route) => ({
      id: route.id,
      kind: "route",
      name: route.name,
      type_label: null,
      parent_id: route.asset,
    })),
  ];
}

/**
 * `produce_estate_file`: the hcloud importer's three answers, without hcloud
 * (#509).
 *
 * **A state machine and not a recording**, because what a walk through this
 * harness can certify is the *dialog*: that the token is asked for once, that
 * *land under* is asked only when a new server exists, and that the file comes
 * back and is offered. So this fixture holds one flag -- whether a token has
 * been given -- and one server the estate does not hold, which is enough to
 * make every branch reachable by hand and none of them reachable twice.
 *
 * What it says nothing about is Hetzner: there is no API here, no origin key
 * and no match. `crates/knobas-app/tests/assets_ipc.rs` witnesses the shape
 * against a recording and `just estate-live` witnesses the real system, which
 * is the split stated in `assets::hcloud`'s own header.
 */
let IMPORTER_TOKEN: string | null = null;

/** The server this fixture's hcloud holds and the estate does not. */
const IMPORTER_NEW_SERVER = "knobas-scratch";

/**
 * The exact text the last produce answered with.
 *
 * Held so that {@link estateFileOf} can accept it: that function's rule is
 * *one file and no other*, and since #509 this harness has two of its own --
 * the estate it draws its Tree from, and the file its own importer just made.
 * Compared as **text** rather than by a name or a shape, so widening the rule
 * did not turn it into a second parser.
 */
let PRODUCED_FILE: string | null = null;

function estateProduce(args: Record<string, unknown>) {
  const token = typeof args.token === "string" ? args.token.trim() : "";
  if (token !== "") IMPORTER_TOKEN = token;
  if (IMPORTER_TOKEN === null) return { state: "token_needed" };

  // **The argument being absent is the question; a `Landing` is an answer, and
  // `{ parent: null }` is the top of the estate** (`assets::Landing`, #509).
  // Read the way the backend reads it, because reading `{ parent: null }` as
  // *nothing said yet* is exactly the defect the second ruling of 2026-09-08
  // was raised on, and a fixture that repeated it would let the walk go green
  // over it.
  const landing = (args.landUnder ?? null) as { parent?: string | null } | null;
  const held = FIXTURE_ESTATE.some((asset) => asset.name === IMPORTER_NEW_SERVER);
  if (!held && landing === null) {
    return { state: "landing_needed", servers: [IMPORTER_NEW_SERVER] };
  }
  const entry: Record<string, unknown> = {
    id: "asset:hcloud-164750999",
    type: "vm",
    name: IMPORTER_NEW_SERVER,
    properties: {
      hcloud_id: "164750999",
      server_type: "cx23",
      os: "ubuntu-24.04",
      location: "fsn1",
      ip: "203.0.113.9",
    },
  };
  // No `parent` key at all for the top, which is what `FileAsset::parent` reads
  // as an asset at the top of the estate.
  const parent = landing?.parent ?? null;
  if (!held && parent !== null) entry.parent = parent;
  PRODUCED_FILE = JSON.stringify(
    { version: 1, name: "Hetzner Cloud", assets: [entry], routes: [] },
    null,
    2,
  );
  return {
    state: "ready",
    file: PRODUCED_FILE,
    new_servers: held ? [] : [IMPORTER_NEW_SERVER],
  };
}

/**
 * `preview_estate_import`: what an import of this file would do here.
 *
 * **A replay of one recorded answer, and it says so.** Against the estate as
 * loaded, every one of the file's thirty-two entries is already in the tree —
 * which is criterion 3 of #440 and the state a demo profile is in after its
 * first start. Delete an asset in the harness and the entry moves to *new*,
 * because that much is a set membership and not a rule.
 *
 * **`changes` is always empty, and that is the honest answer here rather than
 * a shortcut.** What belongs in it is decided by `assets::HAND_EDITED` —
 * *"a hand edit is any activity line by the user on that property"* — over
 * `knobas.activity`. This fixture has no activity table and no actor on a
 * property, so it cannot tell a value the file wrote from a value a person
 * typed, and a group composed by guessing would put the dialog's most
 * consequential sentence in front of a reader with nothing behind it. What
 * answers that group is `crates/knobas-app/tests/assets_ipc.rs`, over a real
 * PostgreSQL, and the dialog's rendering of all three groups is
 * `AssetsView.import.test.svelte.ts`.
 *
 * **`args.producer` is read by nothing here, and that is deliberate** (#508).
 * The producer decides the **origin key** — the second matching rule, used
 * when a file entry's id is not one the tree holds — and honouring it would be
 * a second implementation of `assets::matched_by_origin_key` living in a
 * fixture, which is the copy that goes stale. So this replay matches by id
 * alone, whichever producer the chooser sent, and what witnesses the rule is
 * the IPC-seam suite over a real PostgreSQL. A walk through this harness
 * certifies the chooser's layout and never the rule behind it.
 */
function estatePreview(args: Record<string, unknown>) {
  const file = estateFileOf(args);
  const held = (id: string) =>
    FIXTURE_ESTATE.some((asset) => asset.id === id) ||
    FIXTURE_ROUTES.some((route) => route.id === id);
  const entries = importEntries(file);
  return {
    name: ESTATE.name,
    known: entries.filter((entry) => held(entry.id)),
    new: entries.filter((entry) => !held(entry.id)),
    changes: [],
    // No monitor in this fixture answers to any name, because there is no
    // mirror here and no Kuma: every name the file gives is therefore
    // unresolved, and the dialog draws that group rather than an empty
    // *Monitors to attach* (#445). Composed from the file rather than
    // recorded, so deleting an asset in the harness moves its names with it.
    monitor_links: [],
    unresolved: file.assets.flatMap((entry) =>
      (entry.monitors ?? []).map((monitor_name) => ({
        asset_id: entry.id,
        asset_name: entry.name,
        monitor_name,
      })),
    ),
  };
}

/**
 * `apply_estate_import`: create what the preview called new, and nothing else.
 *
 * The plan is the preview's, which is the property `assets::apply_import`
 * makes structural -- an entry the preview did not list cannot be written. On
 * an untouched estate that is nothing at all, and the dialog reports six
 * zeroes; after a delete it is the row coming back, with the origin line the
 * real import writes.
 */
function estateApply(args: Record<string, unknown>) {
  const file = estateFileOf(args);
  const wanted = new Set(estatePreview(args).new.map((entry) => entry.id));
  const outcome = {
    assets_created: 0,
    routes_created: 0,
    properties_set: 0,
    properties_kept: 0,
    monitors_kept: 0,
    monitors_linked: 0,
  };
  for (const entry of file.assets) {
    if (!wanted.has(entry.id)) continue;
    FIXTURE_ESTATE.push(assetFromFile(entry));
    ASSET_HISTORY.push({
      entity_id: entry.id,
      verb: "imported",
      detail: { estate: ESTATE.name },
      actor: "import",
    });
    outcome.assets_created += 1;
    outcome.monitors_kept += (entry.monitors ?? []).length;
  }
  for (const route of file.routes) {
    if (!wanted.has(route.id)) continue;
    FIXTURE_ROUTES.push(routeFromFile(route));
    outcome.routes_created += 1;
  }
  return outcome;
}

/**
 * `search`, narrowed to the estate — what `assets/tree.ts`'s `estateQuery`
 * asks for.
 *
 * A **substring match over names**, where the real corpus is PostgreSQL FTS
 * over the name and the ancestor path with `corpus::ASSET`'s weights. So this
 * answers the same *shape* and a different question, which is all a browser
 * pass needs: what it certifies is that the box draws its offers and that
 * taking one opens the columns. What certifies the query itself is
 * `crates/knobas-app/tests/search_ipc.rs`, against a real database.
 */
function estateSearch(args: Record<string, unknown>) {
  const query = (args.query ?? {}) as {
    raw?: string;
    filters?: { kinds?: string[] };
  };
  // `list:<id>` routes to the list and not to the corpus, which is what
  // `Searcher::search` does with a `Prefix::List` -- and it is the only way a
  // smart list is ever opened, because `Launcher.svelte` opens one by typing
  // its id into the box. Without this branch the rail draws but every row on
  // it answers with the refusal below.
  const listed = /^list:([a-z0-9-]+)$/.exec((query.raw ?? "").trim());
  if (listed) return fakeSmartListItems({ id: listed[1]!, limit: 50 });
  if (!(query.filters?.kinds ?? []).includes("asset")) {
    throw {
      code: "not_ready",
      message: "the fixture answers the Tree's asset search only (#430) — there is no mirror here",
      source_id: null,
    };
  }
  const needle = (query.raw ?? "").trim().toLowerCase();
  const hits = FIXTURE_ESTATE.filter(
    (asset) => needle !== "" && asset.name.toLowerCase().includes(needle),
  ).map((asset, index) => ({
    entity_id: asset.id,
    kind: "asset",
    source_id: "asset",
    updated_at: null,
    synced_at: SYNCED_AT,
    title: asset.name,
    path: assetPathText(asset),
    rank: 1 - index / 10,
    snippet: [],
  }));
  return {
    interpreted: {
      text: query.raw ?? "",
      prefix: null,
      filters: { sources: [], kinds: ["asset"], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups:
      hits.length === 0
        ? []
        : [
            {
              kind: "asset",
              label: "Asset",
              plural: "Assets",
              monogram: "AS",
              total: hits.length,
              hits,
            },
          ],
    total: hits.length,
    took_ms: 2,
    coverage: [],
  };
}

/** `knobas.asset.path_text`: the ancestors' names, outermost first. */
function assetPathText(asset: (typeof FIXTURE_ESTATE)[number]): string | null {
  const names: string[] = [];
  let walk = asset.parent_id;
  while (walk !== null) {
    const held = FIXTURE_ESTATE.find((row) => row.id === walk);
    if (!held) break;
    names.unshift(held.name);
    walk = held.parent_id;
  }
  return names.length === 0 ? null : names.join(" / ");
}

// -- the demo corpus --------------------------------------------------------

/**
 * Where the fixture pretends its items live.
 *
 * **Loopback, and not the mock adapter's own `tidewater.example` host.**
 * `house-rules.test.ts` refuses any non-loopback URL anywhere under `app/src/`
 * — including one that is only ever data, and including one that appears only
 * in a comment, since that rule scans raw text rather than stripping prose.
 * Both halves of that are right as they stand: a remote host named in the
 * frontend is a fetch waiting to happen, the rule fails *closed*, and
 * loosening a working lint so a fixture could spell a prettier hostname is
 * exactly how a lint stops catching things. Nothing serves this address, which
 * is the honest state of affairs: *Open in browser* under `?fake-ipc` proves
 * the button and its scheme guard, not the page at the other end.
 */
const FIXTURE_HOST = "http://127.0.0.1:5399";

/** When the fixture claims it last synced. Fixed, so screenshots are stable. */
const SYNCED_AT = "2026-08-22T14:30:00Z";

/**
 * A slice of the Tidewater dataset, plus one kind nothing declares.
 *
 * Enough to fill the mockup's tiles and no more: this exists so a browser can
 * be pointed at the shell, not so the fixture can stand in for PostgreSQL. The
 * `incident` row is the §3a case — an *undeclared* kind, which must get its
 * own tile and the generic detail view rather than being dropped.
 */
const CORPUS: {
  entity_id: string;
  kind: string;
  title: string;
  updated_at: string | null;
  body_text: string;
  author: string | null;
  web_url: string | null;
  deleted_at: string | null;
  payload: Record<string, unknown>;
}[] = [
  row("mock:PAY-231", "ticket", "Retry failed SEPA payouts", "2026-08-22T11:48:00Z", "mara", {
    key: "PAY-231",
    // The shape a *source* writes a project in, not a shape of this file's
    // choosing: the mock adapter puts it at `fields.project` because Jira does
    // (#207), and `project_key_read!` reads it there. A flat `project` here
    // would make the fixture's rooms disagree with the real backend's.
    fields: { project: { key: "PAY", name: "Payments Platform" } },
    status: "In Progress",
    priority: "High",
    assignee: "mara",
    story_points: 3,
    blocked: false,
    description:
      "Payouts to two SEPA banks fail with a 409 on retry. The retry window has to be idempotent before we can turn the scheduler back on.",
    comments: [{ by: "mara" }, { by: "jonas" }],
  }),
  // Deliberately in **no** project, key prefix notwithstanding: the miss
  // direction ADR-0010 fixes, and the one a QA pass has to be able to see. It
  // stays in *All work* and in the mock source's room, and appears in neither
  // project room.
  row("mock:PAY-228", "ticket", "Idempotency key on the payout endpoint", "2026-08-21T16:05:00Z", "jonas", {
    key: "PAY-228",
    status: "In Review",
    priority: "Normal",
    assignee: "jonas",
  }),
  row("mock:OPS-77", "ticket", "Rotate the staging database credentials", "2026-08-19T09:00:00Z", "priya", {
    key: "OPS-77",
    fields: { project: { key: "OPS", name: "Operations" } },
    status: "To Do",
    priority: "Low",
    assignee: null,
  }),
  // The repo and one of its branches (#499). Both are here so a QA pass can
  // see the two checkout states the panel exists to keep apart: this repo has
  // a clone under the fixture's clones root, and `mock:ledger` deliberately
  // does not -- so one detail shows a path and the other shows the clone
  // command to copy. The keys follow interfaces §4.2's Gitea grammar
  // (`owner/repo`, `owner/repo@refs/heads/<name>`), which is what the real
  // backend resolves a branch's repository by.
  {
    ...row("mock:tidewater/payout-service", "repo", "tidewater/payout-service", "2026-08-22T10:30:00Z", "mara", {
      full_name: "tidewater/payout-service",
      description: "Payout scheduling and SEPA retries.",
    }),
    // A forge-shaped URL rather than the fixture's default: it is what the
    // clone command is built from, and a person reading the panel should see
    // the command they would actually run.
    web_url: "https://gitea.example.com/tidewater/payout-service",
  },
  row(
    "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry",
    "branch",
    "feat/PAY-231-idempotent-retry",
    "2026-08-22T09:40:00Z",
    "mara",
    { name: "feat/PAY-231-idempotent-retry", repository: "tidewater/payout-service" },
  ),
  // A second repo, with no clone on this fixture's disk: the *no checkout*
  // arm, and the one that shows the clone command.
  {
    ...row("mock:tidewater/ledger", "repo", "tidewater/ledger", "2026-08-18T12:00:00Z", "jonas", {
      full_name: "tidewater/ledger",
      description: "The ledger service.",
    }),
    web_url: "https://gitea.example.com/tidewater/ledger",
  },
  row("mock:payout-service#142", "pr", "Idempotent retry window", "2026-08-22T10:12:00Z", "mara", {
    num: 142,
    repo: "payout-service",
    state: "open",
    approvals: "1/2",
  }),
  // The commit and the build carry the ticket's key -- in the branch name and
  // in the message body -- because that is the evidence `FAKE_SUGGESTIONS`
  // below cites, and a proposal whose reason the corpus cannot bear out is a
  // fixture lying about the one thing the tray exists to show.
  row("mock:9f2c1ab", "commit", "payout: key the retry on the mandate id", "2026-08-22T09:40:00Z", "mara", {
    sha: "9f2c1ab",
    repo: "payout-service",
    branch: "feat/PAY-231-idempotent-retry",
    description: "A replayed payout with the same mandate id is now a no-op.\n\nRefs PAY-231.",
  }),
  row("mock:payout-service#318", "build", "payout-service #318", "2026-08-22T10:20:00Z", null, {
    cfg: "payout-service",
    num: 318,
    status: "failed",
    branch: "feat/PAY-231-idempotent-retry",
    // The build's parameters as a build source mirrors them into `body_text`
    // (`knobas_source_teamcity::map::build_item`): the branch, the status, the
    // trigger. That blob is what `build_parameter_key` reads.
    description: "branch feat/PAY-231-idempotent-retry, status failed, triggered by a push",
  }),
  // The page shares enough of PAY-231's vocabulary to clear `similar_text`'s
  // floor of five stems, so the `similarity` proposal below is one the real
  // detector would make of this pair rather than a badge painted on for QA.
  row("mock:ENG-SEPA", "page", "SEPA retry design", "2026-08-20T15:30:00Z", "priya", {
    space: "ENG",
    id: "ENG-SEPA",
    section: "Payments",
    description:
      "How the payout scheduler retries failed SEPA payouts: the retry window, the 409 on a replayed payout, and the mandate id as the idempotency key.",
  }),
  // The §3a case: no adapter declares this kind, and it still has to work.
  row("mock:INC-1", "incident", "Payout queue backed up for 40 minutes", "2026-08-22T08:05:00Z", "priya", {
    id: "INC-1",
    severity: "SEV2",
    resolved: true,
    affected_services: ["payout-service", "ledger"],
  }),
  {
    ...row("mock:PAY-198", "ticket", "Legacy payout reconciliation (withdrawn)", null, null, {
      key: "PAY-198",
      status: "Deleted",
    }),
    web_url: null,
    deleted_at: "2026-08-22T12:00:00Z",
  },
];

/** One corpus entry, with the fields every row shares filled in. */
function row(
  entity_id: string,
  kind: string,
  title: string,
  updated_at: string | null,
  author: string | null,
  payload: Record<string, unknown>,
) {
  return {
    entity_id,
    kind,
    title,
    updated_at,
    author,
    body_text: String(payload["description"] ?? title),
    web_url: `${FIXTURE_HOST}/browse/${entity_id.slice(entity_id.indexOf(":") + 1)}`,
    deleted_at: null as string | null,
    payload,
  };
}

/**
 * What a detection pass would propose over this corpus, in the detector's own
 * words (`knobas_core::suggest::RULES`), so the tray reads under `?fake-ipc`
 * as it reads over a real mirror.
 *
 * Both classes, because the badge exists to tell them apart (#41 story 16):
 * two `exact_key` rows from the records that carry PAY-231's key, and one
 * `similarity` guess from the page that shares the ticket's vocabulary. The
 * similarity row runs page → ticket because `similar_text` orders a pair by
 * entity id, and `ENG-SEPA` sorts before `PAY-231`; its reason names the
 * first four shared words the way the rule does, as words here rather than
 * as the stems the rule actually emits. Every end is an entity the
 * corpus holds, so each is openable -- `linkEnd` throws otherwise, at read
 * time, which is where a dangling fixture row would be noticed.
 */
const FAKE_SUGGESTIONS: LinkRow[] = [
  proposal(1, "mock:payout-service#318", "mock:PAY-231", "build_parameter_key", "exact_key",
    "this build's parameters name PAY-231", "2026-08-22T14:30:03Z"),
  proposal(2, "mock:9f2c1ab", "mock:PAY-231", "commit_message_key", "exact_key",
    "the commit message mentions PAY-231", "2026-08-22T14:30:02Z"),
  proposal(3, "mock:ENG-SEPA", "mock:PAY-231", "similar_text", "similarity",
    "both mention 409, fail, idempotent, payout", "2026-08-22T14:30:01Z"),
];

/** The proposals answered this session, by link id. Accepted or dismissed is
 * one fact to the fixture, because leaving the list is all either does here. */
const ANSWERED = new Set<string>();

/** One unconfirmed link, as the detector writes one. */
function proposal(
  n: number,
  from_id: string,
  to_id: string,
  rule: string,
  rule_class: Exclude<LinkRow["rule_class"], "source_relation" | null>,
  reason: string,
  created_at: string,
): LinkRow {
  return {
    id: `link:fake-${n}`,
    from_id,
    to_id,
    relation: "related",
    origin: "suggested",
    note: null,
    created_by: "knobas",
    created_at,
    confirmed_at: null,
    rule,
    rule_class,
    reason,
  };
}

/** The end of a proposal, hydrated from the corpus the way the real read joins it. */
function linkEnd(entity_id: string): LinkEnd {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entity_id);
  if (!entry) throw new Error(`fake-tauri: a proposal names ${entity_id}, which the corpus does not hold`);
  return { entity_id, kind: entry.kind, title: entry.title, deleted_at: entry.deleted_at };
}

/**
 * `room_suggestions`: the proposals still waiting, newest first, scoped like
 * `list_entities`. `total` is the count before `limit`, never `rows.length`:
 * it is the number the tray's heading shows.
 */
function roomSuggestions(args: Record<string, unknown>): SuggestionPage {
  // The fixture has no link graph, so a stored context's room has nothing to
  // propose -- the same answer `context_members` and `list_entities` give.
  if (args["ctx"]) return { rows: [], total: 0 };
  const sources = (args["sources"] as string[] | undefined) ?? [];
  if (sources.length && !sources.includes("mock")) return { rows: [], total: 0 };
  const limit = Number(args["limit"] ?? 50);

  const waiting = FAKE_SUGGESTIONS.filter((link) => !ANSWERED.has(link.id)).sort((a, b) =>
    b.created_at.localeCompare(a.created_at),
  );
  const rows: SuggestionEntry[] = waiting
    .slice(0, limit)
    .map((link) => ({ link, from: linkEnd(link.from_id), to: linkEnd(link.to_id) }));
  return { rows, total: waiting.length };
}

/**
 * `accept_suggestion` and `dismiss_suggestion`. Idempotent, as the real ones
 * are: answering a row twice resolves and changes nothing. An id no proposal
 * carries is refused the way the real command refuses it.
 */
function answerSuggestion(args: Record<string, unknown>): null {
  const id = String(args["linkId"] ?? "");
  if (!FAKE_SUGGESTIONS.some((link) => link.id === id)) {
    throw { code: "not_found", message: `${id} is not a link`, source_id: null };
  }
  ANSWERED.add(id);
  return null;
}

/** The stored contexts the fixture session holds. Starts empty on purpose:
 * the new-context flow is what a QA pass wants to see working. */
const FAKE_CONTEXTS: {
  id: string;
  kind: string;
  title: string;
  anchor_id: string | null;
  created_at: string;
  archived_at: string | null;
}[] = [];

let nextContext = 1;
function fakeContext(kind: string, title: string, anchor_id: string | null) {
  return {
    id: `ctx:fake-${nextContext++}`,
    kind,
    title,
    anchor_id,
    created_at: new Date().toISOString(),
    archived_at: null,
  };
}

/** The mirror row `list_entities` returns — the six columns, and no more. */
function mirrorRow(entry: (typeof CORPUS)[number]) {
  return {
    entity_id: entry.entity_id,
    kind: entry.kind,
    source_id: "mock",
    title: entry.title,
    updated_at: entry.updated_at,
    synced_at: SYNCED_AT,
  };
}

/**
 * `mini_board` (#177), over the fixture.
 *
 * The dev bridge's own answer, so the Tickets tile has a board to draw under
 * `just dev` and `just demo`. Like `listEntities` below, it *restates* the
 * command's rule — the leading four, then the rest folded and alphabetical,
 * then the statusless column last — and a restatement is a copy that can
 * drift. The command is the authority; a divergence here is a bug in this
 * file, never a second opinion about the order. It reads the fixture's
 * payloads the way the command's second `coalesce` arm reads the mock source's
 * — a flat `status` and `priority` — because that is the shape this corpus is
 * in.
 */
function miniBoard(args: Record<string, unknown>) {
  // The fixture has no link graph, so a stored context's membership is
  // honestly empty — the same answer `context_members` and `list_entities`
  // give.
  if (args["ctxId"]) return { columns: [], sources: [] };

  const sources = (args["sources"] as string[] | undefined) ?? [];
  if (sources.length && !sources.includes("mock")) return { columns: [], sources: [] };

  const text = (value: unknown) =>
    typeof value === "string" && value.trim() !== "" ? value.trim() : null;

  const project = args["project"] as string | null | undefined;
  const tickets = CORPUS.filter(
    (entry) => entry.kind === "ticket" && entry.deleted_at === null && inProject(entry, project),
  )
    .slice()
    .sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""));

  const columns: { status: string | null; cards: ReturnType<typeof boardCard>[] }[] = [];
  for (const entry of tickets) {
    const status = text(entry.payload["status"]);
    const column = columns.find((candidate) => candidate.status === status);
    if (column) column.cards.push(boardCard(entry));
    else columns.push({ status, cards: [boardCard(entry)] });
  }

  const LEADING = ["To Do", "In Progress", "In Review", "Done"];
  const rank = (status: string | null): [number, number, string] => {
    if (status === null) return [2, 0, ""];
    const at = LEADING.findIndex((leading) => leading.toLowerCase() === status.toLowerCase());
    return at === -1 ? [1, 0, status.toLowerCase()] : [0, at, status.toLowerCase()];
  };
  columns.sort((a, b) => {
    const [aClass, aAt, aFolded] = rank(a.status);
    const [bClass, bAt, bFolded] = rank(b.status);
    return aClass - bClass || aAt - bAt || aFolded.localeCompare(bFolded);
  });

  const observed = [...new Set(tickets.map((entry) => text(entry.payload["status"])))]
    .filter((status): status is string => status !== null)
    .sort((a, b) => {
      const [aClass, aAt, aFolded] = rank(a);
      const [bClass, bAt, bFolded] = rank(b);
      return aClass - bClass || aAt - bAt || aFolded.localeCompare(bFolded);
    });

  return {
    columns,
    sources: columns.length ? [{ source_id: "mock", statuses: observed }] : [],
  };
}

/**
 * The project a fixture entry carries, at the path the real read looks at.
 *
 * A restatement of `project_key_read!`'s Jira arm and of its failure
 * direction: absence, never a guess. An entry with no readable key belongs to
 * no project room and stays in every other room it was in.
 */
function projectOf(entry: (typeof CORPUS)[number]): { key: string; name: string | null } | null {
  const fields = entry.payload["fields"];
  const project = (fields as Record<string, unknown> | undefined)?.["project"] as
    | Record<string, unknown>
    | undefined;
  const text = (value: unknown) =>
    typeof value === "string" && value.trim() !== "" ? value.trim() : null;
  const key = text(project?.["key"]);
  return key === null ? null : { key, name: text(project?.["name"]) };
}

/** Whether an entry is admitted by a room's project scope. */
function inProject(entry: (typeof CORPUS)[number], project: string | null | undefined): boolean {
  return !project || projectOf(entry)?.key === project;
}

/** `list_projects` (#209): the projects the fixture's live corpus shows. */
function listProjects() {
  const seen = new Map<string, { source_id: string; key: string; name: string | null }>();
  for (const entry of CORPUS) {
    if (entry.deleted_at !== null) continue;
    const project = projectOf(entry);
    if (project === null || seen.has(project.key)) continue;
    seen.set(project.key, { source_id: "mock", key: project.key, name: project.name });
  }
  return [...seen.values()].sort((a, b) => a.key.localeCompare(b.key));
}

/** One card of the fixture's board. */
function boardCard(entry: (typeof CORPUS)[number]) {
  const priority = entry.payload["priority"];
  return {
    entity_id: entry.entity_id,
    source_id: "mock",
    key: entry.entity_id.slice(entry.entity_id.indexOf(":") + 1),
    title: entry.title,
    priority: typeof priority === "string" && priority.trim() !== "" ? priority.trim() : null,
  };
}

/** `list_entities`, filtered, ordered and paged the way the real one is. */
function listEntities(args: Record<string, unknown>) {
  const filter = (args["filter"] ?? {}) as {
    sources?: string[];
    kinds?: string[];
    order?: string;
    context?: string | null;
    project?: string | null;
    include_deleted?: boolean;
  };
  const limit = Number(args["limit"] ?? 50);
  const offset = Number(args["offset"] ?? 0);

  // The fixture has no link graph, so a context's membership is honestly
  // empty — the same answer `context_members` gives.
  if (filter.context) return { rows: [], total: 0 };

  let rows = CORPUS.filter((entry) => filter.include_deleted || entry.deleted_at === null);
  if (filter.sources?.length) rows = rows.filter(() => filter.sources?.includes("mock"));
  if (filter.kinds?.length) rows = rows.filter((entry) => filter.kinds?.includes(entry.kind));
  rows = rows.filter((entry) => inProject(entry, filter.project));

  rows = [...rows].sort((a, b) =>
    filter.order === "title_asc"
      ? a.title.localeCompare(b.title)
      : (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
  );

  return { rows: rows.slice(offset, offset + limit).map(mirrorRow), total: rows.length };
}

/* ----------------------------------------------------------------- notes */

/**
 * The notes this session has written, by id. **Fixture only.**
 *
 * A note is the one kind knobas owns rather than mirrors (#46), so there is
 * nothing in `CORPUS` to answer from: the browser walk that shows a pasted URL
 * turning into a chip (#497), and the one that presses *New note* and reads the
 * links panel (#502), both need somewhere to write one. Stateful within the
 * session, like `FAKE_CONTEXTS` above and for the same reason — a walk that
 * made a note and then could not read it back would show a bug this app does
 * not have.
 */
const NOTES = new Map<
  string,
  { title: string; body_md: string; created_at: string; born: NoteLinkInput[] }
>();

/**
 * `create_note`, minting an id the way the backend's `note:<uuid>` does, and
 * keeping the links the note was **born with** (#502). **Fixture only.**
 *
 * Kept rather than drawn: there is no link table here, so what this can show is
 * the one thing the walk is looking at — that the room and the foreground reach
 * the command as two links, and that the panel then draws a chip for each.
 * Whether the rows exist, whether they make the note a member of the context,
 * and what happens to a target no row ever carried are questions only a
 * database can answer, and `crates/knobas-app/tests/entity.rs` asks them there.
 */
function createNote(args: Record<string, unknown>) {
  const id = `note:${Math.random().toString(16).slice(2, 6)}`;
  const born = Array.isArray(args["links"]) ? (args["links"] as NoteLinkInput[]) : [];
  // The title and body the caller passed, and not two empty strings: *New
  // note* sends neither, but a **capture** creates its note on the first
  // keystroke and sends both (#503), and a fixture that dropped them would
  // draw an empty note over a walk whose whole point is what was typed.
  NOTES.set(id, {
    title: String(args["title"] ?? ""),
    body_md: String(args["bodyMd"] ?? ""),
    created_at: SYNCED_AT,
    born,
  });
  return noteDetail(id);
}

/** `get_note`. A note this session did not write is a `not_found`, as it is. */
function getNote(args: Record<string, unknown>) {
  const id = String(args["noteId"] ?? "");
  if (!NOTES.has(id)) {
    throw { code: "not_found", message: `there is no note ${id}`, source_id: null };
  }
  return noteDetail(id);
}

/** `save_note`: idempotent, and the answer carries the reconciled refs. */
function saveNote(args: Record<string, unknown>) {
  const id = String(args["noteId"] ?? "");
  const existing = NOTES.get(id);
  if (!existing) {
    throw { code: "not_found", message: `there is no note ${id}`, source_id: null };
  }
  NOTES.set(id, {
    ...existing,
    title: String(args["title"] ?? ""),
    body_md: String(args["bodyMd"] ?? ""),
  });
  return noteDetail(id);
}

/**
 * One note: its `[[refs]]` resolved against what this fixture holds, and the
 * links it was born with drawn the way the panel draws them. **Fixture only.**
 *
 * A **second, fixture-only** implementation of `knobas_core::note`'s
 * `parse_refs` and `reconcile_refs`, on the same terms as {@link resolveUrl}
 * below: the rule that decides what a reference *is* lives in Rust and is
 * tested against a real database, and there is no database here. What this owes
 * is the outcomes a walk has to be able to see — a body naming an entity comes
 * back with a chip to draw and a body naming nothing does not (#497), and a
 * note born in a room comes back with the links it was born with (#502).
 *
 * `notes/NoteView.test.svelte.ts` stands in for the same backend rule, and the
 * two deliberately do not share: this module is the dev harness, and
 * `shell/house-rules.test.ts` pins `App.svelte` as its only importer outside
 * `shell/dev/`. Lifting the scanner somewhere both could reach would put a
 * second `[[…]]` scanner into app code, which is what `notes/note-body.ts`
 * exists to refuse.
 */
function noteDetail(id: string) {
  const note = NOTES.get(id)!;
  const refs = [...note.body_md.matchAll(/\[\[([^\]]+)\]\]/g)].map((found) => {
    const targetId = found[1]!.trim();
    return { target_id: targetId, target: noteLinkEnd(targetId) };
  });
  const links = note.born
    .map((born) => ({ born, other: noteLinkEnd(born.target_id) }))
    .filter((drawn): drawn is { born: NoteLinkInput; other: LinkEnd } => drawn.other !== null)
    .map(({ born, other }, index) => ({
      link: {
        id: `link:born-${id}-${index}`,
        from_id: id,
        to_id: born.target_id,
        relation: born.relation,
        // `manual`, because that is what the command writes and because the
        // panel refuses to unlink an `implied` row with a message about a
        // `[[reference]]` — which would be false about a capture link.
        origin: "manual" as const,
        note: null,
        created_by: "user",
        created_at: SYNCED_AT,
        confirmed_at: SYNCED_AT,
        rule: null,
        rule_class: null,
        reason: null,
      },
      other,
    }));
  return {
    note: {
      id,
      title: note.title,
      body_md: note.body_md,
      created_at: note.created_at,
      updated_at: SYNCED_AT,
    },
    refs,
    links,
  };
}

/**
 * The other end of a note's ref or born link, out of whatever this fixture
 * holds — the mirror's corpus, or a context this session made.
 *
 * `null` where nothing carries the id — an unresolved ref, and for a born link
 * no link at all: the same failure direction the real command has.
 */
function noteLinkEnd(entityId: string): LinkEnd | null {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (entry) {
    return {
      entity_id: entityId,
      kind: entry.kind,
      title: entry.title,
      deleted_at: entry.deleted_at,
    };
  }
  const context = FAKE_CONTEXTS.find((candidate) => candidate.id === entityId);
  if (context) {
    return { entity_id: entityId, kind: "ctx", title: context.title, deleted_at: null };
  }
  return null;
}

/**
 * `resolve_url` (#496): what a pasted link names, over this fixture's corpus.
 *
 * A **second** implementation of the normalisation rule, and deliberately so:
 * the rule itself is SQL (`knobas_core::web_url_normalized!`) and there is no
 * database here. What this owes is not fidelity to the SQL but the two
 * outcomes a QA walk has to be able to see — a link that opens its entity, and
 * one that offers the browser — so it does the three things the rule does to
 * the URLs this corpus actually holds and no more.
 */
function resolveUrl(args: Record<string, unknown>) {
  const url = String(args["url"] ?? "");
  if (!/^https?:\/\//i.test(url)) {
    throw { code: "invalid", message: `${url} is not an http or https URL`, source_id: null };
  }
  const key = (raw: string) => {
    const parsed = new URL(raw);
    parsed.hash = "";
    parsed.pathname = parsed.pathname.replace(/\/+$/, "");
    return parsed.href.replace(/\/$/, "");
  };
  const wanted = key(url);
  const found = CORPUS.find((entry) => entry.web_url !== null && key(entry.web_url) === wanted);
  return found ? { entity_id: found.entity_id, kind: found.kind } : null;
}

/** `get_entity`, including the two refusals the detail view branches on. */
function getEntity(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (!id.includes(":") || id.startsWith(":") || id.endsWith(":")) {
    throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key`, source_id: null };
  }
  const entry = CORPUS.find((candidate) => candidate.entity_id === id);
  if (!entry) {
    throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  }
  return {
    row: mirrorRow(entry),
    source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true },
    // Null, where the real command now resolves the adapter's KindInfo from
    // the registry. Harmless divergence: the fallback path draws the same
    // words — knobas' own vocabulary (`kinds.ts` layer 2) matches the mock's
    // declared KindInfo for every kind in this corpus.
    kind_info: null,
    body_text: entry.body_text,
    author: entry.author,
    payload: entry.payload,
    web_url: entry.web_url,
    deleted_at: entry.deleted_at,
    links: [],
    activity: FAKE_ACTIVITY.filter((line) => line.entity_id === id),
  };
}

/**
 * The checkout fixture (#499): a clones root, what the scan "found" under it,
 * and the overrides this session has set.
 *
 * `let` and a mutable map, because the panel's whole point is that setting a
 * path changes what the next read says — a fixture that answered the same
 * thing after a write would make the QA pass prove nothing.
 */
const DEFAULT_CLONES_ROOT = "/Users/mara/src";

let fakeClonesRoot: string | null = DEFAULT_CLONES_ROOT;

/**
 * `?fake-clones-root=<path>` seeds it, and `?fake-clones-root=` (empty) leaves
 * it unset.
 *
 * The `?fake-db=` precedent: a QA pass has to be able to put a screen in a
 * state the default fixture is not in, and *no clones root set* is a different
 * sentence on the panel from *nothing found under one*. A headless run can
 * therefore point the fixture at a temporary directory of its own, which is
 * what #499's third criterion asks for — the fixture has no filesystem, so the
 * root is a label rather than a walk, and the walk is `knobas-core`'s to prove.
 */
function seedClonesRoot(params: URLSearchParams): void {
  const given = params.get("fake-clones-root");
  if (given === null) return;
  fakeClonesRoot = given.trim() === "" ? null : given.trim();
}

/** What the scan would find, per repo entity. */
const FAKE_SCANNED: Record<string, string> = {
  "mock:tidewater/payout-service": "/Users/mara/src/payout-service",
};

const fakeOverrides: Record<string, string> = {};

/** The repo a checkout question is about: itself, or a branch's repository. */
function fakeRepoOf(entityId: string): string | null {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (!entry) return null;
  if (entry.kind === "repo") return entry.entity_id;
  if (entry.kind !== "branch") return null;
  // The same prefix rule the backend uses: the longest repo id this branch's
  // id starts with (interfaces §4.2's nested key grammar).
  const repos = CORPUS.filter(
    (candidate) => candidate.kind === "repo" && entityId.startsWith(candidate.entity_id),
  ).sort((a, b) => b.entity_id.length - a.entity_id.length);
  return repos[0]?.entity_id ?? null;
}

/** `entity_checkout`, including the refusal a non-repo kind gets. */
function fakeCheckout(entityId: string) {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (!entry) {
    throw { code: "not_found", message: `${entityId} is not in the mirror`, source_id: null };
  }
  if (entry.kind !== "repo" && entry.kind !== "branch") {
    throw {
      code: "invalid",
      message: `a checkout belongs to a repo or a branch, and ${entityId} is a ${entry.kind}`,
      source_id: null,
    };
  }
  const repo = fakeRepoOf(entityId);
  const repoRow = CORPUS.find((candidate) => candidate.entity_id === repo);
  const url = repoRow?.web_url ?? null;
  const override = repo === null ? undefined : fakeOverrides[repo];
  // Keyed on the *root* as well as the repo, so a run pointed at a temporary
  // directory of its own sees the honest *no checkout* rather than a path
  // under a root nobody set.
  const scanned =
    repo === null || fakeClonesRoot !== DEFAULT_CLONES_ROOT ? undefined : FAKE_SCANNED[repo];
  const path = override ?? scanned ?? null;
  return {
    repo_entity_id: repo,
    repo_url: url,
    path,
    found_by: override !== undefined ? "override" : path !== null ? "scan" : "nothing",
    clones_root: fakeClonesRoot,
    clone_command: url === null ? null : `git clone ${url}`,
  };
}

/**
 * The open commands (#501): what each button on a repo detail would run.
 *
 * The fixture has no processes, so nothing here starts one — what it stands in
 * for is the *state* the panel and the settings section branch on: a template
 * set, a template that is this platform's default, and none at all. macOS's
 * three defaults are transcribed, because the browser this runs in has no
 * platform to ask.
 */
const FAKE_COMMANDS: { action: string; label: string; default: string | null }[] = [
  { action: "vscode", label: "Open in VS Code", default: 'open -a "Visual Studio Code" {path}' },
  { action: "jetbrains", label: "Open in JetBrains", default: 'open -a "IntelliJ IDEA" {path}' },
  { action: "terminal", label: "Open terminal here", default: "open -a Terminal {path}" },
];

/** What this session has set, per action. */
const fakeTemplates: Record<string, string> = {};

/**
 * The action whose spawn refuses, from `?fake-open-fails=<action>`.
 *
 * The `?fake-clones-root` precedent: a QA pass has to be able to put a screen
 * in a state the default fixture is not in, and *the program would not start*
 * is the one arm of this feature a fixture cannot reach by itself. With it
 * set, pressing that button draws the failure the real backend produces for a
 * template naming a program this machine has not got.
 */
let fakeOpenFails: string | null = null;

/**
 * The action with no template at all, from `?fake-open-unset=<action>`.
 *
 * *Not configured* is the state every action is in off macOS, and macOS is the
 * only platform anyone has run knobas on — so without this the one screen spec
 * #491 describes for Windows and Linux could not be looked at by anybody. It
 * is the same shape as `?fake-clones-root`: a QA pass putting a screen into a
 * state the default fixture is not in.
 */
let fakeOpenUnset: string | null = null;

function seedOpenState(params: URLSearchParams): void {
  const fails = params.get("fake-open-fails");
  if (fails !== null) fakeOpenFails = fails.trim() === "" ? null : fails.trim();
  const unset = params.get("fake-open-unset");
  if (unset !== null) fakeOpenUnset = unset.trim() === "" ? null : unset.trim();
}

/** `checkout_commands` — the stored template, else the platform's default. */
function fakeCommands() {
  return FAKE_COMMANDS.map((command) => {
    const stored = fakeTemplates[command.action];
    const platform = command.action === fakeOpenUnset ? null : command.default;
    return {
      action: command.action,
      label: command.label,
      template: stored ?? platform,
      is_default: stored === undefined,
    };
  });
}

/**
 * `set_checkout_command`, refusals included.
 *
 * A **second** implementation of the template rule, deliberately narrow: the
 * rule is `knobas_core::checkout::expand` and is asserted there over every
 * shape. What this owes is the one refusal a QA pass has to be able to see —
 * a placeholder that is not `{path}`, named — plus the empty-clears-it
 * behaviour the Reset button depends on.
 */
function fakeSetCommand(action: string, template: unknown) {
  const known = FAKE_COMMANDS.find((command) => command.action === action);
  if (!known) {
    throw {
      code: "invalid",
      message: `'${action}' is not something knobas opens a checkout with`,
      source_id: null,
    };
  }
  const value = typeof template === "string" ? template.trim() : "";
  if (value === "") {
    delete fakeTemplates[action];
    return fakeCommands();
  }
  const other = [...value.matchAll(/\{([^}]*)\}/g)].map((match) => match[1]).find((n) => n !== "path");
  if (other !== undefined) {
    throw {
      code: "invalid",
      message:
        `'${value}' cannot be run: {${other}} is not something knobas fills in: ` +
        "the only value a command knobas runs may take is {path}, the checkout on this disk",
      source_id: null,
    };
  }
  if (!value.includes("{path}")) {
    throw {
      code: "invalid",
      message: `'${value}' cannot be run: the command has no {path}, so it would open nothing`,
      source_id: null,
    };
  }
  fakeTemplates[action] = value;
  return fakeCommands();
}

/**
 * `open_checkout`. Starts nothing — there is no process in a browser — and
 * refuses for the action `?fake-open-fails` names, in the shape the real
 * command refuses a program that is not installed.
 */
function fakeOpen(action: string) {
  const known = fakeCommands().find((command) => command.action === action);
  if (!known || known.template === null) {
    throw {
      code: "invalid",
      message: `${known?.label ?? action} is not configured on this platform`,
      source_id: null,
    };
  }
  if (action === fakeOpenFails) {
    throw {
      code: "invalid",
      message: `'${known.template}' could not be run: No such file or directory (os error 2)`,
      source_id: null,
    };
  }
  return null;
}

/**
 * The pair the capture window reads its two born links from (#503).
 *
 * A **stored** room and an open ticket, so the walk exercises the case with
 * both links rather than the degenerate one: the SEPA context this fixture's
 * `list_contexts` answers with, and the ticket its rooms open on.
 */
const FAKE_CAPTURE_CONTEXT = { context: "ctx:sepa", foreground: "mock:PAY-231" };

/**
 * `?fake-shortcut=<accelerator>` seeds a stored shortcut, and
 * `?fake-shortcut-refused` makes it read as one the operating system would not
 * hand over -- the branch a browser can otherwise never reach, since no browser
 * can ask a window server for a key.
 */
function seedCaptureShortcut(params: URLSearchParams): void {
  const stored = params.get("fake-shortcut");
  if (stored !== null) fakeShortcutStored = stored.trim() === "" ? null : stored.trim();
  fakeShortcutRefused = params.has("fake-shortcut-refused");
}

/**
 * The capture shortcut this session has stored, and why it is not holding.
 *
 * Seeded by {@link seedCaptureShortcut}.
 */
let fakeShortcutStored: string | null = null;
let fakeShortcutRefused = false;

function fakeShortcut() {
  return {
    accelerator: fakeShortcutStored,
    refusal:
      fakeShortcutStored !== null && fakeShortcutRefused
        ? `${fakeShortcutStored} is registered by another application`
        : null,
  };
}

function fakeSetShortcut(accelerator: unknown) {
  const typed = typeof accelerator === "string" ? accelerator.trim() : "";
  fakeShortcutStored = typed === "" ? null : typed;
  return fakeShortcut();
}

/** A handful of log lines, so the History panel has something to draw. */
const FAKE_ACTIVITY = [
  {
    id: 3,
    at: "2026-08-22T14:30:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:PAY-231",
    detail: { upserted: 9 },
  },
  {
    id: 2,
    at: "2026-08-22T11:49:00Z",
    actor: "user",
    verb: "opened",
    entity_id: "mock:PAY-231",
    detail: {},
  },
  {
    id: 1,
    at: "2026-08-22T08:10:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:INC-1",
    detail: { upserted: 1 },
  },
];

/** `recent_activity`, globally or scoped to one entity. */
function recentActivity(args: Record<string, unknown>) {
  const entityId = args["entityId"];
  const limit = Number(args["limit"] ?? 20);
  const lines =
    typeof entityId === "string"
      ? FAKE_ACTIVITY.filter((line) => line.entity_id === entityId)
      : FAKE_ACTIVITY;
  return lines.slice(0, limit);
}

/** Everything in `AppStatus` that is not the lifecycle state. */
const DEMO_STATUS = {
  first_run: false,
  demo: true,
  source_count: 1,
  app_version: "0.1.0-fixture",
};

/**
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state.
 *
 * Without it the fake answers `ready` immediately, which is right for QA of
 * every other screen and useless for QA of this one — the loading screen would
 * be gone before the shutter opened.
 */
function fakeDbState(params: URLSearchParams): unknown {
  switch (params.get("fake-db")) {
    case "starting":
      return { state: "starting", detail: "first run: downloading and initialising PostgreSQL" };
    case "migrating":
      return { state: "migrating" };
    case "failed":
      return { state: "failed", message: "port 50861 is in use by another process" };
    default:
      return { state: "ready" };
  }
}

// -- the sources cockpit's fixture -------------------------------------------
//
// Below `CORPUS` and `SYNCED_AT` because it reads them: a `const` is in its
// temporal dead zone until its declaration runs, and these are evaluated at
// module load.

/** What the fixture's adapters declare — §3a's display metadata. */
const KIND_INFO = [
  { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true },
  { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false },
];

/** The three sources the sources view draws. */
const FIXTURE_SOURCES = [
  fixtureSource("mock", "mock", "Tidewater (mock)", "https://mock.tidewater.example", "ok", 21, 90, null),
  {
    ...fixtureSource("gitea", "gitea", "Tidewater Gitea", "https://git.tidewater.example", "unauthorized", 48, 91, "Pat"),
    health: {
      source_id: "gitea",
      state: "unauthorized",
      checked_at: SYNCED_AT,
      detail: "401 from /api/v1/user",
      secret_expires_at: null,
    },
    next_run_at: null,
  },
  {
    ...fixtureSource("jira", "jira", "Tidewater Jira", "https://jira.tidewater.example", "ok", 213, 92, "UserPassword"),
    health: {
      source_id: "jira",
      state: "ok",
      checked_at: SYNCED_AT,
      detail: null,
      // Eight days out from the fixture's clock: amber, and the reading a
      // screenshot of the expiry countdown needs.
      secret_expires_at: "2026-08-30T14:30:00Z",
    },
  },
];

/**
 * The `base_url` is passed in and spelled out at each call site rather than
 * built from `id`.
 *
 * `house-rules.test.ts` checks the *hostname* of every URL in `app/src/`
 * against the reserved names of RFC 2606, and it cannot check one that a
 * template builds at run time: a host assembled from an interpolated segment
 * could be any host at all as far as a scan is concerned, so the rule refuses
 * it. A literal is what the rule can actually verify, and three of them is a
 * small price for a lint that fails closed.
 *
 * (The rule scans raw text and does not strip prose, so this comment does not
 * spell the interpolated form either — the note beside `FIXTURE_HOST` makes
 * the same point about the same rule.)
 */
function fixtureSource(
  id: string,
  adapterKind: string,
  displayName: string,
  baseUrl: string,
  state: string,
  itemCount: number,
  runId: number,
  /**
   * `null` is a source that needs no credential, which is the compiled-in mock
   * and nothing else here. Spelled per fixture rather than defaulted, because a
   * default would put one word in the credential column of every row and the
   * column exists to tell them apart.
   */
  authKind: string | null,
) {
  return {
    id,
    adapter_kind: adapterKind,
    display_name: displayName,
    base_url: baseUrl,
    enabled: true,
    sync_interval_secs: 900,
    config: {},
    auth_kind: authKind,
    health: {
      source_id: id,
      state,
      checked_at: SYNCED_AT,
      detail: null as string | null,
      secret_expires_at: null as string | null,
    },
    last_run: {
      // Distinct per source, because `knobas.sync_run.id` is a primary key and
      // the run log keys its rows on it. Three rows sharing an id made Svelte
      // throw `each_key_duplicate` mid-render, which aborts the *whole* update
      // — the diagnostics panel and its `.dbbar` both silently stayed on their
      // initial values. A fixture that cannot happen in the database is still
      // a fixture that breaks the screen it exists to show.
      id: runId,
      source_id: id,
      trigger: "schedule",
      started_at: "2026-08-22T14:29:48Z",
      finished_at: SYNCED_AT,
      outcome: state === "unauthorized" ? "unauthorized" : "ok",
      upserted: itemCount,
      deleted: 0,
      swept: 0,
      error: state === "unauthorized" ? "401 from /api/v1/user" : null,
      cursor_after: null as string | null,
    },
    next_run_at: "2026-08-22T14:45:00Z",
    item_count: itemCount,
    kinds: KIND_INFO,
  };
}

/** One descriptor template per adapter the Add-source form can offer. */
const FIXTURE_ADAPTERS = [
  {
    id: "mock",
    adapter_kind: "mock",
    name: "Tidewater mock",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["ApiToken"],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The mock needs no configuration, which is the empty-form case.
    config_schema: { type: "object", properties: {} },
  },
  {
    id: "jira",
    adapter_kind: "jira",
    name: "Jira Data Center",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The generated-form case: a select with a default, two lists, two texts.
    // The *same* transcription the form tests render, not a second copy of it.
    // A fixture that drifted from the corpus under test would leave browser QA
    // agreeing with nothing — and this is the import that makes `fixtures.ts`'s
    // "not test-only" true rather than aspirational.
    config_schema: JIRA_SCHEMA,
  },
];

/**
 * The write queue a QA pass sees: one write held on a changed target, one the
 * source refused, one still waiting on a server that did not answer.
 *
 * The held row's two snapshots differ by a reply, which is exactly what
 * `knobas_core::write_queue::project` holds a comment against -- so the
 * side-by-side in the panel shows the real thing rather than two copies of one
 * string.
 */
const FIXTURE_QUEUE = [
  {
    id: 31,
    source_id: "mock",
    entity_id: "mock:PAY-231",
    op: "comment",
    payload: {
      Comment: {
        entity: "mock:PAY-231",
        body: "Taking this — the retry job is dropping the idempotency key.",
      },
    },
    target_snapshot: {
      op: "comment",
      live: true,
      text: "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled.",
    },
    state: "held",
    wait_reason: null,
    detail: null,
    queued_at: "2026-08-25T11:41:00Z",
    attempted_at: "2026-08-25T11:52:00Z",
    attempts: 2,
    held_snapshot: {
      op: "comment",
      live: true,
      text:
        "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled." +
        "\n\njonas.weiss: already on it — it is the idempotency key.",
    },
    settled_at: null,
    source_enabled: true,
  },
  {
    id: 30,
    source_id: "mock",
    entity_id: "mock:PAY-228",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-228", body: "Closing — superseded by PAY-231." } },
    target_snapshot: { op: "comment", live: true, text: "Payout retries pile up" },
    state: "refused",
    wait_reason: null,
    detail: "this issue type does not accept comments",
    queued_at: "2026-08-25T10:12:00Z",
    attempted_at: "2026-08-25T10:12:00Z",
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
  {
    id: 29,
    source_id: "mock",
    entity_id: "mock:PAY-300",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-300", body: "Ledger drift is the same root cause." } },
    target_snapshot: { op: "comment", live: true, text: "Payout ledger drift" },
    state: "pending",
    wait_reason: "unreachable",
    detail: "connection timed out",
    queued_at: "2026-08-25T11:55:00Z",
    attempted_at: "2026-08-25T11:56:00Z",
    attempts: 3,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  },
];

const FIXTURE_DB_STATS = {
  db_bytes: 222_298_112,
  entity_count: CORPUS.length,
  item_count: CORPUS.length,
  per_source: [{ source_id: "mock", items: CORPUS.length, synced_at: SYNCED_AT }],
  oldest_synced_at: "2026-08-21T08:00:00Z",
  newest_synced_at: SYNCED_AT,
};

/** `sync_status` for one fixture source, derived from its last run. */
function fakeStatus(source: (typeof FIXTURE_SOURCES)[number]) {
  return {
    source_id: source.id,
    running: false,
    run_id: source.last_run?.id ?? null,
    started_at: null,
    last_finished_at: source.last_run?.finished_at ?? null,
    last_outcome: source.last_run?.outcome ?? null,
    next_run_at: source.next_run_at,
    backoff_until: null,
  };
}

/** `list_sync_runs`, newest first, optionally scoped to one source. */
function fakeRuns(args: Record<string, unknown>) {
  const sourceId = args["sourceId"];
  const runs = [
    ...FIXTURE_SOURCES.map((source) => source.last_run).filter((run) => run !== null),
    // One still in flight, because "running" is a state the log has to be able
    // to draw and a fixture of finished runs can never show it.
    {
      id: 93,
      source_id: "mock",
      trigger: "manual",
      started_at: SYNCED_AT,
      finished_at: null,
      outcome: null,
      upserted: 0,
      deleted: 0,
      swept: 0,
      error: null,
      cursor_after: null,
    },
  ];
  return typeof sourceId === "string" ? runs.filter((run) => run.source_id === sourceId) : runs;
}

// -- the Monitors tab (#448) ------------------------------------------------

/**
 * The monitors the estate file asks for, as `monitor_roster` answers them.
 *
 * **Derived from the file and not listed here.** `estate.json` names each
 * asset's monitors by their Uptime Kuma name (story 25), so the set of
 * monitors that ought to exist is already written down once; a second list
 * here would be a fixture that drifts from the file the Import loads.
 *
 * What this *does* invent is the reading — a state, a response time, a day of
 * samples — because `/metrics` is a live system and there is nothing on disk
 * to read it from. The states are dealt round in a fixed order so that every
 * chip on the tab has something in it: a roster where everything is `up` can
 * be photographed without showing that the chips do anything.
 */
const FIXTURE_MONITOR_STATES = ["up", "warn", "down", "pending", "up", "maintenance", "up"];

/**
 * How many minutes ago the newest sample of the fixture roster is.
 *
 * A whole number of half hours, so the fixture's bars start on a bucket edge
 * and a screenshot is not one segment shorter than the next reader's.
 */
const FIXTURE_SAMPLE_STEP_MIN = 30;

/**
 * One monitor's last day, one sample per half hour — forty-eight of them, so
 * the bar is full rather than a strip with the left three quarters missing.
 *
 * The blips are placed by index and not at random: a fixture that redrew
 * itself on every reload would make two QA screenshots of the same build
 * disagree.
 */
function fakeSamples(state: string, seed: number, now: number) {
  return Array.from({ length: 48 }, (_, index) => {
    const takenAt = new Date(now - (47 - index) * FIXTURE_SAMPLE_STEP_MIN * 60_000);
    // Two hours with no reading at all, so a gap is on the screenshot: a bar
    // that is never interrupted cannot show what an interruption looks like.
    const quiet = index >= 10 + (seed % 5) && index < 14 + (seed % 5);
    // One bad half hour per monitor, in a different place for each.
    const blip = index === 30 + (seed % 9);
    return {
      taken_at: takenAt.toISOString(),
      state: quiet ? null : blip ? "down" : state,
    };
  });
}

/**
 * `open_alerts`: one alert per monitor the fixture roster has in trouble.
 *
 * **Derived from `fakeMonitorRoster`**, so the strip and the Monitors tab are
 * one statement about one fixture rather than two lists that can disagree —
 * the rule the backend follows, where both surfaces read one table. A
 * `pending` or `maintenance` monitor is deliberately not in it: those two
 * words neither open an alert nor close one (`CONTEXT.md`, **Alert**), and a
 * fixture that drew them would teach a reader the wrong rule.
 *
 * `opened_at` is derived from the row's index so two screenshots of the same
 * build agree, and the first one is acked so the list can show what *seen, not
 * fixed* looks like without anything here having to remember a click.
 */
function fakeOpenAlerts() {
  const now = Date.now();
  return fakeMonitorRoster()
    .filter((row) => row.state === "down" || row.state === "warn")
    .map((row, index) => ({
      id: index + 1,
      monitor_id: row.entity_id,
      monitor_name: row.name,
      state: row.state,
      opened_at: new Date(now - (index + 1) * 37 * 60_000).toISOString(),
      acked_at:
        index === 0 || FIXTURE_ACKED.has(row.entity_id)
          ? new Date(now - 12 * 60_000).toISOString()
          : null,
      assets: row.assets,
    }));
}

/**
 * `submit_write`: the row the queue answers with, for the fixture's own
 * Pause / Resume.
 *
 * `sent`, with no wait reason: what a fixture must not do is show a write
 * *pending* for ever, which is what a `pending` row with nothing to flush it
 * would look like.
 */
function fakeQueuedWrite(args: Record<string, unknown>) {
  const payload = (args.payload ?? {}) as Record<string, { entity?: string }>;
  const [tag, body] = Object.entries(payload)[0] ?? ["PauseMonitor", {}];
  const entity = body?.entity ?? "kuma:1";
  const now = new Date().toISOString();
  return {
    id: (FIXTURE_WRITE_ID += 1),
    source_id: entity.split(":")[0] ?? "kuma",
    entity_id: entity,
    op: tag === "ResumeMonitor" ? "resume_monitor" : "pause_monitor",
    payload,
    target_snapshot: null,
    state: "sent",
    wait_reason: null,
    detail: null,
    queued_at: now,
    attempted_at: now,
    attempts: 1,
    held_snapshot: null,
    settled_at: now,
    source_enabled: true,
  };
}

let FIXTURE_WRITE_ID = 0;

/**
 * The monitors a reader has acked in this session, by monitor entity id.
 *
 * Module state, like the fixture's other writes: `?fake-ipc` is a browser
 * with no backend behind it, so the only place an ack can be remembered is
 * here. Cleared by a reload, which is what a fixture is.
 */
const FIXTURE_ACKED = new Set<string>();

/**
 * `ack_alert`: mark one monitor's open alert seen, and answer with it.
 *
 * **Leaves the alert open**, which is the whole of what #446 decided: only a
 * return to `up` closes one, so the card stays and starts reading acked. A
 * monitor with nothing open is `not_found`, the refusal the real command
 * makes for an alert that recovered while a reader was looking at it.
 */
function fakeAck(args: Record<string, unknown>) {
  const monitorId = typeof args.monitorId === "string" ? args.monitorId : "";
  const alert = fakeOpenAlerts().find((row) => row.monitor_id === monitorId);
  if (!alert) {
    throw { code: "not_found", message: `no open alert on ${monitorId}`, source_id: null };
  }
  FIXTURE_ACKED.add(monitorId);
  return { ...alert, acked_at: new Date().toISOString() };
}

/**
 * The launcher's smart-list rail (#504) -- **fixture-only**.
 *
 * A restatement of `knobas_search::lists`' seven lists, in this file's own
 * terms, and a restatement is a copy that can drift: the Rust registry is the
 * authority and a divergence here is a bug in this file, never a second
 * opinion. `miniBoard` and `listEntities` carry the same warning for the same
 * reason. Nothing here proves a predicate; what it lets a browser see is the
 * rail, the counts beside it and what a row opens.
 *
 * **The three literals per list are pinned** since #506:
 * `knobas_search::lists`' `the_builtin_registry_matches_its_typescript_fixture`
 * parses the arrays below and compares the id, label and blurb triples against
 * `BUILTINS` itself. Everything the two paragraphs after this one call a
 * divergence — the counts, the badges, the row order, and what a reader with
 * no identity configured actually reads on the two `@me` rows — is outside
 * that pin, deliberately, and the test's own doc says so.
 *
 * **The four mirror lists honestly read 0.** This fixture's corpus is frozen
 * at `SYNCED_AT` -- 22 August 2026 -- so nothing in it changed today and
 * nothing synced in the last hour; and no source here carries a username, so
 * the two `@me` lists are empty for the reason the real ones would be.
 *
 * Their **descriptions** are the Rust blurbs, copied, and for the two `@me`
 * lists that is a divergence rather than a copy: with no identity configured
 * `lists::describe` replaces the blurb with `describe_missing_identity()` --
 * three sentences about *Test connection* filling a source's username in.
 * Restating that here would be the longest copied string in this file and the
 * likeliest to rot, and it is not what #504 is about. What a walk sees on
 * those two rows is therefore the blurb where the app would show the advice.
 *
 * **The three estate lists are derived from the estate this file draws**, each
 * from the nearest thing the fixture has to the rule:
 *
 * * *Not monitored* is `fakeUnmonitored` -- the assets the estate file names no
 *   monitor for -- which is what the backend's read answers over a mirror that
 *   has resolved those names into links.
 * * *Open alerts in my contexts* is the assets of `fakeOpenAlerts`, **with the
 *   context half of the rule dropped**: the fixture has no link graph and no
 *   context holds anything (`context_members` and `context_assets` both answer
 *   empty), so membership cannot be modelled here at all. An honest fixture
 *   would therefore answer 0 and the list could not be walked; this answers the
 *   alerts and says here that the routing clause is missing. What the clause
 *   does is asserted over a real database in `search_ipc.rs`.
 * * *Certificates expiring* is the roster rows whose `cert_days_remaining` is
 *   under thirty -- the one certificate the roster invents, at nine days.
 *
 * The change badge is `false` on every row: a badge is a comparison against a
 * stamp in `knobas.setting`, and this fixture has no setting table to remember
 * one in. A badge that was always on would be the more misleading of the two.
 */
function fakeSmartLists() {
  const mirror = [
    ["changed-today", "Changed today", "Everything a source touched since midnight."],
    [
      "mine",
      "My items",
      "Items your configured accounts are the author of, from the last 30 days.",
    ],
    ["mine-stale", "Mine, untouched 14 days", "Yours, and nothing has moved them in a fortnight."],
    ["just-synced", "Just synced", "What the last hour of syncing brought in."],
  ] as const;
  const estate = [
    [
      "not-monitored",
      "Not monitored",
      "Estate assets with no monitor attached to them.",
    ],
    [
      "alerts-in-context",
      "Open alerts in my contexts",
      "Assets a context you have not archived holds, with a monitor in trouble.",
    ],
    [
      "certs-expiring",
      "Certificates expiring",
      "Assets whose certificate runs out in under 30 days.",
    ],
  ] as const;

  return [
    ...mirror.map(([id, label, description]) => ({
      id,
      label,
      count: 0,
      changed: false,
      description,
      saved: false,
      needs_attention: false,
    })),
    ...estate.map(([id, label, description]) => ({
      id,
      label,
      count: fakeListAssets(id).length,
      changed: false,
      description,
      saved: false,
      needs_attention: false,
    })),
    ...fakeSavedLists(),
  ];
}

/**
 * The lists a reader saved (#506) -- **fixture-only**, and mutable, because
 * saving one is the gesture the walk is here to make.
 *
 * Seeded with **one row of each kind the rail can draw**: a runnable saved
 * query, and one that needs attention. The second is seeded rather than made
 * because no command can make one -- `knobas_search::saved::create` runs the
 * grammar first and refuses a query it could not run, so a needs-attention row
 * is what a *grammar change* leaves behind, and the backend's own IPC-seam
 * test writes its row with SQL for the same reason.
 *
 * `needs_attention` is carried as **data** rather than re-derived here. The
 * verdict is `saved::plan`'s, over §4's grammar and this installation's
 * vocabulary, and a second reading of that in TypeScript is exactly the copy
 * that drifts. What the walk sees is the two states drawn; what decides them
 * is asserted over a real database in `crates/knobas-app/tests/search_ipc.rs`.
 */
const FIXTURE_SAVED: { id: string; label: string; query: string; broken?: boolean }[] = [
  { id: "gitea-boxes", label: "Gitea boxes", query: "gitea" },
  { id: "todays-palette", label: "Today's palette", query: "> palette", broken: true },
];

/**
 * What one saved list stands for here: a **substring match over asset names**,
 * where the real count is PostgreSQL FTS over four corpora -- the mirror,
 * notes, assets and routes -- with `corpus::ALL`'s weights. The same stand-in
 * `estateSearch` already makes, for the same reason: this fixture has no
 * mirror to search.
 *
 * The change badge is `false` on every row, as it is on the built-ins: a badge
 * is a comparison against a stamp in `knobas.setting`, and there is no setting
 * table here to remember one in.
 */
function fakeSavedAssets(query: string) {
  const needle = query.trim().toLowerCase();
  return needle === ""
    ? []
    : FIXTURE_ESTATE.filter((asset) => asset.name.toLowerCase().includes(needle));
}

function fakeSavedLists() {
  return FIXTURE_SAVED.map((list) => ({
    id: list.id,
    label: list.label,
    count: list.broken ? 0 : fakeSavedAssets(list.query).length,
    changed: false,
    // The saved query is a saved list's own blurb, and a refused one says
    // which rule refused it -- `saved::Refusal::description`, whose heading is
    // the wording the rail draws.
    description: list.broken
      ? "Needs attention: the saved query starts with a prefix that is not a search. Delete it and save the search again."
      : list.query,
    saved: true,
    needs_attention: list.broken === true,
  }));
}

/**
 * `create_smart_list`: save the box's query -- **fixture-only**.
 *
 * The id is slugged and de-duplicated the way `knobas_search::saved`'s `slug`
 * and `free_id` do it -- **against the built-ins as well as the saved rows**,
 * because `list:<id>` is one namespace and a fixture that let a new list take
 * `mine` would draw two rows under one `{#each}` key. What it does *not* do is
 * judge the query: every refusal the real command makes past a blank one is
 * §4's grammar's, and this fixture has no grammar. The panel is what keeps an
 * unrunnable query away from here -- `Session.canSave` reads the backend's own
 * interpretation and offers the control on results only.
 */
function fakeCreateSmartList(args: Record<string, unknown>) {
  const label = String(args["label"] ?? "").trim();
  const query = String(args["query"] ?? "").trim();
  if (label === "" || query === "") {
    throw { code: "invalid", message: "a saved list needs a name and a query", source_id: null };
  }
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48)
    .replace(/-+$/, "");
  const base = slug === "" ? "list" : slug;
  const taken = new Set(fakeSmartLists().map((list) => list.id));
  let id = base;
  for (let suffix = 2; taken.has(id); suffix += 1) {
    id = `${base}-${suffix}`;
  }
  FIXTURE_SAVED.push({ id, label, query });
  const made = fakeSavedLists().find((list) => list.id === id);
  if (!made) throw { code: "internal", message: "the fixture lost the row it just made", source_id: null };
  return made;
}

/** `rename_smart_list`: the id does not move, exactly as it does not there. */
function fakeRenameSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const label = String(args["label"] ?? "").trim();
  const row = FIXTURE_SAVED.find((list) => list.id === id);
  if (!row) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  if (label === "") {
    throw { code: "invalid", message: "a saved list needs a name", source_id: null };
  }
  row.label = label;
  return null;
}

/** `delete_smart_list`. */
function fakeDeleteSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const at = FIXTURE_SAVED.findIndex((list) => list.id === id);
  if (at < 0) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  FIXTURE_SAVED.splice(at, 1);
  return null;
}

/**
 * The assets one estate list answers with -- **fixture-only**, see
 * `fakeSmartLists` for what each stands in for and what it drops.
 */
function fakeListAssets(id: string): (typeof FIXTURE_ESTATE)[number][] {
  const byId = (assetId: string) => FIXTURE_ESTATE.find((asset) => asset.id === assetId);
  switch (id) {
    case "not-monitored":
      return fakeUnmonitored()
        .map((row) => byId(row.id))
        .filter((asset) => asset !== undefined);
    case "alerts-in-context":
      return fakeOpenAlerts()
        .flatMap((alert) => alert.assets)
        .map((asset) => byId(asset.id))
        .filter((asset) => asset !== undefined);
    case "certs-expiring":
      return fakeMonitorRoster()
        .filter((row) => row.cert_days_remaining !== null && row.cert_days_remaining < 30)
        .flatMap((row) => row.assets)
        .map((asset) => byId(asset.id))
        .filter((asset) => asset !== undefined);
    default:
      return [];
  }
}

/**
 * `smart_list_items`: one list's rows, in the shape a search answers in --
 * which is the whole point of a smart list on this side of the wire, and what
 * lets the launcher render one with the component it renders results with.
 *
 * An unknown id is refused the way the real command refuses it, so a typo in
 * `list:` under `?fake-ipc` looks like a typo and not like an empty list.
 *
 * **The order is the fixture's, not the statement's** -- the last of the
 * divergences `fakeSmartLists` lists. `rows_over_estate!` returns an estate
 * list `order by coalesce(path,'') asc, title asc, entity_id asc`; these rows
 * come back in whatever order the roster and the alert list already build,
 * because reproducing that ordering here would be a fourth restatement to keep
 * in step and the walk asserts nothing about it. What a browser sees is which
 * rows, not which first.
 */
function fakeSmartListItems(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const known = fakeSmartLists().some((list) => list.id === id);
  if (!known) {
    throw { code: "invalid", message: `unknown smart list: ${id}`, source_id: null };
  }
  const limit = Number(args["limit"] ?? 20);
  // A saved list answers with its own query's rows, and a built-in with its
  // rule's -- one id namespace and one shape, which is what lets the launcher
  // open either by typing `list:<id>`.
  const saved = FIXTURE_SAVED.find((list) => list.id === id);
  const assets = saved ? fakeSavedAssets(saved.query) : fakeListAssets(id);
  const hits = assets.slice(0, limit).map((asset) => ({
    entity_id: asset.id,
    kind: "asset",
    source_id: "asset",
    updated_at: null,
    synced_at: SYNCED_AT,
    title: asset.name,
    path: assetPathText(asset),
    // A list row was matched against nothing, so it carries no rank and no
    // excerpt -- the rule the real response follows.
    rank: 0,
    snippet: [],
  }));
  return {
    interpreted: {
      text: "",
      prefix: "list",
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups:
      assets.length === 0
        ? []
        : [
            {
              kind: "asset",
              label: "Asset",
              plural: "Assets",
              monogram: "AS",
              // The list's total, never the page's.
              total: assets.length,
              hits,
            },
          ],
    total: assets.length,
    took_ms: 2,
    coverage: [],
  };
}

/**
 * `unmonitored_assets`: every asset in the file for which it names no Uptime
 * Kuma monitor.
 *
 * **Derived from the file and not listed here**, `fakeMonitorRoster`'s rule:
 * `estate.json` says which monitors each asset asks for, so *which assets ask
 * for none* is already written down once and a second list would drift from
 * it.
 */
function fakeUnmonitored() {
  return FIXTURE_ESTATE.filter((asset) => asset.monitors.length === 0)
    .map((asset) => ({
      id: asset.id,
      type_id: asset.type_id,
      type_label: asset.type_label,
      monogram: asset.monogram,
      name: asset.name,
      path: assetPathText(asset),
    }))
    .sort(
      (left, right) =>
        (left.path ?? "").localeCompare(right.path ?? "") ||
        left.name.localeCompare(right.name),
    );
}

/**
 * `monitor_roster`: every monitor the file names, with a reading, a day of
 * samples and the asset it watches.
 *
 * The **last** sample is what the row's `state` says, which is the rule the
 * backend follows (`assets::monitor_roster`): the chip and the bar's
 * right-hand end are one statement, and a fixture that let them disagree
 * would make the tab look right while hiding the one thing that could be
 * wrong.
 */
function fakeMonitorRoster() {
  const now = Date.now();
  const named = FIXTURE_ESTATE.flatMap((asset) =>
    asset.monitors.map((name) => ({ name, asset })),
  );
  return named.map((entry, index) => {
    const state = FIXTURE_MONITOR_STATES[index % FIXTURE_MONITOR_STATES.length] ?? "up";
    const samples = fakeSamples(state, index, now);
    // The last one, for the reason above.
    const last = samples.at(-1);
    // One paused monitor, because a roster with none cannot show what a
    // silenced check looks like and it is the state the tab most needs to be
    // able to say out loud.
    const tombstoned = entry.name === "confluence (tunnel)";
    return {
      entity_id: `kuma:${index + 1}`,
      source_id: "kuma",
      name: entry.name,
      state: last?.state ?? null,
      monitor_type: entry.name.includes("(tunnel)") ? "port" : "http",
      target: entry.name.includes("(tunnel)")
        ? `${entry.asset.name}.invalid:8080`
        : `http://localhost:3001/${entry.name.replace(/\W+/g, "-")}`,
      response_time_ms: state === "warn" ? 2_140 : 13 + index * 7,
      checked_at: new Date(now - 60_000).toISOString(),
      uptime: [
        { window: "1d", ratio: state === "down" ? 0.62 : 0.998 },
        { window: "30d", ratio: 0.9962 },
      ],
      // One certificate check, since most monitors watch no certificate and a
      // roster where every row had one would hide that the column is optional.
      cert_days_remaining: index === 0 ? 9 : null,
      web_url: tombstoned ? null : `http://localhost:3001/dashboard/${index + 1}`,
      tombstoned,
      assets: [
        {
          id: entry.asset.id,
          name: entry.asset.name,
          path: assetPathText(entry.asset),
        },
      ],
      // The fixture's Kuma has an account, so the tab draws its buttons
      // (issue #452) -- and the per-row half of the rule is the fixture's
      // too: the paused monitor is the one offered *Resume*. A fixture that
      // offered both on every row would photograph a tab that cannot exist.
      actions: [tombstoned ? "resume_monitor" : "pause_monitor"],
      samples,
    };
  });
}
