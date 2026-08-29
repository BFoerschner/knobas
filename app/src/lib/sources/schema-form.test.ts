/**
 * The Add-source form is **generated** from an adapter's `config_schema`
 * (spec §3a), so this module is the only thing standing between a new
 * adapter and a usable form. Everything here is about the two failure modes
 * that matter: a shape the subset does not cover must degrade rather than
 * crash, and a secret must never become a field.
 */
import { describe, expect, test } from "vitest";

import { EMPTY_SCHEMA, GITEA_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "./fixtures";
import { defaultValues, schemaFields, validate } from "./schema-form";

describe("schemaFields", () => {
  test("an enum becomes a select with its default preselected", () => {
    const [flavor] = schemaFields(JIRA_SCHEMA);
    expect(flavor).toMatchObject({ key: "flavor", label: "Deployment flavor", required: true });
    expect(flavor?.help).toBe("Data Center speaks REST v2; Cloud is not supported yet.");
    expect(flavor?.control).toEqual({
      kind: "select",
      options: ["datacenter", "cloud"],
      default: "datacenter",
    });
  });

  test("an array of strings becomes a list control", () => {
    const projects = schemaFields(JIRA_SCHEMA).find((f) => f.key === "projects");
    expect(projects?.control).toEqual({ kind: "list", default: [] });
  });

  test("integer bounds survive", () => {
    const n = schemaFields(TEAMCITY_SCHEMA).find((f) => f.key === "builds_per_config");
    expect(n?.control).toEqual({ kind: "number", integer: true, min: 1, max: 500, default: 100 });
  });

  test("a boolean becomes a toggle", () => {
    const f = schemaFields({
      type: "object",
      properties: { verify_tls: { type: "boolean", default: true } },
    })[0];
    expect(f?.control).toEqual({ kind: "toggle", default: true });
  });

  test("declaration order is preserved — it is the adapter author's intent", () => {
    expect(schemaFields(GITEA_SCHEMA).map((f) => f.key)).toEqual(["owners", "repos", "username"]);
  });

  test("a property with no title is humanised from its key", () => {
    const f = schemaFields({ type: "object", properties: { jql_filter: { type: "string" } } })[0];
    expect(f?.label).toBe("Jql filter");
  });

  test("an adapter with no configuration yields no fields", () => {
    expect(schemaFields(EMPTY_SCHEMA)).toEqual([]);
  });

  /**
   * `{"type": ["string", "null"]}` is how every shipped adapter spells an
   * optional string — Jira's `username`, `jql_filter` and `epic_link_field`,
   * Gitea's `username`. Read as "not a string", each of them draws a JSON
   * textarea, so typing a username into it is a parse error unless the reader
   * knows to quote it. The nullable member is the schema saying "unset is
   * allowed", which this form already expresses by omitting an empty value.
   */
  test("a nullable string is a text field, not a JSON textarea", () => {
    const f = schemaFields({
      type: "object",
      properties: { username: { type: ["string", "null"], default: null, title: "Username" } },
    })[0];
    expect(f?.control).toEqual({ kind: "text", default: "" });
  });

  test("nullability does not change what any other type draws", () => {
    const fields = schemaFields({
      type: "object",
      properties: {
        page_size: { type: ["integer", "null"], minimum: 1, maximum: 1000, default: 100 },
        verify_tls: { type: ["boolean", "null"], default: true },
        owners: { type: ["array", "null"], items: { type: "string" } },
      },
    });
    expect(fields.map((f) => f.control.kind)).toEqual(["number", "toggle", "list"]);
  });

  test("a type union that is not just 'or null' still degrades to JSON", () => {
    const f = schemaFields({
      type: "object",
      properties: { either: { type: ["string", "integer"] } },
    })[0];
    expect(f?.control.kind).toBe("json");
  });

  test("a shape the subset does not cover degrades to a JSON control, never to a crash", () => {
    const f = schemaFields({
      type: "object",
      properties: { matrix: { type: "array", items: { type: "object" } } },
    })[0];
    expect(f?.control.kind).toBe("json");
  });

  test("garbage in, empty out", () => {
    expect(schemaFields(null)).toEqual([]);
    expect(schemaFields(undefined)).toEqual([]);
    expect(schemaFields("nonsense")).toEqual([]);
    expect(schemaFields({ type: "string" })).toEqual([]);
    expect(schemaFields({ type: "object" })).toEqual([]);
    expect(schemaFields({ type: "object", properties: [] })).toEqual([]);
  });

  /**
   * Contract §3: "Nothing secret ever reaches Postgres". `config` is a
   * Postgres column, so an adapter that declares a secret in its config schema
   * would have the form write a token into the database. Failing loudly is the
   * only answer that cannot be ignored — a filtered-out field would leave the
   * adapter apparently working and its credential silently unset.
   */
  test.each(["password", "token", "secret", "api_key", "apiKey", "Passphrase", "private_key"])(
    "a `%s` property is a bug in the adapter, and throws",
    (key) => {
      expect(() => schemaFields({ type: "object", properties: { [key]: { type: "string" } } }))
        .toThrow(/secret/i);
    },
  );

  test("a property whose head noun is not a credential is not a secret", () => {
    // `token_endpoint` is a URL and `secret_scanning` is a Gitea repository
    // flag. The guard reads the *last* word, so it does not refuse to render
    // either adapter's form — a guard that fired on any word would.
    const keys = schemaFields({
      type: "object",
      properties: { token_endpoint: { type: "string" }, secret_scanning: { type: "boolean" } },
    }).map((f) => f.key);
    expect(keys).toEqual(["token_endpoint", "secret_scanning"]);
  });
});

describe("defaultValues", () => {
  test("seeds every control from its declared default", () => {
    expect(defaultValues(schemaFields(JIRA_SCHEMA))).toEqual({
      flavor: "datacenter",
      projects: [],
      jql_filter: "",
      username: "",
    });
    expect(defaultValues(schemaFields(TEAMCITY_SCHEMA))).toEqual({
      project_ids: [],
      build_type_ids: [],
      builds_per_config: 100,
    });
  });
});

describe("validate", () => {
  test("reports missing required fields by key and builds the config object", () => {
    const fields = schemaFields(JIRA_SCHEMA);
    const bad = validate(fields, { flavor: "", projects: ["PAY"], jql_filter: "", username: "" });
    expect(bad.errors.flavor).toMatch(/required/i);

    const good = validate(fields, {
      flavor: "datacenter",
      projects: ["PAY", "OPS"],
      jql_filter: "",
      username: "mara",
    });
    expect(good.errors).toEqual({});
    // Empty optional values are omitted, not sent as "" — the adapter's own
    // defaults must be able to apply.
    expect(good.config).toEqual({ flavor: "datacenter", projects: ["PAY", "OPS"], username: "mara" });
  });

  test("rejects a value outside an enum and a number outside its bounds", () => {
    expect(validate(schemaFields(JIRA_SCHEMA), { flavor: "onprem" }).errors.flavor).toMatch(/one of/i);
    expect(
      validate(schemaFields(TEAMCITY_SCHEMA), { builds_per_config: 5000 }).errors.builds_per_config,
    ).toMatch(/500/);
    expect(
      validate(schemaFields(TEAMCITY_SCHEMA), { builds_per_config: 0 }).errors.builds_per_config,
    ).toMatch(/1/);
  });

  test("rejects a non-integer where the schema says integer", () => {
    expect(
      validate(schemaFields(TEAMCITY_SCHEMA), { builds_per_config: 12.5 }).errors.builds_per_config,
    ).toMatch(/whole number/i);
  });

  test("a number that is not a number at all is an error, not NaN in the config", () => {
    const out = validate(schemaFields(TEAMCITY_SCHEMA), { builds_per_config: "many" });
    expect(out.errors.builds_per_config).toMatch(/number/i);
    expect(out.config).not.toHaveProperty("builds_per_config");
  });

  test("an empty list is omitted, and a list keeps only non-blank entries", () => {
    const fields = schemaFields(GITEA_SCHEMA);
    const out = validate(fields, { owners: [], repos: ["acme", "  ", "beta"], username: "" });
    expect(out.errors).toEqual({});
    expect(out.config).toEqual({ repos: ["acme", "beta"] });
  });

  test("a json control parses its text, and reports the parse error in place", () => {
    const fields = schemaFields({
      type: "object",
      properties: { matrix: { type: "array", items: { type: "object" } } },
    });
    expect(validate(fields, { matrix: "{ not json" }).errors.matrix).toMatch(/json/i);
    expect(validate(fields, { matrix: '[{"a":1}]' }).config).toEqual({ matrix: [{ a: 1 }] });
  });

  test("a false toggle is kept, because false is a value and not an absence", () => {
    const fields = schemaFields({
      type: "object",
      properties: { verify_tls: { type: "boolean", default: true } },
    });
    expect(validate(fields, { verify_tls: false }).config).toEqual({ verify_tls: false });
  });

  test("a required field that was never touched is reported, not silently defaulted", () => {
    const fields = schemaFields(JIRA_SCHEMA);
    expect(validate(fields, {}).errors.flavor).toMatch(/required/i);
  });

  test("nothing outside the schema reaches the config object", () => {
    const fields = schemaFields(GITEA_SCHEMA);
    const out = validate(fields, { owners: ["acme"], password: "hunter2" });
    expect(out.config).toEqual({ owners: ["acme"] });
  });
});
