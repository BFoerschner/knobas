/**
 * `>` — the navigation half of spec §4's action palette.
 *
 * §4 gave `>` two jobs: *"do it here"* rows that act on the selected result
 * (start a timer, comment, change status) and, "with an empty query, the app's
 * navigation menu". The first is retired (#1500, the 2026-10-21 ruling on
 * #1416): the timer is ⌘T's, and commenting and status are the job page's. The
 * second is a list of addresses: every place the shell has one for (#1417).
 */
import { NAV, SECTION_NAME, type Section, sectionHash } from "../tf/route";
import { SETTINGS_SECTIONS, type SettingsSection, settingsHash } from "../tf/settings/settings";

/** One row of the `>` palette. */
export interface LauncherAction {
  /** Stable id, used as the list key. */
  id: string;
  label: string;
  /** The second line: what pressing it does. */
  detail: string;
  /** Where it goes. Every action here is a navigation. */
  hash: string;
}

/** What each section holds, in one line: the row's second line. */
const SECTION_DETAIL: Record<Exclude<Section, "job">, string> = {
  tickets: "Every ticket from every configured source",
  inbox: "What asks for you: mentions, reviews, failed builds",
  code: "Repositories, branches and pull requests",
  builds: "Pipelines and their runs",
  docs: "The wiki's spaces and pages",
  deps: "Hosts, services and what each one exposes",
  monitors: "Uptime checks and their current state",
  sets: "Groups of repos, pipelines and pages shown on tickets by rule",
  review: "Links knobas found between items, for you to accept or reject",
  time: "Your day, your worklogs and your standup",
  settings: "Sources, checkouts, time, notifications and the rest",
};

/** What each section of Settings holds. *Sources* keeps the line it had since #1085. */
const SETTINGS_DETAIL: Record<SettingsSection, string> = {
  sources: "Add, edit and sync the systems knobas reads",
  checkouts: "Where your clones live and which editor opens them",
  time: "How knobas helps you account for your day",
  notifications: "What knobas tells you about, and how",
  standup: "Where the standup protocol is published",
  monitoring: "How Uptime Kuma data is read",
  notes: "Notes and how you capture them",
  backup: "Export and restore what knobas keeps",
  appearance: "Theme and density",
  performance: "How many pages stay open in the background",
  renderers: "Your own commands that draw files and Markdown fences",
  keyboard: "The shortcuts",
  logs: "The sync log and the local database",
};

/**
 * Spec §4's navigation menu (#1417): the top bar's sections in `NAV` order,
 * then Settings and its sections in the order its own navigation draws them.
 * Every address and name comes from the module that owns it; each row's
 * detail line is a `Record` keyed by the section, so a section the shell
 * gains does not compile until it has one here.
 *
 * *Job* has no nav entry (spec #2396), so no row either. *Diagnostics* has no address at all (interfaces §2.3 puts the sync
 * log under Settings › Logs and database, which is a row). *Reload demo data*
 * is a **write** (`demo_load`), refused outside the demo profile (ruling
 * P13), and the launcher does not know which profile it is in: it belongs in
 * the `actions` prop, which exists so the shell can append what only the
 * shell knows. The 2026-10-21 ruling on #1417 left such rows out.
 *
 * A settings section is labelled *Settings › <section>*: Settings has a
 * *Time* of its own, and the prefix is what `>set` finds.
 */
export function navigationActions(): LauncherAction[] {
  return [
    ...[...NAV, "settings" as const].map((section) => ({
      id: section,
      label: SECTION_NAME[section],
      detail: SECTION_DETAIL[section],
      hash: sectionHash(section),
    })),
    ...SETTINGS_SECTIONS.map(([section, name]) => ({
      id: `settings-${section}`,
      label: `${SECTION_NAME.settings} › ${name}`,
      detail: SETTINGS_DETAIL[section],
      hash: settingsHash(section),
    })),
  ];
}

/** The actions matching `text`, in the order they were declared. */
export function filterActions(actions: LauncherAction[], text: string): LauncherAction[] {
  const needle = text.trim().toLowerCase();
  if (!needle) return actions;
  return actions.filter(
    (action) =>
      action.label.toLowerCase().includes(needle) ||
      action.detail.toLowerCase().includes(needle),
  );
}
