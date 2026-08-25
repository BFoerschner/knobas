/**
 * Hand a URL to the operating system's browser.
 *
 * ## Why this is not `openUrl(url)`
 *
 * `web_url` comes from an adapter, which is to say from a **remote system's
 * data**: a Jira instance decides what to put in it, and knobas mirrors it
 * verbatim. Passing an arbitrary scheme to the OS opener makes "click a
 * ticket" mean "run whatever has a handler registered" — `file:///`,
 * `vscode://`, `ms-msdt:` — which is a remote-code path that starts with a
 * row in somebody's backlog.
 *
 * The allow-list is here because the frontend is the only caller. The
 * plugin's own permission scope in `capabilities/default.json` is a second
 * wall behind it, and neither is a substitute for the other: this one gives a
 * message a person can act on, that one holds if a future caller forgets.
 */
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * The only two schemes a mirrored URL may carry.
 *
 * Compared against `URL.protocol`, which is what the parser resolved — not
 * against the raw string, which `JavaScript:`, a leading newline, or a
 * percent-escape can all disguise.
 */
const ALLOWED = new Set(["http:", "https:"]);

/**
 * Open `url` in the reader's browser, or refuse it.
 *
 * @throws if `url` is not a URL, or is not `http:`/`https:`. The caller shows
 * the message: a source configured with an `ftp://` base URL is something a
 * person can fix, and it must not look like a bug in knobas.
 */
export async function openExternal(url: string): Promise<void> {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(`refused: ${JSON.stringify(url)} is not a URL`);
  }
  if (!ALLOWED.has(parsed.protocol)) {
    throw new Error(`refused: ${parsed.protocol} is not http or https`);
  }
  await openUrl(url);
}
