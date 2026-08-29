/**
 * The config schemas of the adapters M1 ships, as JSON Schema.
 *
 * These are the shapes {@link ../sources/schema-form} has to render, so they
 * are the test corpus for it. They are transcribed rather than imported: an
 * adapter's real `config_schema` reaches the frontend over IPC as `unknown`,
 * so the form model must be correct about the *wire* shape and not about a
 * Rust type it can never see.
 *
 * ## The rule, because a transcription drifts
 *
 * > **A property that is here is verbatim. A property may be absent, and each
 * > absence is named.**
 *
 * Verbatim means all of it — `type`, `title`, `description`, `default`,
 * bounds, `items` — not just the parts a test happens to read. The looser
 * version of this rule is what shipped a bug: the fixture spelled Jira's
 * `username` as `{type: "string"}` while every adapter spells an optional
 * string `{type: ["string", "null"]}`, so `controlFor` fell through to a JSON
 * textarea and typing a username was a parse error unless you knew to quote
 * it (#82, fixed in #110). It was invisible in tests, because the fixture
 * disagreed with reality and the tests agreed with the fixture.
 *
 * Two more drifts of the same kind were found afterwards and are corrected
 * here (#124). Both changed behaviour, and the first is the shape to watch
 * for, because it made the fixture *stricter* than reality — a test can pass
 * against an invented constraint while the real form accepts what the test
 * believes is rejected:
 *
 * - Jira's fixture declared `required: ["flavor"]`; the adapter declares no
 *   `required` at all, so `validate` emitted a required error the real form
 *   never produces. `required` is still a shape this form model supports, and
 *   it is exercised by a schema that says so about itself — see
 *   `schema-form.test.ts` — rather than by a shipped adapter pretending to.
 * - TeamCity's `builds_per_config` said `maximum: 500` against the adapter's
 *   `MAX_BUILDS_PER_CONFIG = 10_000`, twenty times too low: a bound the form
 *   enforced and the source did not.
 *
 * ## What is absent, and why
 *
 * The absences are named rather than counted, because a name is checkable and
 * a count is one more thing to keep right. They are absent because putting all
 * of them in would make this a second copy of three adapters, with three times
 * the surface to keep verbatim — and all but one draw a control this corpus
 * already exercises: `epic_link_field` is a nullable string like `jql_filter`,
 * and the rest are integers like `builds_per_config`.
 *
 * - Jira: `epic_link_field`, `page_size`, `rate_per_sec`, `rate_burst`,
 *   `connect_timeout_secs`, `request_timeout_secs`.
 * - Gitea: `commits_per_repo`, `prs_per_repo`, `include_pr_comments`,
 *   `rate_limit_per_sec`.
 * - TeamCity: `rate_limit_per_sec`.
 *
 * The one that does cost coverage is Gitea's `include_pr_comments`: it is the
 * only boolean any M1 adapter declares, so with it absent every `toggle` test
 * in the suite runs on a schema written for the test rather than on one an
 * adapter ships. That is a gap, not a licensed absence.
 *
 * Not test-only. `Diagnostics`/`AddSource` never read them, but a QA fixture
 * does, and a `.test.ts` module cannot be imported from a `.svelte` one.
 */

/** Jira Data Center — `knobas-source-jira`'s `descriptor::config_schema`. */
export const JIRA_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    flavor: {
      type: "string",
      enum: ["datacenter", "cloud"],
      default: "datacenter",
      title: "Deployment",
      description: "Data Center / Server speaks REST v2. Cloud is not supported yet.",
    },
    projects: {
      type: "array",
      default: [],
      title: "Projects",
      description:
        "Project keys to sync, e.g. PAY. Leave empty to sync everything this account can see.",
      items: { type: "string", pattern: "^[A-Z][A-Z0-9_]{0,31}$" },
    },
    jql_filter: {
      type: ["string", "null"],
      default: null,
      title: "JQL filter",
      description: "Alternative to Projects: any JQL, without an ORDER BY.",
    },
    username: {
      type: ["string", "null"],
      default: null,
      title: "Username",
      description:
        "Your Jira account. Filled in by Test connection; used for @me and My items. Also the login name for user + password authentication.",
    },
  },
} as const;

/** Gitea — `knobas-source-gitea`'s `config::config_schema`. */
export const GITEA_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    owners: {
      type: "array",
      items: { type: "string" },
      default: [],
      title: "Owners",
      description:
        "Only sync repositories under these owners. Empty syncs every repository the token can see.",
    },
    repos: {
      type: "array",
      items: { type: "string", pattern: "^[^/]+/[^/]+$" },
      default: [],
      title: "Repositories",
      description:
        "owner/name. When set, only these are synced and no repository listing is done.",
    },
    username: {
      type: ["string", "null"],
      default: null,
      title: "Username",
      description: "Your Gitea account. Filled in by Test connection; used for @me filters.",
    },
  },
} as const;

/** TeamCity — `knobas-source-teamcity`'s `config::config_schema`. */
export const TEAMCITY_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    project_ids: {
      type: "array",
      items: { type: "string" },
      title: "Projects",
      description:
        "TeamCity project ids to sync. Leave empty for every project this token can see.",
    },
    build_type_ids: {
      type: "array",
      items: { type: "string" },
      title: "Build configurations",
      description:
        "Build configuration ids to sync. Leave empty for every configuration in the selected projects.",
    },
    // `maximum` is the adapter's `MAX_BUILDS_PER_CONFIG`, and it is the
    // retained history rather than a page size — a full sync keeps the newest
    // this many builds per configuration and mirrors nothing older.
    builds_per_config: {
      type: "integer",
      minimum: 1,
      maximum: 10_000,
      default: 100,
      title: "Builds kept per configuration",
      description:
        "How many finished builds a full sync fetches per configuration. Older builds are not mirrored.",
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
