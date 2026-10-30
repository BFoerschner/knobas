---
status: accepted, amended by ADR-0035
---

# knobas clones a repository into the clones root

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md), as [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md) did for `git status`.

ADR-0016 let nothing knobas spawns write to a working tree, so *Open in…* was dead on every repository not on this machine. Decided 2026-10-15 (Björn, D16): **knobas may run exactly one writing git command, `git clone`, into a new folder directly under the [clones root](../../CONTEXT.md#checkout).** It still never checks out, fetches, pulls or does anything else in an existing clone.

A clone needs a URL, so ADR-0016's concern about mirrored values applies. The rules:

- **No shell.** git runs as an argument list, with `--` before the URL and the folder.
- **The URL is built, never taken from the mirror.** It is an address a person typed plus the repository's full name. Since #1197 any code-host source clones: the adapter names which of its instance's addresses to use in the descriptor's `clone_from` (Gitea names its base URL), and knobas appends the checked name. The name must match `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`; any other name is refused. A mirrored `web_url` or clone URL is never used.
- **Only https and ssh.** `-c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always`, so no `ext::` and no `file://`. No submodules.
- **Always a new folder,** `<clones root>/<repository name>`. If the folder exists and is a clone of this repository, knobas adopts it. Anything else is refused and said so. knobas never writes into an existing folder.
- **No credentials from knobas.** git uses the person's own ssh agent or credential helper.
- **Cancellable.** Stopping removes the half-made folder and says so.

## Rejected

- Keep the copy-the-command button. One click is the point of *Clone*.
- Use the mirrored clone URL. A remote system wrote it, which is the argument ADR-0016 keeps out.
- Clone through a git library in process. A native dependency for one command git already does (ADR-0021).

## Consequences

- One IPC command, `clone_repo`, runs in the background with progress events parsed from `--progress` and can be cancelled. It has a §10.8 entry.
- The checkout scan finds the result as it finds any clone.
- The witness is a live test against testenv Gitea: clone, refuse an existing folder that is not a clone, refuse a bad name, cancel. Unit tests cover the argument list. No desktop witness, because nothing is opened.
