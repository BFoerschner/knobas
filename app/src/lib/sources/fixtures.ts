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
    username: { type: "string", title: "Username", description: "Basic auth only." },
  },
  required: ["flavor"],
} as const;

/** Gitea — contract §4.2. */
export const GITEA_SCHEMA = {
  type: "object",
  properties: {
    owners: { type: "array", items: { type: "string" }, title: "Owners" },
    repos: { type: "array", items: { type: "string" }, title: "Repositories" },
    username: { type: "string", title: "Username" },
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
  },
} as const;

/** What `knobas-source-mock` declares today: an adapter that needs no configuring. */
export const EMPTY_SCHEMA = { type: "object", properties: {} } as const;
