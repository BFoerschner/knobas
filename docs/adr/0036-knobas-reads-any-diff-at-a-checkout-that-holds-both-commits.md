---
status: accepted
---

# knobas reads any diff at a checkout that holds both commits

Amends [ADR-0026](0026-knobas-reads-a-checkouts-uncommitted-diff.md). Spec #2234 (step 6b) gives a diff between any two commits an address of its own, and asks that it come from local git when the repository's checkout holds both commits, and from the source's compare read otherwise; *Ignore whitespace* is offered when the diff is local. ADR-0026 lets knobas run `git diff` at a checkout for the uncommitted changes only, with no commit name as an argument. Decided 2026-10-30 (implementer on #2272, as the spec asks): **knobas may run read-only `git rev-parse` and `git diff` between two commits at a checkout, on ADR-0026's terms, with the two commits as arguments once checked.**

- **The commands** (`knobas_app::checkout::range`): `git rev-parse --verify --quiet <sha>^{commit}` for each end, which both checks the checkout holds that commit and resolves it to its whole sha; `git rev-parse --is-shallow-repository --show-object-format` when a base `<sha>^` has no parent here; and `git diff --no-ext-diff --no-color --no-textconv -M --src-prefix=a/ --dst-prefix=b/ [-w] <base> <head> --`. Each runs under `working_tree`'s environment (`-C <checkout>`, no optional locks, no fsmonitor, no prompt, closed stdin), with `-c diff.autoRefreshIndex=false` on the diff as in ADR-0026.
- **Never fetched.** Every command runs with `GIT_NO_LAZY_FETCH=1` and `-c protocol.allow=never`: a partial clone missing an object answers "missing" rather than reaching the network. `GIT_CEILING_DIRECTORIES` is the checkout's parent, so a checkout whose `.git` is gone is no repository rather than whichever repository holds its folder. Inherited `GIT_ALLOW_PROTOCOL`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT` (`working_tree::PROTOCOL_VARIABLES`) and `GIT_EXTERNAL_DIFF` are removed from git's environment, since each could undo a flag here, and `GIT_NO_REPLACE_OBJECTS=1` keeps the checkout's replace refs from changing what a sha means. No hook runs: `rev-parse` and a diff between two commits run none, `core.fsmonitor` is off, and `--no-ext-diff` and `--no-textconv` keep a `diff.external`, a diff driver's `command` or a `textconv` the repository's configuration names from running. A checkout that lacks either commit is not read; the source answers, and nothing is fetched into the checkout to make it hold them.
- **Checked names only** ([ADR-0034](0034-open-in-passes-a-code-objects-git-names-once-checked.md)'s grammar): an end reaches git only as 7 to 64 hex digits, the base with at most one trailing `^`. A branch name is always the source's: the checkout's branch of the same name may stand anywhere, so it is never read locally.
- **A root commit.** `<root>^` has no parent; locally the old side is the empty tree of the repository's object format (SHA-1 or SHA-256), which the source cannot answer. A shallow clone's cut also has no parent here, so a shallow checkout sends such a range to the source.
- **Whitespace.** `-w` is passed only when the reader asks to ignore whitespace. The source cannot: no Gitea or Forgejo endpoint returns diff text with whitespace ignored ([fact check, 2026-10-30](../decisions/2026-10-30-gitea-thread-resolve-and-diff-whitespace.md)). On a source-served diff the control is shown disabled with that reason.
- **Capped and timed like ADR-0026**: the output is read up to ADR-0020 rule 6's ceiling and git is stopped past it; the whole local read has 10 seconds, after which the source answers.
- Still forbidden: anything that writes a working tree, the index, a ref or the object store, and anything that touches the network.

## Rejected

- Fetch the missing commit into the checkout. That writes the object store and the remote-tracking refs of a checkout the person owns, and reaches the network on their credentials.
- Read a branch-name range locally. The local branch is the person's, not the source's; the same address would show two different diffs depending on whether a checkout exists.
- Ask the source for the root range and fall back to local git only then. Local first is what makes the diff instant and offline, and the request counter can witness it.

## Consequences

- One IPC command, `entity_range_diff(entityId, base, head, ignoreWhitespace) -> RangeDiff`, whose `read_at` (`checkout` or `source`) is what enables *Ignore whitespace*. It is announced as a frozen-surface change (§10.8).
- The source fallback is the `compare:<base>...<head>` section through the read cache (ADR-0032), so a two-sha range costs the source one read per session.
- `knobas_app::checkout::range::holds_commits` is the one "does this checkout hold these commits" read; the *Diff tool* (#2275, ADR-0035) asks it before it is offered.
- Witnessed by Rust tests against the real `git` in temp repositories: a local read, a short sha and a parent resolved, a missing commit, a branch name, whitespace ignored on a reformat, a root range, a shallow clone, and a recording `git` asserting every command is `rev-parse` or `diff` with the network shut and the refs and index unchanged (`checkout::range`'s tests); and at the app's seam, zero source reads when the checkout holds both commits and one read, kept, when it does not (`tests/it/live_reads.rs`).
