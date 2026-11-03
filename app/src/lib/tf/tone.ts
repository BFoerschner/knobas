/**
 * The class a state draws its tone with (spec #2463): `tf-<tone>` on a
 * `tf-chip` or a `tf-dot`. A state with no tone (*No colour*, a toneless
 * declared value) and no state at all are the plain chip, the neutral style,
 * and the plain dot, the muted one.
 */
import type { Tone } from "../ipc/entity";

export function toneClass(state: { tone: Tone | null } | null | undefined): string {
  return state?.tone ? `tf-${state.tone}` : "";
}
