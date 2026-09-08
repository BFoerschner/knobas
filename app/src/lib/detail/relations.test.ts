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
import {
  CAPTURED_FROM,
  CAPTURED_IN,
  DEFAULT_RELATION,
  RELATIONS,
  bornWith,
  drawn,
  groupLinks,
  readingFor,
  readingOf,
} from "./relations";

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
 * The two a capture draws (#502, spec #491 stories 40-43), as literal words
 * from both ends.
 *
 * Curated because an unknown relation reads the same word from both ends: an
 * uncurated `captured-in` would print *captured in* on the **context's** panel
 * over a row that is the note, which says the room was captured in the
 * thought. The panel is the only place these words are ever read, so the two
 * readings are the whole of what being in the menu buys.
 */
test("the two relations a capture draws read as sentences from both of their ends", () => {
  expect(readingOf(CAPTURED_IN, true)).toBe("captured in");
  expect(readingOf(CAPTURED_IN, false)).toBe("captured here");
  expect(readingOf(CAPTURED_FROM, true)).toBe("captured from");
  expect(readingOf(CAPTURED_FROM, false)).toBe("captured from here");
  expect(
    RELATIONS.map((relation) => relation.id),
    "both are offered by the dialog, not only understood by the panel",
  ).toEqual(expect.arrayContaining([CAPTURED_IN, CAPTURED_FROM]));
});

/**
 * The same two at the grouping level, which is what the panel draws — and the
 * end that matters is the **context's**, since that is the one nobody sees
 * while writing the note.
 */
test("a captured-in link reads one way on the note and the other on its context", () => {
  const note = "note:7f2cf0d4";
  const context = "ctx:2f1a5d6e";
  const captured = entry({ id: "c", relation: CAPTURED_IN, from: note, to: context });

  expect(groupLinks([captured], note).map((group) => group.reading)).toEqual(["captured in"]);
  expect(groupLinks([captured], context).map((group) => group.reading)).toEqual(["captured here"]);
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

/**
 * Issue #445: *Link to…* draws a monitor's attachment **from either end**,
 * and both ends read the right way round.
 *
 * The stored word is one word — `monitored-by`, `knobas_app::assets`'
 * `MONITORED_BY`, which the import writes and every estate read filters on —
 * and it is a sentence about the *asset*: `knobas-gitea monitored by gitea`.
 * A reader standing on the **monitor** who picks that word would draw the row
 * backwards and get `gitea monitored by knobas-gitea`, which is the opposite
 * of the truth. So `monitors` is offered from that end, and it is the same
 * row: `inverseOf` says which word is stored, and the ends are swapped.
 *
 * Asserted as literal words and literal ids, never by re-running `drawn`'s own
 * rule: a table with `monitors` and `monitored-by` transposed would satisfy
 * any expectation computed the way the code computes it.
 */
test("monitors is the monitor's end of monitored-by, and reads that way from both", () => {
  expect(readingOf("monitored-by", true)).toBe("monitored by");
  expect(readingOf("monitored-by", false)).toBe("monitors");
});

test("an ordinary relation is drawn from the entity whose detail is open", () => {
  expect(drawn("blocks", "mock:PAY-231", "mock:PAY-228")).toEqual({
    fromId: "mock:PAY-231",
    toId: "mock:PAY-228",
    relation: "blocks",
  });
  // A relation nobody curated is nobody's inverse either: §5a's open
  // vocabulary is stored as typed and drawn the way it was asked for.
  expect(drawn("supersedes-eventually", "mock:PAY-231", "mock:PAY-228")).toEqual({
    fromId: "mock:PAY-231",
    toId: "mock:PAY-228",
    relation: "supersedes-eventually",
  });
});

test("monitors, picked from a monitor, stores monitored-by with the asset at the from end", () => {
  expect(drawn("monitors", "kuma:7", "asset:knobas-gitea")).toEqual({
    fromId: "asset:knobas-gitea",
    toId: "kuma:7",
    relation: "monitored-by",
  });

  // And the row that produces reads the right sentence from each end — which
  // is the whole reason for the swap.
  const row = entry({ relation: "monitored-by", from: "asset:knobas-gitea", to: "kuma:7" });
  expect(readingFor(row, "kuma:7")).toBe("monitors");
  expect(readingFor(row, "asset:knobas-gitea")).toBe("monitored by");
});

/**
 * The same gesture from the asset's end draws the identical row — which is
 * what makes "from either end" one link rather than two spellings of one fact
 * that every estate read would then have to know about.
 */
test("monitored-by, picked from an asset, draws the same row as monitors picked from the monitor", () => {
  expect(drawn("monitored-by", "asset:knobas-gitea", "kuma:7")).toEqual(
    drawn("monitors", "kuma:7", "asset:knobas-gitea"),
  );
});

// --- the two links a captured note is born with (#502, #503) -----------------

/**
 * The pair a room or a capture hands in becomes the two born links, in that
 * order.
 *
 * Here rather than in either caller because the function is one and the callers
 * are two (`shell/Room.svelte`'s *New note*, `capture/capture.svelte.ts`'s
 * window): what each of them hands in is its own test's, and what the pair
 * becomes is this one's.
 */
test("the room and the foreground become the two born links, in that order", () => {
  expect(bornWith({ context: "ctx:sepa", foreground: "mock:PAY-231" })).toEqual([
    { target_id: "ctx:sepa", relation: "captured-in" },
    { target_id: "mock:PAY-231", relation: "captured-from" },
  ]);
});

/**
 * The two halves are independent, which is the case a single link would get
 * wrong: a derived room has no context and the reader is still looking at
 * something, and a stored room with nothing open is the other way round.
 */
test("a derived room contributes no captured-in, and an empty foreground no captured-from", () => {
  expect(bornWith({ context: null, foreground: "mock:PAY-231" })).toEqual([
    { target_id: "mock:PAY-231", relation: "captured-from" },
  ]);
  expect(bornWith({ context: "ctx:sepa", foreground: null })).toEqual([
    { target_id: "ctx:sepa", relation: "captured-in" },
  ]);
  expect(bornWith({ context: null, foreground: null })).toEqual([]);
});
