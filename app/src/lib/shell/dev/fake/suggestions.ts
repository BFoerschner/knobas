/**
 * `?fake-ipc`: Suggested links (#237): the tray's proposals and the two answers to one.
 */
import type { LinkEnd, LinkRow, SuggestionEntry, SuggestionPage } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { CORPUS, FAKE_CONTEXTS } from "./corpus";

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
  // The Proposed links view (#835) wants a queue to compare with
  // prototype-6's: more of the detector's own rules over the same corpus,
  // every strength the shell draws, and targets past PAY-231.
  proposal(4, "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry", "mock:PAY-231",
    "branch_name_key", "exact_key", "the branch name contains PAY-231", "2026-08-22T14:29:00Z"),
  proposal(5, "mock:OPS-77", "mock:PAY-228", "source_recorded_relation", "source_relation",
    "mock already records this link (blocks)", "2026-08-22T14:28:00Z", "blocks"),
  proposal(6, "mock:PAY-231", "mock:PAY-232", "similar_text", "similarity",
    "both mention metric, payout, retry, sepa", "2026-08-22T14:27:00Z"),
  proposal(7, "mock:PAY-231", "mock:PAY-244", "similar_text", "similarity",
    "both mention fail, payout, retry, window", "2026-08-22T14:26:00Z"),
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
  rule_class: Exclude<LinkRow["rule_class"], null>,
  reason: string,
  created_at: string,
  relation = "related",
): LinkRow {
  return {
    id: `link:fake-${n}`,
    from_id,
    to_id,
    relation,
    // A relation the source records is written by the detector with the
    // source's origin (`knobas_core::suggest::RULES`).
    origin: rule_class === "source_relation" ? "source" : "suggested",
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
  // A stored context's room proposes what touches its anchor (#820's drawer
  // asks a ticket's): the fixture keeps no membership past the anchor, so the
  // anchor is the whole of the room's scope here.
  const ctx = typeof args["ctx"] === "string" ? args["ctx"] : null;
  const anchor = ctx === null ? null : (FAKE_CONTEXTS.find((row) => row.id === ctx)?.anchor_id ?? "");
  const sources = (args["sources"] as string[] | undefined) ?? [];
  if (sources.length && !sources.includes("mock")) return { rows: [], total: 0, more: [] };
  const limit = Number(args["limit"] ?? 50);

  const waiting = FAKE_SUGGESTIONS.filter(
    (link) => !ANSWERED.has(link.id) && (anchor === null || link.from_id === anchor || link.to_id === anchor),
  ).sort((a, b) =>
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
    // stored contexts: a row answered here leaves every later read for as long
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
