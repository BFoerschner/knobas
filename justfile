# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# `front` runs before the cargo recipes because it is the fast half: a
# svelte-check error or a broken `vite build` is reported in seconds instead of
# after a full workspace compile. It is *not* a build dependency of the cargo
# recipes -- `tauri::generate_context!` only reads `frontendDist` when the
# `custom-protocol` feature is on (production, i.e. `tauri build`); a dev build
# with `devUrl` set takes the empty default asset set and never looks at
# `app/dist`. Nothing here may create that directory either: a missing one is
# exactly how `tauri build` refuses to bundle an app with no frontend in it.
check: fmt front clippy clippy-libs inventory test

# The test inventory: every test this workspace defines, by name, committed.
#
# `just check` going green is not evidence that the tests you wrote still
# exist. A test that is *deleted* takes its own failure with it, so the suite
# reports success on what is left and the count moves in whatever direction the
# rest of the commit pushed it. This has happened here: a mis-scoped splice
# removed four tests in one commit while three were added in the same commit,
# so the file-level count *rose*; the loss was found only because a mutation
# that used to die stopped dying, and two of the four survived a name-by-name
# audit because an audit can only look for names someone already suspects.
#
# Diffing the names is the check that does not depend on suspicion. A deleted
# test cannot hide behind an added one, because both are lines.
#
# This does not forbid deleting a test -- retiring one is often right. It makes
# the deletion *visible*: `just inventory-update` turns it into a `-` line in
# the PR diff, where a reviewer sees it and asks why, instead of it being
# invisible against a green suite.
#
# Scope: unit and integration tests, keyed by package, target and test name, so
# two crates that both have `tests/mockd.rs` stay distinct and a test that moves
# between crates reads as one removal plus one addition. Doctests are not listed
# -- cargo builds no binary for them, so there is nothing to enumerate; they are
# still run by `test`. Nor are the tests named in
# `test-inventory-conditional.txt`: a `#[cfg(target_os = ...)]` test exists on
# one platform and not another, and the file is written on macOS and checked on
# `ubuntu-latest`, so it cannot be pinned by a list that has to match on both.
# Each line there costs the gate one test and is argued for in place.
inventory:
    #!/usr/bin/env bash
    set -euo pipefail
    just _inventory-write /tmp/knobas-inventory-actual.txt
    if ! diff -u test-inventory.txt /tmp/knobas-inventory-actual.txt; then
        echo >&2
        echo "error: the test inventory does not match test-inventory.txt." >&2
        echo "  '-' lines are tests that no longer exist. If that is deliberate," >&2
        echo "  run 'just inventory-update' and commit it, so the removal shows up" >&2
        echo "  in the diff a reviewer reads." >&2
        exit 1
    fi

# Regenerate `test-inventory.txt`. Run this whenever you add or remove a test,
# and commit the result alongside the change that caused it.
inventory-update:
    #!/usr/bin/env bash
    set -euo pipefail
    just _inventory-write test-inventory.txt
    echo "test-inventory.txt: $(wc -l < test-inventory.txt | tr -d ' ') tests"

# Enumerate the workspace's tests into the file named by $1.
#
# `--no-run` builds the test binaries and `--message-format=json` names them;
# asking each binary to `--list` itself is what makes the result the *harness's*
# answer rather than a guess parsed out of the source. `--list` enumerates, it
# does not execute, so nothing here starts a database.
#
# The package name comes from `package_id`, which cargo spells two ways
# depending on version (`<path>#<version>` and `<path>#<name>@<version>`); both
# are handled. LC_ALL=C keeps the sort byte-wise, so the file does not churn
# when a machine's locale differs.
#
# `test-inventory-conditional.txt` is subtracted here rather than at diff time,
# so `test-inventory.txt` means one thing on every machine: the tests that exist
# everywhere. Filtering only the *actual* side would leave the committed file
# platform-shaped, and filtering both sides at the diff would still let
# `inventory-update` write a file whose contents depend on who ran it.
_inventory-write FILE:
    #!/usr/bin/env bash
    set -euo pipefail
    env -u RUSTUP_TOOLCHAIN cargo test --workspace --no-run --message-format=json 2>/dev/null \
      | jq -r 'select(.executable != null and .profile.test == true)
               | (.package_id | if test("#.*@") then (split("#")[1] | split("@")[0])
                                else (split("#")[0] | split("/") | last) end) as $pkg
               | "\($pkg)\t\(.target.kind[0])/\(.target.name)\t\(.executable)"' \
      | while IFS=$'\t' read -r pkg target exe; do
            "$exe" --list 2>/dev/null | sed -n 's/: test$//p' | sed "s|^|${pkg}\t${target}\t|"
        done \
      | grep -vxF -f <(sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' test-inventory-conditional.txt) \
      | LC_ALL=C sort > "{{FILE}}"

fmt:
    env -u RUSTUP_TOOLCHAIN cargo fmt --all --check

clippy:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets -- -D warnings

# The same lint pass over the libraries alone -- which is the only way to see
# them the way something that merely *depends* on them does.
#
# `--all-targets` pulls in every crate's dev-dependencies, and cargo unifies
# features across the whole invocation. `knobas-db` dev-depends on itself with
# `test-util` on, so that one dev-dependency silently turns the feature on for
# the library build too, and the lib gets linted in a configuration nothing
# ships. Items whose only callers are behind `test-util` then look live, and
# their dead-code warnings surface only later -- in `tauri dev`, or for any
# consumer building `knobas-db` without the feature.
#
# Dropping `--all-targets` for `--lib` drops the dev-dependencies with it, so
# each library is checked with the features a consumer actually gets. Almost
# free after the pass above: same crates, same profile, a subset of the units.
clippy-libs:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --lib -- -D warnings

test:
    env -u RUSTUP_TOOLCHAIN cargo test --workspace

# Install app/ dependencies if they are missing or older than the lockfile.
#
# `npm ci` deletes and reinstalls node_modules wholesale: right on a fresh
# clone or after the lockfile moves, pure waste on every other run. Staleness
# is a timestamp comparison, not a content one -- npm's hidden
# `node_modules/.package-lock.json` is its own flattened view of the tree and is
# never byte-equal to `package-lock.json`, so comparing the two files would
# reinstall every single time. npm rewrites the hidden file on install, so
# "lockfile is newer" means exactly "installed against an older lockfile",
# including after a pull or a branch switch.
deps:
    #!/usr/bin/env bash
    set -euo pipefail
    cd app
    if [ ! -d node_modules ] || [ package-lock.json -nt node_modules/.package-lock.json ]; then
        npm ci
    fi

front: deps
    cd app && npm run check && npm run build

# The desktop app against the embedded database. Set KNOBAS_DB_URL to point it
# at a server you manage instead.
#
# The Tauri CLI is a devDependency of `app/` (there is no `cargo tauri-cli` in
# this repo), so `deps` is what provisions it; it then has to run from the crate
# that owns `tauri.conf.json`, which is why the CLI comes off PATH rather than
# from the current directory.
dev: deps
    cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri dev

# The demo profile: its own data directory, its own database on its own port,
# its own keychain service. Demo data never mixes with a real corpus
# (interfaces §8 P13), and `demo_load` is refused outside it.
#
# Two `--`: the first ends the Tauri CLI's own arguments, the second ends
# cargo's, so `--demo` reaches the knobas binary itself. The startup log line
# `profile demo=true` is the acceptance test for that.
demo: deps
    cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri dev -- -- --demo
