---
status: accepted
---

# `SourceError` carries structured HTTP status; one shared error surface in `knobas-http`

Adapters classify HTTP failures (retry, skip, fatal, credential), but `knobas-http` collapsed 401 and 403 into one variant and consumed the body before an adapter could read the API's own error envelope, so by the third adapter three private workarounds existed. Decided 2026-08-27 (Björn): **`SourceError` carries the structured status, 401 and 403 are distinct variants, and a body-to-message hook lets the caller map a failing response body into the error message.** All adapters use the shared surface; a bare 401 is fatal by status, not by probing.

## Rejected

- Behavioural classification. Tried through M1; it produced Gitea's `credential_still_good` probe and made a bare 401 impossible to treat as fatal by status.

## Consequences

- `crates/knobas-http/**` is a frozen surface (§10.8). A new adapter starts on the shared surface rather than adding a workaround.
