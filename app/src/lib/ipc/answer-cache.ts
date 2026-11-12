/**
 * **The answer cache** (the instant-open pass of PR #2479, 2026-11-12,
 * `docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`):
 * the last answer to each read of the mirror, kept in memory, so a page the
 * kept pages dropped, or one opened for the first time after the cache was
 * warmed, draws in the frame after the press instead of waiting for the
 * backend again.
 *
 * **Same answers.** Only the reads in {@link READS} are kept: each is a
 * function of the mirror and the settings, with no side effect. An answer is
 * dropped the moment the mirror may have moved under it:
 *
 * - a sync run ends with something new (`sync:state` with `last_changed`):
 *   every answer a source filling one of that source's roles could change
 *   ({@link SCOPE}; a read with no scope is dropped by every such run);
 * - a refresh of one item ends (`item:refreshed`) or a held view moved
 *   (`view:fresh`): every answer;
 * - any command that is not a read of {@link READS} or {@link QUIET}: a write,
 *   or a read this file does not know. Every answer, except that a visit
 *   (`visit_entity`) keeps the reads that never read visits ({@link VISIT_BLIND}),
 *   the timer's heartbeat drops only the time reads ({@link TIME_READS}), and
 *   the merge follower (`follow_merges`, asked every few seconds) drops
 *   everything only when it moved a ticket.
 *
 * So a kept answer is what the backend would answer now, as a kept page's
 * drawing is: the kept pages hold what they drew until a run that changed
 * something ends (`TfKeptPage.svelte`), and this holds the reads behind them
 * the same way.
 *
 * **Warm.** {@link warm} asks again, in the background and one at a time, the
 * reads the window made most recently (kept in `localStorage` between
 * sessions, at most {@link WARM_KEYS}): at start, so a page's first open finds
 * its answer, and after a run drops answers, so a return finds the new one.
 *
 * Off until {@link enableAnswerCache}, which puts it in front of the bridge's
 * own `invoke` (`window.__TAURI_INTERNALS__`), so every command of every
 * module passes through it. A test mounts a page and counts its reads, and
 * every read there goes to the port it set up.
 */
