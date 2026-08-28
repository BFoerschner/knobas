/**
 * `config_schema` → a form model.
 *
 * Spec §3a: *"An adapter's descriptor declares everything the app needs to
 * host it: its config schema (the* Add source *form is **generated** from it,
 * not hand-built per adapter)"*. So there is no per-adapter table anywhere in
 * this file, and adding a fourth adapter adds no code here.
 *
 * ## The subset, and why there is one
 *
 * JSON Schema can express far more than a 140px-label form can render. This
 * module covers the shapes an adapter's *configuration* actually takes —
 * string, enum, integer/number with bounds, boolean, array-of-string — and
 * degrades everything else to a JSON textarea rather than refusing to draw
 * the form. That direction matters: an adapter whose schema outgrew the
 * subset stays configurable by hand on the day it ships, and the fix (teach
 * the subset a shape, or simplify the schema) is a later decision rather than
 * a release blocker.
 *
 * ## What is *not* a degradation
 *
 * A property named for a secret **throws**. See {@link schemaFields}.
 */
import { humanise } from "../shell/humanise";

/** How one field is drawn, and what an untouched one is worth. */
export type Control =
  | { kind: "text"; default: string }
  | { kind: "select"; options: string[]; default: string }
  | { kind: "number"; integer: boolean; min: number | null; max: number | null; default: number | null }
  | { kind: "toggle"; default: boolean }
  /** An array of strings, edited one value per line. */
  | { kind: "list"; default: string[] }
  /** Anything the subset does not cover, edited as JSON text. */
  | { kind: "json"; default: unknown };

/** One row of the generated form. */
export interface SchemaField {
  /** The property name — the key this field's value is written under. */
  key: string;
  /** `title`, or the key humanised. */
  label: string;
  /** `description`, rendered under the control and wired by `aria-describedby`. */
  help: string | null;
  required: boolean;
  control: Control;
}

/** What {@link validate} produces: per-key messages, and the config to send. */
export interface Validated {
  errors: Record<string, string>;
  config: Record<string, unknown>;
}

/**
 * Head nouns that mean "a credential".
 *
 * Matched against the **last** word of the property name, not against any
 * word in it. English compounds are head-final — `api_key` and `apiKey` *are*
 * keys, while `token_endpoint` is an endpoint and `secret_scanning` is a Gitea
 * repository flag. A guard that fired on any word would refuse to render two
 * legitimate adapters' forms, which is this check breaking working software
 * rather than catching broken software.
 *
 * `key` is in the set knowing it is the loosest member: it is the only way to
 * catch `api_key`/`apiKey`, which is the shape a real adapter gets wrong. An
 * adapter that genuinely wants a non-secret `…_key` property renames it — and
 * that is the cheaper error of the two, because it is caught by a thrown
 * message rather than by a token sitting in a Postgres column.
 */
const SECRET_HEADS = new Set([
  "password",
  "passphrase",
  "token",
  "secret",
  "credential",
  "credentials",
  "key",
  "pat",
]);

/** `api_key` → `["api", "key"]`, `apiKey` → `["api", "key"]`. Lowercased. */
function words(key: string): string[] {
  return key
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((word) => word.toLowerCase());
}

/** A plain JSON object — not an array, not null. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * The fields an adapter's `config_schema` asks for, in declaration order.
 *
 * Order is the adapter author's intent — it is the only sequencing signal the
 * descriptor carries — and both `JSON.parse` and an object literal preserve
 * insertion order for string keys, so it survives the trip over IPC.
 *
 * Anything that is not an object schema with properties yields `[]`: the
 * schema arrives as `unknown` from a process this one does not control, and a
 * malformed one must produce an empty form, not an exception in a dialog.
 *
 * @throws if a property is named for a secret. Contract §3 — "Nothing secret
 * ever reaches Postgres" — and `config` *is* a Postgres column, so such a
 * schema would have this form write a credential into the database. A
 * silently filtered field would leave the adapter looking configured with its
 * credential unset; loud is the only safe direction.
 */
export function schemaFields(schema: unknown): SchemaField[] {
  if (!isRecord(schema)) return [];
  if (schema.type !== "object") return [];
  const properties = schema.properties;
  if (!isRecord(properties)) return [];

  const required = new Set(
    Array.isArray(schema.required) ? schema.required.filter((key) => typeof key === "string") : [],
  );

  return Object.entries(properties).map(([key, raw]) => {
    if (SECRET_HEADS.has(words(key).at(-1) ?? "")) {
      throw new Error(
        `config_schema declares '${key}', which is a secret. Secrets live in the OS keychain and are set with set_source_secret; nothing secret may reach the config column.`,
      );
    }
    const property = isRecord(raw) ? raw : {};
    return {
      key,
      label: typeof property.title === "string" ? property.title : humanise(key),
      help: typeof property.description === "string" ? property.description : null,
      required: required.has(key),
      control: controlFor(property),
    };
  });
}

