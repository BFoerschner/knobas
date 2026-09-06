/**
 * The relation vocabulary, and how a link reads from each of its two ends.
 *
 * A link record is directed — `from_id → to_id` — and carries one word. The
 * two ends of it do not read the same sentence: `PAY-231 blocks PAY-228` is
 * the same row as `PAY-228 blocked by PAY-231`, and a panel that printed
 * `blocks` on both would state the opposite of the truth on half its rows
 * (spec §5a, #40's story 7).
 *
 * **One table, on the frontend, on purpose.** The backend folds a relation to
 * lower case and stores it verbatim; it does not know English. Wording is a
 * rendering decision, so it lives where the rendering does, and the dialog's
 * curated list and the panel's inverse labels are the same list rather than
 * two that drift.
 *
 * Relations are **open**: §5a lets the user type their own. An unknown one
 * reads the same word from both ends — knobas cannot invent the inverse of a
 * word it has never seen, and a guessed one ("…d by") would put language
 * nobody typed on screen.
 */
import type { LinkEntry } from "../ipc/entity";

/**
 * What a link says when nobody named a relation —
 * `knobas_app::commands::entity::DEFAULT_RELATION`.
 *
 * Spelled here as well as there because the two are different jobs: the
 * backend's is the value it writes, this one is what the dialog pre-fills so a
 * quick link costs no extra decisions.
 */
export const DEFAULT_RELATION = "related";

/** One offered relation, and its two readings. */
export interface Relation {
  /** The stored value: lower case, because the backend folds it on write. */
  id: string;
  /** How the link reads from the end it was drawn **from**. */
  forward: string;
  /** How it reads from the end it points **at**. */
  inverse: string;
}

/**
 * The relations the dialog offers, in the order it offers them.
 *
 * Curated, not exhaustive: it is a menu that saves typing, not a closed
 * vocabulary. `related` is first because it is the default.
 *
 * The forward reading is a *phrase*, not the stored id — `depends-on` is
 * stored with a hyphen because it is a key, and reads "depends on" because
 * that is a sentence.
 */
export const RELATIONS: readonly Relation[] = [
  { id: "related", forward: "related to", inverse: "related to" },
  { id: "blocks", forward: "blocks", inverse: "blocked by" },
  { id: "depends-on", forward: "depends on", inverse: "depended on by" },
  { id: "implements", forward: "implements", inverse: "implemented by" },
  { id: "documents", forward: "documents", inverse: "documented by" },
  // The relation a published standup protocol carries (#289). Curated here
  // because an unknown relation reads the same word from both ends, and
  // "published-as" on a *page* would be the opposite of what the row says.
  { id: "published-as", forward: "published as", inverse: "published from" },
  { id: "duplicates", forward: "duplicates", inverse: "duplicated by" },
  { id: "fixes", forward: "fixes", inverse: "fixed by" },
  { id: "runs-on", forward: "runs on", inverse: "hosts" },
  // The four spec #427 story 15 names beyond containment, of which `runs-on`
  // was already here (#289's neighbour). They are ordinary relations, not a
  // second vocabulary: ADR-0014 makes *holding* a parent field, and everything
  // else an asset says about another thing is a link like any other.
  { id: "deployed-from", forward: "deployed from", inverse: "deploys" },
  // `documented-in` is the same fact as `documents` read from the other end,
  // so its inverse is that word and the panel groups the two together under
  // one heading — which is right: a page that documents a container and a
  // container documented in a page are one sentence, and two headings saying
  // it twice would be two headings.
  { id: "documented-in", forward: "documented in", inverse: "documents" },
  // The relation the estate file's monitor names become (#439) and the one a
  // source room's Assets tile reads — `knobas_app::assets::MONITORED_BY`,
  // pinned to this list by that module's mirror test.
  { id: "monitored-by", forward: "monitored by", inverse: "monitors" },
];

/**
 * How `relation` reads from one of its ends.
 *
 * `viewedIsFrom` is whether the entity being looked at is the end the link was
 * drawn from. An unknown relation is returned as typed, from either side.
 */
export function readingOf(relation: string, viewedIsFrom: boolean): string {
  const known = RELATIONS.find((candidate) => candidate.id === relation);
  if (!known) return relation;
  return viewedIsFrom ? known.forward : known.inverse;
}

/** How one link entry reads from `entityId`'s side. */
export function readingFor(entry: LinkEntry, entityId: string): string {
  return readingOf(entry.link.relation, entry.link.from_id === entityId);
}

/** One header and the rows under it. */
export interface LinkGroup {
  /** The heading: how every row under it reads from here. */
  reading: string;
  entries: LinkEntry[];
}

/**
 * The panel's groups, keyed by **how the rows read** rather than by the stored
 * relation.
 *
 * The distinction is the point: a `blocks` link drawn from here and one drawn
 * at here are two different sentences, and one header over both would be wrong
 * for half of them.
 *
 * Groups come out in the order their first row arrived, and rows keep the
 * order the read handed over — which is newest first.
 */
export function groupLinks(links: LinkEntry[], entityId: string): LinkGroup[] {
  const groups: LinkGroup[] = [];
  for (const entry of links) {
    const reading = readingFor(entry, entityId);
    const group = groups.find((candidate) => candidate.reading === reading);
    if (group) {
      group.entries.push(entry);
    } else {
      groups.push({ reading, entries: [entry] });
    }
  }
  return groups;
}