/** The bridge's own `invoke`, as `window.__TAURI_INTERNALS__` holds it. */
type Invoke = (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;

/** The reads kept: a function of the mirror and the settings, and nothing else. */
export const READS: ReadonlySet<string> = new Set([
  "list_entities",
  "get_entity",
  "docs_tree",
  "docs_outline",
  "docs_nodes",
  "docs_page",
  "inbox_items",
  "inbox_count",
  "code_facts",
  "entity_briefs",
  "card_reads",
  "pipelines",
  "pipeline_runs",
  "named_pipelines",
  "pipeline_settings",
  "run_log",
  "run_chain",
  "monitor_roster",
  "monitors_down",
  "open_alerts",
  "unmonitored_assets",
  "mini_board",
  "ticket_views",
  "filter_suggestions",
  "standup_digest",
  "standup_protocol",
  "job_reach",
  "list_sources",
  "list_adapters",
  "keys",
  "day_blocks",
  "get_note",
  "checkout_settings",
  "sync_status",
  "credential_health",
]);

/**
 * Commands that change nothing a kept read answers, so they drop nothing: the
 * status the shell polls, what the window records about itself, and the reads
 * that go past the mirror (a checkout's git, a source's own view) and are
 * never kept.
 */
export const QUIET: ReadonlySet<string> = new Set([
  "app_status",
  "write_queue_counts",
  "pending_writes",
  "current_timer",
  "frontend_ready",
  "record_capture_context",
  "clone_all_state",
  "pages_kept_open",
  "refused_writes",
  "notification_kinds",
  "diff_pipelines",
  "command_steps",
  "renderer_defaults",
  "user_renderers",
  "list_installed_adapters",
  "standup_publish_target",
  "list_contexts",
  "entity_view",
  "entity_checkout",
  "entity_checkouts",
  "entity_thread",
  "job_link_sets",
  "entity_range_diff",
  "docs_search",
  "search",
  "launcher_home",
]);

/** The reads a visit (`visit_entity`) cannot move: none of them reads the activity log. */
export const VISIT_BLIND: ReadonlySet<string> = new Set([
  "list_entities",
  "docs_nodes",
  "docs_page",
  "code_facts",
  "entity_briefs",
  "pipelines",
  "pipeline_runs",
  "named_pipelines",
  "pipeline_settings",
  "run_log",
  "run_chain",
  "monitor_roster",
  "monitors_down",
  "open_alerts",
  "unmonitored_assets",
  "mini_board",
  "ticket_views",
  "filter_suggestions",
  "list_sources",
  "list_adapters",
  "checkout_settings",
]);

/**
 * The sources' own status, which a page draws a line from above what it lists (a source whose last sync failed):
 * kept like the mirror's answers, so the line is there in the first frame, and dropped by every event that moves it,
 * a run's start or end (`sync:state`) and a credential's health (`source:health`), whatever the run changed.
 */
export const STATUS_READS: Readonly<Record<string, "sync" | "health">> = { sync_status: "sync", credential_health: "health" };

/** What the timer's heartbeat (`timer_heartbeat`) can move: it stores an observation of what the reader has open. */
export const TIME_READS: ReadonlySet<string> = new Set(["day_blocks", "standup_digest"]);

/** The roles whose sources' runs can change a read's answer; a read not here is changed by any run. */
const SCOPE: Readonly<Record<string, readonly string[]>> = {
  docs_tree: ["wiki"],
  docs_outline: ["wiki"],
  docs_nodes: ["wiki"],
  docs_page: ["wiki"],
  pipelines: ["ci"],
  pipeline_runs: ["ci"],
  named_pipelines: ["ci"],
  run_log: ["ci"],
  run_chain: ["ci"],
  monitor_roster: ["monitoring"],
  monitors_down: ["monitoring"],
  open_alerts: ["monitoring"],
  unmonitored_assets: ["monitoring"],
  code_facts: ["code-host"],
  mini_board: ["ticket-system"],
  ticket_views: ["ticket-system"],
  filter_suggestions: ["ticket-system"],
};

/** `list_entities`' kinds, by the role whose sources hold them. */
const KIND_ROLE: Readonly<Record<string, string>> = {
  pr: "code-host",
  repo: "code-host",
  branch: "code-host",
  commit: "code-host",
  ticket: "ticket-system",
  page: "wiki",
  build: "ci",
  build_config: "ci",
  monitor: "monitoring",
};

/** How many answers are kept, the least recently used dropped first. */
export const KEPT_ANSWERS = 400;
/** How many recent reads {@link warm} asks again, and `localStorage` keeps between sessions. */
export const WARM_KEYS = 120;
const STORE_KEY = "knobas.answer-cache.recent";

/**
 * Reads every session makes, with the arguments the pages ask them with, warmed even when no session before
 * remembered any: a new profile's first open of Docs, Builds and Monitors finds them too.
 */
const SEED: readonly { cmd: string; args: unknown }[] = [
  { cmd: "docs_outline", args: {} },
  { cmd: "pipelines", args: {} },
  { cmd: "monitor_roster", args: {} },
  { cmd: "open_alerts", args: {} },
  { cmd: "ticket_views", args: {} },
  { cmd: "inbox_items", args: { shelf: "stream" } },
];

interface Kept {
  cmd: string;
  args: unknown;
  /** The roles whose runs drop it, `null` for every run's. */
  roles: readonly string[] | null;
  value: unknown;
}

let enabled = false;
/** The bridge's `invoke` behind the cache. */
let inner: Invoke = () => Promise.reject(new Error("answer cache: no bridge"));
const kept = new Map<string, Kept>();
const asking = new Map<string, { generation: number; answer: Promise<unknown> }>();
/** Moves on every drop: an answer asked before a drop is not kept, since the mirror may have moved under it. */
let generation = 0;
/** The reads made, most recent last: what {@link warm} asks again. */
const recent = new Map<string, { cmd: string; args: unknown }>();
/** Each source's claimed roles, as the last `list_sources` answer said. */
const rolesOf = new Map<string, string[]>();

function keyOf(cmd: string, args: unknown): string {
  return `${cmd} ${JSON.stringify(args ?? {})}`;
}

function rolesFor(cmd: string, args: unknown): readonly string[] | null {
  if (cmd === "list_entities") {
    const kinds = (args as { filter?: { kinds?: unknown } } | undefined)?.filter?.kinds;
    if (!Array.isArray(kinds) || kinds.length === 0) return null;
    const roles = kinds.map((kind) => KIND_ROLE[String(kind)]);
    return roles.every((role) => role !== undefined) ? [...new Set(roles as string[])] : null;
  }
  return SCOPE[cmd] ?? null;
}

/** A copy the caller may change: a page sorting or filling in what it was handed changes nothing kept. */
function copy<T>(value: T): T {
  return value === null || typeof value !== "object" ? value : structuredClone(value);
}

/** The last drops and what made them, for a probe (`window.__knobasAnswers`). */
const drops: { why: string; at: number; left: number }[] = [];
let why = "";

/** Drop every kept answer `keep` does not hold on to. */
function drop(keep: (entry: Kept) => boolean = () => false): void {
  generation += 1;
  queueMicrotask(() => {
    drops.push({ why, at: Math.round(performance.now()), left: kept.size });
    if (drops.length > 50) drops.shift();
  });
  for (const [key, entry] of kept) if (!keep(entry)) kept.delete(key);
  asking.clear();
}

/** Drop the status reads `event` moves, and nothing else: no answer of the mirror's waited on them. */
let statusMoves = 0;
function dropStatus(event: "sync" | "health"): void {
  statusMoves += 1;
  for (const [key, entry] of kept) if (STATUS_READS[entry.cmd] === event) kept.delete(key);
  for (const [key] of asking) if (key.startsWith(event === "sync" ? "sync_status " : "credential_health ")) asking.delete(key);
}

/** A source's run ended with something new: drop what a source of its roles could change. */
export function runChanged(sourceId: string): void {
  const roles = rolesOf.get(sourceId);
  // A source whose roles were never read: every answer, to be sure.
  if (roles === undefined) return drop();
  drop((entry) => entry.roles !== null && !entry.roles.some((role) => roles.includes(role)));
}

/** Drop every kept answer. */
export function forgetAnswers(): void {
  drop();
}

function remember(cmd: string, args: unknown): void {
  const key = keyOf(cmd, args);
  recent.delete(key);
  recent.set(key, { cmd, args });
  while (recent.size > WARM_KEYS) recent.delete(recent.keys().next().value!);
}

function saveRecent(): void {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify([...recent.values()]));
  } catch {
    // No storage (a private window, a test): the next start warms nothing, which is how it was.
  }
}

