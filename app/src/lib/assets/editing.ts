/**
 * What the Tree's writes need to decide before they call anything — pure, so
 * the decisions are readable and testable without a window or a database
 * (issue #429, spec #427 stories 6, 17 and 34).
 *
 * `tree.ts` is the same shape one axis over: that module is the *layout*
 * arithmetic, this one is the *form* arithmetic. Both exist so that
 * `AssetsView.svelte` is fetching and drawing and nothing else.
 *
 * ## The type table is read, never copied
 *
 * Everything here that knows what a type is takes an {@link AssetType} list as
 * an argument, and that list comes off `asset_types`. There is no list of
 * sixteen types in this file and there must not be one: the ids travel in the
 * share export, and a second copy is the half that goes stale.
 *
 * ## Who refuses what
 *
 * The backend is the authority on a property value — a blank text, a date that
 * is not `YYYY-MM-DD`, a URL with no scheme and a typed key given the wrong
 * kind are all refused by `assets::edit`, by name, and the pane shows the
 * sentence it answers with. {@link parseProperty} therefore refuses exactly
 * **one** thing, and only because it is a thing the wire cannot carry: a
 * number that is not a number. `JSON.stringify(NaN)` is `null`, so an
 * unparsable number would not arrive as a bad number — it would arrive as a
 * cleared property, which is a different edit from the one the reader made.
 */
import type {
  AssetProperty,
  AssetType,
  PropertyEdit,
  PropertyKind,
  PropertyValue,
} from "../ipc/assets";

/** The types offered when creating, split into the usual ones and the rest. */
export interface TypeChoices {
  /**
   * What the parent's type conventionally holds, in the table's own order.
   *
   * Empty at the top of the estate — there is no parent to have a convention —
   * and empty under a type that suggests nothing (a table, a network, a
   * `custom`). Empty is the honest answer there, not a reason to guess.
   */
  usual: AssetType[];
  /** Everything else, in the table's order. Together with `usual`, all of them. */
  rest: AssetType[];
}

/**
 * Which types to offer for a new asset under a parent of `parentTypeId`.
 *
 * `null` is the estate's top level. Story 17 is *"the type conventions
 * suggesting what usually goes here **and any type allowed**"*, so the two
 * lists always add up to the whole table: `usual` is an ordering, never a
 * filter, and a reader who wants a database server directly under a site can
 * still pick one.
 *
 * A `parentTypeId` the table does not carry — an estate file naming a type
 * this build has lost — behaves like the top level: nothing is usual, and
 * everything is offered.
 */
export function typeChoices(types: AssetType[], parentTypeId: string | null): TypeChoices {
  const parent = types.find((candidate) => candidate.id === parentTypeId);
  const suggested = parent?.suggests ?? [];
  return {
    usual: types.filter((type) => suggested.includes(type.id)),
    rest: types.filter((type) => !suggested.includes(type.id)),
  };
}

/**
 * The *usual here* line, or `null` where nothing is usual.
 *
 * `null` rather than an empty string: the caller draws nothing at all, and a
 * label reading "Usual here:" with nothing after it is worse than no label.
 */
export function usualHere(choices: TypeChoices): string | null {
  if (choices.usual.length === 0) return null;
  return `Usual here: ${choices.usual.map((type) => type.label).join(", ")}`;
}

/**
 * The type a create dialog opens on: the first usual one, else the first of
 * the table.
 *
 * The table's order is spec #427's — outermost thing first, down to the
 * smallest — so falling back to its head means the top of the estate opens on
 * *Site*, which is where §12.1's chain begins.
 */
export function defaultTypeId(choices: TypeChoices): string | null {
  return choices.usual[0]?.id ?? choices.rest[0]?.id ?? null;
}

/**
 * Which kind an editor for `property` should offer.
 *
 * Three sources, in order, and the middle one is why the type table is on the
 * wire at all: a **declared key nobody has filled in** arrives with
 * `value: null`, and there is no kind in a `null`. Without the schema, a
 * reader typing a service's `port` would be guessing whether the backend wants
 * `8080` or `"8080"` — and `assets::edit` refuses the wrong one by name.
 *
 * `text` last, for a custom key whose stored value could not be read back.
 */
export function kindFor(property: AssetProperty, schema: AssetType | undefined): PropertyKind {
  if (property.value !== null) return property.value.kind;
  const declared = schema?.properties.find((candidate) => candidate.key === property.key);
  return declared?.kind ?? "text";
}

/** What an editor for `property` starts with — its current value, as text. */
export function draftOf(property: AssetProperty): string {
  return property.value === null ? "" : String(property.value.value);
}

/**
 * How each of the four kinds is offered and drawn.
 *
 * One table rather than a `switch` per surface: before this the kinds were
 * enumerated three times over — an `if`-cascade choosing an `<input type>`, a
 * hand-written list of `<option>`s, and this module's own parse — and three
 * enumerations of a closed four-member set are three places a fifth kind
 * (`secret`, deferred with its keychain convention) would have to be
 * remembered.
 *
 * The **order is the wire's**, `PropertyKind::ALL`'s, so the picker reads the
 * way the union is declared.
 */
export const PROPERTY_KINDS: { kind: PropertyKind; label: string; input: string }[] = [
  { kind: "text", label: "Text", input: "text" },
  { kind: "number", label: "Number", input: "number" },
  // `date` earns a native picker: the backend wants `YYYY-MM-DD` and this is
  // the one control that cannot produce anything else.
  { kind: "date", label: "Date", input: "date" },
  { kind: "url", label: "URL", input: "url" },
];

/** The `<input type>` an editor for `kind` uses. */
export function inputTypeFor(kind: PropertyKind): string {
  return PROPERTY_KINDS.find((entry) => entry.kind === kind)?.input ?? "text";
}

/** A parsed field: a value to send, a clear, or a refusal to show in place. */
export type Parsed = { value: PropertyValue | null } | { refused: string };

/**
 * One typed field as the wire carries it — or `null` for **clear**.
 *
 * Blank is a clear and not a blank value: `assets::PropertyValue::vet` refuses
 * an empty text with *"clear the property instead"*, so a reader who empties a
 * field and saves means the property to go, and this is where that reading is
 * made. `null` on a `PropertyEdit` removes the key, which is the unambiguous
 * clear the tagged union exists for.
 *
 * The only refusal is a number that is not one — see the module docs.
 */
export function parseProperty(kind: PropertyKind, raw: string): Parsed {
  const trimmed = raw.trim();
  if (trimmed === "") return { value: null };
  if (kind === "number") {
    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed)) {
      return { refused: `“${trimmed}” is not a number.` };
    }
    return { value: { kind: "number", value: parsed } };
  }
  return { value: { kind, value: trimmed } };
}

/**
 * The edit that sets or clears one property.
 *
 * A helper rather than an object literal at each call site: the pane writes
 * this shape from three places — a typed row, a custom row and the *add a
 * property* form — and a tagged union spelled by hand three times is three
 * chances to spell the tag differently.
 */
export function propertyEdit(key: string, value: PropertyValue | null): PropertyEdit {
  return { field: "property", key, value };
}
