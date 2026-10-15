/**
 * The Connections drawer, pure (#820, R-4.3, R-6.2): what *Linked on KEY*
 * lists, what the button counts, and which proposals are this ticket's.
 *
 * In TF1 a job's connections are the links drawn on its ticket. Link sets and
 * the parent's links join in TF5 (#867), each as a group of its own and with
 * its hidden rows under *Hidden on KEY*; this file then gains their rows, and
 * the drawer already draws the groups.
 */
import type { EntityDetail, LinkEntry, SuggestionEntry } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";

/**
 * The job section a kind belongs to, in prototype-6's words (its `SECL`), so a
 * row says where on the job it shows. A kind no section draws says its own
 * name, capitalised, rather than nothing.
 */
const SECTION: Record<string, string> = {
  pr: "Code",
  branch: "Code",
  commit: "Code",
  repo: "Code",
  build: "Builds",
  pipeline: "Builds",
  page: "Pages",
  monitor: "Monitors",
  asset: "Assets",
  route: "Assets",
};

export function sectionOf(kind: string): string {
  return SECTION[kind] ?? kind.charAt(0).toUpperCase() + kind.slice(1);
}

/** The links drawn on the ticket whose far end still stands, newest first as the detail reads them. */
export function linkedOn(detail: Pick<EntityDetail, "links">): LinkEntry[] {
  return detail.links.filter((entry) => entry.other.deleted_at === null);
}

/** The rows the drawer's filter keeps: every word in the title or the section. */
export function filterLinks(links: readonly LinkEntry[], text: string): LinkEntry[] {
  return links.filter((entry) => matchesAll(`${entry.other.title} ${sectionOf(entry.other.kind)}`, text));
}

/** The proposals with this ticket at one end: what *N proposed* counts and *Judge them* opens. */
export function proposalsFor(rows: readonly SuggestionEntry[], entityId: string): SuggestionEntry[] {
  return rows.filter((row) => row.link.from_id === entityId || row.link.to_id === entityId);
}

/** How many proposals a ticket's room is asked for: enough that one ticket's are all there. */
export const PROPOSAL_LIMIT = 200;
