/**
 * The ticket page card's sections table (spec #2396, #2401): one row a
 * section the ticket reaches, with its count, the worst state from its
 * rollup, and what pressing it opens. A ticket with sub-tasks leads with
 * *Sub-tasks*. A section with nothing in it draws no row; once every read
 * has answered and no row is left, the card says *Nothing connected yet.*
 *
 * Pressing a section row opens that section's view on *This job* narrowed to
 * the ticket, chosen or not (`scoped.svelte.ts`); Assets opens the asset tree
 * at the ticket's first asset, with all of its assets marked; Sub-tasks narrows
 * Tickets to `under:<key>` and names it in the title. All six land under the same
 * bar, *Back to <key>*.
 *
 * A row counts what its landing shows. Code lands on two tabs, repositories and
 * pull requests, so its count is both and its words name each; a branch or a
 * commit has no tab of its own, and counts only where nothing else is reached.
 *
 * The Tickets preview's *Around this ticket* is the same rows, in the same words
 * (F-109): the card reads them piece by piece as its page fills, the preview all at
 * once ({@link readCardReads}), and both draw through `TfCardRows.svelte`.
 */
import type { CheckoutView, EntityRow, MiniBoard, Reached } from "../../ipc/entity";
import type { AssetDetail } from "../../ipc/assets";
import { sectionHash } from "../route";
import { isDone } from "../tickets/tickets";
import type { Subtasks } from "./job-reader.svelte";
import {
  assetTone,
  assetsRollup,
  buildsRollup,
  codeRollup,
  keyOf,
  monitorsRollup,
  pagesRollup,
  worstSignal,
  type AssetLine,
  type Rollup,
  type Signal,
} from "./job";
import { scopedTicket, type ScopedSection } from "./scoped.svelte";
import { ASSET_LIMIT, readSection, SECTION_READS, type ReachPorts, type SectionRead } from "./sections";

/** One row of the table. */
export interface CardSection {
  name: string;
  count: number;
  /** The one state the row draws, or `null`. */
  state: Signal | null;
  /** Words in the state's place where a count stands for two kinds and no state is drawn. */
  note?: string;
  /** The rollup's whole words, for the row's title. */
  words: string;
  href: string;
  /** What a press sets before the address is followed: a section's scope, the search line, or nothing. */
  opens: ScopedSection | "subtasks" | "assets" | null;
}

/** What the reader has read for the table; `null` is still being read. */
export interface CardReads {
  entityId: string;
  subtasks: Subtasks | null;
  code: readonly EntityRow[] | null;
  unpushed: ReadonlySet<string>;
  builds: readonly EntityRow[] | null;
  pages: readonly EntityRow[] | null;
  monitors: readonly EntityRow[] | null;
  assets: readonly AssetLine[] | null;
}

/** The rows drawn so far, and whether every read has answered. */
export interface CardTable {
  rows: CardSection[];
  settled: boolean;
}

/** A rollup in one line of words: "7 configurations: 1 failing, 1 running, 5 passing". */
export function rollupWords(rollup: Rollup): string {
  const lead = rollup.lead.replace(/:$/, "");
  return rollup.signals.length === 0 ? lead : `${lead}: ${rollup.signals.map((signal) => signal.text).join(", ")}`;
}

/** "2 repositories", "1 repository". */
function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Code's row: what its two tabs hold between them, each kind named. */
function codeRow(rows: readonly EntityRow[], unpushed: ReadonlySet<string>): CardSection | null {
  if (rows.length === 0) return null;
  const rollup = codeRollup(rows, unpushed);
  const repos = rows.filter((row) => row.kind === "repo").length;
  const prs = rows.filter((row) => row.kind === "pr").length;
  const landing = repos + prs;
  const base = { name: "Code", href: sectionHash("code"), opens: "code" as const };
  if (landing === 0) return { ...base, count: rows.length, state: worstSignal(rollup), words: rollupWords(rollup) };
  const whole = [repos > 0 ? counted(repos, "repository", "repositories") : null, prs > 0 ? counted(prs, "pull request", "pull requests") : null].filter((part) => part !== null).join(", ");
  const state = worstSignal(rollup);
  const words = rollup.signals.length === 0 ? whole : `${whole}: ${rollup.signals.map((signal) => signal.text).join(", ")}`;
  // One kind: the count already says it. Both: the row's words say how the count divides, in the room a card's row has.
  if (repos === 0 || prs === 0) return { ...base, count: landing, state, words };
  const someRepos = counted(repos, "repo", "repos");
  if (state === null) return { ...base, count: landing, state, note: `${someRepos}, ${counted(prs, "PR", "PRs")}`, words };
  return { ...base, count: landing, state: state.tone === "info" ? { ...state, text: `${someRepos}, ${state.text}` } : state, words };
}

