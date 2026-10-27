/**
 * `?fake-ipc`: Suggested links (#237): the tray's proposals and the two answers to one.
 */
import type {
  EvidenceExcerpt,
  EvidencePart,
  LinkEnd,
  LinkRow,
  ProposalEvidence,
  SuggestionEntry,
  SuggestionPage,
} from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { CORPUS, FAKE_CONTEXTS } from "./corpus";
import { keyOf } from "../../../time/format";

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
  proposal(3, "confluence:sepa", "mock:PAY-231", "similar_text", "similarity",
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
  // PR #142's second commit (#841), so its Commits tab says *proposed link,
  // judge it* where prototype-6's `a41f2c` does.
  proposal(8, "mock:tidewater/payout-service@a41f2c08d3e9", "mock:PAY-231", "commit_message_key", "exact_key",
    "the commit message mentions PAY-231", "2026-08-22T14:25:00Z"),
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

/** The rules that find a ticket's key in the other end's text. */
const KEY_RULES = ["branch_name_key", "commit_message_key", "build_parameter_key", "page_text_key"];

/** A corpus row's text as the detector reads it: title, then body. */
function textOf(entity_id: string): string {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entity_id);
  if (!entry) return "";
  return entry.body_text && entry.body_text !== entry.title ? `${entry.title}\n${entry.body_text}` : entry.title;
}

/**
 * `text` with every `needle` marked where a word starts with it,
 * case-insensitively: the whole word when `stems` (a shared word is a stem,
 * as the rule emits it), else the needle as a whole word (a key).
 */
function markIn(text: string, needles: string[], stems: boolean): EvidencePart[] {
  const parts: EvidencePart[] = [];
  const escaped = needles.map((needle) => needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`(?<!\\w)(?:${escaped.join("|")})${stems ? "\\w*" : "(?!\\w)"}`, "gi");
  let at = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > at) parts.push({ text: text.slice(at, match.index), marked: false });
    parts.push({ text: match[0], marked: true });
    at = match.index + match[0].length;
  }
  if (at < text.length) parts.push({ text: text.slice(at), marked: false });
  return parts;
}

/**
 * `proposal_evidence` (#836), worked out over the corpus the way
 * `knobas_core::evidence` works it out over the mirror: a key is marked where
 * the item names it, shared words where both ends use them. The support and
 * counter-evidence are the engine's sentences for this corpus, which has no
 * accepted or dismissed siblings and no other keys beside the one found.
 */
function proposalEvidence(args: Record<string, unknown>): ProposalEvidence {
  const id = String(args["linkId"] ?? "");
  const link = FAKE_SUGGESTIONS.find((candidate) => candidate.id === id);
  if (!link) throw { code: "not_found", message: `${id} is not a link`, source_id: null };
  const rule = link.rule ?? "";
  const excerpts: EvidenceExcerpt[] = [];
  const support: string[] = [];
  const against: string[] = [];
  if (KEY_RULES.includes(rule)) {
    const key = keyOf(link.to_id);
    excerpts.push({ entity_id: link.from_id, parts: markIn(textOf(link.from_id), [key], false) });
  } else if (rule === "similar_text") {
    const words = (link.reason ?? "").replace(/^both mention /, "").split(", ");
    for (const end of [link.from_id, link.to_id]) {
      const parts = markIn(textOf(end), words, true);
      if (parts.some((part) => part.marked)) excerpts.push({ entity_id: end, parts });
    }
    support.push(`Shared words: ${words.join(", ")}.`);
    against.push("Wording alone often matches unrelated items.");
  }
  if (against.length === 0) against.push("Nothing found against it.");
  return { link_id: id, reason: link.reason, excerpts, support, against };
}

/** This feature's commands. */
/** `?fake-suggestions=<n>` (perf audit, #2200's class): `n` more proposals of PAY-231's one commit, added at load. */
FAKE_SUGGESTIONS.push(
  ...Array.from({ length: Number(new URLSearchParams(globalThis.location?.search).get("fake-suggestions") ?? 0) || 0 }, (_, i) =>
    proposal(1000 + i, "mock:9f2c1ab", "mock:PAY-231", "similar_text", "similarity", `both mention payout ${i}`, "2026-08-01T10:00:00Z"),
  ),
);

export function handlers(): Handlers {
  return {

    // Suggestions (#237): the proposals read and its two answers. Stateful
    // within the session like the stored contexts: a row answered here leaves
    // every later read for as long as the page lives. No event is emitted: the
    // real command emits `activity:new`, and the fixture emits nothing at all.
    // No detection pass: the sync scheduler requests one in Rust (#1485), and
    // `detect_suggestions` retired (#1611).
    room_suggestions: (args) => roomSuggestions(args),
    // Answered, never performed: there is no link graph to accept into and no
    // tombstone to remember a dismissal by. The two are one thing here -- the
    // row stops being listed -- because that is all a QA pass can witness.
    accept_suggestion: (args) => answerSuggestion(args),
    dismiss_suggestion: (args) => answerSuggestion(args),
    // #836: why a proposal was made, read for the one open.
    proposal_evidence: (args) => proposalEvidence(args),
  };
}

keepSeed(ANSWERED);