function controlFor(property: Record<string, unknown>): Control {
  const { type } = property;

  if (Array.isArray(property.enum)) {
    const options = property.enum.filter((option): option is string => typeof option === "string");
    // An enum whose members are not all strings is not a `<select>` this form
    // can round-trip, so it falls through to JSON rather than losing members.
    if (options.length === property.enum.length && options.length > 0) {
      const fallback = options[0] ?? "";
      return {
        kind: "select",
        options,
        default: typeof property.default === "string" ? property.default : fallback,
      };
    }
    return { kind: "json", default: property.default ?? null };
  }

  if (type === "string") {
    return { kind: "text", default: typeof property.default === "string" ? property.default : "" };
  }

  if (type === "integer" || type === "number") {
    return {
      kind: "number",
      integer: type === "integer",
      min: typeof property.minimum === "number" ? property.minimum : null,
      max: typeof property.maximum === "number" ? property.maximum : null,
      default: typeof property.default === "number" ? property.default : null,
    };
  }

  if (type === "boolean") {
    return { kind: "toggle", default: property.default === true };
  }

  if (type === "array") {
    const items = property.items;
    if (isRecord(items) && items.type === "string" && !Array.isArray(items.enum)) {
      const fallback = Array.isArray(property.default)
        ? property.default.filter((value): value is string => typeof value === "string")
        : [];
      return { kind: "list", default: fallback };
    }
    return { kind: "json", default: property.default ?? [] };
  }

  return { kind: "json", default: property.default ?? null };
}

/** A fresh form's values: every field at its declared default. */
export function defaultValues(fields: SchemaField[]): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  for (const field of fields) {
    values[field.key] = field.control.kind === "json"
      ? jsonText(field.control.default)
      : field.control.default;
  }
  return values;
}

/** A `json` control edits *text*, so its default is text too. */
function jsonText(value: unknown): string {
  if (value === null || value === undefined) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return "";
  }
}

/**
 * Check a form's values and build the object to send as `config`.
 *
 * **Empty optional values are omitted rather than sent as `""`.** An adapter
 * declares its own defaults, and a config row carrying `jql_filter: ""` is a
 * form's blank field masquerading as a deliberate choice — the adapter can no
 * longer tell "unset" from "set to nothing" and its default can never apply.
 * `false` and `0` are values, not absences, and are kept.
 */
export function validate(
  fields: SchemaField[],
  values: Record<string, unknown>,
): Validated {
  const errors: Record<string, string> = {};
  const config: Record<string, unknown> = {};

  for (const field of fields) {
    const raw = values[field.key];
    const { control } = field;

    if (control.kind === "select") {
      const value = typeof raw === "string" ? raw : "";
      if (value === "") {
        if (field.required) errors[field.key] = `${field.label} is required.`;
        continue;
      }
      if (!control.options.includes(value)) {
        errors[field.key] = `${field.label} must be one of ${control.options.join(", ")}.`;
        continue;
      }
      config[field.key] = value;
      continue;
    }

    if (control.kind === "text") {
      const value = typeof raw === "string" ? raw.trim() : "";
      if (value === "") {
        if (field.required) errors[field.key] = `${field.label} is required.`;
        continue;
      }
      config[field.key] = value;
      continue;
    }

    if (control.kind === "number") {
      if (raw === "" || raw === null || raw === undefined) {
        if (field.required) errors[field.key] = `${field.label} is required.`;
        continue;
      }
      const value = typeof raw === "number" ? raw : Number(raw);
      if (!Number.isFinite(value)) {
        errors[field.key] = `${field.label} must be a number.`;
        continue;
      }
      if (control.integer && !Number.isInteger(value)) {
        errors[field.key] = `${field.label} must be a whole number.`;
        continue;
      }
      if (control.min !== null && value < control.min) {
        errors[field.key] = `${field.label} must be at least ${control.min}.`;
        continue;
      }
      if (control.max !== null && value > control.max) {
        errors[field.key] = `${field.label} must be at most ${control.max}.`;
        continue;
      }
      config[field.key] = value;
      continue;
    }

    if (control.kind === "toggle") {
      // A toggle is always answered — it has two states and no third — so it
      // is written whether or not it was touched.
      config[field.key] = raw === true;
      continue;
    }

    if (control.kind === "list") {
      const list = toList(raw);
      if (list.length === 0) {
        if (field.required) errors[field.key] = `${field.label} is required.`;
        continue;
      }
      config[field.key] = list;
      continue;
    }

    // json
    const text = typeof raw === "string" ? raw.trim() : "";
    if (text === "") {
      if (field.required) errors[field.key] = `${field.label} is required.`;
      continue;
    }
    try {
      config[field.key] = JSON.parse(text);
    } catch {
      errors[field.key] = `${field.label} must be valid JSON.`;
    }
  }

  return { errors, config };
}

/**
 * A list control's value, from either shape it can be in.
 *
 * The textarea hands back one string with newlines in it; a caller that
 * already split it hands back an array. Blank lines are dropped — they are
 * how a person separates groups while typing, not entries they meant to send.
 */
export function toList(raw: unknown): string[] {
  const parts = typeof raw === "string" ? raw.split("\n") : Array.isArray(raw) ? raw : [];
  return parts
    .filter((part): part is string => typeof part === "string")
    .map((part) => part.trim())
    .filter((part) => part !== "");
}
