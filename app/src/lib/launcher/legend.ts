/**
 * The `?` card's colour legend (#2421, spec #2419): per kind with states,
 * each tone a row's monogram can take and the state words that give it.
 *
 * Built from the merged map the *state colours* read answers (#2467, spec
 * #2463): the adapters' declared tones with the reader's overrides applied,
 * and the seen words the reader coloured. Never written by hand, so a word an
 * adapter adds or retones, or one the reader moves, reaches the legend with
 * no edit here. A word that draws no tone (*No colour*, or a seen word left
 * alone) is on no line.
 *
 * A seen word left at its Jira status category's tone (#2468) is on no line
 * either: the category colours every status no name maps, including ones
 * not seen yet, so a kind that reads one ends with `CATEGORY_LINE` instead
 * of listing them. A word the reader coloured is listed under its tone.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import type { Tone } from "../ipc/entity";
import { type KindStateColours, stateColours } from "../ipc/sources";

export interface LegendLine {
  kind: string;
  tones: { tone: Tone; words: string[] }[];
  /** The line the kind ends with, where its tone also comes from its tickets' Jira status category. */
  rest: string | null;
}

/** What a kind whose words fall back to Jira's status category ends with. */
export const CATEGORY_LINE = "other statuses: by Jira category";

/** The tones in the order a legend reads them: settled-good, in motion, needs a look, failed, over. */
const ORDER: Tone[] = ["ok", "info", "warn", "err", "muted"];

export function legendOf(colours: KindStateColours[]): LegendLine[] {
  return colours
    .map(({ kind, words }) => ({
      kind,
      tones: ORDER.map((tone) => ({
        tone,
        words: [...new Set(words.filter((word) => word.tone === tone && word.origin !== "category").map((word) => word.word))],
      })).filter((line) => line.words.length > 0),
      rest: words.some((word) => word.category_tone !== null) ? CATEGORY_LINE : null,
    }))
    .filter((line) => line.tones.length > 0 || line.rest !== null);
}

/**
 * What the legend reads, injectable as the session's ports are: the *state
 * colours* read, and the event a set or a clear announces itself on
 * (`activity:new`, #2465 Ruling 1).
 */
export interface LegendPorts {
  stateColours: () => Promise<KindStateColours[]>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

export const realLegendPorts: LegendPorts = {
  stateColours: () => stateColours(),
  listen: (event, handler) => tauriListen(event, handler),
};
