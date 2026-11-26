import assert from "node:assert/strict";
import { test } from "node:test";
import { createRouter } from "../src/router.js";
import { JIRA, WIKI, fakeClock, fakeHost, fakeStorage, fakeTabs, nav, rulesReply } from "./fakes.js";

const PAY = "https://jira.tidewater.example/browse/PAY-231";

/** knobas running with `rules` enabled; `handoff` answered by `onHandoff`. */
function knobas(rules, onHandoff) {
  return (message) => {
    if (message.type === "get_rules") return rulesReply(rules);
    if (message.type === "handoff") return { ok: true, type: "handoff", ...onHandoff(message) };
    return { ok: false, error: "invalid_request" };
  };
}

const opened = () => ({ action: "opened", target: "entity", source_id: "jira", entity_id: "jira:PAY-231", entity_kind: "ticket" });

async function started({ host, tabs = fakeTabs({ 1: "about:blank" }), storage = fakeStorage(), clock = fakeClock(), ...rest }) {
  const router = createRouter({
    connect: () => host.connect(),
    tabs,
    storage,
    now: clock.now,
    timeouts: { rules: 50, handoff: 80 },
    ...rest,
  });
  await router.start();
  return { router, host, tabs, storage, clock };
}

test("a matching link in a new tab is handed to knobas and the tab closes", async () => {
  const { router, host, tabs } = await started({ host: fakeHost({ answer: knobas([JIRA], opened) }) });
  const answer = await router.onBeforeRequest(nav(PAY, { tabId: 1 }));
  assert.deepEqual(answer, { cancel: true });
  assert.deepEqual(host.of("handoff").map((m) => m.url), [PAY]);
  assert.deepEqual(tabs.removed, [1]);
});

test("a link that matches no enabled source loads without a word to knobas", async () => {
  const { router, host } = await started({ host: fakeHost({ answer: knobas([JIRA], opened) }) });
  const before = host.sent.length;
  assert.equal(router.onBeforeRequest(nav("https://example.org/news")), undefined);
  assert.equal(router.onBeforeRequest(nav("https://jira.tidewater.example.org/browse/PAY-1")), undefined);
  assert.equal(host.sent.length, before);
});

/** True when Firefox loads the page: no blocking response, or one that cancels nothing. */
const loads = (answer) => answer === undefined || answer.cancel !== true;

test("a matching link clicked on a page already open in the tab cancels that load and keeps the tab", async () => {
  const tabs = fakeTabs({ 7: "https://chat.example/room/42" });
  const { router } = await started({ host: fakeHost({ answer: knobas([JIRA], opened) }), tabs });
  assert.deepEqual(await router.onBeforeRequest(nav(PAY, { tabId: 7 })), { cancel: true });
  assert.deepEqual(tabs.removed, []);
});

test("Known items only: a page knobas does not mirror loads in Firefox and the tab stays", async () => {
  const host = fakeHost({ answer: knobas([JIRA], () => ({ action: "ignore", reason: "not_known_item", source_id: "jira" })) });
  const { router, tabs } = await started({ host });
  const url = "https://jira.tidewater.example/secure/Dashboard.jspa";
  assert.ok(loads(await router.onBeforeRequest(nav(url, { tabId: 1 }))));
  assert.deepEqual(host.of("handoff").map((m) => m.url), [url]);
  assert.deepEqual(tabs.removed, []);
});

test("Whole source: a page that is not an item opens in the source's site inside knobas and the tab closes", async () => {
  const site = "https://wiki.tidewater.example/confluence/spaces/ENG/overview";
  const host = fakeHost({ answer: knobas([JIRA, WIKI], (m) => ({ action: "opened", target: "site", source_id: "wiki", url: m.url })) });
  const { router, tabs } = await started({ host });
  assert.deepEqual(await router.onBeforeRequest(nav(site, { tabId: 1 })), { cancel: true });
  assert.deepEqual(host.of("handoff").map((m) => m.url), [site]);
  assert.deepEqual(tabs.removed, [1]);
});

