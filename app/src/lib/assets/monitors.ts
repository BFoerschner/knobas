/**
 * The Monitors tab's arithmetic (#448, spec #427 story 68): which chip a
 * monitor belongs to, what the chips count, and the buckets the 24-hour bar is
 * drawn from.
 *
 * Pure, and separate from the tab for `tree.ts`' reason: both answers are
 * *drawable when wrong* — a bar of forty-eight segments looks like a bar of
 * forty-eight segments whatever hour each one stands for — so they are
 * asserted as arithmetic rather than photographed as a component.
 *
 * ## The bar is bucketed, and that is not the same as one segment per sample
 *
 * The ticket asks for "one segment per sample in the last day". At Uptime
 * Kuma's own cadence — knobas' sync interval floor is 60 s
 * (`knobas_sync::config::check_interval`) — that is 1 440 rectangles per row
 * and eight rows on the estate the demo loads, which is eleven thousand
 * elements for a strip a few hundred pixels wide. The mockup brief settled the
 * question before the ticket was written (`mockups/shared/assets.md`: *"Check
 * interval 60 s; 24 h bar = 1 440 checks (render as a 48-segment bar)"*), so
 * the bar is {@link BAR_BUCKETS} half-hour buckets and a bucket reads as the
 * **worst** thing that happened in it. Where samples are sparser than the
 * bucket — a monitor polled every half hour or less often — the two readings
 * are the same picture.
 *
 * ## Empty is a gap, and a gap is a fact
 *
 * A bucket with no samples draws as nothing, not as *up*. It says knobas was
 * not watching then: the app was closed, the source was off, or the monitor
 * did not exist yet. That is what makes "a monitor younger than a day draws
 * what exists" fall out of the bucketing rather than needing a rule of its
 * own.
 */
import type { MonitorRow, MonitorSample, UnmonitoredAsset } from "../ipc/assets";

/**
 * The chips the tab draws, in the order it draws them.
 *
 * The five the ticket names, plus `other`. The sixth is not padding: Kuma's
 * `maintenance` is a real state with no chip of its own, and a monitor whose
 * state did not resolve is a genuine miss (`0021` stores null rather than
 * inventing a word). Folding either into *up* would make the counts a
 * fiction, and dropping them would make the chips add up to fewer monitors
 * than the list under them holds — and *the chips' counts match the mirror* is
 * the tab's first acceptance criterion.
 */
export const CHIPS = ["up", "warn", "down", "pending", "paused", "other"] as const;

/** One of {@link CHIPS}. */
export type ChipState = (typeof CHIPS)[number];

/** What the chips are labelled. `other` says what it holds, since it is not a state. */
export const CHIP_LABELS: Record<ChipState, string> = {
  up: "Up",
  warn: "Warn",
  down: "Down",
  pending: "Pending",
  paused: "Paused",
  other: "Other",
};

/**
 * Which chip a monitor belongs to.
 *
 * **Tombstoned first, whatever the state says.** A monitor Kuma has stopped
 * publishing keeps the state it was last sampled at, so a paused check reads
 * `up` for ever; counting it among the healthy is the one wrong answer this
 * tab must not give, because the whole point of pausing is that nobody is
 * looking.
 *
 * **`paused` is a chip and not a state**, which is why it is excluded from the
 * match below. The five words a sample can carry are
 * `knobas_sync::samples::STATES` — up, down, warn, pending, maintenance — and
 * *paused* is none of them: it is what a **tombstone** means, and the line
 * above is the only thing that may put a monitor on that chip. A future Kuma
 * publishing the literal word `paused` would therefore land in *Other* rather
 * than be counted as tombstoned, which is the honest answer: knobas would know
 * the monitor is still in the mirror, and *Paused* on this tab means it is
 * not.
 */
export function chipOf(row: MonitorRow): ChipState {
  if (row.tombstoned) return "paused";
  const state = row.state;
  return (CHIPS as readonly string[]).includes(state ?? "") && state !== "paused"
    ? (state as ChipState)
    : "other";
}

/** One chip and how many monitors are in it. */
export interface ChipCount {
  state: ChipState;
  count: number;
}

/**
 * Every chip with its count, in {@link CHIPS} order — zeroes included.
 *
 * A chip that vanished at zero would move the chip beside it under the
 * reader's pointer at exactly the moment something recovered, which is the
 * moment they are most likely to be clicking.
 */
export function chipCounts(rows: MonitorRow[]): ChipCount[] {
  const tally = new Map<ChipState, number>(CHIPS.map((state) => [state, 0]));
  for (const row of rows) {
    const chip = chipOf(row);
    tally.set(chip, (tally.get(chip) ?? 0) + 1);
  }
  return CHIPS.map((state) => ({ state, count: tally.get(state) ?? 0 }));
}

/**
 * The roster narrowed to one chip, or the whole of it for no filter.
 *
 * The same {@link chipOf} the counts are made of, so a chip reading `warn 3`
 * and a filtered list of two is not a state this can reach.
 */
export function filtered(rows: MonitorRow[], chip: ChipState | null): MonitorRow[] {
  return chip === null ? rows : rows.filter((row) => chipOf(row) === chip);
}

/** How far back the bar reaches. The backend bounds its answer to the same day. */
export const BAR_WINDOW_MS = 24 * 60 * 60 * 1000;

/**
 * How many segments the bar is drawn in — the mockup's number, which makes
 * each one half an hour.
 */
export const BAR_BUCKETS = 48;

