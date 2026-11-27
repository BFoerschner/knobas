// What happens to a top-level navigation (ADR-0061). The router is the whole
// extension but the `browser.*` wiring in background.js: it caches the rules
// knobas exports, asks knobas's `handoff` about a URL one of them matches,
// and turns the answer into a `webRequest` blocking response.

import { createNativeClient } from "./native.js";
import { HANDOFF_TIMEOUT_MS, PROTOCOL_VERSION, RULES_TIMEOUT_MS, RULES_TTL_MS } from "./protocol.js";
import { matchRule } from "./rules.js";

/** What a tab shows before its first page: a tab opened for this link alone. */
const FRESH_TAB = new Set(["about:blank", "about:newtab", "about:home", "about:privatebrowsing"]);

/**
 * How long a tab knobas sent a page to keeps that page's source in Firefox:
 * long enough for a sign-in detour through another host and back.
 */
export const BYPASS_TAB_MS = 120_000;

/** Request ids already let through, so a server redirect is not asked about again. */
const PASSED_KEEP = 256;

/**
 * @param {object} deps
 * @param {() => object} deps.connect `() => browser.runtime.connectNative(HOST_NAME)`
 * @param {{get(id: number): Promise<{url?: string}>, remove(id: number): Promise<void>}} deps.tabs
 * @param {{get(key: string): Promise<object>, set(items: object): Promise<void>}} deps.storage `browser.storage.local`
 * @param {() => number} [deps.now]
 * @param {{rules?: number, handoff?: number}} [deps.timeouts]
 * @param {number} [deps.rulesTtlMs]
 * @param {(status: object) => void} [deps.onStatus] called whenever `status()` changes
 */
export function createRouter({ connect, tabs, storage, now = Date.now, timeouts = {}, rulesTtlMs = RULES_TTL_MS, onStatus = () => {} }) {
  const native = createNativeClient(connect);
  const rulesTimeout = timeouts.rules ?? RULES_TIMEOUT_MS;
  const handoffTimeout = timeouts.handoff ?? HANDOFF_TIMEOUT_MS;

  let rules = [];
  let rulesAt = -Infinity;
  /** @type {Map<string, true>} request ids let through, oldest first */
  const passed = new Map();
  /** @type {Map<number, {sourceId: string, until: number}>} tabs a bypass left on a source */
  const bypassTabs = new Map();

  function pass(details) {
    passed.set(details.requestId, true);
    if (passed.size > PASSED_KEEP) passed.delete(passed.keys().next().value);
    return {};
  }

  function bypassed(tabId, sourceId) {
    const held = bypassTabs.get(tabId);
    if (!held) return false;
    if (held.until <= now()) {
      bypassTabs.delete(tabId);
      return false;
    }
    return held.sourceId === sourceId;
  }

  let paused = false;
  /** What the last answer said about knobas: see `status()`. */
  let knobas = "unknown";
  let started = null;
  let refreshing = null;

  function status() {
    return { paused, knobas, sources: rules.length };
  }

  function changed(update) {
    const before = JSON.stringify(status());
    update();
    if (JSON.stringify(status()) !== before) onStatus(status());
  }

  /** `knobas` for an answer or a NativeFailure; rules go with an unreachable knobas. */
  function heard(answerOrFailure) {
    const code = answerOrFailure?.ok === false ? answerOrFailure.error : answerOrFailure?.code;
    const reading =
      {
        knobas_not_running: "not_running",
        not_ready: "not_ready",
        no_host: "no_host",
      }[code] ?? (answerOrFailure?.ok === true ? "running" : "error");
    changed(() => {
      knobas = reading;
      if (reading === "not_running" || reading === "no_host") rules = [];
    });
  }

  async function fetchRules() {
    try {
      const answer = await native.request({ type: "get_rules" }, rulesTimeout);
      if (answer.ok && answer.protocol_version !== PROTOCOL_VERSION) {
        changed(() => {
          knobas = "incompatible";
          rules = [];
        });
      } else {
        heard(answer);
        if (answer.ok) changed(() => (rules = Array.isArray(answer.rules) ? answer.rules : []));
      }
    } catch (failure) {
      heard(failure);
    }
    rulesAt = now();
  }

  /** Reads the rules again; one read at a time, callers share it. */
  function refreshRules() {
    refreshing ??= fetchRules().finally(() => (refreshing = null));
    return refreshing;
  }

  async function load() {
    try {
      const saved = await storage.get("paused");
      changed(() => (paused = saved?.paused === true));
    } catch {
      // No saved state: interception stays on.
    }
    await refreshRules();
  }

  async function togglePause() {
    if (started) await started.promise;
    changed(() => (paused = !paused));
    try {
      await storage.set({ paused });
    } catch {
      // Paused for this session only.
    }
    if (!paused) await refreshRules();
  }

  async function handoff(details, rule) {
    const fresh = tabs.get(details.tabId).then(
      (tab) => FRESH_TAB.has(tab.url ?? ""),
      () => false,
    );
    let answer;
    try {
      answer = await native.request({ type: "handoff", url: details.url }, handoffTimeout);
    } catch (failure) {
      heard(failure);
      return pass(details);
    }
    if (answer.ok && answer.action === "bypass") bypassTabs.set(details.tabId, { sourceId: rule.source_id, until: now() + BYPASS_TAB_MS });
    // An answer about this URL (`invalid_url`) says nothing about knobas.
    if (answer.ok || answer.error === "knobas_not_running" || answer.error === "not_ready") heard(answer);
    if (!(answer.ok && answer.action === "opened")) return pass(details);
    if (await fresh) tabs.remove(details.tabId).catch(() => {});
    return { cancel: true };
  }

  /**
   * The `webRequest.onBeforeRequest` blocking listener: `undefined` (load)
   * at once for anything no rule matches, else a promise of `{ cancel: true }`
   * when knobas opened the URL, or `{}` to let Firefox load it.
   */
  function onBeforeRequest(details) {
    if (details.type !== "main_frame" || details.method !== "GET" || details.tabId < 0) return undefined;
    if (passed.has(details.requestId)) return undefined;
    if (!started) return undefined;
    if (started.pending) return started.promise.then(() => route(details));
    if (paused) return undefined;
    if (refreshing || now() - rulesAt > rulesTtlMs) return refreshRules().then(() => route(details));
    return route(details);
  }

  function route(details) {
    if (paused) return undefined;
    const rule = matchRule(rules, details.url);
    if (!rule) return undefined;
    if (details.originUrl && matchRule(rules, details.originUrl)?.source_id === rule.source_id) return pass(details);
    if (bypassed(details.tabId, rule.source_id)) return pass(details);
    return handoff(details, rule);
  }

  function forgetTab(tabId) {
    bypassTabs.delete(tabId);
  }

  return {
    /** Reads the saved pause state, then the rules; navigations meanwhile wait for it. */
    start() {
      const promise = load().finally(() => (started.pending = false));
      started = { promise, pending: true };
      return promise;
    },
    refreshRules,
    togglePause,
    status,
    onBeforeRequest,
    forgetTab,
  };
}
