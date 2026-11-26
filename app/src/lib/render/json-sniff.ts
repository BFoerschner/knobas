/**
 * What is JSON for the JSON view (the XML view's design, 2026-11-23, section 25): a file of one of
 * {@link JSON_ENDINGS} whose first character that is no white space is `{` or `[`. JSON with comments (`.jsonc`,
 * `.json5`) stays Source: jq does not read it. Small: start-up holds it, the view loads with the first document.
 */
export const JSON_ENDINGS = ["json", "geojson", "webmanifest", "har", "ipynb", "jsonl", "ndjson"] as const;
const ENDINGS = new Set<string>(JSON_ENDINGS);

const endingOf = (path: string): string => {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
};

/** Whether `text` at `path` opens in the JSON view. */
export function isJsonFile(text: string, path: string): boolean {
  if (!ENDINGS.has(endingOf(path))) return false;
  let at = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  while (at < text.length) {
    const c = text.charCodeAt(at);
    if (c !== 32 && c !== 9 && c !== 10 && c !== 13) break;
    at += 1;
  }
  const c = text.charCodeAt(at);
  return c === 123 || c === 91;
}

/** Whether a file holds a value a line. */
export const isDelimited = (path: string): boolean => endingOf(path) === "jsonl" || endingOf(path) === "ndjson";
