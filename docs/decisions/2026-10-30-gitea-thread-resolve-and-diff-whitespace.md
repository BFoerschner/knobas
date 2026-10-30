# Gitea: resolving review threads, and whitespace in diffs, 2026-10-30

A fact check for spec #2234 (#2253), not a ruling. #2277 reads answer 1 and #2272 reads answer 2. Both were checked against the testenv products on 2026-10-30: **Gitea 1.27.3** (`/api/v1/version`, port 3000) and **Forgejo 17.0.0+gitea-1.22.0** (port 3002), each against its own served `/swagger.v1.json`. The probe made a throwaway repository `knobas/fact-check-2253` on each product, ran, and deleted it again, so the seeded fixture was not touched. It had two pull requests:

- **PR 1** changes two files. `pay.py` is reindented, with trailing spaces and doubled inner spaces and no other change. `other.txt` has a real one-line change (`alpha` → `beta`).
- **PR 2** changes `pay.py` alone: reindented, with one real edit (`return 1` → `return 2`).

The PRs have one review, a single `COMMENT` thread on `pay.py`, posted by the admin `knobas` with its token.

## 1. Resolve and unresolve a review thread

**Gitea: possible.** The endpoints are `POST /repos/{owner}/{repo}/pulls/comments/{id}/resolve` and `POST …/unresolve` (`repoResolvePullReviewComment`, `repoUnresolvePullReviewComment`). `{id}` is the thread's first review comment, the `id` that `GET /repos/{owner}/{repo}/pulls/{index}/reviews/{review}/comments` lists. Neither takes a body. Each answers `204 No Content`. The state is in that comment's `resolver` field: `null` while open, and the resolving user once resolved.

```
GET  /api/v1/repos/knobas/fact-check-2253/pulls/1/reviews/94/comments
→ 200  [{"id":4088,"path":"pay.py","position":2,"resolver":null,…}]

POST /api/v1/repos/knobas/fact-check-2253/pulls/comments/4088/resolve
→ 204 No Content

GET  /api/v1/repos/knobas/fact-check-2253/pulls/1/reviews/94/comments
→ 200  [{"id":4088,"path":"pay.py","resolver":{"id":1,"login":"knobas",…},…}]

POST /api/v1/repos/knobas/fact-check-2253/pulls/comments/4088/resolve     (already resolved)
→ 204 No Content                                                         (idempotent, no error)

POST /api/v1/repos/knobas/fact-check-2253/pulls/comments/4088/unresolve
→ 204 No Content
GET  …/pulls/1/reviews/94/comments  → 200  [{"id":4088,…,"resolver":null}]

POST /api/v1/repos/knobas/fact-check-2253/pulls/comments/999999/resolve
→ 404  {"message":"comment does not exist [id: 999999, issue_id: 0]",…}
```

The swagger also lists `400` and `403` for these endpoints. The probe did not produce them: the only user it tried was the repository's admin.

**Forgejo 17: not possible.** Forgejo's swagger has no resolve or unresolve path, and both URLs answer `405 Method Not Allowed` with an empty body, even for a comment id that does not exist. `PullReviewComment.resolver` is still there and is read-only: Forgejo returns the state that its web UI sets.

```
POST http://127.0.0.1:3002/api/v1/repos/knobas/fact-check-2253/pulls/comments/6231/resolve
→ 405 Method Not Allowed   (Content-Length: 0)
POST …/pulls/comments/6231/unresolve  → 405
GET  …/pulls/1/reviews/161/comments   → 200  [{"id":6231,…,"resolver":null}]
```

For #2277, resolve depends on the product. A Gitea source can resolve and unresolve, and a Forgejo source can only show `resolver`. The probe did not check whether `/api/v1/version` is enough to tell the two apart (Forgejo's version string contains `+gitea-`). One way to decide is to send the request and treat a `405` as "this source cannot resolve".

## 2. Whitespace options on the diff endpoints

The swagger documents one whitespace parameter, `whitespace` on `GET /repos/{owner}/{repo}/pulls/{index}/files`, with values `ignore-all`, `ignore-change`, `ignore-eol` and `show-all`. The probe sent each value, and no parameter, to every diff endpoint.

| Endpoint | Gitea 1.27.3 | Forgejo 17 |
|---|---|---|
| `GET /pulls/{index}.diff` (`repoDownloadPullDiffOrPatch`) | ignored: every value returns the same body as no parameter | ignored, the same |
| `GET /git/commits/{sha}.diff` (`repoDownloadCommitDiffOrPatch`) | ignored | ignored |
| `GET /compare/{base}...{head}?output=diff` (`repoCompareDiff`) | ignored. `output=diff` returns the raw diff, unchanged | ignored. Forgejo has no `output` parameter and returns the JSON compare |
| `GET /compare/{base}...{head}` (JSON) | ignored | ignored |
| `GET /pulls/{index}/files` (`repoGetPullRequestFiles`) | honoured in the counts only, and **500 when a file drops out** (below) | honoured in the counts only. A file that is whitespace-only drops out of the list |

`/pulls/{index}/files` returns `ChangedFile` objects (`filename`, `status`, `additions`, `deletions`, `changes`, URLs). They have no patch or hunk text, so honouring the parameter changes the counts and the file list and never gives a diff to draw:

```
GET /api/v1/repos/knobas/fact-check-2253/pulls/2/files
→ 200  [{"filename":"pay.py","status":"changed","additions":3,"deletions":3,"changes":6,…}]
GET …/pulls/2/files?whitespace=ignore-all      (also ignore-change)
→ 200  [{"filename":"pay.py","status":"changed","additions":1,"deletions":1,"changes":2,…}]
GET …/pulls/2/files?whitespace=ignore-eol      (also show-all)
→ 200  same as no parameter (the reindent is not end-of-line whitespace)
```

On Gitea 1.27.3, `ignore-all` and `ignore-change` panic if they would leave a file out of the list. That is PR 1, where `pay.py` has whitespace changes only. It happened again on a retry:

```
GET /api/v1/repos/knobas/fact-check-2253/pulls/1/files?whitespace=ignore-all
→ 500 Internal Server Error  (text/plain)
PANIC: runtime error: index out of range [1] with length 1
/go/src/gitea.dev/routers/api/v1/repo/pull.go:1623
```

Forgejo answers the same request with `200 [{"filename":"other.txt","additions":1,"deletions":1,…}]`, so `pay.py` drops out cleanly.

For #2272: **not possible**. No Gitea or Forgejo endpoint returns diff text with whitespace ignored. The `.diff` and compare endpoints take no whitespace parameter, and the one endpoint that does returns counts. *Ignore whitespace* therefore works only on a local `git diff`. On a diff the source serves it stays disabled, and the reason is that the source's API cannot return a diff with whitespace ignored.

## How to rerun

The probe scripts take a base URL, a token and an output directory. They are throwaway, so they are not in the repository. To reproduce, take the Gitea lock (`live_lock_hold <name> gitea` from `testenv/live-lib.sh`) and use the token in `testenv/seed-state.json` (`.gitea.token`). For Forgejo, run `testenv/live up forgejo` and use `testenv/seed-state-forgejo.json`. Then run the requests above against a scratch repository, delete it afterwards, and run `live down forgejo`.
