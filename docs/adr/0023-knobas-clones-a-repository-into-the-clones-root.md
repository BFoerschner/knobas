---
status: accepted
---

# knobas clones a repository into the clones root

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md), as [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md) did for `git status`.

ADR-0016 says nothing knobas spawns writes to a working tree: no clone, no checkout, no fetch. A repository with no checkout shows the clone command to copy. In the ticket-first redesign (requirements R-8.25, D16) that leaves *Open in…* dead on every repository that is not on this machine, and the reader goes to a terminal to paste a command knobas could have run.

Decided 2026-10-15 (Björn, D16): **knobas may run exactly one writing git command, `git clone`, into a new folder directly under the [clones root](../../CONTEXT.md#checkout).** It still never checks out, fetches, pulls or does anything else in an existing clone.

ADR-0016's concern is that a value from the mirror would reach a spawned program. A clone needs a URL, so that concern applies here, and these rules answer it:

- **No shell.** git runs as an argument list, with `--` before the URL and the folder.
- **The URL is built, never taken from the mirror.** It is the Gitea source's address, which a person typed, plus the repository's full name. The name must match `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`, and any other name is refused. A mirrored `web_url` or clone URL is never used.
- **Only https and ssh.** `-c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always`, so no `ext::` and no `file://`. No submodules.
- **Always a new folder,** `<clones root>/<repository name>`, named by the checked name. If the folder exists and is a clone of this repository, knobas adopts it. If it is anything else, knobas refuses and says so. knobas never writes into an existing folder.
- **No credentials from knobas.** git uses the person's own ssh agent or credential helper, so knobas holds nothing new.
- **Cancellable.** Stopping removes the half-made folder and says so.

## Considered options

- **Keep the copy-the-command button.** Rejected by Björn: one click is the point of *Clone*.
- **Use the mirrored clone URL.** Rejected: it is a value a remote system wrote, the exact argument ADR-0016 keeps out.
- **Clone through a git library in process.** Rejected for the reason ADR-0021 gives: a native dependency for one command that git already does.

## Consequences

- One IPC command, `clone_repo`, runs in the background with progress events parsed from `--progress` and can be cancelled. It needs a §10.8 entry.
- The checkout scan finds the result as it finds any clone, so the rest of the checkout panel is unchanged.
- The witness is a live test against testenv Gitea: clone, refuse an existing folder that is not a clone, refuse a bad name, and cancel. There are unit tests on the argument list. No desktop witness is needed, because nothing is opened.
