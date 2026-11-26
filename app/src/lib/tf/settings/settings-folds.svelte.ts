/**
 * Which parents of Settings' nav are folded (the subtrees ruling, 2026-11-21): kept in the webview's `localStorage`
 * under `knobas.settings.folded`, the folded parents' keys joined by commas, cleared when none is. It stores what is
 * folded, so a parent added later starts open, and every parent is open until folded.
 */
import { keptValue, windowStorage, type KeptStorage } from "../kept-value";
import type { SettingsSection } from "./settings";

export const FOLDED_KEY = "knobas.settings.folded";

export interface SettingsFolds {
  isOpen(section: SettingsSection): boolean;
  set(section: SettingsSection, open: boolean): void;
  /** Tests only: every parent open, nothing kept. */
  reset(): void;
}

export function createSettingsFolds(storage: () => KeptStorage | null): SettingsFolds {
  const kept = keptValue(storage, FOLDED_KEY);
  const read = (kept.read() ?? "").split(",").filter((key) => key !== "") as SettingsSection[];
  let folded = $state<SettingsSection[]>(read);
  const write = () => (folded.length === 0 ? kept.clear() : kept.write(folded.join(",")));
  return {
    isOpen: (section) => !folded.includes(section),
    set(section, open) {
      if (open === !folded.includes(section)) return;
      folded = open ? folded.filter((key) => key !== section) : [...folded, section];
      write();
    },
    reset() {
      folded = [];
      kept.clear();
    },
  };
}

export const settingsFolds = createSettingsFolds(windowStorage);
