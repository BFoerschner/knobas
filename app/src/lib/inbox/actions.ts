/**
 * The forms an inbox action takes on the wire (issue #45).
 *
 * An entry's `actions` are `knobas_source::WriteOp` **identifiers** — the
 * backend has already filtered them to what that item's source declares — and
 * this is where each becomes a payload `submitWrite` can carry and a word a
 * button can say.
 *
 * ## Why this is a table and not a switch inside the view
 *
 * `crates/knobas-app/src/inbox.rs` holds the other half: every category's
 * candidate ops, and a test there reads *this file* and fails if a candidate
 * has no form here. A category that grows an op the interface cannot render
 * would otherwise be a button that silently never appears, on the one surface
 * whose whole promise is that the action an item wants is one press away.
 *
 * ## Why an op absent from here is skipped rather than drawn disabled
 *
 * `WriteOp` grows per milestone (ADR-0006), so a newer backend can offer an
 * identifier this build has never heard of. Drawing a dead button for it would
 * be the interface lying in the other direction — story 23 asks for the button
 * to be *absent* when it cannot work, and "this build does not know how" is
 * one of the ways it cannot work.
 */
import type { WriteOpPayload } from "../ipc/sources";

/** What one action needs from the reader before it can go. */
export type ActionForm =
  /** Press and it goes. */
  | { kind: "immediate"; label: string; build: (entity: string) => WriteOpPayload }
  /** Opens a box; the words are the payload. */
  | { kind: "text"; label: string; placeholder: string; build: (entity: string, body: string) => WriteOpPayload };

/**
 * One entry per op an inbox category asks for.
 *
 * `approve` carries an **empty body** deliberately: `WriteOp::Approve`'s own
 * documentation says "an approval with no words is an approval", and story 12
 * is that unblocking somebody is immediate. Somebody who wants to say
 * something as well has *Comment* beside it.
 */
export const ACTION_FORMS: Record<string, ActionForm> = {
  approve: {
    kind: "immediate",
    label: "Approve",
    build: (entity) => ({ Approve: { entity, body: "" } }),
  },
  rerun_build: {
    kind: "immediate",
    label: "Re-run",
    build: (entity) => ({ RerunBuild: { entity } }),
  },
  comment: {
    kind: "text",
    label: "Comment",
    placeholder: "Reply…",
    build: (entity, body) => ({ Comment: { entity, body } }),
  },
};

/** The form for an op, or `null` if this build has none. */
export function formFor(op: string): ActionForm | null {
  return ACTION_FORMS[op] ?? null;
}

/**
 * The actions of one entry this build can actually draw, in the order the
 * backend offered them — which is the category's order, best first.
 */
export function drawable(actions: string[]): { op: string; form: ActionForm }[] {
  return actions
    .map((op) => ({ op, form: formFor(op) }))
    .filter((entry): entry is { op: string; form: ActionForm } => entry.form !== null);
}
