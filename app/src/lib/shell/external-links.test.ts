import { afterEach, expect, test, vi } from "vitest";

import { externalUrl, installExternalLinks } from "./external-links";

const ORIGIN = "http://localhost:3000";

let off: (() => void) | null = null;
afterEach(() => {
  off?.();
  off = null;
  document.body.replaceChildren();
});

test("only an http(s) address on another origin is the browser's", () => {
  expect(externalUrl("https://gitea.example/t/x/src/main/README.md", ORIGIN)).toBe("https://gitea.example/t/x/src/main/README.md");
  expect(externalUrl("http://gitea.example:3000/x", ORIGIN)).toBe("http://gitea.example:3000/x");
  expect(externalUrl("#/repo/live-gitea%3At%2Fx", ORIGIN)).toBeNull();
  expect(externalUrl("/index.html#/inbox", ORIGIN)).toBeNull();
  expect(externalUrl(`${ORIGIN}/#/inbox`, ORIGIN)).toBeNull();
  expect(externalUrl("javascript:alert(1)", ORIGIN)).toBeNull();
  expect(externalUrl("file:///etc/passwd", ORIGIN)).toBeNull();
  expect(externalUrl(null, ORIGIN)).toBeNull();
  // Tauri's own origin on macOS is `tauri://localhost`, whose URL origin is "null": a web page is still another one.
  expect(externalUrl("https://gitea.example/x", "null")).toBe("https://gitea.example/x");
  expect(externalUrl("#/x", "null")).toBeNull();
});

function link(href: string, attrs: Record<string, string> = {}): HTMLAnchorElement {
  const a = document.createElement("a");
  a.href = href;
  a.textContent = "Open on Gitea";
  for (const [key, value] of Object.entries(attrs)) a.setAttribute(key, value);
  document.body.append(a);
  return a;
}

test("a press on a link to a web page opens it in the browser, and the window does not go there", () => {
  const open = vi.fn(() => Promise.resolve());
  off = installExternalLinks(window, open);
  const press = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
  link("https://gitea.example/t/x", { target: "_blank" }).dispatchEvent(press);
  expect(open).toHaveBeenCalledWith("https://gitea.example/t/x");
  expect(press.defaultPrevented).toBe(true);
  // A middle press would open a window of its own.
  const middle = new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
  link("https://gitea.example/t/y").dispatchEvent(middle);
  expect(open).toHaveBeenLastCalledWith("https://gitea.example/t/y");
  expect(middle.defaultPrevented).toBe(true);
});

test("an in-app link, and one whose own handler opened it, are left alone", () => {
  const open = vi.fn(() => Promise.resolve());
  off = installExternalLinks(window, open);
  const inApp = new MouseEvent("click", { bubbles: true, cancelable: true });
  link("#/settings/checkouts").dispatchEvent(inApp);
  expect(inApp.defaultPrevented).toBe(false);
  const handled = link("https://gitea.example/t/x");
  handled.addEventListener("click", (event) => event.preventDefault());
  handled.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  expect(open).not.toHaveBeenCalled();
});
