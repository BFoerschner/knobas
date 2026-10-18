/**
 * What both *Add a source* dialogs share: the rooms UI's `AddSource.svelte`
 * and the new shell's `tf/settings/TfAddSource.svelte` (#903). The rules a
 * new source is held to, and the one fill *Test connection* applies.
 */
import type { AuthMethod, SourceDescriptor } from "../ipc/sources";
import type { SchemaField } from "./schema-form";

/** How an `AuthMethod` reads to a person. */
export const AUTH_LABEL: Record<AuthMethod, string> = {
  UserPassword: "Username and password",
  Pat: "Personal access token",
  ApiToken: "API token",
  OAuth: "OAuth",
};

/** Contract §4.1. The id is the entity namespace and is immutable (P10). */
export const SOURCE_ID_PATTERN = /^[a-z][a-z0-9-]{0,31}$/;

/** A new source's schedule, unless the reader picks another: every 15 minutes. */
export const DEFAULT_INTERVAL_SECS = 900;

/**
 * The floor of an adapter this build does not know, and the one every adapter
 * declares unless it says otherwise: `knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS`.
 */
export const DEFAULT_MIN_SYNC_INTERVAL_SECS = 60;

/**
 * The schedule a new source of `adapter` starts with (#1188): its declared
 * floor when that is under a minute (Uptime Kuma 45 s, TeamCity 20 s), because
 * an adapter that declares one is one whose state is watched; else
 * {@link DEFAULT_INTERVAL_SECS}.
 */
export function defaultIntervalFor(
  adapter: Pick<SourceDescriptor, "min_sync_interval_secs">,
): number {
  return adapter.min_sync_interval_secs < DEFAULT_MIN_SYNC_INTERVAL_SECS
    ? adapter.min_sync_interval_secs
    : DEFAULT_INTERVAL_SECS;
}

/**
 * The config property an adapter puts its identity in, by convention.
 *
 * Every shipped adapter spells it `username`, and so must the next, or `@me`,
 * *My items* and *Mine, untouched* have nothing to resolve against on that
 * source. Keying on the property **name** keeps the dialogs adapter-agnostic:
 * a convention every adapter follows, not a table of what each one's config
 * keys mean (#82).
 */
export const IDENTITY_FIELD = "username";

/**
 * Put what a passing *Test connection* found into the form: the account into
 * the identity field (#82), and each `discovered` value into the field of that
 * key (#297).
 *
 * The identity and a discovered id are the same shape of fact, a value the far
 * end owns and the reader cannot know, so there is one rule. Jira's Epic Link
 * custom field id is minted per instance: one copied off another server does
 * not fail, it silently syncs the wrong field.
 *
 * **Only a text field, and only an empty one.** A value already there was
 * typed by somebody who meant it (a service account, an id they know), and a
 * test run afterwards must not quietly replace it. The account is written in
 * the server's own spelling, never normalised: author matching is
 * case-sensitive. A key naming a property the adapter does not declare fills
 * nothing, and an absent account is not a failure.
 */
export function fillFound(
  fields: SchemaField[],
  values: Record<string, unknown>,
  account: string | null,
  discovered: Record<string, string> | null | undefined,
): void {
  const found: Record<string, string> = { ...(discovered ?? {}) };
  if (account !== null && account !== "") found[IDENTITY_FIELD] = account;
  for (const [key, value] of Object.entries(found)) {
    if (typeof value !== "string" || value === "") continue;
    const field = fields.find((candidate) => candidate.key === key);
    if (!field || field.control.kind !== "text") continue;
    const current = values[key];
    if (typeof current === "string" && current.trim() !== "") continue;
    values[key] = value;
  }
}
