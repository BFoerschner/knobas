/**
 * Who did something, as the history panel shows them.
 *
 * `knobas.activity.actor` is either `"user"` or `"sync:<source_id>"`
 * (`knobas_core::activity`), and the two mean genuinely different things —
 * one is a person's decision, the other is a mirror catching up. The panel has
 * to be able to tell them apart at a glance, which is why this is a parse and
 * not a `substring` at the call site.
 */

/** An actor, ready to draw. */
export interface Actor {
  /** `user`, `sync`, or `other` for an actor spelling knobas does not know. */
  kind: "user" | "sync" | "other";
  /** The two-character chip. */
  monogram: string;
  /** What the chip means, spelled out. */
  label: string;
}

export function parseActor(actor: string): Actor {
  if (actor === "user") {
    return { kind: "user", monogram: "ME", label: "you" };
  }
  if (actor.startsWith("sync:")) {
    const source = actor.slice("sync:".length);
    return {
      kind: "sync",
      monogram: source.slice(0, 2).toUpperCase() || "??",
      label: `synced by ${source}`,
    };
  }
  // Not a guess and not a crash: a row written by something that has not been
  // built yet still renders, and says exactly what the column holds.
  return { kind: "other", monogram: actor.slice(0, 2).toUpperCase() || "??", label: actor };
}
