/**
 * The `?` card's copy (#1421): it says what the launcher does today, so no
 * summary names a milestone. M3 and M4 shipped, and a card that still said
 * the estate "arrives in M4" told the reader `asset:` was not there yet.
 */
import { expect, test } from "vitest";

import { SYNTAX } from "./syntax";

test("no summary on the ? card names a milestone", () => {
  expect(SYNTAX.length, "the table is empty, so this proves nothing").toBeGreaterThan(0);
  const dated = SYNTAX.filter((entry) => /\bM\d/.test(entry.summary) || /arrives in M/.test(entry.summary));
  expect(dated.map((entry) => `${entry.token}: ${entry.summary}`)).toEqual([]);
});

test("env: stays on the card, greyed out, and says where to go instead", () => {
  const env = SYNTAX.find((entry) => entry.token === "env:");
  expect(env?.expect).toBe("reported");
  expect(env?.summary).toBe("not a filter yet; use asset: or Dependencies (also health:)");
});
