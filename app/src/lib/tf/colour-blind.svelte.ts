/**
 * Colour-blind mode (Björn, 2026-11-22: "make it possible to enable a colorblind mode that warns when there's colors
 * that certain colorblind people will have a problem with"). Off until turned on in Settings › Appearance. On, every
 * theme the window draws, and every theme chosen for it, is checked for states that read the same to someone with
 * protanopia, deuteranopia or tritanopia, and each such pair is named with its roles: *Failed and Passed read the same
 * under deuteranopia.*
 *
 * The simulation is Machado, Oliveira and Fernandes (2009), "A Physiologically-based Model for Simulation of Color
 * Vision Deficiency", IEEE TVCG 15(6), at severity 1.0, on linear sRGB (`theme/colour.ts`'s `simulate`); two colours
 * read the same where their OKLab distance under it is under 0.08 (`theme/app-check.ts`'s `APART`).
 *
 * Kept in `localStorage` as `knobas.colour-blind` (`on`, or absent), this computer's like the themes.
 */
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";
import { APART } from "./theme/app-check";
import type { Colours } from "./theme/app-derive";
import { deltaE, simulate, type Vision } from "./theme/colour";

export const COLOUR_BLIND_KEY = "knobas.colour-blind";

/** The colours a person tells a state by, with the names the Theme page gives them. */
export const STATE_ROLES = [
  ["ok", "Passed"],
  ["warn", "Attention"],
  ["err", "Failed"],
  ["info", "Running"],
  ["muted", "Unknown"],
] as const;
type StateKey = (typeof STATE_ROLES)[number][0];

/** Each vision by the name of the colour blindness it simulates. */
export const VISION_NAME: Record<Vision, string> = { protan: "protanopia", deutan: "deuteranopia", tritan: "tritanopia" };
const VISIONS: readonly Vision[] = ["protan", "deutan", "tritan"];

export interface Confusion {
  a: StateKey;
  b: StateKey;
  vision: Vision;
  /** Their OKLab distance under that vision. */
  delta: number;
  /** *Failed and Passed read the same under deuteranopia.* */
  sentence: string;
}

/**
 * The pairs of states that read the same under each colour blindness, closest first: every pair under {@link APART}
 * for that vision, and where a pair is close under more than one, each is named.
 */
export function confusions(c: Pick<Colours, StateKey>): Confusion[] {
  const out: Confusion[] = [];
  for (const vision of VISIONS)
    for (let i = 0; i < STATE_ROLES.length; i += 1)
      for (let j = i + 1; j < STATE_ROLES.length; j += 1) {
        const [a, an] = STATE_ROLES[i]!;
        const [b, bn] = STATE_ROLES[j]!;
        const delta = deltaE(simulate(c[a], vision), simulate(c[b], vision));
        if (delta < APART) out.push({ a, b, vision, delta, sentence: `${an} and ${bn} read the same under ${VISION_NAME[vision]}.` });
      }
  return out.sort((x, y) => x.delta - y.delta);
}

export interface ColourBlind {
  readonly on: boolean;
  set(on: boolean): void;
  /** Tests only. */
  reset(): void;
}

export function createColourBlind(storage: () => KeptStorage | null): ColourBlind {
  const kept = keptValue(storage, COLOUR_BLIND_KEY);
  let on = $state(kept.read() === "on");
  const set = (next: boolean) => {
    on = next;
    if (next) kept.write("on");
    else kept.clear();
  };
  return {
    get on() {
      return on;
    },
    set,
    reset: () => set(false),
  };
}

export const colourBlind: ColourBlind = createColourBlind(windowStorage);