test("a page knobas sent to the browser loads, and its redirects and later clicks in that tab stay in Firefox", async () => {
  let bypassLeft = 1;
  const host = fakeHost({
    answer: knobas([JIRA], () => (bypassLeft-- > 0 ? { action: "bypass" } : opened())),
  });
  const tabs = fakeTabs({ 1: "about:blank" });
  const { router } = await started({ host, tabs });
  const first = nav(PAY, { tabId: 1, requestId: "chain" });
  assert.ok(loads(await router.onBeforeRequest(first)));
  // Jira bounces the same request through its login and back: one request id.
  assert.ok(loads(await router.onBeforeRequest(nav(`${PAY}?login=1`, { tabId: 1, requestId: "chain" }))));
  // A script on the login page sends the tab back to the ticket: a new request in that tab.
  tabs.urls.set(1, "https://sso.example/login");
  assert.ok(loads(await router.onBeforeRequest(nav(PAY, { tabId: 1, originUrl: "https://sso.example/login" }))));
  assert.equal(host.of("handoff").length, 1);
  assert.deepEqual(tabs.removed, []);
  // Another tab is not covered by that bypass.
  tabs.urls.set(2, "about:blank");
  assert.deepEqual(await router.onBeforeRequest(nav(PAY, { tabId: 2 })), { cancel: true });
});

test("a link on a page of the same source stays in Firefox", async () => {
  const host = fakeHost({ answer: knobas([JIRA], opened) });
  const { router } = await started({ host, tabs: fakeTabs({ 3: "https://jira.tidewater.example/browse/PAY-1" }) });
  const answer = router.onBeforeRequest(nav(PAY, { tabId: 3, originUrl: "https://jira.tidewater.example/browse/PAY-1" }));
  assert.ok(loads(await answer));
  assert.equal(host.of("handoff").length, 0);
});

test("only top-level GET navigations in a tab are looked at", async () => {
  const host = fakeHost({ answer: knobas([JIRA], opened) });
  const { router } = await started({ host });
  assert.equal(router.onBeforeRequest(nav(PAY, { method: "POST" })), undefined);
  assert.equal(router.onBeforeRequest(nav(PAY, { type: "sub_frame" })), undefined);
  assert.equal(router.onBeforeRequest(nav(PAY, { tabId: -1 })), undefined);
  assert.equal(host.of("handoff").length, 0);
});

test("pause on the toolbar stops interception at once, survives a restart, and resume brings it back", async () => {
  const host = fakeHost({ answer: knobas([JIRA], opened) });
  const storage = fakeStorage();
  const statuses = [];
  const clock = fakeClock();
  const { router } = await started({ host, storage, clock, onStatus: (s) => statuses.push(s) });
  await router.togglePause();
  assert.equal(router.status().paused, true);
  assert.equal(statuses.at(-1).paused, true);
  const sent = host.sent.length;
  clock.advance(60_000); // stale rules are not refreshed while paused either
  assert.equal(router.onBeforeRequest(nav(PAY)), undefined);
  assert.equal(host.sent.length, sent, "paused, the extension sends knobas nothing");

  const again = await started({ host, storage });
  assert.equal(again.router.status().paused, true);
  assert.equal(again.router.onBeforeRequest(nav(PAY)), undefined);
  await again.router.togglePause();
  assert.equal(again.router.status().paused, false);
  assert.deepEqual(await again.router.onBeforeRequest(nav(PAY)), { cancel: true });
});

test("a navigation before startup has read the pause state and the rules waits for both", async () => {
  for (const paused of [true, false]) {
    const host = fakeHost({ answer: knobas([JIRA], opened) });
    const storage = fakeStorage({ paused });
    const router = createRouter({ connect: () => host.connect(), tabs: fakeTabs({ 1: "about:blank" }), storage, timeouts: { rules: 50, handoff: 80 } });
    const starting = router.start();
    const early = router.onBeforeRequest(nav(PAY));
    await starting;
    if (paused) {
      assert.ok(loads(await early));
      assert.equal(host.of("handoff").length, 0);
    } else {
      assert.deepEqual(await early, { cancel: true });
    }
  }
});

