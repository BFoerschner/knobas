/**
 * The ticket page card's sections table (spec #2396, #2401): one row a
 * section the ticket reaches, with its count, the worst state from its
 * rollup, and what pressing it opens. A ticket with sub-tasks leads with
 * *Sub-tasks*. A section with nothing in it draws no row; once every read
 * has answered and no row is left, the card says *Nothing connected yet.*
 *
 * Pressing a section row opens that section's view on *This job* narrowed to
 * the ticket, chosen or not (`scoped.svelte.ts`); Assets opens the ticket's
 * own Dependencies opening, which already is; Sub-tasks writes `under:<key>`
 * in the Tickets view's search line.
 */
import type { EntityRow } from "../../ipc/entity";
import { sessionFilter } from "../filter/filter-state.svelte";
import { sectionHash } from "../route";
import type { Subtasks } from "./job-reader.svelte";
import {
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

/** The Tickets view's filter bar with no saved view open (`TicketsPage.svelte`). */
export const TICKETS_FILTER = "tickets-all";

/** One row of the table. */
export interface CardSection {
  name: string;
  count: number;
  /** The one state the row draws, or `null`. */
  state: Signal | null;
  /** The rollup's whole words, for the row's title. */
  words: string;
  href: string;
  /** What a press sets before the address is followed: a section's scope, the search line, or nothing. */
  opens: ScopedSection | "subtasks" | null;
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

export function cardSections(read: CardReads): CardTable {
  const key = keyOf(read.entityId);
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
  const section = (name: string, count: number, rollup: Rollup, href: string, opens: ScopedSection | null) => {
    if (count > 0) rows.push({ name, count, state: worstSignal(rollup), words: rollupWords(rollup), href, opens });
  };
  if (read.code !== null) section("Code", read.code.length, codeRollup(read.code, read.unpushed), sectionHash("code"), "code");
  if (read.builds !== null) section("Builds", read.builds.length, buildsRollup(read.builds), sectionHash("builds"), "builds");
  if (read.pages !== null) section("Docs", read.pages.length, pagesRollup(read.pages), sectionHash("docs"), "docs");
  if (read.monitors !== null) section("Monitors", read.monitors.length, monitorsRollup(read.monitors), sectionHash("monitors"), "monitors");
  if (read.assets !== null) section("Assets", read.assets.length, assetsRollup(read.assets), `#/deps/job/${encodeURIComponent(key)}`, null);
  const settled = [read.subtasks, read.code, read.builds, read.pages, read.monitors, read.assets].every((slot) => slot !== null);
  return { rows, settled };
}

/** What a press on `row` sets before its address is followed. */
export function pressSection(row: CardSection, entityId: string): void {
  if (row.opens === "subtasks") sessionFilter(TICKETS_FILTER).text = `under:${keyOf(entityId)}`;
  else if (row.opens !== null) scopedTicket.open(row.opens, entityId);
}
