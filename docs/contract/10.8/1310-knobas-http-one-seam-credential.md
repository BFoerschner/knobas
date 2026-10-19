- **`crates/knobas-http/**` — issue #1310 (2026-10-19): one HTTP seam under the built-in
  adapters, starting with the credential mapping.** Spec #1302, which moves three things every
  built-in adapter copies into the transport: the credential mapping (this issue), the error body
  excerpt (#1313) and the page walk (#1315). This entry covers all three. The credential function
  lands here; the excerpt option and the walker are named in advance, and each follow-up adds its
  signatures below in an `Amendment (#<issue>, <date>):` line. All three are additive: no change to
  the SPI, the commands, `HttpConfig`, `Auth`, retry, budget, classification or deadlines. Ratified
  by the implementer on Opus; the rulings are in `docs/decisions/2026-10-unattended-rulings.md`
  under `## #1310`.

  **`crates/knobas-http` — additive (#1310):**

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TokenSpelling { Bearer, Token }      // Auth::Bearer, Auth::GiteaToken
  #[derive(Debug, Clone, Copy)]
  pub struct CredentialRules {
      pub pat: TokenSpelling,                   // how AuthMethod::Pat is sent
      pub methods: &'static [AuthMethod],       // what the adapter speaks
      pub unconfigured: &'static str,           // the refusal for no method
      pub unsupported: fn(AuthMethod) -> String,// the refusal for any other
  }
  pub fn credential(rules: &CredentialRules, auth: Option<AuthMethod>,
      username: Option<&str>, secret: Option<&str>) -> Result<Auth, SourceError>;
  ```

  It judges in this order, the order the adapter copies used. No method gives
  `Protocol(unconfigured)`. A method outside `methods`, or `OAuth`, which has no wire form, gives
  `Protocol(unsupported(method))`. User + password with no username gives `Protocol("user +
  password authentication needs a username in the source configuration")`. Only then does a
  missing secret give `Unauthorized { status: None }`, so a misconfigured source is never
  reported as `missing_secret`. `Pat` maps to `rules.pat`, `UserPassword` to `Basic { username,
  secret }`, and `ApiToken` to `Basic { "", secret }` (Uptime Kuma's `/metrics`). `username` is
  used as given.

  **Named in advance, signatures to follow:** an option on the transport's JSON read that puts
  the leading part of a non-JSON or failing body into the message, bounded as Jira's wrapper
  bounds it (#877, #1313), and a page walker that owns the loop, the cap and the rule that an
  empty page ends the walk and a short one does not, for page plus limit, offset plus total and
  next link (#1315).

  **The adapter (Jira):** `knobas-source-jira`'s `http::CREDENTIAL` states Jira Data Center's
  rules with its existing messages word for word, and `source::build` calls
  `knobas_http::credential` with them. Its private `credential`, `Scheme` and `scheme` are gone.
  Confluence and TeamCity (#1311), then Gitea and Kuma (#1312), follow and need no surface beyond
  the one above.

  Pinned by: `knobas-http`'s `tests/transport.rs`
  `a_configured_method_maps_to_one_credential_or_one_refusal`. It is a table over every
  `AuthMethod` and both token spellings that asserts the header the built client sends, or the
  refusal's class and its whole message, for the missing-secret, unsupported-method and
  misconfigured refusals, and for the configuration outranking a missing secret. Witnessed against
  the real server by `just jira-live`.
