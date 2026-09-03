/**
 * The §3a generic payload projection.
 *
 * Spec §3a, verbatim: *"an **unknown kind gets a generic detail view** —
 * title, metadata fields projected from the raw payload, body text, the links
 * panel, and actions derived from the adapter's declared capabilities. A new
 * ticket system is browsable on day one."*
 *
 * In M1 **every** kind uses this view. The tailored per-kind views of spec §5
 * are write-backs (status dropdown, approve, re-run) and their *read* halves
 * cannot be built either: the fields they show — `status`, `approvals`,
 * `checks` — live inside each adapter's own payload shape and differ per
 * adapter, which is the table §3a forbids knobas from carrying.
 *
 * ## Nothing here renders
 *
 * Every string this produces is source text: whatever somebody typed into a
 * ticket, `<script>` included (gotcha 7). The projector deliberately does no
 * escaping and no formatting — `PayloadView` interpolates, Svelte escapes, and
 * a projector that pre-rendered anything would be the one place that defence
 * could be undone.
 */
import { humanise } from "../shell/humanise";

/**
 * A value, projected — without the key it came under.
 *
 * Split out from [`Projected`] rather than written as three full members of
 * one union, because `Omit<>` over a union distributes into a shape that has
 * none of the three's own fields, and every `.text` access downstream then
 * stops type-checking.
 */
export type ProjectedValue =
  | { kind: "scalar"; text: string }
  /** A string long enough to need its own paragraph. */
  | { kind: "text"; text: string }
  /** An array or object: a one-line summary, with the JSON behind a disclosure. */
  | { kind: "nested"; summary: string; json: string };

/** One projected field. */
export type Projected = { key: string; label: string } & ProjectedValue;

/**
 * How long a string may be before it gets its own block.
 *
 * The `.kv` value column is one line beside a 150 px label; a description does
 * not fit there and a status does. Exported so the boundary can be tested from
 * both sides rather than guessed at.
 */
export const LONG_TEXT = 80;

/** What an absent value looks like. Never the word `null`. */
const ABSENT = "—";

/**
 * Every field of `payload`, in the order the source sent them.
 *
 * A payload that is not an object is one field called *Payload*; `null` and
 * `undefined` project to nothing at all, because a detail view with an empty
 * *Details* section is more honest than one with a row saying "—".
 */
export function projectPayload(payload: unknown): Projected[] {
  if (payload === null || payload === undefined) return [];

  if (typeof payload !== "object") {
    return [
      { key: "payload", label: "Payload", kind: "text", text: String(payload) },
    ];
  }

  if (Array.isArray(payload)) {
    return [{ key: "payload", label: "Payload", ...nested(payload) }];
  }

  return Object.entries(payload).map(([key, value]) => ({
    key,
    label: humanise(key),
    ...project(value),
  }));
}

/** One value, without its key. */
function project(value: unknown): ProjectedValue {
  if (value === null || value === undefined)
    return { kind: "scalar", text: ABSENT };
  if (typeof value === "object") return nested(value);

  const text = String(value);
  return text.length > LONG_TEXT
    ? { kind: "text", text }
    : { kind: "scalar", text };
}

/** An array or object: how big it is, and the whole of it behind a disclosure. */
function nested(value: object): {
  kind: "nested";
  summary: string;
  json: string;
} {
  const summary = Array.isArray(value)
    ? count(value.length, "item")
    : count(Object.keys(value).length, "field");
  return { kind: "nested", summary, json: stringify(value) };
}

function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

/**
 * `JSON.stringify`, which is allowed to fail.
 *
 * A payload is a source system's JSON as `serde_json` decoded it, so a cycle
 * cannot arrive over the IPC — but a `BigInt`, or a value with a throwing
 * `toJSON`, still can from a caller that is not the bridge. A projector that
 * threw would take the whole detail view down over one unreadable field, so
 * the field says it is unreadable and the other twenty still render.
 */
function stringify(value: object): string {
  try {
    return JSON.stringify(value, null, 2) ?? ABSENT;
  } catch (error) {
    return `(this value could not be rendered: ${error instanceof Error ? error.message : String(error)})`;
  }
}