test("with knobas closed the page loads, and so it does when the host is missing, hangs or drops", async (t) => {
  await t.test("knobas not running", async () => {
    let running = true;
    const host = fakeHost({
      answer: (m) => (running ? knobas([JIRA], opened)(m) : { ok: false, error: "knobas_not_running", message: "knobas is not running" }),
    });
    const { router, tabs } = await started({ host });
    running = false;
    assert.ok(loads(await router.onBeforeRequest(nav(PAY, { tabId: 1 }))));
    assert.deepEqual(tabs.removed, []);
  });
  await t.test("knobas still starting", async () => {
    const host = fakeHost({ answer: knobas([JIRA], () => ({})) });
    host.answer = (m) => (m.type === "get_rules" ? rulesReply([JIRA]) : { ok: false, error: "not_ready" });
    const { router, tabs } = await started({ host });
    assert.ok(loads(await router.onBeforeRequest(nav(PAY, { tabId: 1 }))));
    assert.deepEqual(tabs.removed, []);
  });
  await t.test("no host manifest: no rules, so nothing is intercepted", async () => {
    const host = fakeHost({ installed: false });
    const { router } = await started({ host });
    assert.equal(router.onBeforeRequest(nav(PAY)), undefined);
    assert.equal(router.status().knobas, "no_host");
  });
  await t.test("host never answers", async () => {
    const host = fakeHost({ answer: knobas([JIRA], opened) });
    const { router, tabs } = await started({ host });
    host.answer = () => "hang";
    const t0 = Date.now();
    assert.ok(loads(await router.onBeforeRequest(nav(PAY, { tabId: 1 }))));
    assert.ok(Date.now() - t0 >= 70, "waited for the handoff timeout");
    assert.deepEqual(tabs.removed, []);
  });
  await t.test("host drops mid-request, and the next request reconnects", async () => {
    const host = fakeHost({ answer: knobas([JIRA], opened) });
    const { router, tabs } = await started({ host, timeouts: { rules: 50, handoff: 5000 } });
    host.answer = () => "disconnect";
    const t0 = Date.now();
    assert.ok(loads(await router.onBeforeRequest(nav(PAY, { tabId: 1 }))));
    assert.ok(Date.now() - t0 < 1000, "a dropped port answers at once, not at the timeout");
    assert.deepEqual(tabs.removed, []);
    host.answer = knobas([JIRA], opened);
    assert.deepEqual(await router.onBeforeRequest(nav(PAY, { tabId: 1 })), { cancel: true });
    assert.equal(host.connects, 2);
  });
});

test("rules are read at startup and again on the first navigation after they go stale", async () => {
  let running = false;
  const host = fakeHost({
    answer: (m) => (running ? knobas([JIRA], opened)(m) : { ok: false, error: "knobas_not_running", message: "knobas is not running" }),
  });
  const clock = fakeClock();
  const { router } = await started({ host, clock, rulesTtlMs: 10_000 });
  assert.equal(host.of("get_rules").length, 1);
  assert.equal(router.status().knobas, "not_running");
  // knobas starts after Firefox; within the TTL nothing is asked.
  running = true;
  assert.equal(router.onBeforeRequest(nav(PAY)), undefined);
  assert.equal(host.of("get_rules").length, 1);
  clock.advance(10_001);
  assert.deepEqual(await router.onBeforeRequest(nav(PAY)), { cancel: true });
  assert.equal(host.of("get_rules").length, 2);
  assert.equal(router.status().knobas, "running");
  assert.equal(router.status().sources, 1);
  // A source turned off in knobas stops matching once the rules refresh.
  host.answer = knobas([], opened);
  clock.advance(10_001);
  assert.ok(loads(await router.onBeforeRequest(nav(PAY))));
  assert.equal(host.of("handoff").length, 1);
});

test("rules from another protocol version are not used", async () => {
  const host = fakeHost({ answer: (m) => (m.type === "get_rules" ? { ...rulesReply([JIRA]), protocol_version: 2 } : opened()) });
  const { router } = await started({ host });
  assert.equal(router.onBeforeRequest(nav(PAY)), undefined);
  assert.equal(router.status().knobas, "incompatible");
});
