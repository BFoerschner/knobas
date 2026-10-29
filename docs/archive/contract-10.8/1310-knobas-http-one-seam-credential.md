- **`crates/knobas-http/**` — issue #1310 (2026-10-19): one HTTP seam under the built-in
  adapters, starting with the credential mapping.** Spec #1302, which moves three things every
  built-in adapter copies into the transport: the credential mapping (this issue), the error body
  excerpt (#1313) and the page walk (#1315). This entry covers all three. The credential function
  lands here; the excerpt option and the walker are named in advance, and each follow-up adds its
  signatures below in an `Amendment (#<issue>, <date>):` line. All three are additive: no change to
  the SPI, the commands, `HttpConfig`, `Auth`, retry, budget, classification or deadlines. Ratified
  by the implementer on Opus; the rulings are in `docs/archive/2026-10-unattended-rulings.md`
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

  **The adapter (Jira):** `knobas-source-jira`'s `http::CREDENTIAL_RULES` states Jira Data Center's
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

  **Amendment (#1313, 2026-10-19): the excerpt option.** Additive in `crates/knobas-http`, which
  now depends on `serde_json` (already in the tree through `reqwest`):

  ```rust
  pub const BODY_EXCERPT_CHARS: usize = 300;    // Jira's bound since #877
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct BodyExcerpt { pub contract: &'static str }
  impl HttpClient {
      pub async fn get_json_with<T: DeserializeOwned, Q: Serialize + ?Sized>(&self,
          path: &str, query: &Q, excerpt: Option<BodyExcerpt>) -> Result<T, SourceError>;
  }
  ```

  With `None` it is `get_json`, message for message (`decoding <path>: <error>`); `get_json`
  itself is unchanged. With `Some`, a body that does not decode reads `<path> did not answer with
  the shape <contract> documents: <serde error>; it began <the first 300 characters, Debug-quoted>`,
  and a body that breaks off reads `<path>: the answer's body broke off: <error>`: the two messages
  Jira's wrapper built, word for word. A failing status is untouched: it is `status_error`'s, with
  the caller's `BodyMessage`. Jira's `JiraHttp` and its decoding `get_json` are gone; `http::client`
  builds the transport's `HttpClient` and `http::get_json` passes `BodyExcerpt { contract: "the
  Jira REST v2 contract" }`. Pinned by `tests/transport.rs`
  `a_body_that_is_not_the_shape_asked_for_names_its_start_when_asked` (present when asked, absent
  when not, bounded in characters) and `the_excerpt_leaves_a_failing_status_and_a_good_body_alone`,
  and on Jira's side by `http::tests::a_body_that_is_not_jiras_shape_names_its_start`.

  **Amendment (#1315, 2026-10-19): the page walker.** Additive in `crates/knobas-http`, a new
  private module `walk` whose items are re-exported at the crate root:

  ```rust
  pub trait Paged {                             // an adapter's page type
      fn rows(&self) -> usize;
      fn total(&self) -> Option<usize> { None } // Jira's total, Gitea's X-Total-Count
      fn next_link(&self) -> Option<&str> { None } // Confluence's _links.next
  }
  impl<T> Paged for Vec<T>;                     // rows only
  impl<T> Paged for (Vec<T>, Option<usize>);    // rows and a total
  pub trait Protocol {
      type At;
      const EMPTY_ENDS: bool;
      fn first(&self) -> Self::At;
      fn after(&self, pages: u32, rows: usize, page: &impl Paged)
          -> Result<Option<Self::At>, SourceError>;
  }
  #[derive(Debug, Clone, Copy)] pub struct PageNumber;   // At = u32, from 1
  #[derive(Debug, Clone, Copy)] pub struct Offset;       // At = u32, from 0
  #[derive(Debug, Clone, Copy)] pub struct NextLink { pub product: &'static str } // At = Option<String>
  #[derive(Debug)] pub enum Walked<T> { Page(T), End, Capped }
  pub struct PageWalk<P: Protocol, F>;          // Debug
  impl<P: Protocol, F: FnMut(P::At) -> Fut, Fut: Future<Output = Result<T, SourceError>>, T: Paged>
      PageWalk<P, F> {
      pub fn new(protocol: P, cap: u32, fetch: F) -> Self;
      pub async fn next(&mut self) -> Result<Walked<T>, SourceError>;
      pub fn rows(&self) -> usize;              // records on the pages handed out
  }
  ```

  `PageNumber` asks `1, 2, 3, …`; `Offset` asks `0` and then the records read so far. Both end on
  an empty page, which is not handed out, or once the records read reach the total the last page
  reported; a short page never ends them. `NextLink` asks `None` first and then the link the page
  before offered; it ends on a page with no link, follows an empty page that has one, and refuses
  a link that does not start with `/` with `Protocol("<product> answered with a continuation link
  this adapter cannot follow: <link, Debug-quoted> is not a path rooted at the instance. Refusing
  rather than reporting a walk that stopped early as a completed one.")`, raised on the call after
  that page. `next` answers `Capped`, never `End`, when the walk would ask for a page past `cap`
  pages, so `cap` pages are read at most; whether that fails the run stays the adapter's. The
  crate docs gain one bullet naming the walk; nothing else in the crate changes.

  **The adapters:** Gitea's five sync walks (repositories, branches, pull requests, issues,
  commits) and its label read walk `PageNumber`, and its `sync::last_page` is gone; Jira's search
  walks `Offset` under its `MAX_PAGES` and its comment completion walks `Offset` with no cap
  (`u32::MAX`), as before; Confluence's `sync::Walk` wraps a `NextLink { product: "Confluence" }`
  walk under its `MAX_PAGES` and keeps its own refusal message. Every cap and every message is
  unchanged. TeamCity's widening re-read (`sync::all_of`) is not a page sequence and stays its
  own, documented there. Pinned by `tests/transport.rs`'s `walk::*` (the empty-page stop and
  short-page continue for page number and offset, the total stop, the cap answered as `Capped`
  with no request past it, the link followed through an empty page, the absolute-link refusal
  word for word, a failing fetch), and on each adapter's side by its existing paging tests,
  unchanged: Jira's and Confluence's page-cap tests, Gitea's cap and label tests. Witnessed against
  the real servers by `just gitea-live` and `just atlassian-live`.
