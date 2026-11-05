/**
 * The `?` card's copy (#1421): it says what the launcher does today, so no
 * summary names a milestone. M3 and M4 shipped, and a card that still said
 * the estate "arrives in M4" told the reader `asset:` was not there yet.
 */
import { expect, test } from "vitest";

import { SYNTAX, addFilter, narrowFilters, overlayFilters } from "./syntax";

test("no summary on the ? card names a milestone", () => {
  expect(SYNTAX.length, "the table is empty, so this proves nothing").toBeGreaterThan(0);
  const dated = SYNTAX.filter((entry) => /\bM\d/.test(entry.summary) || /arrives in M/.test(entry.summary));
  expect(dated.map((entry) => `${entry.token}: ${entry.summary}`)).toEqual([]);
});

test("env: stays on the card, greyed out, and says where to go instead", () => {
  const env = SYNTAX.find((entry) => entry.token === "env:");
  expect(env?.expect).toBe("reported");
  expect(env?.summary).toBe("not a filter yet; use asset: (also health:)");
});

/** The table's row for `token`. */
function row(token: string) {
  const found = SYNTAX.find((entry) => entry.token === token);
  if (!found) throw new Error(`no row ${token}`);
  return found;
}

test("a # letter merges into the # filter token wherever it stands, in its case", () => {
  expect(addFilter("sepa #R", row("#l"), SYNTAX)).toBe("sepa #Rl ");
  expect(addFilter("sepa #rL", row("#l"), SYNTAX)).toBe("sepa #rL ");
});

test("a #word that is not filter letters is no filter token, so the letter goes first", () => {
  expect(addFilter("#PAY-210", row("#t"), SYNTAX)).toBe("#t #PAY-210 ");
  // `#tx`: `x` is no letter, so `#tx` is the word `tx`.
  expect(addFilter("#tx", row("#r"), SYNTAX)).toBe("#r #tx ");
  expect(addFilter("#", row("#r"), SYNTAX)).toBe("#r # ");
});

test("on an empty search a filter is the box's first word", () => {
  expect(addFilter("", row("#d"), SYNTAX)).toBe("#d ");
  expect(addFilter("  ", row(">"), SYNTAX)).toBe("> ");
});

test("the overlay holds every # letter, then >, @me and t, each one word", () => {
  const shown = overlayFilters(SYNTAX).map(({ entry, word }) => `${entry.token} ${word}`);
  expect(shown).toEqual([
    "#t tickets",
    "#p pull requests",
    "#r repositories",
    "#b builds",
    "#m monitors",
    "#d pages",
    "#l lists",
    "#i inbox",
    "> go to",
    "@me mine",
    "t time",
  ]);
});

test("narrowing keeps the filters whose token and word hold the typed letters in order", () => {
  const all = overlayFilters(SYNTAX);
  const tokens = (typed: string) => narrowFilters(all, typed).map(({ entry }) => entry.token);
  expect(tokens("")).toHaveLength(all.length);
  expect(tokens("PULL")).toEqual(["#p"]);
  expect(tokens("p r")).toEqual(["#p", "#r"]);
  expect(tokens("sl"), "letters out of order do not match `lists`").not.toContain("#l");
});
