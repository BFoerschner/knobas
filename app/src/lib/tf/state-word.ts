/**
 * **One state word** (design system, Words): what a chip says, in sentence case, whichever source wrote it.
 *
 * A source says *FAILURE*, *In Progress* or *merged*. The shell draws them one way, so a chip reads what the filters and
 * the chart's legend read. A word with an acronym in it keeps it, because *HTTP* is not *Http*.
 */

/** The source's words that mean a word the filters already use. */
const SAME_AS: Readonly<Record<string, string>> = {
  success: "Passed",
  successful: "Passed",
  passed: "Passed",
  failure: "Failed",
  failed: "Failed",
};

/** Words that stay in capitals inside a state. */
const ACRONYM = /^(?:HTTP|HTTPS|TCP|DNS|SSL|TLS|PR|CI|CD|API)$/;

export function stateWord(text: string): string {
  const said = text.trim();
  if (said === "") return "";
  const same = SAME_AS[said.toLowerCase()];
  if (same !== undefined) return same;
  const words = said.split(/\s+/).map((word) => (ACRONYM.test(word) ? word : word.toLowerCase()));
  const first = words[0]!;
  words[0] = first.charAt(0).toUpperCase() + first.slice(1);
  return words.join(" ");
}
