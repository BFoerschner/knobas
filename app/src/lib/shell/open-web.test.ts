/**
 * Where a web address opens (ADR-0048, decisions §12): the site of its source when that source's pages open inside
 * knobas, else the browser.
 */
import { describe, expect, test, vi } from "vitest";

import type { SiteSetting } from "../ipc/site";
import { createSiteStore } from "../tf/site/site.svelte";
import { openWeb, owner, pressElsewhere, webPlace, webPlaceName, type WebPorts } from "./open-web";

function setting(source: string, base: string, on = true, extra: Partial<SiteSetting> = {}): SiteSetting {
  return {
    source,
    open_inside: on,
    base,
    own: new URL(base).origin,
    allowed: [],
    suggested: [],
    kept_since: null,
    offered: true,
    persists: true,
    ...extra,
  };
}

function store(...settings: SiteSetting[]) {
  const held = createSiteStore();
  held.hold(settings);
  held.nameAll(settings.map((one) => ({ id: one.source, display_name: one.source.toUpperCase() })));
  return held;
}

describe("webPlace", () => {
  test("with the switch off, the browser", () => {
    const s = store(setting("jira", "https://jira.example", false));
    expect(webPlace("https://jira.example/browse/A-1", { source: "jira" }, s)).toEqual({ to: "browser" });
    expect(webPlace("https://jira.example/browse/A-1", {}, s)).toEqual({ to: "browser" });
  });

  test("with it on, an own address opens the site, with or without the source named", () => {
    const s = store(setting("jira", "https://jira.example"));
    expect(webPlace("https://jira.example/browse/A-1", { source: "jira" }, s)).toEqual({ to: "site", source: "jira" });
    expect(webPlace("https://jira.example/browse/A-1", {}, s)).toEqual({ to: "site", source: "jira" });
  });

  test("a press that asks for the other place is the browser", () => {
    const s = store(setting("jira", "https://jira.example"));
    expect(webPlace("https://jira.example/browse/A-1", { source: "jira", elsewhere: true }, s)).toEqual({ to: "browser" });
  });

  test("a foreign origin with the source named is the browser", () => {
    const s = store(setting("gitea", "https://git.example"));
    expect(webPlace("https://gitea.lan.test:3000/o/r", { source: "gitea" }, s)).toEqual({ to: "browser" });
  });

  test("an allowed origin with the source named opens its site", () => {
    const s = store(setting("gitea", "https://git.example", true, { allowed: ["https://gitea.lan.test:3000"] }));
    expect(webPlace("https://gitea.lan.test:3000/o/r", { source: "gitea" }, s)).toEqual({ to: "site", source: "gitea" });
  });

  test("two sources on one origin: the longest base at a segment wins", () => {
    const s = store(setting("jira", "https://tools.example/jira"), setting("jira2", "https://tools.example/jira2"));
    expect(owner("https://tools.example/jira2/browse/B-1", s)).toBe("jira2");
    expect(owner("https://tools.example/jira/browse/A-1", s)).toBe("jira");
    expect(owner("https://tools.example/jiraX", s)).toBeNull();
    expect(webPlace("https://tools.example/jira2/browse/B-1", { source: "jira" }, s)).toEqual({ to: "site", source: "jira2" });
  });

  test("by host alone is not enough: the port and the scheme are the origin's", () => {
    const s = store(setting("jira", "https://jira.example"));
    expect(webPlace("http://jira.example/x", {}, s)).toEqual({ to: "browser" });
    expect(webPlace("https://jira.example:8443/x", {}, s)).toEqual({ to: "browser" });
    expect(webPlace("https://jira.example.evil.test/x", {}, s)).toEqual({ to: "browser" });
  });

  test("an address that is not http or https never reaches a site", () => {
    const s = store(setting("jira", "https://jira.example"));
    for (const url of ["javascript:alert(1)", "file:///etc/passwd", "ms-msdt:x", "not a url"]) {
      expect(webPlace(url, { source: "jira" }, s)).toEqual({ to: "browser" });
    }
  });

  test("a source the platform does not offer opens the browser", () => {
    const s = store(setting("jira", "https://jira.example", true, { offered: false }));
    expect(webPlace("https://jira.example/x", { source: "jira" }, s)).toEqual({ to: "browser" });
  });
});

describe("openWeb", () => {
  function ports(): WebPorts & { calls: string[] } {
    const calls: string[] = [];
    return {
      calls,
      browse: (url) => void calls.push(`browse ${url}`),
      openSite: (source, url) => {
        calls.push(`site ${source} ${url}`);
        return Promise.resolve();
      },
      suggest: (source, origin) => void calls.push(`suggest ${source} ${origin}`),
    };
  }

  test("hands the browser what is not a site's, and a site what is", () => {
    const s = store(setting("jira", "https://jira.example"));
    const p = ports();
    openWeb("https://news.example/", {}, p, s);
    openWeb("https://jira.example/browse/A-1", { source: "jira" }, p, s);
    expect(p.calls).toEqual(["browse https://news.example/", "site jira https://jira.example/browse/A-1"]);
  });

  test("a refused scheme goes to the browser's opener, which refuses it, never to a site", () => {
    const s = store(setting("jira", "https://jira.example"));
    const p = ports();
    openWeb("javascript:alert(1)", { source: "jira" }, p, s);
    expect(p.calls).toEqual(["browse javascript:alert(1)"]);
  });

  test("an Open in source that names an address its site may not show offers it in Settings", () => {
    const s = store(setting("gitea", "https://git.example"));
    const p = ports();
    openWeb("https://gitea.lan.test:3000/o/r", { source: "gitea" }, p, s);
    expect(p.calls).toEqual(["suggest gitea https://gitea.lan.test:3000", "browse https://gitea.lan.test:3000/o/r"]);
  });

  test("the chip names the place", () => {
    const s = store(setting("jira", "https://jira.example"));
    expect(webPlaceName("https://jira.example/x", {}, s)).toBe("JIRA");
    expect(webPlaceName("https://news.example/x", {}, s)).toBe("the browser");
  });

  test("a middle press and a modified press ask for the browser", () => {
    expect(pressElsewhere({ button: 1, metaKey: false, ctrlKey: false })).toBe(true);
    expect(pressElsewhere({ button: 0, metaKey: true, ctrlKey: false })).toBe(true);
    expect(pressElsewhere({ button: 0, metaKey: false, ctrlKey: true })).toBe(true);
    expect(pressElsewhere({ button: 0, metaKey: false, ctrlKey: false })).toBe(false);
    vi.restoreAllMocks();
  });
});
