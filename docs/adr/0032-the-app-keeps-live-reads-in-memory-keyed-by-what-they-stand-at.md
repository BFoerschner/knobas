---
status: accepted
---

# The app keeps live reads in memory, keyed by what they stand at

A [source view](../../CONTEXT.md#source-view) was a live read that was never stored: opening a file twice at one commit cost Gitea the same round trips twice. Decided 2026-10-30 (Björn, Code-area speed grilling, spec #2234 *Step 1*): **the app process keeps live reads in memory, never in the database or on disk, keyed by what they stand at.**

- **Fixed reads are kept for the session.** A read that names a whole commit sha (a file or a directory at the commit, a commit's diff, later a compare between two shas and a pull request's diff at its head) cannot change, so the same ask is answered from memory with no request to the source. A pull request's *Files* carries its reviews' threads, which move, so it cannot be a fixed view: its diff is kept by the built source instead, bounded by bytes and dropped with the source, keyed by what the `.diff` stands at, not by the record's head: the merge base the record names and the newest commit of the pull request's commits listing, which follows `refs/pull/N/head` as the `.diff` does. Forgejo's record names a pushed head seconds before its queue moves that ref, so a diff is kept only when the listing's head is the record's head before and after the `.diff` is read (amended 2026-10-30, #2321, measured on Gitea 1.27 and Forgejo 17; Gitea's `view::pr::diffs`).
- **Moving reads are served stale and read again behind.** A read keyed by a branch name or another moving reference answers from memory when it is there and is re-read in the background; the next read gets the fresh answer (#2242).
- **Everything else is live, as before.** The default.
- **The adapter says which, before the read.** `Source::view_standing(entity, section)` answers *Fixed*, *Moving* or *Live*, reading nothing; its default is *Live*, so an adapter, an external one included, keeps nothing until it says so. The app parses no adapter's ids. A write's grants are kept the same way where `Source::refusal_standing(ask)` says they move (amended 2026-10-30, #2241, #2242): a moved view is told to the shell (`view:fresh`), a moved grant is the next ask's. A file's bytes are kept where `Source::bytes_standing(entity, path, commit)` says *Fixed* (amended 2026-10-30, #2256), measured by their length, never read behind.
- **What is kept is the adapter's answer.** The app's own work on it (rule 7's check against the mirror, a row's commit hint) is done on every read, because the mirror moves under a fixed view.
- **Bounded by bytes, least recently used out first.** A dropped read is read again on demand. A read larger than the bound is answered and not kept.
- **Keyed by the built source.** Each source is built once and lent to every live read (#2240). Rebuilding it (a new address, configuration or credential, a removal) drops what was read through it: a new address may be another server, and a new credential may read less.

## Rejected

- Store the reads in the database. Live reads stay out of the [mirror](../../CONTEXT.md#mirror): it is written by sync alone (ADR-0031), and a disk cache would outlive the credential that read it.
- An HTTP-level cache in `knobas-http`. Only the adapter knows which of its URLs name a fixed commit, and a cache under the adapter could not skip the repository lookups a view makes around the file.
- Key the read by the response, after reading. The point is to send nothing.

## Consequences

- ADR-0020 rule 1 is amended: *never stored* now reads *never written to the mirror or disk*, which it already said of the shell's own memory.
- A fixed view must carry nothing that moves: Gitea's file at a commit offers no *Commit change…*, and a commit's overview (its checks move) and its whole view (its toolbar reads the repository) stay live.
- Round-trip budgets are asserted through the HTTP client's request counter, a `test-util` hook (`knobas_http::counted`).
