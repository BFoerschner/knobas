// The background page: wires the router to Firefox. Everything that decides
// anything is in router.js, which the tests drive without a browser.

import { HOST_NAME } from "./protocol.js";
import { createRouter } from "./router.js";
import { toolbarFor } from "./toolbar.js";

function paint(status) {
  const { icon, badge, title } = toolbarFor(status);
  browser.browserAction.setIcon({ path: icon });
  browser.browserAction.setBadgeText({ text: badge });
  browser.browserAction.setTitle({ title });
}

const router = createRouter({
  connect: () => browser.runtime.connectNative(HOST_NAME),
  tabs: { get: (id) => browser.tabs.get(id), remove: (id) => browser.tabs.remove(id) },
  storage: browser.storage.local,
  onStatus: paint,
});

// Registered before anything is awaited, so a link that starts Firefox is seen.
browser.webRequest.onBeforeRequest.addListener((details) => router.onBeforeRequest(details), { urls: ["http://*/*", "https://*/*"], types: ["main_frame"] }, [
  "blocking",
]);
browser.browserAction.onClicked.addListener(() => router.togglePause());
browser.tabs.onRemoved.addListener((tabId) => router.forgetTab(tabId));
// Coming back from knobas, where a source may just have been turned on.
browser.windows.onFocusChanged.addListener((windowId) => {
  if (windowId !== browser.windows.WINDOW_ID_NONE && !router.status().paused) router.refreshRules();
});

paint(router.status());
router.start();
