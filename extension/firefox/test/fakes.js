// Stand-ins for the browser.* surfaces the router touches and for knobas's
// Native Messaging host behind `runtime.connectNative`. The host speaks the
// shapes in crates/knobas-app/src/link_handoff/protocol.rs (ADR-0061).

/**
 * A fake `dev.knobas.desktop` host. `answer(request)` returns the response
 * body (without `id`), `"hang"` for no answer, or `"disconnect"` to drop the
 * port. `installed: false` behaves like Firefox with no host manifest: the
 * port disconnects at once with "No such native application".
 */
export function fakeHost({ installed = true, answer = () => ({ ok: false, error: "knobas_not_running" }) } = {}) {
  const host = {
    installed,
    answer,
    /** Every request any port carried, in order. */
    sent: [],
    connects: 0,
    ports: [],
    connect() {
      host.connects += 1;
      const messageListeners = [];
      const disconnectListeners = [];
      const port = {
        error: null,
        onMessage: { addListener: (f) => messageListeners.push(f) },
        onDisconnect: { addListener: (f) => disconnectListeners.push(f) },
        postMessage(message) {
          if (port.dead) throw new Error("Attempt to postMessage on disconnected port");
          host.sent.push(message);
          const reply = host.answer(message);
          if (reply === "hang") return;
          setImmediate(() => {
            if (port.dead) return;
            if (reply === "disconnect") return drop(null);
            for (const f of messageListeners) f({ ...reply, id: message.id });
          });
        },
        disconnect() {
          port.dead = true;
        },
      };
      const drop = (message) => {
        port.dead = true;
        port.error = message ? { message } : null;
        for (const f of disconnectListeners) f(port);
      };
      host.ports.push(port);
      if (!host.installed) setImmediate(() => drop("No such native application dev.knobas.desktop"));
      return port;
    },
    /** Requests of one type. */
    of(type) {
      return host.sent.filter((m) => m.type === type);
    },
  };
  return host;
}

/** Tabs by id, each holding the URL it shows now. */
export function fakeTabs(urls = {}) {
  const tabs = {
    urls: new Map(Object.entries(urls).map(([id, url]) => [Number(id), url])),
    removed: [],
    async get(id) {
      if (!tabs.urls.has(id)) throw new Error(`Invalid tab ID: ${id}`);
      return { id, url: tabs.urls.get(id) };
    },
    async remove(id) {
      tabs.removed.push(id);
      tabs.urls.delete(id);
    },
  };
  return tabs;
}

/** `storage.local`. */
export function fakeStorage(initial = {}) {
  const data = { ...initial };
  return {
    data,
    async get(key) {
      return key in data ? { [key]: data[key] } : {};
    },
    async set(items) {
      Object.assign(data, items);
    },
  };
}

/** A settable clock. */
export function fakeClock(start = 1_000_000) {
  const clock = { t: start, now: () => clock.t, advance: (ms) => (clock.t += ms) };
  return clock;
}

let nextRequest = 1;
/** `webRequest.onBeforeRequest` details for a top-level GET. */
export function nav(url, { tabId = 1, requestId, originUrl, type = "main_frame", method = "GET" } = {}) {
  return { url, tabId, requestId: requestId ?? `r${nextRequest++}`, originUrl, type, method };
}

export const JIRA = {
  source_id: "jira",
  display_name: "Tidewater Jira",
  adapter_kind: "jira",
  base_url: "https://jira.tidewater.example",
  scope: "known_items",
  open_inside: true,
  url_forms: [],
};

export const WIKI = {
  source_id: "wiki",
  display_name: "Tidewater Wiki",
  adapter_kind: "confluence",
  base_url: "https://wiki.tidewater.example/confluence",
  scope: "whole_source",
  open_inside: true,
  url_forms: ["/pages/viewpage.action?pageId={}", "/display/{}/{}", "{*}"],
};

export function rulesReply(rules) {
  return { ok: true, type: "rules", protocol_version: 1, rules };
}
