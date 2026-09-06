/**
 * How a link reads from the side you are standing on.
 *
 * A link record is directed (`from_id → to_id`) and one word describes it, but
 * the two ends do not read the same sentence: `PAY-231 blocks PAY-228` is the
 * same row as `PAY-228 blocked by PAY-231`. Spec §5a's story 7 asks for
 * exactly that, and getting it wrong is not a cosmetic bug — it inverts the
 * meaning of every row in the panel.
 */
import { expect, test } from "vitest";

import type { LinkEntry } from "../ipc/entity";
import { DEFAULT_RELATION, RELATIONS, groupLinks, readingOf } from "./relations";

function entry(over: { relation?: string; from?: string; to?: string; id?: string } = {}): LinkEntry {
  const from = over.from ?? "mock:PAY-231";
  const to = over.to ?? "mock:PAY-228";
  return {
    link: {
      id: over.id ?? "00000000-0000-0000-0000-000000000001",
      from_id: from,
      to_id: to,
      relation: over.relation ?? DEFAULT_RELATION,
      origin: "manual",
      note: null,
      created_by: "user",
      created_at: "2026-08-28T09:30:00Z",
      confirmed_at: "2026-08-28T09:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: { entity_id: to, kind: "ticket", title: "Retry storm", deleted_at: null },
  };
}

/**
 * The literal words, not `RELATIONS.find(...)`.
 *
 * An expectation computed the way the code computes it passes by
 * construction: it would still hold if `blocks` and `blocked by` were swapped
 * in the table, which is the one mistake this module can make.
 */
test("a curated relation reads forwards from the end it was drawn from and inverted from the other", () => {
  expect(readingOf("blocks", true)).toBe("blocks");
  expect(readingOf("blocks", false)).toBe("blocked by");
  expect(readingOf("documents", true)).toBe("documents");
  expect(readingOf("documents", false)).toBe("documented by");
  expect(readingOf("runs-on", true)).toBe("runs on");
  expect(readingOf("runs-on", false)).toBe("hosts");
});

/**
 * The four relations an asset draws beyond containment (spec #427 story 15,
 * issue #435), each asserted as literal words from both ends.
 *
 * The pair that matters most is `runs-on` / `hosts`: it is the acceptance
 * criterion's own example, and it is the one where the two ends read as
 * different *verbs* rather than as a word and its passive, so a table that had
 * them the wrong way round would still look like English.
 */
test("an asset's relations read as sentences from both of their ends", () => {
  expect(readingOf("depends-on", true)).toBe("depends on");
  expect(readingOf("depends-on", false)).toBe("depended on by");
  expect(readingOf("runs-on", true)).toBe("runs on");
  expect(readingOf("runs-on", false)).toBe("hosts");
  expect(readingOf("deployed-from", true)).toBe("deployed from");
  expect(readingOf("deployed-from", false)).toBe("deploys");
  expect(readingOf("documented-in", true)).toBe("documented in");
  expect(readingOf("documented-in", false)).toBe("documents");
  expect(readingOf("monitored-by", true)).toBe("monitored by");
  expect(readingOf("monitored-by", false)).toBe("monitors");
});

/**
 * A container linked to a VM with `runs-on` is *hosted* from the VM's side —
 * the acceptance criterion read at the grouping level, which is what the pane
 * actually draws.
 */
test("the pane on each end of one runs-on link reads the opposite heading", () => {
  const container = "asset:postgres";
  const vm = "asset:vm-db-01";
  const runsOn = entry({ id: "r", relation: "runs-on", from: container, to: vm });

  expect(groupLinks([runsOn], container).map((group) => group.reading)).toEqual(["runs on"]);
  expect(groupLinks([runsOn], vm).map((group) => group.reading)).toEqual(["hosts"]);
});

/** A symmetric relation reads the same both ways — and still reads as words. */
test("related reads the same from both ends", () => {
  expect(readingOf(DEFAULT_RELATION, true)).toBe("related to");
  expect(readingOf(DEFAULT_RELATION, false)).toBe("related to");
});

/**
 * §5a: relations are free text. knobas cannot invent the inverse of a word it
 * has never seen, and guessing (`"…d by"`) would put language nobody typed on
 * screen.
 */
test("a user-typed relation knobas does not know reads the same from both ends", () => {
  expect(readingOf("supersedes-eventually", true)).toBe("supersedes-eventually");
  expect(readingOf("supersedes-eventually", false)).toBe("supersedes-eventually");
});

/** The curated list is what the dialog offers and what the lookup keys on. */
test("every curated relation has a lower-case id and two distinct readings", () => {
  expect(RELATIONS.length).toBeGreaterThan(3);
  for (const relation of RELATIONS) {
    expect(relation.id, `${relation.id} is stored folded, so the lookup must key on it`).toBe(
      relation.id.toLowerCase(),
    );
    expect(relation.forward.length, `${relation.id} has no forward reading`).toBeGreaterThan(0);
    expect(relation.inverse.length, `${relation.id} has no inverse reading`).toBeGreaterThan(0);
  }
  expect(
    RELATIONS.map((relation) => relation.id),
    "the default relation has to be offerable",
  ).toContain(DEFAULT_RELATION);
});

/**
 * Grouping is by the *reading*, not by the stored relation.
 *
 * The same word on two rows can be two different sentences: a `blocks` link
 * drawn from here and one drawn at here are "blocks" and "blocked by", and one
 * header over both would say the opposite of the truth for half of them.
 */
test("rows group by how they read from the viewed entity, not by the stored relation", () => {
  const viewed = "mock:PAY-231";
  const groups = groupLinks(
    [
      entry({ id: "a", relation: "blocks", from: viewed, to: "mock:PAY-228" }),
      entry({ id: "b", relation: "blocks", from: "mock:PAY-400", to: viewed }),
      entry({ id: "c", relation: "blocks", from: viewed, to: "mock:PAY-500" }),
    ],
    viewed,
  );

  expect(groups.map((group) => group.reading)).toEqual(["blocks", "blocked by"]);
  expect(groups[0]!.entries.map((item) => item.link.id)).toEqual(["a", "c"]);
  expect(groups[1]!.entries.map((item) => item.link.id)).toEqual(["b"]);
});

/** Groups appear in the order their first row does; rows keep the read's order. */
test("groups and rows keep the order the read handed them over", () => {
  const viewed = "mock:PAY-231";
  const groups = groupLinks(
    [
      entry({ id: "newest", relation: "documents", from: viewed }),
      entry({ id: "middle", relation: "blocks", from: viewed }),
      entry({ id: "oldest", relation: "documents", from: viewed }),
    ],
    viewed,
  );

  expect(groups.map((group) => group.reading)).toEqual(["documents", "blocks"]);
  expect(groups[0]!.entries.map((item) => item.link.id)).toEqual(["newest", "oldest"]);
});
