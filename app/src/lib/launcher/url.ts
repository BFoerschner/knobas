/**
 * Is what the reader typed a link, rather than a query?
 *
 * The launcher's grammar gains **no URL branch** (spec #491): `>`, `#`, `@`,
 * `/` and `?` are the backend's parse and stay the backend's, per ruling P2.
 * This is the one thing the frontend decides for itself, and it decides it
 * *before* the query is sent — because a URL is not a search, and sending one
 * to the FTS engine would answer with whatever words happen to be in it.
 *
 * The precedent is `rows.ts`' `modeOf`, which says the same about the one
 * question the frontend already answers alone: *"An empty box is the one case
 * the frontend decides for itself, and 'is this string empty' is not
 * grammar."* Neither is "does this parse as an absolute http URL".
 *
 * ## Why the platform's parser does the deciding
 *
 * `URL` is what will decide what the *browser* thinks the string is, and
 * `shell/open-external.ts` already compares `URL.protocol` for exactly that
 * reason: a leading newline, a percent-escape or `JavaScript:` all read
 * differently to a hand-written pattern than to the parser. The one pattern
 * below is not a second opinion about URLs — it is about what the reader
 * *typed*, which is a question the parser deliberately does not answer.
 */

/**
 * A paste is written out in full: scheme, `://`, then an authority.
 *
 * Tested against the **typed text** rather than against `URL.protocol`,
 * because the two disagree in one direction that matters here. The WHATWG
 * parser treats `http` as a special scheme and rewrites `http:PAY-231` into
 * an `http:` URL whose host is `pay-231` — a URL with everything this module
 * checks for, out of something a reader typed as a ticket key with a stray
 * colon. Requiring the `://` the reader actually pasted keeps that a query.
 *
 * The parser is still the authority on whether the rest of the string is a
 * URL at all, and on what the two schemes are: `openExternal` allows `http:`
 * and `https:` and nothing else, and recognising any other scheme here would
 * end in a refusal the reader cannot act on.
 */
const WRITTEN_OUT = /^https?:\/\//i;

/**
 * The URL `raw` names, or `null` when it is a query.
 *
 * Whitespace-trimmed and then required to be **whitespace-free**: `new URL`
 * tolerates spaces inside a path by percent-encoding them, so a genuine search
 * like `retry https://jira.example` would otherwise parse and be sent to the
 * resolver as a link nobody pasted.
 *
 * A relative address (`/browse/PAY-231`) is a query as far as this is
 * concerned. It has no host, so it cannot say *which* Jira, and the launcher's
 * `/` prefix already means something else.
 */
export function pastedUrl(raw: string): string | null {
  const text = raw.trim();
  if (text === "" || /\s/.test(text) || !WRITTEN_OUT.test(text)) return null;
  let parsed: URL;
  try {
    parsed = new URL(text);
  } catch {
    return null;
  }
  // A URL with no host names no instance — and `//` alone is not a scheme.
  if (parsed.hostname === "") return null;
  return text;
}