/** One segment of the bar. */
export interface Bucket {
  /** The instant it starts at, inclusive. */
  from: number;
  /** The instant it ends at, exclusive — except the newest, which ends *now*. */
  to: number;
  /**
   * The worst state sampled inside it, or `null` for a half hour with no
   * reading in it at all — which the bar draws as a gap.
   */
  state: string | null;
  /** How many samples fell in it. `0` is the gap. */
  samples: number;
}

/**
 * How bad a state is, smallest first — the order a bucket resolves ties in.
 *
 * A half hour holding one `down` among twenty-nine `up`s is a half hour
 * something was down. A bar that took the majority, or the last reading, would
 * never show an outage shorter than thirty minutes, and most outages are.
 *
 * `maintenance` sorts above `up` because it is not up — somebody suppressed
 * the monitor — and below everything that is a problem, because suppressing it
 * was deliberate.
 */
const SEVERITY: Record<string, number> = {
  down: 0,
  warn: 1,
  pending: 2,
  maintenance: 3,
  up: 4,
};

/**
 * Where a state word this build has no rank for sorts: with `up`, at the
 * harmless end.
 *
 * A word nobody here has heard of is a Kuma newer than this build, and the
 * failure direction that matters is the one that does not invent an outage: a
 * bar that painted an unknown word red would report a problem knobas has no
 * evidence for. It still colours as itself (the class is the word), so it does
 * not silently become green either.
 */
const UNRANKED = 4;

/** How bad a state is, with {@link UNRANKED} for a word this build lacks. */
function severity(state: string): number {
  return SEVERITY[state] ?? UNRANKED;
}

/**
 * The bar for one monitor: {@link BAR_BUCKETS} buckets ending at `now`, oldest
 * first.
 *
 * `now` is the reader's clock and the samples carry absolute instants, so a
 * tab left open for an hour redraws the same samples one hour further left
 * rather than pretending the newest one is current. A sample outside the
 * window — the backend read its day on the database's clock, this one reads it
 * on the reader's — is drawn nowhere rather than clamped into the end bucket,
 * where it would put yesterday's outage at the right-hand edge.
 */
export function bar(samples: MonitorSample[], now: number): Bucket[] {
  const width = BAR_WINDOW_MS / BAR_BUCKETS;
  const start = now - BAR_WINDOW_MS;
  const buckets: Bucket[] = Array.from({ length: BAR_BUCKETS }, (_, index) => ({
    from: start + index * width,
    to: start + (index + 1) * width,
    state: null,
    samples: 0,
  }));

  for (const sample of samples) {
    const taken = Date.parse(sample.taken_at);
    if (Number.isNaN(taken) || taken < start || taken > now) continue;
    // `min` and not a clamp of the value: a sample taken exactly at `now`
    // belongs in the last bucket, which is the one case the half-open
    // arithmetic puts one past the end.
    const index = Math.min(BAR_BUCKETS - 1, Math.floor((taken - start) / width));
    const bucket = buckets[index];
    // Unreachable: the guard above put `taken` inside the window, so the
    // index is between 0 and `BAR_BUCKETS - 1`. Written rather than asserted
    // away because `noUncheckedIndexedAccess` is on for the whole frontend and
    // a `!` here would be the one place in this file that trusts arithmetic
    // over the compiler.
    if (bucket === undefined) continue;
    bucket.samples += 1;
    if (sample.state === null) continue;
    const worse = bucket.state === null || severity(sample.state) < severity(bucket.state);
    if (worse) bucket.state = sample.state;
  }
  return buckets;
}

/**
 * One option of the *Not monitored* roster's type filter (#449).
 *
 * The label and the monogram ride on it rather than being looked up: the
 * backend resolved them from the one type table (`assets::UnmonitoredAsset`),
 * so nothing here keeps a second copy of it.
 */
export interface TypeCount {
  type_id: string;
  label: string;
  monogram: string;
  count: number;
}

/**
 * The filter's options: **the types the roster holds**, each with how many of
 * them are in it, alphabetically by label.
 *
 * *The types the roster holds*, and not the whole type table, which is the one
 * place this list deliberately differs from {@link chipCounts} above. The
 * chips are six fixed states and a chip that vanished at zero would move its
 * neighbour under the reader's pointer at the moment something recovered;
 * these are up to nineteen types over an estate somebody built by hand, and an
 * option with nothing behind it can only empty the list. A roster of gaps
 * offering sixteen dead filters would hide the four live ones.
 *
 * **By label and not by count.** A filter is found by reading its name, and
 * count order would rearrange the row every time somebody attached a monitor.
 * The roster re-reads on mount rather than on a signal, so the order a reader
 * is looking at is the order it stays in.
 */
export function typeCounts(rows: UnmonitoredAsset[]): TypeCount[] {
  const tally = new Map<string, TypeCount>();
  for (const row of rows) {
    const seen = tally.get(row.type_id);
    if (seen) {
      seen.count += 1;
      continue;
    }
    tally.set(row.type_id, {
      type_id: row.type_id,
      label: row.type_label,
      monogram: row.monogram,
      count: 1,
    });
  }
  return [...tally.values()].sort((left, right) => left.label.localeCompare(right.label));
}

/**
 * The roster narrowed to one type, or the whole of it for no filter.
 *
 * Keyed on `type_id` and not on the label, which is what makes a count and its
 * list one statement: two types could in principle be labelled alike, and the
 * id is what the backend counted.
 */
export function byType(rows: UnmonitoredAsset[], typeId: string | null): UnmonitoredAsset[] {
  return typeId === null ? rows : rows.filter((row) => row.type_id === typeId);
}
