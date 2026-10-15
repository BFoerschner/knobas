/**
 * `?fake-ipc`: Suggested links (#237): the tray's proposals and the two answers to one.
 */
import type { LinkEnd, LinkRow, SuggestionEntry, SuggestionPage } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { CORPUS } from "./corpus";

/**
 * What a detection pass would propose over this corpus, in the detector's own
 * words (`knobas_core::suggest::RULES`), so the tray reads under `?fake-ipc`
 * as it reads over a real mirror.
 *
 * Both classes, because the badge exists to tell them apart (#41 story 16):
 * two `exact_key` rows from the records that carry PAY-231's key, and one
 * `similarity` guess from the page that shares the ticket's vocabulary. The
 * similarity row runs page → ticket because `similar_text` orders a pair by
 * entity id, and `ENG-SEPA` sorts before `PAY-231`; its reason names the
 * first four shared words the way the rule does, as words here rather than
 * as the stems the rule actually emits. Every end is an entity the
 * corpus holds, so each is openable -- `linkEnd` throws otherwise, at read
 * time, which is where a dangling fixture row would be noticed.
 */
const FAKE_SUGGESTIONS: LinkRow[] = [
  proposal(1, "mock:payout-service#318", "mock:PAY-231", "build_parameter_key", "exact_key",
    "this build's parameters name PAY-231", "2026-08-22T14:30:03Z"),
  proposal(2, "mock:9f2c1ab", "mock:PAY-231", "commit_message_key", "exact_key",
    "the commit message mentions PAY-231", "2026-08-22T14:30:02Z"),
  proposal(3, "mock:ENG-SEPA", "mock:PAY-231", "similar_text", "similarity",
    "both mention 409, fail, idempotent, payout", "2026-08-22T14:30:01Z"),
];

/** The proposals answered this session, by link id. Accepted or dismissed is
 * one fact to the fixture, because leaving the list is all either does here. */
const ANSWERED = new Set<string>();

/** One unconfirmed link, as the detector writes one. */
function proposal(
  n: number,
  from_id: string,
  to_id: string,
  rule: string,
  rule_class: Exclude<LinkRow["rule_class"], "source_relation" | null>,
  reason: string,
  created_at: string,
): LinkRow {
  return {
    id: `link:fake-${n}`,
    from_id,
    to_id,
    relation: "related",
    origin: "suggested",
    note: null,
    created_by: "knobas",
    created_at,
    confirmed_at: null,
    rule,
    rule_class,
    reason,
  };
}

/** The end of a proposal, hydrated from the corpus the way the real read joins it. */
function linkEnd(entity_id: string): LinkEnd {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entity_id);
  if (!entry) throw new Error(`fake-tauri: a proposal names ${entity_id}, which the corpus does not hold`);
  return { entity_id, kind: entry.kind, title: entry.title, deleted_at: entry.deleted_at };
}

/**
 * `room_suggestions`: the proposals still waiting, newest first, scoped like
 * `list_entities`. `total` is the count before `limit`, never `rows.length`:
 * it is the number the tray's heading shows.
 */
function roomSuggestions(args: Record<string, unknown>): SuggestionPage {
  // The fixture has no link graph, so a stored context's room has nothing to
  // propose -- the same answer `context_members` and `list_entities` give.
  if (args["ctx"]) return { rows: [], total: 0, more: [] };
  const sources = (args["sources"] as string[] | undefined) ?? [];
  if (sources.length && !sources.includes("mock")) return { rows: [], total: 0, more: [] };
  const limit = Number(args["limit"] ?? 50);

  const waiting = FAKE_SUGGESTIONS.filter((link) => !ANSWERED.has(link.id)).sort((a, b) =>
    b.created_at.localeCompare(a.created_at),
  );
  const rows: SuggestionEntry[] = waiting
    .slice(0, limit)
    .map((link) => ({ link, from: linkEnd(link.from_id), to: linkEnd(link.to_id) }));
  return { rows, total: waiting.length, more: [] };
}

/**
 * `accept_suggestion` and `dismiss_suggestion`. Idempotent, as the real ones
 * are: answering a row twice resolves and changes nothing. An id no proposal
 * carries is refused the way the real command refuses it.
 */
function answerSuggestion(args: Record<string, unknown>): null {
  const id = String(args["linkId"] ?? "");
  if (!FAKE_SUGGESTIONS.some((link) => link.id === id)) {
    throw { code: "not_found", message: `${id} is not a link`, source_id: null };
  }
  ANSWERED.add(id);
  return null;
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {

    // Suggestions (#237). The tray awaits `detect_suggestions` and then
    // `room_suggestions` on every room, and it renders a rejection rather than
    // an empty state -- so a table without these keys was a standing red line
    // on every room under `?fake-ipc`. Stateful within the session like the
    // contexts above: a row answered here leaves every later read for as long
    // as the page lives. No event is emitted: the real command emits
    // `activity:new`, but the tray re-reads on its own after answering, and
    // the fixture emits nothing at all.
    //
    // A pass over an unchanged mirror writes nothing, and this mirror never
    // changes, so 0 is the honest constant. The tray discards the number.
    detect_suggestions: () => 0,
    room_suggestions: (args) => roomSuggestions(args),
    // Answered, never performed: there is no link graph to accept into and no
    // tombstone to remember a dismissal by. The two are one thing here -- the
    // row stops being listed -- because that is all a QA pass can witness.
    accept_suggestion: (args) => answerSuggestion(args),
    dismiss_suggestion: (args) => answerSuggestion(args),
  };
}
