/**
 * An adapter's own configuration on a source's page (#904, #1021), and the
 * config the page saves.
 *
 * The fields are generated from the adapter's `config_schema` (spec §3a) by
 * the same form model the *Add a source* dialog uses (`sources/schema-form`),
 * so nothing here names an adapter: Jira's projects, JQL and field ids show
 * because Jira's schema declares them.
 *
 * ## Saving sends the whole config, built from what is stored now
 *
 * `update_source` replaces `config` whole. The sync writes into it too: it
 * records a Jira source's Sprint and Epic Link field ids where the stored
 * config leaves them empty (#889). A page that saved its own copy, read when
 * it opened, would blank an id the sync recorded since. So the page sends the
 * config as stored at the moment of saving, with only the fields the reader
 * changed laid over it ({@link configChanges}, {@link withChanges}).
 */
import { defaultValues, schemaFields, type SchemaField } from "../../sources/schema-form";
import { IDENTITY_FIELD } from "../../sources/new-source";

/**
 * An adapter's config fields, or why there are none (`schemaFields` throws for
 * a schema that declares a secret). A source's page leaves the user out, since
 * Connection draws it; *Add a source* keeps it (`keepUser`).
 */
export function pageFields(
  schema: unknown,
  { keepUser = false }: { keepUser?: boolean } = {},
): { fields: SchemaField[]; error: string | null } {
  try {
    const fields = schemaFields(schema);
    return { fields: keepUser ? fields : fields.filter((field) => field.key !== IDENTITY_FIELD), error: null };
  } catch (cause) {
    return { fields: [], error: cause instanceof Error ? cause.message : String(cause) };
  }
}

/** A stored config as an object; anything else is an empty one. */
function asObject(config: unknown): Record<string, unknown> {
  return typeof config === "object" && config !== null && !Array.isArray(config)
    ? (config as Record<string, unknown>)
    : {};
}

/**
 * The form's values for a stored config: each field as stored, or at its
 * declared default where the config leaves it out.
 */
export function formValues(fields: SchemaField[], config: unknown): Record<string, unknown> {
  const stored = asObject(config);
  const values = defaultValues(fields);
  for (const field of fields) {
    if (!(field.key in stored)) continue;
    const value = stored[field.key];
    const { kind } = field.control;
    if (value === null || value === undefined) {
      values[field.key] = kind === "list" ? [] : kind === "toggle" ? false : "";
    } else if (kind === "json") {
      values[field.key] = JSON.stringify(value, null, 2);
    } else if (kind === "list") {
      values[field.key] = Array.isArray(value) ? [...value] : value;
    } else {
      values[field.key] = value;
    }
  }
  return values;
}

/**
 * What the reader changed: each key whose value differs between the config
 * the form was read from and the one it holds now. `undefined` is a field the
 * reader emptied, which the save removes.
 */
export function configChanges(
  loaded: Record<string, unknown>,
  edited: Record<string, unknown>,
  keys: readonly string[],
): Record<string, unknown> {
  const changes: Record<string, unknown> = {};
  for (const key of keys) {
    if (JSON.stringify(loaded[key]) === JSON.stringify(edited[key])) continue;
    changes[key] = edited[key];
  }
  return changes;
}

/** The stored config with the reader's changes laid over it. */
export function withChanges(stored: unknown, changes: Record<string, unknown>): Record<string, unknown> {
  const next: Record<string, unknown> = { ...asObject(stored) };
  for (const [key, value] of Object.entries(changes)) {
    if (value === undefined) delete next[key];
    else next[key] = value;
  }
  return next;
}
