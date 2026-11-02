/**
 * The Connections drawer, pure (#820, R-4.3, R-6.2): what *Linked on KEY*
 * lists, what the button counts, and which proposals are this ticket's.
 *
 * A job's connections are the links drawn on its ticket, then one group per
 * link set that applies and one for the parent (#867, R-6.2), read by
 * `job_link_sets`; a set's rows hidden on this ticket go under *Hidden on KEY*.
 */
import type { EntityDetail, JobLinkSets, LinkEntry, LinkSetRuleRow, SuggestionEntry } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";
import { kindRegistry } from "../../shell/kind-registry.svelte";
import { entityAddress } from "../address";
import { itemHash, keyOf, runKindsOf } from "./job";

/**
 * The job section a kind belongs to, in prototype-6's words (its `SECL`), so a
 * row says where on the job it shows. A kind no section draws says its own
 * name, capitalised, rather than nothing. A run of a kind the CI adapters claim
 * (`workflow_run`) is a build, read from the shared registry as `itemHash`
 * reads it (#1282).
 */
const SECTION: Record<string, string> = {
  pr: "Code",
  branch: "Code",
  commit: "Code",
  repo: "Code",
  build: "Builds",
  pipeline: "Builds",
  build_config: "Builds",
  page: "Pages",
  monitor: "Monitors",
  asset: "Assets",
  route: "Assets",
};

export function sectionOf(kind: string, runKinds: readonly string[] = runKindsOf(kindRegistry)): string {
  if (runKinds.includes(kind)) return SECTION.build!;
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

/**
 * Why a set applies, in prototype-6's words (its `viaLabel`): "via epic
 * PAY-200", "via project PAY", "via parent PAY-231" or "applied by you". A
 * rule's value is an entity id or `<source>:<project>`, so the key is what
 * follows the source.
 */
export function viaLabel(rule: Pick<LinkSetRuleRow, "kind" | "value">): string {
  const key = keyOf(rule.value);
  if (rule.kind === "epic") return `via epic ${key}`;
  if (rule.kind === "project") return `via project ${key}`;
  if (rule.kind === "parent") return `via parent ${key}`;
  return "applied by you";
}

/** One row of a set's or the parent's group. */
export interface GroupRow {
  /** The entity id: what *Hide here* and the link name. */
  id: string;
  title: string;
  /** Where the row links; `null` for an entity knobas has no row for. */
  href: string | null;
  /** The job section it shows in. */
  section: string;
}

/**
 * Where a job's connections come from beside its own links: a link set that
 * applies, or the parent (R-7.3). What *Also on this job* draws a chip for and
 * the drawer a group for.
 */
export interface JobGroup {
  /** `set:<id>` or `parent:<ticket>`, unique per job. */
  key: string;
  /** The set's name, or "Parent PAY-231". */
  label: string;
  /** The reason: "via epic PAY-200", or the parent's line. */
  via: string;
  /** Where the chip goes: the set's editor, or the parent's job. */
  href: string;
  /** The set's id; `null` for the parent, whose rows have no *Hide here*. */
  setId: string | null;
  /** The rows not hidden on this ticket. */
  rows: GroupRow[];
}

/**
 * A group row's link: a pipeline (a `pipeline` member, or a far end the Builds
 * section holds that is not a run) to its page under Builds, the rest as a job
 * row links. `sectionOf` is asked without run kinds so a run stays `itemHash`'s.
 */
function rowHash(id: string, kind: string | null, memberKind: string | null): string | null {
  if (kind === null) return null;
  if (memberKind === "pipeline" || (sectionOf(kind, []) === "Builds" && kind !== "build")) {
    return `#/builds/${encodeURIComponent(id)}`;
  }
  return itemHash({ entity_id: id, kind });
}

/** The parent's reason, prototype-6's words. */
export const PARENT_VIA = "sub-tasks show their parent’s connections";

/** The sets that apply, by name, then the parent: the chips and the groups, in order. */
export function groupsOf(read: JobLinkSets | null): JobGroup[] {
  if (read === null) return [];
  const sets: JobGroup[] = read.sets.map((set) => ({
    key: `set:${set.id}`,
    label: set.name,
    via: viaLabel(set.via),
    href: `#/sets/${encodeURIComponent(set.id)}`,
    setId: set.id,
    rows: set.members
      .filter((member) => !member.hidden)
      .map((member) => ({
        id: member.entity_id,
        title: member.title ?? member.entity_id,
        href: rowHash(member.entity_id, member.entity_kind, member.kind),
        section: sectionOf(member.kind),
      })),
  }));
  if (read.parent === null) return sets;
  const key = keyOf(read.parent.ticket);
  return [
    ...sets,
    {
      key: `parent:${read.parent.ticket}`,
      label: `Parent ${key}`,
      via: PARENT_VIA,
      href: entityAddress("ticket", read.parent.ticket),
      setId: null,
      rows: read.parent.links.map((link) => ({
        id: link.entity_id,
        title: link.title ?? link.entity_id,
        href: rowHash(link.entity_id, link.kind, null),
        section: sectionOf(link.kind),
      })),
    },
  ];
}

/** A set's member hidden on this ticket: a *Hidden on KEY* row. */
export interface HiddenRow {
  id: string;
  title: string;
}

/** A set's member hidden on this ticket, once however many sets hold it: *Hidden on KEY*. */
export function hiddenOf(read: JobLinkSets | null): HiddenRow[] {
  const out = new Map<string, string>();
  for (const set of read?.sets ?? []) {
    for (const member of set.members) {
      if (member.hidden && !out.has(member.entity_id)) out.set(member.entity_id, member.title ?? member.entity_id);
    }
  }
  return [...out].map(([id, title]) => ({ id, title }));
}

/** The names of the sets that also hold `entityId`, for a *Linked on* row's "also in". */
export function alsoIn(read: JobLinkSets | null, entityId: string): string[] {
  return (read?.sets ?? []).filter((set) => set.members.some((member) => member.entity_id === entityId)).map((set) => set.name);
}

/** The group rows the drawer's filter keeps: every word in the title or the section. */
export function filterRows(rows: readonly GroupRow[], text: string): GroupRow[] {
  return rows.filter((row) => matchesAll(`${row.title} ${row.section}`, text));
}
