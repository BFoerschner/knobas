- **`crates/knobas-http/**` — issue #1122 (2026-10-17): a request can opt out of the transport's
  retries, so a write that timed out after landing is not sent again.** Found reviewing PR #1119
  (#983). `send` retried a timeout, and a timed-out POST may already have reached the source:
  Kuma's `add` could make two monitors on a live session, or three when the session was reaped
  between the attempts (the resend's `400 Session ID unknown` read as *not delivered* and bought a
  fresh login). No change to the SPI, the commands or any `HttpConfig`.

  **`crates/knobas-http` — additive:**

  ```rust
  impl Request {
      pub fn no_retry(self) -> Self; // one attempt; its failure returned as it is
  }
  ```

  `send` and `send_streaming` send such a request once, through the limiter and under
  `SEND_BUDGET` as before. Every failure of an attempt that reached the source comes back
  unretried: a timeout, and a `429`/`502`/`503`/`504` too, since a gateway's `502`/`504` can't say
  whether the source behind it acted. A connect error (`reqwest`'s `is_connect`: the connection
  never opened, so no byte left this machine) is still retried, up to `MAX_ATTEMPTS`. Every
  request that does not call it keeps `MAX_ATTEMPTS` for every transient failure.

  **Which writes opt out (the audit):**

  | adapter | write | ruling |
  | --- | --- | --- |
  | Kuma | every socket POST (`40`, `login`, pong, `add`, `pauseMonitor`, `resumeMonitor`) | once |
  | Jira | comment, create, worklog, transition (`post_json`) | once. A transition is usually refused the second time, but not on a workflow that loops back to its own status |
  | Confluence | page create, comment (`post_json`) | once. A page create is also refused while the title is taken in the space |
  | Confluence | page update (`PUT`) | retried: the version it carries makes a second arrival a `409` |
  | Gitea | comment, approval, review (`post_once`) | once |
  | Gitea | branch create, pull request create, merge | retried: Gitea refuses a second one (`409`, `409`, `405`) |
  | Gitea | branch update, pull request edit (`PATCH`), branch delete, file commit (`PUT` with `sha`) | retried: idempotent, or refused the second time (`sha` precondition) |
  | TeamCity | trigger and custom run (`POST buildQueue`) | once |
  | TeamCity | parameter set (`PUT`) | retried: idempotent |

  A write sent once and failed reaches the write queue as its error. Whether the queue resends
  an unclaimed create after an `Unreachable` (`UNCLAIMED_OPS`) is a separate question that this
  entry leaves open.
