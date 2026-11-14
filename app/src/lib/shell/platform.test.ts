/**
 * Key hints as the platform writes them (the Windows pass, 2026-11-14): the Mac's glyphs on the Mac, the names joined
 * by `+` on Windows and Linux, read off the webview's user agent.
 */
import { afterEach, expect, test, vi } from "vitest";

import { chordLabel, chordParts, keyHint, onMac } from "./platform";

const AGENTS = {
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  jsdom: "Mozilla/5.0 (linux) AppleWebKit/537.36 (KHTML, like Gecko) jsdom/26.1.0",
};
const on = (agent: keyof typeof AGENTS) => vi.spyOn(navigator, "userAgent", "get").mockReturnValue(AGENTS[agent]);

afterEach(() => vi.restoreAllMocks());

test("WKWebView is the Mac; WebView2 and WebKitGTK are not; jsdom draws the Mac's, so a test reads the same anywhere", () => {
  on("mac");
  expect(onMac()).toBe(true);
  on("windows");
  expect(onMac()).toBe(false);
  on("linux");
  expect(onMac()).toBe(false);
  on("jsdom");
  expect(onMac()).toBe(true);
});

test("on the Mac every hint is as written", () => {
  on("mac");
  expect(keyHint("⌘K")).toBe("⌘K");
  expect(keyHint("Type. Escape or ⌘Enter to keep it.")).toBe("Type. Escape or ⌘Enter to keep it.");
  expect(chordLabel("⌘⇧P")).toBe("⌘⇧P");
  expect(chordParts("⌘⇧P")).toEqual(["⌘", "⇧", "P"]);
});

test("off the Mac ⌘ is Ctrl, each chord in a sentence is named and joined by +, modifiers in Ctrl, Alt, Shift order", () => {
  on("windows");
  expect(keyHint("⌘K")).toBe("Ctrl+K");
  expect(keyHint("⌘/")).toBe("Ctrl+/");
  expect(keyHint("Settings (⌘,)")).toBe("Settings (Ctrl+,)");
  expect(keyHint("⌘?")).toBe("Ctrl+?");
  expect(keyHint("Type. Escape or ⌘Enter to keep it.")).toBe("Type. Escape or Ctrl+Enter to keep it.");
  expect(keyHint("Press a modifier and a letter or digit, such as ⌘⇧O")).toBe("Press a modifier and a letter or digit, such as Ctrl+Shift+O");
  expect(keyHint("⇧⌥⌘N")).toBe("Ctrl+Alt+Shift+N");
  expect(keyHint("Titles and keys only. ⌘D searches everything")).toBe("Titles and keys only. Ctrl+D searches everything");
  expect(keyHint("Esc")).toBe("Esc");
  expect(keyHint("↑↓")).toBe("↑↓");
});

test("a chord a person bound is named as pressed: ⌃ is Ctrl, and ⌘ off the Mac is the Windows key", () => {
  on("windows");
  expect(chordLabel("⌃⇧O")).toBe("Ctrl+Shift+O");
  expect(chordParts("⌃⇧O")).toEqual(["Ctrl", "Shift", "O"]);
  expect(chordLabel("⌘1")).toBe("Win+1");
  // Held modifiers before the letter: named alone.
  expect(chordParts("⌃⇧")).toEqual(["Ctrl", "Shift"]);
});