export function cardSections(read: CardReads): CardTable {
  const rows: CardSection[] = [];
  if (read.subtasks !== null && read.subtasks.total > 0) {
    const { total, open } = read.subtasks;
    rows.push({
      name: "Sub-tasks",
      count: total,
      state: open > 0 ? { tone: "info", text: `${open} open` } : null,
      words: `${total} ${total === 1 ? "sub-task" : "sub-tasks"}, ${open} open`,
      href: sectionHash("tickets"),
      opens: "subtasks",
    });
  }
  const section = (name: string, count: number, rollup: Rollup, href: string, opens: ScopedSection | "assets") => {
    if (count > 0) rows.push({ name, count, state: worstSignal(rollup), words: rollupWords(rollup), href, opens });
  };
  if (read.code !== null) {
    const code = codeRow(read.code, read.unpushed);
    if (code !== null) rows.push(code);
  }
  if (read.builds !== null) section("Builds", read.builds.length, buildsRollup(read.builds), sectionHash("builds"), "builds");
  if (read.pages !== null) section("Docs", read.pages.length, pagesRollup(read.pages), sectionHash("docs"), "docs");
  if (read.monitors !== null) section("Monitors", read.monitors.length, monitorsRollup(read.monitors), sectionHash("monitors"), "monitors");
  if (read.assets?.[0] !== undefined) section("Assets", read.assets.length, assetsRollup(read.assets), `#/asset/${encodeURIComponent(read.assets[0].id)}`, "assets");
  const settled = [read.subtasks, read.code, read.builds, read.pages, read.monitors, read.assets].every((slot) => slot !== null);
  return { rows, settled };
}

/** What a press on `row` sets before its address is followed. */
export function pressSection(row: CardSection, entityId: string): void {
  if (row.opens === "subtasks") scopedTicket.openSubtasks(entityId);
  else if (row.opens === "assets") scopedTicket.openAssets(entityId);
  else if (row.opens !== null) scopedTicket.open(row.opens, entityId);
}

/** How many of a ticket's branches have their checkout read for Code's *not pushed* (#2401). */
export const UNPUSHED_LIMIT = 10;

/** The sub-tasks a ticket's `under:<key>` board holds: a column that is not *Done* is open. */
export function countSubtasks(board: Pick<MiniBoard, "columns">): Subtasks {
  const total = board.columns.reduce((sum, column) => sum + column.total, 0);
  const open = board.columns.filter((column) => !isDone(column.status)).reduce((sum, column) => sum + column.total, 0);
  return { total, open };
}

/** The reached assets as the Assets row reads them: each one's health is its own read. */
export async function readAssetLines(getAsset: (id: string) => Promise<AssetDetail>, reached: readonly Reached[]): Promise<AssetLine[]> {
  const ids = reached.filter((row) => row.kind === "asset").slice(0, ASSET_LIMIT);
  const read = await Promise.allSettled(ids.map((row) => getAsset(row.entity_id)));
  return read.flatMap((answer) =>
    answer.status === "fulfilled"
      ? [
          {
            id: answer.value.asset.id,
            name: answer.value.asset.name,
            type: answer.value.asset.type_label,
            environment: answer.value.effective_environment?.value ?? null,
            on: answer.value.held_by.at(-1)?.name ?? null,
            tone: assetTone(answer.value.asset.health),
          },
        ]
      : [],
  );
}

/**
 * Code's *not pushed* (#2401): a branch counts when its checkout on this machine
 * stands on that branch with commits ahead of its upstream. The mirror's branches
 * are the forge's, so only the checkout knows. A branch with no checkout, or one
 * standing elsewhere, counts nothing.
 */
export async function readUnpushed(checkout: (id: string) => Promise<CheckoutView>, rows: readonly EntityRow[]): Promise<ReadonlySet<string>> {
  const branches = rows.filter((row) => row.kind === "branch").slice(0, UNPUSHED_LIMIT);
  const answers = await Promise.allSettled(branches.map(async (row) => checkout(row.entity_id)));
  return new Set(
    branches
      .filter((row, at) => {
        const answer = answers[at]!;
        if (answer.status !== "fulfilled") return false;
        const tree = answer.value.tree;
        return tree !== null && tree.status === "read" && tree.branch === row.title && tree.ahead > 0;
      })
      .map((row) => row.entity_id),
  );
}

/** The reads {@link readCardReads} makes. */
export interface CardReadPorts extends ReachPorts {
  getAsset: (id: string) => Promise<AssetDetail>;
  /** The ticket system's board under a search, for the Sub-tasks row. */
  ticketBoard: (query: string) => Promise<MiniBoard>;
  /** A branch's checkout on this machine. */
  entityCheckout: (id: string) => Promise<CheckoutView>;
}

/**
 * Everything the table needs for ticket `id`, at once and with no context promoted
 * (#1030): the card's reads, for a page that does not build the card. A read that
 * fails counts nothing, as its row draws nothing.
 */
export async function readCardReads(ports: CardReadPorts, id: string): Promise<CardReads> {
  const rows = (read: SectionRead) => readSection(ports.listEntities, id, read).catch(() => [] as EntityRow[]);
  const [code, builds, pages, monitors, reached, subtasks] = await Promise.all([
    rows(SECTION_READS.code),
    rows(SECTION_READS.builds),
    rows(SECTION_READS.pages),
    rows(SECTION_READS.monitors),
    ports.jobReach(id).catch(() => [] as Reached[]),
    ports.ticketBoard(`under:${keyOf(id)}`).then(countSubtasks, () => ({ total: 0, open: 0 })),
  ]);
  const [assets, unpushed] = await Promise.all([readAssetLines(ports.getAsset, reached), readUnpushed(ports.entityCheckout, code)]);
  return { entityId: id, subtasks, code, unpushed, builds, pages, monitors, assets };
}
