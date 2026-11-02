/**
 * The `?` card's colour legend (#2421, spec #2419): per kind with states,
 * each tone a row's monogram can take and the state words that give it.
 *
 * Built from the states the adapters declare (`KindPaths.state`, through
 * `kindRegistry.declaredStates`), never written by hand, so a word an
 * adapter adds or retones reaches the legend with no edit here.
 */
import type { Tone } from "../ipc/entity";
import type { StateValue } from "../ipc/sources";

export interface LegendLine {
  kind: string;
  tones: { tone: Tone; words: string[] }[];
}

/** The tones in the order a legend reads them: settled-good, in motion, needs a look, failed, over. */
const ORDER: Tone[] = ["ok", "info", "warn", "err", "muted"];

/** What a declared value says on a row: its rename, else the value as the source writes it. */
function wordOf(value: StateValue): string {
  return value.text ?? String(value.value);
}

export function legendOf(declared: { kind: string; values: StateValue[] }[]): LegendLine[] {
  return declared
    .map(({ kind, values }) => ({
      kind,
      tones: ORDER.map((tone) => ({
        tone,
        words: [...new Set(values.filter((value) => value.tone === tone).map(wordOf))],
      })).filter((line) => line.words.length > 0),
    }))
    .filter((line) => line.tones.length > 0);
}
