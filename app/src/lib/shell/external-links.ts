/**
 * A link to a page on the web opens in the browser, never in the app's window
 * (Björn, 2026-11-13: "It opens inside the tauri shell... make it open in the
 * browser").
 *
 * The window is a WebView: a plain `<a>` to a web page pressed in it
 * navigates the WebView itself to that page, where there is no way back and no
 * address bar, and `target="_blank"` opens a bare WebView window on Windows.
 * So every press on a link to another origin goes through {@link openExternal}
 * (http and https only), with a toast when it refuses:
 *
 * - **{@link openLink}** is a link's own `onclick`, for a link the app draws
 *   to a source's page (*The rest on the source*, a cut file's source);
 * - **{@link installExternalLinks}** is the net under every other: a press on
 *   a link to another origin that nothing handled (a link in synced text, one
 *   added later without `openLink`) is sent to the browser too. It listens on
 *   the window after every handler, so a link whose own handler already
 *   opened it (`event.defaultPrevented`) is left alone.
 *
 * Why not also the Tauri side's `on_navigation`: on macOS wry calls it for
 * every frame's navigation, iframes included (`wkwebview/navigation.rs` has no
 * main-frame test), so refusing an off-origin address there would also refuse
 * Docs' embeds and a renderer's frame. The decision note of 2026-11-13 says so.
 */
import { ipcErrorMessage } from "../ipc";
import { openExternal } from "./open-external";
import { push } from "./toasts.svelte";

/**
 * `href` as an absolute http(s) URL on another origin than `origin` (the
 * page's `location.origin`), or `null` for an address of the app's own. A
 * relative address is the app's own, but for one that names a host
 * (`//gitea.example/…`), which is read as http(s). `origin` may be `"null"`:
 * Tauri's `tauri://localhost` has no web origin.
 */
export function externalUrl(href: string | null, origin: string): string | null {
  if (href === null || href === "" || href.startsWith("#")) return null;
  let url: URL;
  try {
    url = new URL(href.startsWith("//") ? `https:${href}` : href);
  } catch {
    // Not absolute: an address inside the app.
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  return url.origin === origin ? null : url.href;
}

/** Hand `url` to the browser, and say so in a toast when it refuses. */
export function openInBrowser(url: string, open: (url: string) => Promise<void> = openExternal): void {
  open(url).catch((cause: unknown) => {
    push({ text: `Could not open the browser: ${ipcErrorMessage(cause)}`, tone: "err" });
  });
}

/** A link's `onclick`: its page opens in the browser, and the window stays where it is. */
export function openLink(event: MouseEvent): void {
  const link = event.currentTarget instanceof HTMLAnchorElement ? event.currentTarget : null;
  const url = link === null ? null : externalUrl(link.getAttribute("href"), location.origin);
  if (url === null) return;
  event.preventDefault();
  openInBrowser(url);
}

/**
 * Send every unhandled press on a link to another origin to the browser.
 * Answers the uninstaller.
 */
export function installExternalLinks(target: Window = window, open: (url: string) => Promise<void> = openExternal): () => void {
  const onPress = (event: MouseEvent) => {
    if (event.defaultPrevented) return;
    // A middle press opens a window of its own; a right press is the context menu's.
    if (event.type === "auxclick" && event.button !== 1) return;
    const link = (event.target as Element | null)?.closest?.("a[href]");
    if (!(link instanceof HTMLAnchorElement)) return;
    const url = externalUrl(link.getAttribute("href"), target.location.origin);
    if (url === null) return;
    event.preventDefault();
    openInBrowser(url, open);
  };
  target.addEventListener("click", onPress);
  target.addEventListener("auxclick", onPress);
  return () => {
    target.removeEventListener("click", onPress);
    target.removeEventListener("auxclick", onPress);
  };
}
