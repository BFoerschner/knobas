# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# `front` runs before the cargo recipes, not after: `tauri::generate_context!`
# embeds the built frontend at compile time, so `app/dist` has to exist before
# anything compiles knobas-app. On a fresh clone the reverse order fails to
# build rather than failing a test.
check: fmt front clippy test

fmt:
    env -u RUSTUP_TOOLCHAIN cargo fmt --all --check

clippy:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets -- -D warnings

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
