# 2026-11-22: Light CI on every push; the full gate stays manual

**Asked.** Björn: "github ci is slow, are there lighter checks we could run there that actually would help?" To the
proposed list: "do that".

**Taken.** A new workflow, `.github/workflows/light.yml`, runs on every push to `ux-polish` and `main` and on pull
requests, with cancel-in-progress per ref, on standard `ubuntu-latest` runners. Two jobs, no cargo build:

- **guards** (under a minute): no migration in the newest release tag modified, deleted or renamed; the checksum list
  `migrations.sha384` matches every file (SHA-384 of its bytes, the same value `knobas-db`'s Rust test compares), names
  no missing file and keeps every released line; every `knobas:adapter@X` world the tag declares is byte-identical at
  HEAD, wherever it lives now; `cargo fmt --all --check`; `doc-lists.py` and the conflict-marker scan.
- **front**: `npm ci`, svelte-check, the whole vitest suite, `vite build`.

**Why these.** Each has shipped as a slip from agents that ran only front-end gates. The worst was lessons-learned 12:
a search and replace edited migration 0101's comment, which would have stopped every ux26 database from starting, and
only a Rust test could see it. knobas is a public repository now, and standard hosted runners cost nothing there, so
the 2026-09-05 reason for having no automatic trigger (a billed runner per commit) no longer covers checks this cheap.

**Not changed.** `check.yml` is still manual and disabled (2026-09-05, #2035): a full Rust build per push takes 15 to
40 minutes on a hosted runner. Clippy, the Rust tests and the IPC mirror stay with the local gate and the release's final
run.

**Which release counts.** A pre-release tag sits one commit off the branch (the commit that writes the version into the
manifests), so no ux tag is reachable from `ux-polish`. The guards use the newest `v*` tag whose migrations and WIT
worlds equal those at its merge base with HEAD: on `ux-polish` that is the newest ux pre-release, on `main` the newest
tag `main` has merged. So `knobas:adapter@1.6.0` stays free to change until a tag carries it.

**Not run twice.** A pull request whose head is `ux-polish` or `main` of this repository skips its jobs: the push run
already checked the same commit.
