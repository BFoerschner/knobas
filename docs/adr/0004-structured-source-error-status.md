---
status: accepted
---

# `SourceError` carries structured HTTP status; one shared error surface in `knobas-http`

Adapters must classify HTTP failures (retry / skip / fatal / credential) but the error reached them message-shaped: `knobas-http` collapsed 401 and 403 into one variant and consumed the response body before an adapter could lift the API's own error envelope out of it. By the third adapter, three private workarounds existed — Jira reconstructs messages afterwards in `humanize`, and Gitea's `credential_still_good` probe exists *only* because 401 and 403 are indistinguishable. Decided 2026-08-27 (Björn): `SourceError` carries the structured status, 401 and 403 become distinct, and a body→message hook lets the caller map a failing response body into the error message. All three adapters migrate onto the shared surface in the same change, so a bare 401 can be Fatal by status — per the original brief — instead of behaviourally.

## Considered options

- **Behavioural classification (the status quo)** — genuinely tried through M1. It produced the probe workaround and made a bare 401 impossible to treat as fatal by status. The moment to unify a seam is when the third caller appears; it has.

## Consequences

- Frozen-contract change: xhigh review tier, one package (status + de-collapse + hook + three adapter migrations).
- Confluence (M3) starts on the shared surface instead of becoming a fourth workaround.
