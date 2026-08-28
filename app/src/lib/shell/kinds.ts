/**
 * Kinds: which tile they land in, and what to call them.
 *
 * **§3a is the constraint.** knobas may not carry a table of every kind every
 * adapter will ever emit — "a new ticket system is browsable on day one".
 * So there are three layers, in this order:
 *
 * 1. what the **adapter** declares (`KindInfo`), which is the only layer that
 *    can be right about a kind knobas has never seen;
 * 2. knobas' **own vocabulary** — the kinds `0001_init.sql` enumerates for its
 *    own schema (`ticket|pr|build|page|note|commit|branch|repo`), because
 *    `pr` is a knobas word before it is any adapter's and "Prs" is not what it
 *    means;
 * 3. a **generic** humaniser, so an undeclared `incident` still reads as
 *    *Incidents* rather than being dropped.
 *
 * Layer 2 is the one worth arguing about, so: it is not a per-adapter table.
 * It maps knobas' own vocabulary — the words in its own migration — and it is
 * consulted only after the adapter has had its say.
 */
import type { KindInfo } from "../ipc/entity";
import { humanise } from "./humanise";

/** One room tile: a heading and the kinds it draws. */
export interface TileSpec {
  /** Stable id — a bucket name, or the kind itself for an open kind. */
  id: string;
  /** The `.lab` heading. */
  label: string;
  /** The kinds this tile fetches. Never empty. */
  kinds: string[];
}

/**
 * The mockup's designed rooms, in the order it drew them.
 *
 * `Time in this context` and `Assets` are M3/M4 and are not here: they are not
 * kinds of synced item at all.
 */
const BUCKETS: { id: string; label: string; kinds: string[] }[] = [
  { id: "tickets", label: "Tickets", kinds: ["ticket"] },
  { id: "code", label: "Code", kinds: ["pr", "commit", "branch", "repo"] },
  { id: "builds", label: "Builds", kinds: ["build"] },
  { id: "docs", label: "Docs", kinds: ["page"] },
  { id: "notes", label: "Notes", kinds: ["note"] },
];

/** knobas' own kind vocabulary — layer 2. See the module docs. */
const VOCABULARY: Record<string, { label: string; plural: string; monogram: string }> = {
  ticket: { label: "Ticket", plural: "Tickets", monogram: "TK" },
  pr: { label: "Pull request", plural: "Pull requests", monogram: "PR" },
  commit: { label: "Commit", plural: "Commits", monogram: "CM" },
  branch: { label: "Branch", plural: "Branches", monogram: "BR" },
  repo: { label: "Repository", plural: "Repositories", monogram: "RP" },
  build: { label: "Build", plural: "Builds", monogram: "BU" },
  page: { label: "Page", plural: "Pages", monogram: "PG" },
  note: { label: "Note", plural: "Notes", monogram: "NT" },
};

/**
 * The tiles a room with these kinds in it draws.
 *
 * A bucket carries only the kinds actually present, so a tile never queries a
 * kind the corpus does not have; a bucket nothing fills is not drawn; and
 * anything no bucket claims gets a tile of its own, in the order the corpus
 * reported it.
 *
 * `declared` is layer 1 — what an adapter says this kind is called. Passing it
 * is what makes §3a's *"a new source's items get grouped, chipped and labeled
 * without touching core"* literally true for the room: an open kind's tile
 * then carries the adapter's own plural rather than knobas humanising the
 * word. Omitting it falls through to layers 2 and 3, which is what a caller
 * with no registry to hand gets.
 */
export function tilesFor(
  kinds: string[],
  declared: (kind: string) => KindInfo | null = () => null,
): TileSpec[] {
  const present = [...new Set(kinds)];
  const claimed = new Set<string>();
  const tiles: TileSpec[] = [];

  for (const bucket of BUCKETS) {
    const mine = bucket.kinds.filter((kind) => present.includes(kind));
    if (mine.length === 0) continue;
    for (const kind of mine) claimed.add(kind);
    tiles.push({ id: bucket.id, label: bucket.label, kinds: mine });
  }

  for (const kind of present) {
    if (claimed.has(kind)) continue;
    tiles.push({ id: kind, label: kindLabel(kind, declared(kind)), kinds: [kind] });
  }

  return tiles;
}

/** What one of these is called — *Pull request*, *Incident*. */
export function kindSingular(kind: string, info?: KindInfo | null): string {
  return info?.label ?? VOCABULARY[kind]?.label ?? humanise(kind);
}

/** What several of these are called — *Pull requests*, *Incidents*. */
export function kindLabel(kind: string, info?: KindInfo | null): string {
  return info?.plural ?? VOCABULARY[kind]?.plural ?? pluralise(humanise(kind));
}

/**
 * The two-character chip — *PR*, *IN*.
 *
 * `?` for an empty kind: the mirror's `kind` column is `not null` but a
 * degenerate value is a bad row, not a reason for an empty box the reader
 * cannot even point at.
 */
export function kindMonogram(kind: string, info?: KindInfo | null): string {
  if (info?.monogram) return info.monogram;
  const known = VOCABULARY[kind]?.monogram;
  if (known) return known;
  return kind.slice(0, 2).toUpperCase() || "?";
}

/**
 * English plurals, to the depth a source system's vocabulary reaches.
 *
 * Not a linguistics engine: these are the endings that appear in the words
 * work-tracking systems actually use for their item types. Anything stranger
 * than this is a kind the adapter should be declaring a `plural` for.
 */
function pluralise(word: string): string {
  if (/[^aeiou]y$/i.test(word)) return `${word.slice(0, -1)}ies`;
  if (/(s|x|z|ch|sh)$/i.test(word)) return `${word}es`;
  return `${word}s`;
}
