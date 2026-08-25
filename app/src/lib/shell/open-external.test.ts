/**
 * *Open in browser*, and the reason it is not a one-line call.
 *
 * `web_url` arrives from an adapter — which is to say from a **remote
 * system's data**. Handing an arbitrary scheme to the OS opener turns "click a
 * ticket" into "run a program": `file:///`, `vscode://`, and on Windows
 * anything else the shell has a handler registered for. The check lives here
 * because the frontend is the only caller; the plugin's own scope is a second
 * wall, not a substitute for this one.
 *
 * The allowed URLs below are **loopback**, and not for lack of imagination:
 * `house-rules.test.ts` refuses any non-loopback URL anywhere under `app/src/`,
 * test files included, and that rule is worth more intact than this file is
 * worth pretty. `https://127.0.0.1:8443/…` is as `https:` as any other.
 */
import { beforeEach, expect, test, vi } from "vitest";

const opened: string[] = [];
let fail: Error | null = null;

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: async (url: string) => {
    opened.push(url);
    if (fail) throw fail;
  },
}));

const { openExternal } = await import("./open-external");

beforeEach(() => {
  opened.length = 0;
  fail = null;
});

test("opens http and https", async () => {
  await openExternal("https://127.0.0.1:8443/browse/PAY-231");
  expect(opened).toEqual(["https://127.0.0.1:8443/browse/PAY-231"]);

  await openExternal("http://127.0.0.1:5399/browse/PAY-231");
  expect(opened).toHaveLength(2);
});

test.each([
  "javascript:alert(1)",
  "file:///etc/passwd",
  "data:text/html,<script>",
  "vscode://x",
  "ms-msdt:/id",
  "smb://share/x",
  "",
  "   ",
  "/etc/passwd",
  "not a url at all",
])("refuses %s", async (url) => {
  await expect(openExternal(url)).rejects.toThrow(/refused/i);
  expect(opened, `${url} reached the opener`).toEqual([]);
});

/**
 * The refusal names the scheme it refused.
 *
 * A message that said only "refused" would make the one case a user can act on
 * — a source configured with an `ftp://` base URL — indistinguishable from a
 * bug in knobas.
 */
test("a refusal says what it refused", async () => {
  await expect(openExternal("vscode://x")).rejects.toThrow(/vscode:/);
  await expect(openExternal("nonsense")).rejects.toThrow(/not a URL/i);
});

/**
 * Case and whitespace are not a way round the check.
 *
 * `new URL` lowercases a scheme and trims leading control characters, so this
 * is asserting the parser is what decides — not a `startsWith` on the raw
 * string, which is what a hand-rolled guard would be.
 */
test("the scheme is what the URL parser says it is, not what the string looks like", async () => {
  await openExternal("HTTPS://127.0.0.1:8443/x");
  expect(opened).toEqual(["HTTPS://127.0.0.1:8443/x"]);

  await expect(openExternal("JavaScript:alert(1)")).rejects.toThrow(/refused/i);
  await expect(openExternal("\njavascript:alert(1)")).rejects.toThrow(/refused/i);
  // A scheme with nothing after it is not a URL, whatever the scheme is.
  await expect(openExternal("https:")).rejects.toThrow(/refused/i);
});

/** A failure from the OS is the caller's to show, not something to swallow. */
test("a failure from the opener propagates", async () => {
  fail = new Error("no handler for https");
  await expect(openExternal("https://127.0.0.1:8443/x")).rejects.toThrow("no handler for https");
});