/**
 * The bridge's `invoke`, answering a read of {@link READS} from memory when it
 * holds the answer.
 */
export function cachedInvoke(cmd: string, args?: unknown, options?: unknown): Promise<unknown> {
  return ask(cmd, args, options, false);
}

/**
 * {@link cachedInvoke}; `warmed` for {@link warm}'s own reads, whose answer nobody holds, so it is kept as it came and
 * not copied: a copy of the 571 KB pipeline list is a long task of its own on a slow machine.
 */
function ask(cmd: string, args: unknown, options: unknown, warmed: boolean): Promise<unknown> {
  if (!enabled) return inner(cmd, args, options);
  why = cmd;
  if (!READS.has(cmd)) {
    if (cmd === "follow_merges") {
      // A pass that moves the tickets whose pull request merged: it changed the mirror only if it moved one.
      return inner(cmd, args, options).then((moved) => {
        if (moved !== 0) drop();
        return moved;
      });
    }
    if (cmd === "visit_entity") drop((entry) => VISIT_BLIND.has(entry.cmd));
    else if (cmd === "timer_heartbeat") drop((entry) => !TIME_READS.has(entry.cmd));
    else if (!QUIET.has(cmd) && !cmd.startsWith("plugin:")) drop();
    return inner(cmd, args, options);
  }
  remember(cmd, args);
  const key = keyOf(cmd, args);
  const held = kept.get(key);
  if (held !== undefined) {
    kept.delete(key);
    kept.set(key, held);
    return Promise.resolve(copy(held.value));
  }
  const inFlight = asking.get(key);
  // A second asker of a read on its way shares it, with a copy of what was kept (the first may change its own).
  if (inFlight !== undefined && inFlight.generation === generation) return inFlight.answer.then((value) => copy(kept.get(key)?.value ?? value));
  const at = generation;
  const statusAt = statusMoves;
  const answer = inner(cmd, args, options).then((value) => {
    if (cmd === "list_sources" && Array.isArray(value)) {
      for (const row of value as { id?: unknown; roles?: { role?: unknown }[] }[]) {
        if (typeof row.id === "string") rolesOf.set(row.id, (row.roles ?? []).map((claim) => String(claim.role)));
      }
    }
    // Kept as a copy taken before the asker sees it, and only when nothing dropped answers meanwhile.
    if (at === generation && (STATUS_READS[cmd] === undefined || statusAt === statusMoves)) {
      kept.set(key, { cmd, args, roles: rolesFor(cmd, args), value: warmed ? value : copy(value) });
      while (kept.size > KEPT_ANSWERS) kept.delete(kept.keys().next().value!);
    }
    return value;
  });
  asking.set(key, { generation: at, answer });
  const done = () => {
    if (asking.get(key)?.answer === answer) asking.delete(key);
  };
  void answer.then(done, done);
  return answer;
}

