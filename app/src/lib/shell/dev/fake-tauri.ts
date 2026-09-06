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
import type { LinkEnd, LinkRow, SuggestionEntry, SuggestionPage } from "../../ipc/entity";
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
    recent_activity: (args) => recentActivity(args),

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
        throw { code: "not_found", message: `${anchor} is not in the local index`, source_id: null };
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
    get_route: (args) => routeDetail(args),
    // The room's Assets tile (#434). Empty for `context_members`' reason: the
    // fixture has no link graph, so no context holds anything and membership
    // -- assets included -- is honestly nothing. A stored room under
    // `?fake-ipc` therefore draws the tile's empty state rather than a red
    // "command not found".
    context_assets: () => [],
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
 * **Not all nineteen**: this is a fixture a browser is pointed at, and nine
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
    suggests: ["module", "connector"],
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
 * The **real** estate, read out of the file rather than copied into this one.
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

/**
 * The estate the Tree draws: twenty-three assets, five levels at the deepest.
 *
 * Stateful within the session, like the contexts: an asset created, renamed,
 * moved or deleted here stays that way for as long as the page lives, so the
 * whole create / edit / move walk can be driven in a browser. Nothing is
 * persisted -- reload and the estate is the file again.
 */
const FIXTURE_ESTATE: FixtureAsset[] = ESTATE.assets.map((entry) => {
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
});

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
 *
 * A route declares no schema, so every property is a custom row --
 * `assets::route_row_of` calls `custom_properties` and not `properties_of`.
 */
const FIXTURE_ROUTES: FixtureRoute[] = ESTATE.routes.map((route) => ({
  id: route.id,
  asset_id: route.asset,
  target_id: route.target ?? null,
  name: route.name,
  url: route.url,
  visibility: "internal",
  properties: customPropertiesOf(bagOf(route)),
}));

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
const ASSET_HISTORY: { entity_id: string; verb: string; detail: unknown }[] = [];

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
    history: [
      // This session's own writes first, newest first, which is what makes
      // *every mutation appears in the pane's history immediately* (#429)
      // something a QA walk can see rather than take on trust.
      ...ASSET_HISTORY.filter((line) => line.entity_id === asset.id)
        .map((line, index) => ({ id: 1000 + index, at: SYNCED_AT, actor: "user", ...line }))
        .reverse(),
      // The line every asset in this estate really carries, and the only one:
      // it arrived by import, and `assets::insert_asset` writes no `created`
      // line beside the origin line (#439, story 23). The actor is `import`
      // and not `user`, which is the whole of `assets::HAND_EDITED`'s
      // question -- a fixture that said `user` here would draw a pane in which
      // every property is hand-edited and frozen against the next import.
      {
        id: 1,
        at: SYNCED_AT,
        actor: "import",
        verb: "imported",
        entity_id: asset.id,
        detail: { estate: ESTATE.name },
      },
    ],
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

/** `get_entity`, including the two refusals the detail view branches on. */
function getEntity(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (!id.includes(":") || id.startsWith(":") || id.endsWith(":")) {
    throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key`, source_id: null };
  }
  const entry = CORPUS.find((candidate) => candidate.entity_id === id);
  if (!entry) {
    throw { code: "not_found", message: `${id} is not in the local index`, source_id: null };
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
