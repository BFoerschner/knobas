/**
 * The config schemas of the adapters M1 ships, as JSON Schema.
 *
 * These are the shapes {@link ../sources/schema-form} has to render, so they
 * are the test corpus for it. They are transcribed from the contract's §4.2
 * config column rather than imported from the adapters, and that is
 * deliberate: an adapter's real `config_schema` reaches the frontend over IPC
 * as `unknown`, so the form model must be correct about the *wire* shape and
 * not about a Rust type it can never see. A transcription that drifts from an
 * adapter shows up as a form that renders the wrong control — which is
 * exactly the failure these fixtures exist to reproduce in a test rather than
 * in the window.
 *
 * Not test-only. `Diagnostics`/`AddSource` never read them, but a QA fixture
 * does, and a `.test.ts` module cannot be imported from a `.svelte` one.
 */

/** Jira Data Center — contract §4.2. */
export const JIRA_SCHEMA = {
  type: "object",
  properties: {
    flavor: {
      type: "string",
      enum: ["datacenter", "cloud"],
      default: "datacenter",
      title: "Deployment flavor",
      description: "Data Center speaks REST v2; Cloud is not supported yet.",
    },
    projects: { type: "array", items: { type: "string" }, title: "Projects" },
    jql_filter: { type: "string", title: "JQL filter" },
    // `["string", "null"]`, verbatim from the adapter: it is how every shipped
    // adapter spells an optional string, and reading it as "not a string" is
    // what drew this field as a JSON textarea. The description is the
    // adapter's too — it is one half of #82 and worth having under a test.
    username: {
      type: ["string", "null"],
      default: null,
      title: "Username",
      description:
        "Your Jira account. Filled in by Test connection; used for @me and My items. Also the login name for user + password authentication.",
    },
  },
  required: ["flavor"],
} as const;

/** Gitea — contract §4.2. */
export const GITEA_SCHEMA = {
  type: "object",
  properties: {
    owners: { type: "array", items: { type: "string" }, title: "Owners" },
    repos: { type: "array", items: { type: "string" }, title: "Repositories" },
    username: {
      type: ["string", "null"],
      default: null,
      title: "Username",
      description: "Your Gitea account. Filled in by Test connection; used for @me filters.",
    },
  },
} as const;

/** TeamCity — contract §4.2. */
export const TEAMCITY_SCHEMA = {
  type: "object",
  properties: {
    project_ids: { type: "array", items: { type: "string" }, title: "Project ids" },
    build_type_ids: { type: "array", items: { type: "string" }, title: "Build config ids" },
    builds_per_config: {
      type: "integer",
      minimum: 1,
      maximum: 500,
      default: 100,
      title: "Builds per config",
    },
    // A plain `"string"`, unlike the other two: the third spelling of the one
    // property the Add-source dialog fills in, so the fill is exercised
    // against every shape an adapter really declares it in.
    username: {
      type: "string",
      title: "Username",
      description:
        "Your TeamCity account. Filled in by Test connection; used for @me and My items. Also the login name for user + password authentication.",
    },
  },
} as const;

/** What `knobas-source-mock` declares today: an adapter that needs no configuring. */
export const EMPTY_SCHEMA = { type: "object", properties: {} } as const;