/** The events that move the mirror under kept answers, as the window hears them. */
export interface CacheEvents {
  /** `sync:state`: a run's transition; a finished one that changed something drops what its source could change. */
  syncState(payload: { source_id?: string; running?: boolean; last_changed?: boolean | null }): void;
  /** `item:refreshed`, `view:fresh`, `checkout:fetched`: every answer. */
  moved(): void;
  /** `source:health`: the credential's health. */
  health(): void;
}

/** The ears {@link enableAnswerCache}'s caller wires to the window's events. */
export const cacheEvents: CacheEvents = {
  syncState(payload) {
    why = "a run's start or end";
    dropStatus("sync");
    if (payload.running === true || payload.last_changed !== true || typeof payload.source_id !== "string") return;
    why = `run of ${payload.source_id}`;
    runChanged(payload.source_id);
    scheduleWarm();
  },
  moved() {
    why = "an item or view moved";
    drop();
  },
  health() {
    why = "a credential's health";
    dropStatus("health");
  },
};

/**
 * Put the cache in front of the bridge (`window.__TAURI_INTERNALS__.invoke`), with the reads the last session made
 * remembered for {@link warm}. Once; no bridge, no cache.
 */
export function enableAnswerCache(): void {
  const bridge = (window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke } }).__TAURI_INTERNALS__;
  if (enabled || typeof bridge?.invoke !== "function") return;
  if (bridge.invoke !== cachedInvoke) {
    inner = bridge.invoke.bind(bridge);
    bridge.invoke = cachedInvoke;
  }
  enabled = true;
  Object.defineProperty(window, "__knobasAnswers", { configurable: true, value: { kept: () => kept.size, drops } });
  // The seeds first, so what the last session read is warmed before them.
  for (const { cmd, args } of SEED) remember(cmd, args);
  try {
    const stored = JSON.parse(localStorage.getItem(STORE_KEY) ?? "[]") as { cmd: string; args: unknown }[];
    for (const { cmd, args } of stored) if (READS.has(cmd)) remember(cmd, args);
  } catch {
    // Nothing stored, or nothing readable: the seeds alone are warmed.
  }
  if (!saving) {
    saving = true;
    for (const event of ["wheel", "scroll", "pointerdown", "keydown", "touchstart"]) addEventListener(event, heard, { capture: true, passive: true });
    addEventListener("pagehide", saveRecent);
    setInterval(saveRecent, 60_000);
  }
}
let saving = false;

/** Off and empty, for a test (the bridge keeps the wrapper, which now only passes through). */
export function disableAnswerCache(): void {
  enabled = false;
  drop();
  recent.clear();
  rolesOf.clear();
}

/** Whether the cache is on, for a test. */
export function readCacheEnabled(): boolean {
  return enabled;
}

let warming: Promise<void> | null = null;
let warmTimer: ReturnType<typeof setTimeout> | null = null;

/** Warm a moment after the last run that dropped answers, so a burst of run ends warms once. */
function scheduleWarm(): void {
  if (warmTimer !== null) clearTimeout(warmTimer);
  warmTimer = setTimeout(() => {
    warmTimer = null;
    void warm();
  }, 1500);
}

/**
 * Ask again, one at a time and newest first, each recent read whose answer is
 * not kept, so the page that made it draws from memory when it is next
 * opened. `idle` waits between reads for the window to have nothing to do.
 */
export function warm(idle: () => Promise<void> = nextIdle): Promise<void> {
  if (!enabled) return Promise.resolve();
  if (warming !== null) return warming;
  warming = (async () => {
    try {
      for (const { cmd, args } of [...recent.values()].reverse()) {
        if (kept.has(keyOf(cmd, args))) continue;
        await idle();
        await ask(cmd, args, undefined, true).catch(() => {});
      }
    } finally {
      warming = null;
    }
  })();
  return warming;
}

/** When the reader last scrolled, pointed or typed: the warm waits {@link WARM_QUIET_MS} past it. */
let lastInput = 0;
/** How long after the reader's last input the warm asks its next read, so its work never lands in a scroll or a key. */
export const WARM_QUIET_MS = 1000;
function heard(): void {
  lastInput = performance.now();
}

async function nextIdle(): Promise<void> {
  while (performance.now() - lastInput < WARM_QUIET_MS) await new Promise((resolve) => setTimeout(resolve, WARM_QUIET_MS - (performance.now() - lastInput)));
  return idleCallback();
}

function idleCallback(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestIdleCallback === "function") requestIdleCallback(() => resolve(), { timeout: 1000 });
    else setTimeout(resolve, 50);
  });
}
