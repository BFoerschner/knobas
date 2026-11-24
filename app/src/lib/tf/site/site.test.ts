/**
 * The sites' store: a site's first page on its own address marks its sign-in kept, as the native side records it
 * (`landed`, *Kept since*), so Settings says so in the same run (the ux30 gate's real app on Windows: "Not signed in
 * here yet" and no *Forget…* right after the site signed in, until knobas started again).
 */
import { expect, test } from "vitest";

import type { SiteSetting, SiteStatus } from "../../ipc/site";
import { createSiteStore } from "./site.svelte";

const SETTING: SiteSetting = {
  source: "wiki",
  open_inside: true,
  base: "https://wiki.example",
  own: "https://wiki.example",
  allowed: [],
  suggested: [],
  kept_since: null,
  offered: true,
  persists: true,
  sign_in_user: null,
  own_sign_in: null,
  confined: false,
};

function status(over: Partial<SiteStatus> = {}): SiteStatus {
  return {
    source: "wiki",
    phase: "live",
    url: "https://wiki.example/display/PAY/Retry",
    title: "SEPA payout retry design",
    own: true,
    loading: false,
    can_back: true,
    can_forward: false,
    shown: false,
    keyboard: "site",
    asks_origin: null,
    failure: null,
    esc_again_ms: 500,
    fill: false,
    ...over,
  };
}

test("a site's first page on its own address marks its sign-in kept from now", () => {
  const store = createSiteStore();
  store.hold([SETTING]);
  const before = Date.now();
  store.update(status());
  const since = store.settings.get("wiki")?.kept_since;
  expect(since).toBeTypeOf("string");
  const at = new Date(since as string).getTime();
  expect(at).toBeGreaterThanOrEqual(before - 1000);
  expect(at).toBeLessThanOrEqual(Date.now() + 1000);
});

test("a page on another address keeps nothing", () => {
  const store = createSiteStore();
  store.hold([SETTING]);
  store.update(status({ own: false, url: "https://sso.example/login" }));
  expect(store.settings.get("wiki")?.kept_since).toBeNull();
});

test("a sign-in already kept keeps its date", () => {
  const store = createSiteStore();
  store.hold([{ ...SETTING, kept_since: "2026-11-20T08:00:00Z" }]);
  store.update(status());
  expect(store.settings.get("wiki")?.kept_since).toBe("2026-11-20T08:00:00Z");
});

test("a status for a source with no setting read adds none", () => {
  const store = createSiteStore();
  store.hold([]);
  store.update(status());
  expect(store.settings.has("wiki")).toBe(false);
});

test("after Forget… the next page on its own address marks it kept again", () => {
  const store = createSiteStore();
  store.hold([{ ...SETTING, kept_since: "2026-11-20T08:00:00Z" }]);
  store.holdOne({ ...SETTING, kept_since: null });
  store.closed("wiki");
  store.update(status());
  expect(store.settings.get("wiki")?.kept_since).not.toBeNull();
});

test("the settings differ from the source list once a source is added or removed, and not before they are read", () => {
  const store = createSiteStore();
  expect(store.differs(["wiki"])).toBe(false);
  store.hold([SETTING, { ...SETTING, source: "wiki2" }]);
  expect(store.differs(["wiki", "wiki2"])).toBe(false);
  expect(store.differs(["wiki2", "wiki"])).toBe(false);
  expect(store.differs(["wiki", "wiki2", "jira"]), "a source added since").toBe(true);
  expect(store.differs(["wiki2"]), "a source removed since keeps its site's setting until read again").toBe(true);
});
