/**
 * The reads and writes Link sets make (#866), injectable so a test needs no
 * bridge; `defaultPorts` is the real IPC.
 */
import type { DepItem, MonitorRow } from "../../ipc/assets";
import { assetTypes, dependencies, monitorRoster } from "../../ipc/assets";
import { kindWord } from "../monitors/monitors";
import { assetLine } from "./sets";
import type {
  EntityPage,
  LinkSetDetail,
  LinkSetDraft,
  LinkSetHint,
  LinkSetMemberKind,
  LinkSetPattern,
  LinkSetPatternPreview,
  LinkSetRow,
  LinkSetRule,
  PipelineRow,
  Project,
  RuleTicket,
} from "../../ipc/entity";
import {
  deleteLinkSet,
  linkSet,
  linkSetHints,
  linkSets,
  listEntities,
  listProjects,
  parentLinks,
  pipelines,
  previewLinkSetPattern,
  previewLinkSetRule,
  saveLinkSet,
  setParentLinks,
} from "../../ipc/entity";
import { filterSuggestions, type ParentSuggestion } from "../../ipc/search";

export interface SetPorts {
  linkSets: () => Promise<LinkSetRow[]>;
  /** R-7.10's hint card (#870). */
  linkSetHints: () => Promise<LinkSetHint[]>;
  linkSet: (id: string) => Promise<LinkSetDetail>;
  saveLinkSet: (draft: LinkSetDraft) => Promise<LinkSetDetail>;
  deleteLinkSet: (id: string) => Promise<void>;
  previewLinkSetRule: (rule: LinkSetRule) => Promise<RuleTicket[]>;
  previewLinkSetPattern: (pattern: LinkSetPattern) => Promise<LinkSetPatternPreview>;
  parentLinks: () => Promise<boolean>;
  setParentLinks: (enabled: boolean) => Promise<boolean>;
  /** Every item of one kind the *Add…* picker offers. */
  candidates: (kind: LinkSetMemberKind) => Promise<Candidate[]>;
  /** The tickets, projects and parents a rule's value can name. */
  ruleChoices: () => Promise<RuleChoices>;
  /** What the estate's types are called, for a member's line: *Container engine*, not `container_engine`. */
  assetTypeNames: () => Promise<Record<string, string>>;
  /** Monitors' roster, so a monitor member's dot reads as its chip on Monitors: a paused one is not green. */
  monitors: () => Promise<MonitorRow[]>;
}

/** One item the picker offers: its id, its name and the line beside it. */
export interface Candidate {
  entity_id: string;
  name: string;
  sub: string;
}

/** What a rule's value dropdown offers. */
export interface RuleChoices {
  tickets: { entity_id: string; title: string }[];
  /** Keys of the tickets some ticket names as its parent: epics and stories. */
  parents: string[];
  projects: Project[];
}

const MANY = 500;

const entityFilter = (kind: string) => ({
  sources: [],
  kinds: [kind],
  updated_within_days: null,
  context: null,
  project: null,
  order: "title_asc" as const,
  include_deleted: false,
});

const fromPage = (page: EntityPage): Candidate[] =>
  page.rows.map((row) => ({ entity_id: row.entity_id, name: row.title, sub: row.path ?? "" }));

const fromPipelines = (rows: PipelineRow[]): Candidate[] =>
  rows.map((row) => ({ entity_id: row.entity_id, name: row.name, sub: row.path.join(" › ") }));

const fromMonitors = (rows: MonitorRow[]): Candidate[] =>
  rows.map((row) => ({ entity_id: row.entity_id, name: row.name, sub: kindWord(row.monitor_type) }));

const fromAssets = (items: DepItem[], names: Record<string, string>): Candidate[] =>
  items
    .filter((item) => item.id.startsWith("asset:"))
    .map((item) => ({
      entity_id: item.id,
      name: item.name,
      sub: assetLine(item.environment ? `${item.kind}, ${item.environment}` : item.kind, names),
    }));

const assetTypeNames = async (): Promise<Record<string, string>> =>
  Object.fromEntries((await assetTypes()).map((type) => [type.id, type.label]));

const ticketParents = (parents: ParentSuggestion[]) =>
  parents.filter((parent) => parent.kind === "ticket").map((parent) => parent.name);

export const defaultPorts: SetPorts = {
  linkSets: () => linkSets(),
  linkSetHints: () => linkSetHints(),
  linkSet: (id) => linkSet(id),
  saveLinkSet: (draft) => saveLinkSet(draft),
  deleteLinkSet: (id) => deleteLinkSet(id),
  previewLinkSetRule: (rule) => previewLinkSetRule(rule),
  previewLinkSetPattern: (pattern) => previewLinkSetPattern(pattern),
  parentLinks: () => parentLinks(),
  setParentLinks: (enabled) => setParentLinks(enabled),
  candidates: async (kind) => {
    switch (kind) {
      case "pipeline":
        return fromPipelines(await pipelines());
      case "monitor":
        return fromMonitors(await monitorRoster());
      case "asset":
        return fromAssets((await dependencies()).items, await assetTypeNames());
      default:
        return fromPage(await listEntities(entityFilter(kind), MANY, 0));
    }
  },
  assetTypeNames,
  monitors: () => monitorRoster(),
  ruleChoices: async () => {
    const [page, suggested, projects] = await Promise.all([
      listEntities(entityFilter("ticket"), MANY, 0),
      filterSuggestions("", true),
      listProjects(),
    ]);
    return {
      tickets: page.rows.map((row) => ({ entity_id: row.entity_id, title: row.title })),
      parents: ticketParents(suggested.parents),
      projects,
    };
  },
};
