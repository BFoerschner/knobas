import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { EXTENSION_ID, HANDOFF_TIMEOUT_MS, HOST_NAME, PROTOCOL_VERSION } from "../src/protocol.js";
import { toolbarFor } from "../src/toolbar.js";
import { JIRA, fakeHost, fakeTabs, rulesReply } from "./fakes.js";

const here = dirname(fileURLToPath(import.meta.url));
const ext = join(here, "..");
const repo = join(ext, "..", "..");
const manifest = JSON.parse(readFileSync(join(ext, "manifest.json"), "utf8"));

test("the identifiers match knobas's side of the protocol", () => {
  const rust = readFileSync(join(repo, "crates/knobas-app/src/link_handoff/protocol.rs"), "utf8");
  const constant = (name) => rust.match(new RegExp(`pub const ${name}: [^=]+= "?([^";]+)"?;`))?.[1];
  assert.equal(constant("HOST_NAME"), HOST_NAME);
  assert.equal(constant("EXTENSION_ID"), EXTENSION_ID);
  assert.equal(Number(constant("PROTOCOL_VERSION")), PROTOCOL_VERSION);
  assert.equal(manifest.browser_specific_settings.gecko.id, EXTENSION_ID);
});

test("the handoff wait outlasts the host's 2 s bridge timeout", () => {
  // Shorter, and knobas could open an item after Firefox had loaded the page too.
  assert.ok(HANDOFF_TIMEOUT_MS > 2000);
});

test("every file the manifest and the background page name is in the extension", () => {
  const named = [manifest.background.page, manifest.browser_action.default_icon, ...Object.values(manifest.icons), "icons/off.svg"];
  const page = readFileSync(join(ext, manifest.background.page), "utf8");
  named.push(...[...page.matchAll(/src="([^"]+)"/g)].map((m) => m[1]));
  for (const file of named) assert.ok(existsSync(join(ext, file)), `${file} is missing`);
  for (const permission of ["nativeMessaging", "webRequest", "webRequestBlocking", "tabs", "storage"]) {
    assert.ok(manifest.permissions.includes(permission), `${permission} permission`);
  }
});

/** A `browser` global good enough for background.js, recording what it was told. */
function fakeBrowser(host, tabs) {
  const listeners = {};
  const on = (name) => ({ addListener: (f, ...args) => (listeners[name] = { f, args }) });
  const painted = [];
  return {
    listeners,
    painted,
    browser: {
      runtime: { connectNative: (name) => (assert.equal(name, HOST_NAME), host.connect()) },
      webRequest: { onBeforeRequest: on("request") },
      browserAction: {
        onClicked: on("click"),
        setIcon: ({ path }) => painted.push(["icon", path]),
        setBadgeText: ({ text }) => painted.push(["badge", text]),
        setTitle: ({ title }) => painted.push(["title", title]),
      },
      tabs: { onRemoved: on("tabRemoved"), get: (id) => tabs.get(id), remove: (id) => tabs.remove(id) },
      windows: { onFocusChanged: on("focus"), WINDOW_ID_NONE: -1 },
      storage: { local: { get: async () => ({}), set: async () => {} } },
    },
  };
}

test("the background page blocks top-level loads and pauses from the toolbar button", async () => {
  const host = fakeHost({
    answer: (m) => (m.type === "get_rules" ? rulesReply([JIRA]) : { ok: true, type: "handoff", action: "opened", target: "entity" }),
  });
  const tabs = fakeTabs({ 1: "about:blank" });
  const fake = fakeBrowser(host, tabs);
  globalThis.browser = fake.browser;
  try {
    await import("../src/background.js");
    const [filter, extra] = fake.listeners.request.args;
    assert.deepEqual(filter.types, ["main_frame"]);
    assert.deepEqual(extra, ["blocking"]);
    await new Promise((r) => setTimeout(r, 20));
    const details = { url: "https://jira.tidewater.example/browse/PAY-1", tabId: 1, requestId: "w1", type: "main_frame", method: "GET" };
    assert.deepEqual(await fake.listeners.request.f(details), { cancel: true });
    assert.deepEqual(tabs.removed, [1]);

    await fake.listeners.click.f();
    assert.deepEqual(fake.painted.filter(([k]) => k === "badge").at(-1), ["badge", "off"]);
    tabs.urls.set(2, "about:blank");
    assert.equal(fake.listeners.request.f({ ...details, tabId: 2, requestId: "w2" }), undefined);
  } finally {
    delete globalThis.browser;
  }
});

test("the toolbar says what a click does and why links stay in Firefox", () => {
  assert.equal(toolbarFor({ paused: true, knobas: "running", sources: 2 }).badge, "off");
  assert.match(toolbarFor({ paused: true, knobas: "running", sources: 2 }).title, /paused.*resume/);
  assert.match(toolbarFor({ paused: false, knobas: "running", sources: 2 }).title, /2 sources open in knobas.*pause/);
  assert.match(toolbarFor({ paused: false, knobas: "running", sources: 0 }).title, /no source/);
  assert.match(toolbarFor({ paused: false, knobas: "not_running", sources: 0 }).title, /not running/);
  assert.match(toolbarFor({ paused: false, knobas: "no_host", sources: 0 }).title, /not installed/);
  assert.equal(toolbarFor({ paused: false, knobas: "running", sources: 1 }).icon, "icons/on.svg");
});
